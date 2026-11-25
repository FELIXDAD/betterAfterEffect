//! A layer-space compute pass for effects with auxiliary images. Each parameter
//! occupies its AE disk-ID slot. This keeps the CPU and WGSL layouts identical.
#[path = "sampled_lights.rs"]
pub(crate) mod lights;
use super::{image_grid, layer_input};
use crate::{
    EffectTarget, TargetEffect,
    gpu::{self, Resource, UniformWriter},
    params::Params,
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};

const SLOTS: usize = 233;
const HEADER: usize = 224;
const BYTES: u32 = (HEADER + SLOTS * 16) as u32;

pub(crate) struct Shader {
    pub label: &'static str,
    pub source: &'static str,
}

impl Shader {
    pub(crate) fn kernel(
        &self,
        rt: &gpu::Runtime,
        format: wgpu::TextureFormat,
    ) -> Result<std::sync::Arc<gpu::Kernel>, RenderError> {
        image_grid::kernel_with_inputs(rt, format, self.label, self.source, BYTES, 5)
    }

    pub fn dispatch<const N: usize>(
        &self,
        target: &mut dyn EffectTarget,
        v: Params<'_>,
        roi: IRect,
        input: &GpuImage,
        aux: [Option<&GpuImage>; N],
        stage: [u32; 2],
    ) -> Result<GpuImage, RenderError> {
        self.dispatch_with_uniforms(target, v, roi, input, aux, stage, |_, _| {})
    }

    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_with_uniforms<const N: usize>(
        &self,
        target: &mut dyn EffectTarget,
        v: Params<'_>,
        roi: IRect,
        input: &GpuImage,
        aux: [Option<&GpuImage>; N],
        stage: [u32; 2],
        extra: impl FnOnce(&mut UniformWriter, usize),
    ) -> Result<GpuImage, RenderError> {
        const { assert!(N <= 4) };
        let rt = gpu::runtime(target.gpu());
        let kernel = self.kernel(&rt, target.format())?;
        let grid = image_grid::grid_scale(v.p, input);
        let layer = if v.p.layer_bounds.is_empty() {
            image_grid::pixel_rect(input.rect, 1. / grid)
        } else {
            image_grid::layer_rect(v.p)
        };
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let mut u = UniformWriter::with_size(BYTES as usize);
        let mut presence = 0;
        image_grid::image_coords(&mut u, 0, input, grid);
        let maps = std::array::from_fn::<_, 4, _>(|i| {
            if let Some(image) = aux.get(i).copied().flatten() {
                presence |= 1 << i;
                image_grid::image_coords(&mut u, (i + 1) * 32, image, grid);
                image
            } else {
                input
            }
        });
        u.i32s(160, &image_grid::rect4(roi))
            .i32s(176, &image_grid::rect4(layer))
            .u32(192, presence)
            .u32(196, stage[0])
            .u32(200, stage[1])
            .u32(204, u32::from(v.p.float))
            .f32(208, v.p.scale as f32)
            .f32(212, v.p.time.as_seconds() as f32)
            .f32(216, (1.0 / v.p.time_step.as_seconds()) as f32);
        for p in &v.def.params {
            let i = p.id as usize;
            if i >= SLOTS {
                continue;
            }
            let offset = HEADER + i * 16;
            match v.value(p.id) {
                Some(Value::Color(c)) => {
                    u.f32s(offset, &c.map(|x| x as f32));
                }
                Some(Value::Vec2(xy)) => {
                    let point = if v.p.value(p.id).is_none()
                        && (p.flags.reference_point_default || p.flags.relative_default)
                    {
                        let units = if p.flags.relative_default {
                            [1., 1.]
                        } else {
                            [640., 360.]
                        };
                        [
                            (f64::from(layer.x0) + xy[0] * f64::from(layer.width()) / units[0])
                                as f32,
                            (f64::from(layer.y0) + xy[1] * f64::from(layer.height()) / units[1])
                                as f32,
                        ]
                    } else {
                        xy.map(|x| (x * v.p.scale) as f32)
                    };
                    u.f32s(offset, &point);
                }
                Some(Value::Int(_) | Value::Bool(_) | Value::Float(_)) => {
                    u.f32(offset, v.f(p.id) as f32);
                }
                _ => {}
            }
        }
        lights::write(v.p.effect, &mut u, HEADER);
        extra(&mut u, HEADER);
        let offset = gpu::push_uniform(&rt, &u);
        let group = rt.bind_group(
            &kernel,
            &[
                Resource::Uniform,
                Resource::View(&input.view),
                Resource::View(&maps[0].view),
                Resource::View(&maps[1].view),
                Resource::View(&maps[2].view),
                Resource::View(&maps[3].view),
                Resource::View(&out.view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(self.label),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &kernel, &group, &[offset], roi.size());
        Ok(out)
    }
}

pub(crate) type Region = fn(Params<'_>, IRect) -> IRect;

pub(crate) fn unchanged(_: Params<'_>, roi: IRect) -> IRect {
    roi
}

/// A single-pass effect. Each algorithm declares its input footprint and output bounds.
pub(crate) struct Sampled {
    pub def: EffectDef,
    pub shader: &'static Shader,
    pub mode: u32,
    pub inputs: &'static [(u32, bool)],
    pub read: Region,
    pub bounds: Region,
}

impl Effect for Sampled {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn prepare_scene(
        &self,
        instance: &mut bae_render::RenderEffect,
        comp: &bae_render::RenderComp,
        layer: &bae_render::RenderLayer,
    ) {
        lights::collect(&self.def, instance, comp, layer);
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        if roi.is_empty() {
            roi
        } else {
            (self.read)(Params::new(&self.def, p), roi)
        }
    }
    fn output_bounds(&self, p: &EffectParams<'_>, input: IRect) -> IRect {
        (self.bounds)(Params::new(&self.def, p), input)
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        self.inputs
            .iter()
            .filter_map(|&(id, own)| layer_input::dependency(p, id, own))
            .collect()
    }
    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, p, input, roi)
    }
}

impl TargetEffect for Sampled {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let mut maps = [None, None, None, None];
        for (slot, &(id, own)) in self.inputs.iter().enumerate() {
            if let Some(dep) = layer_input::dependency(p, id, own) {
                maps[slot] = target.effect_layer(&dep).cloned().or_else(|| {
                    (own && p.layer_bounds.is_empty() && p.value(id).is_none())
                        .then(|| input.clone())
                });
            }
        }
        let aux = maps.each_ref().map(Option::as_ref);
        self.shader.dispatch(
            target,
            Params::new(&self.def, p),
            roi,
            input,
            aux,
            [self.mode, 0],
        )
    }
}
