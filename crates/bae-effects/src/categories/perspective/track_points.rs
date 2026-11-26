use crate::{
    EffectTarget, TargetEffect,
    effects::image_grid,
    gpu::{self, Resource, UniformWriter},
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use std::sync::Arc;
const CAPACITY: usize = 384;
const BYTES: usize = 48 + CAPACITY * 16;

pub(super) struct TrackPoints(pub EffectDef);
fn kernel(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<Arc<gpu::Kernel>, RenderError> {
    image_grid::kernel_with_inputs(
        rt,
        format,
        "bae.effects/camera_track_points",
        include_str!("track_points.wgsl"),
        BYTES as u32,
        1,
    )
}
pub(super) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    kernel(rt, format).map(|_| ())
}
impl Effect for TrackPoints {
    fn def(&self) -> &EffectDef {
        &self.0
    }
    fn render(
        &self,
        t: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(t, p, input, roi)
    }
}
impl TargetEffect for TrackPoints {
    fn encode(
        &self,
        t: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if p.value(406) != Some(&Value::Bool(true)) || roi.is_empty() {
            return Ok(input.clone());
        }
        let Some(Value::FloatList(rows)) = p.value(9001) else {
            return Ok(input.clone());
        };
        let grid = image_grid::grid_scale(p, input);
        let radius = p.value(407).and_then(Value::as_f64).unwrap_or(100.) * 0.035 / grid;
        if rows.is_empty() || radius <= 0. {
            return Ok(input.clone());
        }
        let source = p.value(405).and_then(Value::as_f64) == Some(1.);
        let mut u = UniformWriter::with_size(BYTES);
        image_grid::image_coords(&mut u, 0, input, grid);
        let rect = image_grid::pixel_rect(roi, 1. / grid);
        u.i32s(32, &[rect.x0, rect.y0]);
        let mut n = 0;
        for &[_, x, y, sx, sy, error] in rows.as_chunks::<6>().0 {
            if !source && error < 0. {
                continue;
            }
            if n >= CAPACITY {
                break;
            }
            let (x, y) = if source { (x, y) } else { (sx, sy) };
            let e = (error / 4.).clamp(0., 1.) as f32;
            u.f32s(
                48 + n * 16,
                &[
                    (x / grid) as f32,
                    (y / grid) as f32,
                    e,
                    radius as f32 * (1. + 0.5 * e),
                ],
            );
            n += 1;
        }
        if n == 0 {
            return Ok(input.clone());
        }
        u.u32(40, n as u32);
        let rt = gpu::runtime(t.gpu());
        let pipeline = kernel(&rt, t.format())?;
        let out = t.allocate(roi, grid)?;
        let group = rt.bind_group(
            &pipeline,
            &[
                Resource::Uniform,
                Resource::View(&input.view),
                Resource::View(&out.view),
            ],
        );
        let offset = gpu::push_uniform(&rt, &u);
        {
            let mut pass = t.encoder().begin_compute_pass(&Default::default());
            gpu::dispatch(&mut pass, &pipeline, &group, &[offset], out.texel_size());
        }
        Ok(out)
    }
}
