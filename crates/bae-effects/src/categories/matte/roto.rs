use std::sync::Arc;

use bae_gpu::wgpu;
use bae_model::{EffectDef, Time};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};
use bae_vision::{SPAN, STROKES, segments, span};

use crate::{
    Builtin, EffectTarget, TargetEffect,
    effects::{
        image_grid,
        temporal::{self, flow},
    },
    gpu::{self, Resource, UniformWriter},
    params::Params,
    rust_builtin,
};

const LABEL: &str = "bae.vision.roto";
const BYTES: u32 = 144 + 32 * 32;

pub(super) fn builtin() -> Builtin {
    let (def, names) = super::definition::definition().build();
    rust_builtin(
        (Arc::new(Roto(def)), names),
        "roto-brush",
        "Roto 笔刷和调整边缘",
        &["rotoscope", "前景", "分割"],
    )
}

fn kernel(
    rt: &gpu::Runtime,
    format: wgpu::TextureFormat,
    entry: &'static str,
) -> Result<Arc<gpu::Kernel>, RenderError> {
    image_grid::kernel_at(
        rt,
        format,
        LABEL,
        bae_vision::SEGMENT_SHADER,
        entry,
        BYTES,
        4,
    )
}
pub(super) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    kernel(rt, format, "accumulate")?;
    kernel(rt, format, "resolve")?;
    Ok(())
}

struct Roto(EffectDef);

fn active(p: &EffectParams<'_>) -> bool {
    span(p.value(SPAN)).is_none_or(|[start, end]| p.time >= start && p.time <= end)
}
fn source(p: &EffectParams<'_>, stamp: f64) -> Option<LayerDependency> {
    let delta = Time(stamp as i64) - p.time;
    let t = p.comp_time + Time::from_seconds(delta.as_seconds() * p.layer_time_scale);
    (t != p.comp_time)
        .then(|| temporal::before_effect(p, t))
        .flatten()
}

fn frozen_input(p: &EffectParams<'_>) -> Option<LayerDependency> {
    let mut dep = crate::effects::layer_input::dependency(p, bae_vision::FROZEN, false)?;
    if let Some(bae_model::Value::Vec3([start, source, stretch])) =
        p.value(bae_vision::FROZEN_TIMING)
    {
        dep.time = Time(*start as i64)
            + Time(((p.time.0 as f64 - source) * stretch / 100.).round() as i64);
    }
    Some(dep)
}

impl Effect for Roto {
    fn def(&self) -> &EffectDef {
        &self.0
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        roi.union(&image_grid::layer_rect(p))
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        if !active(p) {
            return Vec::new();
        }
        if let Some(dep) = frozen_input(p) {
            return vec![dep];
        }
        let mut times: Vec<_> = segments(p.value(STROKES))
            .iter()
            .map(|s| s[0] as i64)
            .collect();
        times.sort_unstable();
        times.dedup();
        times
            .into_iter()
            .filter_map(|t| source(p, t as f64))
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

fn dispatch(
    target: &mut dyn EffectTarget,
    entry: &'static str,
    u: &UniformWriter,
    images: [&GpuImage; 4],
    roi: IRect,
    grid: f64,
) -> Result<GpuImage, RenderError> {
    let rt = gpu::runtime(target.gpu());
    let k = kernel(&rt, target.format(), entry)?;
    let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
    let [a, b, c, d] = images;
    let group = rt.bind_group(
        &k,
        &[
            Resource::Uniform,
            Resource::View(&a.view),
            Resource::View(&b.view),
            Resource::View(&c.view),
            Resource::View(&d.view),
            Resource::View(&out.view),
        ],
    );
    let offset = gpu::push_uniform(&rt, u);
    let mut pass = target
        .encoder()
        .begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(LABEL),
            timestamp_writes: None,
        });
    gpu::dispatch(&mut pass, &k, &group, &[offset], roi.size());
    Ok(out)
}

impl TargetEffect for Roto {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let strokes = segments(p.value(STROKES));
        if strokes.is_empty() || !active(p) {
            return Ok(input.clone());
        }
        let v = Params::new(&self.0, p);
        let grid = image_grid::grid_scale(p, input);
        let mut u = UniformWriter::with_size(BYTES as usize);
        image_grid::image_coords(&mut u, 0, input, grid);
        u.i32s(96, &image_grid::rect4(roi))
            .f32(128, (v.f(4) / grid) as f32)
            .f32(132, v.f(9) as f32);
        let frozen = frozen_input(p)
            .map(|dep| {
                target
                    .effect_layer(&dep)
                    .cloned()
                    .ok_or_else(|| RenderError::Other("Frozen matte footage unavailable".into()))
            })
            .transpose()?;
        let mut scores = frozen.clone();
        let mut ordinal = 0usize;
        let live_strokes = if frozen.is_some() { &[][..] } else { strokes };
        for group in live_strokes.chunk_by(|a, b| a[0] == b[0]) {
            let dep = source(p, group[0][0]);
            let base = match &dep {
                Some(d) => target.effect_layer(d).cloned().ok_or_else(|| {
                    RenderError::Other("Roto Brush source frame is unavailable".into())
                })?,
                None => input.clone(),
            };
            let field = if let Some(dep) = dep {
                let pair = flow::Pair {
                    images: [input, &base],
                    times: [p.comp_time, dep.time],
                    layer: &dep.layer,
                    roi,
                    grid,
                    detail: if v.popup(2) == 2 { 50. } else { 25. },
                    analysis: Default::default(),
                };
                Some(pair.forward(target)?.grid_image(grid))
            } else {
                None
            };
            let motion = field.as_ref().unwrap_or(input);
            image_grid::image_coords(&mut u, 32, &base, grid);
            image_grid::image_coords(&mut u, 64, motion, grid);
            for chunk in group.chunks(32) {
                u.u32(112, chunk.len() as u32)
                    .u32(116, u32::from(field.is_some()))
                    .u32(120, u32::from(scores.is_some()));
                for (i, s) in chunk.iter().enumerate() {
                    let at = 144 + 32 * i;
                    u.f32s(at, &[s[2], s[3], s[4], s[5]].map(|v| (v / grid) as f32))
                        .f32(at + 16, (s[1] / grid) as f32)
                        .f32(at + 20, (ordinal + 1) as f32 / strokes.len() as f32);
                    ordinal += 1;
                }
                scores = Some(dispatch(
                    target,
                    "accumulate",
                    &u,
                    [input, &base, motion, scores.as_ref().unwrap_or(input)],
                    roi,
                    grid,
                )?);
            }
        }
        let width = if v.b(13) {
            (0.02 + v.f(15) * 0.003) * (1.5 - v.f(16) * 0.01)
        } else {
            0.02
        };
        if let Some(img) = &frozen {
            let mut aligned = img.clone();
            aligned.rect = IRect::new(
                p.layer_bounds.x0,
                p.layer_bounds.y0,
                p.layer_bounds.x0 + img.rect.width() as i32,
                p.layer_bounds.y0 + img.rect.height() as i32,
            );
            image_grid::image_coords(&mut u, 32, &aligned, grid);
        }
        u.u32(116, u32::from(frozen.is_some())).u32(
            120,
            u32::from(
                p.value(bae_vision::BAKE)
                    .and_then(bae_model::Value::as_bool)
                    == Some(true),
            ),
        );
        u.f32(136, width as f32)
            .f32(140, if v.b(13) { v.f(17) as f32 * 0.003 } else { 0. })
            .u32(124, u32::from(v.b(12)));
        dispatch(
            target,
            "resolve",
            &u,
            [input, input, input, scores.as_ref().unwrap_or(input)],
            roi,
            grid,
        )
    }
}
