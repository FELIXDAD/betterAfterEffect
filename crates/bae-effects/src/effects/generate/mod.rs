//! Generators with image dependencies, temporal parameters or multiple GPU passes.
mod bucket;
mod definitions;
mod eyedropper;
mod rays;
mod waves;

use super::{image_grid, layer_input};
use crate::{
    EffectTarget, TargetEffect,
    gpu::{self, Resource, UniformWriter},
    params::Params,
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use bucket::PaintBucket;
pub(crate) use definitions::{
    cc_light_burst_2_5, cc_light_rays, eyedropper_fill, paint_bucket, radio_waves,
};
use rays::LightRays;
use waves::RadioWaves;

fn point(v: Params<'_>, id: u32) -> [f32; 2] {
    let Some(Value::Vec2(xy)) = v.value(id) else {
        return [0.; 2];
    };
    xy.map(|x| (x * v.p.scale) as f32)
}
fn color(v: Params<'_>, id: u32) -> [f32; 4] {
    match v.value(id) {
        Some(Value::Color(c)) => c.map(|x| x as f32),
        _ => [1.; 4],
    }
}

struct Pass {
    label: &'static str,
    shader: &'static str,
    bytes: u32,
}
impl Pass {
    fn kernel(
        &self,
        rt: &gpu::Runtime,
        format: wgpu::TextureFormat,
    ) -> Result<std::sync::Arc<gpu::Kernel>, RenderError> {
        image_grid::kernel(rt, format, self.label, self.shader, self.bytes)
    }
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        images: [&GpuImage; 2],
        roi: IRect,
        mut u: UniformWriter,
    ) -> Result<GpuImage, RenderError> {
        let grid = image_grid::grid_scale(p, images[0]);
        for (i, image) in images.iter().enumerate() {
            image_grid::image_coords(&mut u, i * 32, image, grid);
        }
        u.i32s(64, &image_grid::rect4(roi));
        let rt = gpu::runtime(target.gpu());
        let kernel = self.kernel(&rt, target.format())?;
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let group = rt.bind_group(
            &kernel,
            &[
                Resource::Uniform,
                Resource::View(&images[0].view),
                Resource::View(&images[1].view),
                Resource::View(&out.view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(self.label),
                timestamp_writes: None,
            });
        gpu::dispatch(
            &mut pass,
            &kernel,
            &group,
            &[gpu::push_uniform(&rt, &u)],
            roi.size(),
        );
        Ok(out)
    }
}
pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    rays::PASS.kernel(rt, format)?;
    waves::PASS.kernel(rt, format)?;
    eyedropper::SAMPLE.kernel(rt, format)?;
    eyedropper::FILL.kernel(rt, format)?;
    bucket::prewarm(rt, format)
}
