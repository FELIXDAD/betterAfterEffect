//! Spatial effects with layer inputs. Pixel coordinates use the render grid.
use super::{image_grid, layer_input};
use crate::{
    EffectTarget, TargetEffect,
    gpu::{self, Resource, UniformWriter},
    params::Params,
};
use bae_model::{EffectDef, ParamKind, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};

pub(super) type Bounds = fn(Params<'_>, IRect) -> IRect;
pub(super) fn unchanged(_: Params<'_>, r: IRect) -> IRect {
    r
}
pub(super) struct Sampled {
    pub def: EffectDef,
    pub label: &'static str,
    pub shader: &'static str,
    pub inputs: &'static [(u32, bool)],
    pub bounds: Bounds,
}
pub(super) const UNIFORM: u32 = 736;
impl Effect for Sampled {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        self.inputs
            .iter()
            .filter_map(|&(id, own)| layer_input::dependency(p, id, own))
            .collect()
    }
    fn input_roi(&self, _: &EffectParams<'_>, r: IRect) -> IRect {
        if r.is_empty() {
            r
        } else {
            IRect::new(-(1 << 26), -(1 << 26), 1 << 26, 1 << 26)
        }
    }
    fn output_bounds(&self, p: &EffectParams<'_>, r: IRect) -> IRect {
        (self.bounds)(Params::new(&self.def, p), r)
    }
    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        r: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, p, input, r)
    }
}
impl TargetEffect for Sampled {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        r: IRect,
    ) -> Result<GpuImage, RenderError> {
        if r.is_empty() {
            return Ok(input.clone());
        }
        let grid = image_grid::grid_scale(p, input);
        let layer = if p.layer_bounds.is_empty() {
            image_grid::pixel_rect(input.rect, 1. / grid)
        } else {
            image_grid::layer_rect(p)
        };
        let rt = gpu::runtime(target.gpu());
        let kernel = image_grid::kernel_with_inputs(
            &rt,
            target.format(),
            self.label,
            self.shader,
            UNIFORM,
            5,
        )?;
        let out = target.allocate(image_grid::pixel_rect(r, grid), grid)?;
        let mut w = UniformWriter::with_size(UNIFORM as usize);
        image_grid::image_coords(&mut w, 0, input, grid);
        let maps: Vec<_> = self
            .inputs
            .iter()
            .map(|&(id, own)| {
                layer_input::dependency(p, id, own)
                    .and_then(|dep| target.effect_layer(&dep))
                    .cloned()
                    .or_else(|| {
                        (own && p.layer_bounds.is_empty() && p.value(id).is_none())
                            .then(|| input.clone())
                    })
            })
            .collect();
        for (i, img) in maps.iter().enumerate() {
            if let Some(img) = img {
                image_grid::image_coords(&mut w, (i + 1) * 32, img, grid);
            }
        }
        w.i32s(160, &image_grid::rect4(r))
            .f32s(
                176,
                &[
                    layer.x0 as f32,
                    layer.y0 as f32,
                    layer.width() as f32,
                    layer.height() as f32,
                ],
            )
            .f32(192, p.scale as f32)
            .u32(196, u32::from(p.float))
            .f32(200, p.time.as_seconds() as f32);
        let params = Params::new(&self.def, p);
        for param in self.def.params.iter().filter(|param| param.id < 33) {
            let i = param.id;
            let offset = 208 + 16 * i as usize;
            match params.value(i) {
                Some(Value::Color(c)) => {
                    w.f32s(offset, &c.map(|v| v as f32));
                }
                Some(Value::Vec2(c)) if param.kind == ParamKind::Point => {
                    let xy = if p.value(i).is_some() {
                        c.map(|a| (a * p.scale) as f32)
                    } else {
                        [
                            layer.x0 as f32 + layer.width() as f32 * c[0] as f32 / 640.,
                            layer.y0 as f32 + layer.height() as f32 * c[1] as f32 / 360.,
                        ]
                    };
                    w.f32s(offset, &xy);
                }
                _ => {
                    w.f32(offset, params.f(i) as f32);
                }
            }
        }
        let offset = gpu::push_uniform(&rt, &w);
        let mut resources = vec![Resource::Uniform, Resource::View(&input.view)];
        for slot in 0..4 {
            resources.push(Resource::View(
                maps.get(slot)
                    .and_then(Option::as_ref)
                    .map_or(&input.view, |m| &m.view),
            ));
        }
        resources.push(Resource::View(&out.view));
        let bind = rt.bind_group(&kernel, &resources);
        let mut pass = target.encoder().begin_compute_pass(&Default::default());
        gpu::dispatch(&mut pass, &kernel, &bind, &[offset], r.size());
        drop(pass);
        Ok(out)
    }
}
