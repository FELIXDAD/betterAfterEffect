//! GPU inverse maps for distortion effects, including referenced layers and masks.
mod definitions;
pub(crate) use definitions::entries;

use super::{layer, point};
use crate::{
    Builtin, EffectTarget, TargetEffect,
    effects::{image_grid, layer_input},
    gpu::{self, Resource, UniformWriter},
    params::Params,
    rust_builtin,
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, ParamKind, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};
use glam::DVec2;
use std::sync::Arc;

#[repr(u32)]
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    MeshWarp,
    BezierWarp,
    Magnify,
    Warp,
    CcPageTurn,
    CcLens,
    CcBendIt,
    CcGriddler,
    CcTiler,
    DetailPreservingUpscale,
}
struct Warp {
    def: EffectDef,
    kind: Kind,
}

impl Warp {
    fn layer_input(&self, p: &EffectParams<'_>) -> Option<LayerDependency> {
        if self.kind == Kind::CcPageTurn {
            layer_input::dependency(p, 5, true)
        } else {
            None
        }
    }
}
impl Effect for Warp {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        self.layer_input(p).into_iter().collect()
    }

    fn input_roi(&self, _: &EffectParams<'_>, roi: IRect) -> IRect {
        if roi.is_empty() {
            roi
        } else {
            IRect::new(-(1 << 26), -(1 << 26), 1 << 26, 1 << 26)
        }
    }
    fn output_bounds(&self, params: &EffectParams<'_>, input: IRect) -> IRect {
        if input.is_empty() {
            return input;
        }
        let p = Params::new(&self.def, params);
        let (origin, size) = layer(p, input);
        let rect = |lo: DVec2, hi: DVec2| {
            IRect::new(
                lo.x.floor() as i32,
                lo.y.floor() as i32,
                hi.x.ceil() as i32,
                hi.y.ceil() as i32,
            )
        };
        match self.kind {
            Kind::DetailPreservingUpscale => {
                let c = origin + size * 0.5;
                let half = size * p.f(1) * 0.005;
                rect(c - half, c + half)
            }
            Kind::Magnify if p.b(9) => {
                let c = point(p, 1, origin, size);
                let r = DVec2::splat(
                    (p.f(2) * if p.popup(11) > 1 { p.f(3) * 0.01 } else { 1. } + p.f(4))
                        * params.scale,
                );
                input.union(&rect(c - r, c + r))
            }
            Kind::BezierWarp => {
                let (lo, hi) = (1..=12)
                    .map(|id| point(p, id, origin, size))
                    .fold((origin, origin + size), |(lo, hi), pt| {
                        (lo.min(pt), hi.max(pt))
                    });
                rect(lo, hi)
            }
            _ => input,
        }
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

const HEADER: usize = 368;
const GRID_BYTES: usize = 32 * 32 * 8;
const COMMON: &str = include_str!("common.wgsl");
impl TargetEffect for Warp {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        params: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let p = Params::new(&self.def, params);
        let grid = image_grid::grid_scale(params, input);
        let fallback = image_grid::pixel_rect(input.rect, 1. / grid);
        let (origin, size) = layer(p, fallback);
        let back = self
            .layer_input(params)
            .and_then(|dep| target.effect_layer(&dep).cloned())
            .or_else(|| {
                (self.kind == Kind::CcPageTurn
                    && params.layer_bounds.is_empty()
                    && params.value(5).is_none())
                .then(|| input.clone())
            });
        let mesh = self.kind == Kind::MeshWarp;
        let group = match self.kind {
            Kind::MeshWarp => 0,
            Kind::CcBendIt | Kind::CcTiler | Kind::CcGriddler => 2,
            Kind::DetailPreservingUpscale => 3,
            _ => 1,
        };
        let bytes = HEADER + if mesh { GRID_BYTES } else { 0 };
        let rt = gpu::runtime(target.gpu());
        let kernel = pipeline(&rt, target.format(), group)?;
        let label = GROUPS[group].0;
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let mut w = UniformWriter::with_size(bytes);
        image_grid::image_coords(&mut w, 0, input, grid);
        if let Some(img) = &back {
            image_grid::image_coords(&mut w, 32, img, grid);
        }
        w.i32s(64, &image_grid::rect4(roi))
            .f32s(
                80,
                &[
                    origin.x as f32,
                    origin.y as f32,
                    size.x as f32,
                    size.y as f32,
                ],
            )
            .u32(96, self.kind as u32)
            .u32(100, u32::from(params.float))
            .f32(104, params.scale as f32)
            .f32(108, params.time.as_seconds() as f32);
        for param in &self.def.params {
            if param.id >= 15 {
                continue;
            }
            let at = 112 + param.id as usize * 16;
            match param.kind {
                ParamKind::Point => {
                    let xy = point(p, param.id, origin, size);
                    w.f32s(at, &[xy.x as f32, xy.y as f32]);
                }
                ParamKind::Color => {
                    if let Some(Value::Color(c)) = p.value(param.id) {
                        w.f32s(at, &c.map(|v| v as f32));
                    }
                }
                _ => {
                    w.f32(at, p.f(param.id) as f32);
                }
            }
        }
        if mesh {
            // Row-major little-endian normalized forward displacements. Changing the
            // grid resolution resamples the authored field rather than reinterpreting it.
            let id = 4;
            if let Some(Value::Bytes(data)) = p.value(id)
                && data.len() >= 2
            {
                let cols = usize::from(data[0]);
                let rows = usize::from(data[1]);
                if (2..=32).contains(&cols)
                    && (2..=32).contains(&rows)
                    && data.len() == 2 + cols * rows * 8
                {
                    w.f32s(352, &[cols as f32, rows as f32]);
                    for (i, b) in data[2..].as_chunks::<4>().0.iter().enumerate() {
                        let n = f32::from_le_bytes(*b);
                        if n.is_finite() {
                            w.f32(HEADER + i * 4, n);
                        }
                    }
                }
            }
        }
        let offset = gpu::push_uniform(&rt, &w);
        let group = rt.bind_group(
            &kernel,
            &[
                Resource::Uniform,
                Resource::View(&input.view),
                Resource::View(back.as_ref().map(|img| &img.view).unwrap_or(&input.view)),
                Resource::View(&out.view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(label),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &kernel, &group, &[offset], roi.size());
        drop(pass);
        Ok(out)
    }
}

const GROUPS: [(&str, &str); 4] = [
    ("bae.effects/warp-grid", include_str!("mesh.wgsl")),
    ("bae.effects/warp-surface", include_str!("surface.wgsl")),
    ("bae.effects/warp-cc", include_str!("cc.wgsl")),
    ("bae.effects/warp-upscale", include_str!("upscale.wgsl")),
];
fn pipeline(
    rt: &gpu::Runtime,
    format: wgpu::TextureFormat,
    group: usize,
) -> Result<Arc<gpu::Kernel>, RenderError> {
    static SOURCES: [std::sync::OnceLock<String>; 4] = [const { std::sync::OnceLock::new() }; 4];
    let (label, body) = GROUPS[group];
    let source = SOURCES[group].get_or_init(|| {
        let blend = if group == 1 {
            include_str!("../../../shaders/generated_blend.wgsl")
                .replace("bae_unpremultiply", "fx_unpremult")
        } else {
            String::new()
        };
        [COMMON, &blend, body, include_str!("dispatch.wgsl")]
            .concat()
            .replace(
                "GRID_FIELD",
                if group == 0 {
                    "grid: array<vec4f, 512>,"
                } else {
                    ""
                },
            )
    });
    let bytes = HEADER + if group == 0 { GRID_BYTES } else { 0 };
    image_grid::kernel_with_inputs(rt, format, label, source, bytes as u32, 2)
}
pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for group in 0..GROUPS.len() {
        pipeline(rt, format, group)?;
    }
    Ok(())
}
