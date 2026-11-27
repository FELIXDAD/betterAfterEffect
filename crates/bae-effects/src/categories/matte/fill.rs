//! Full-frame reconstruction for the Content-Aware Fill sequence worker.
use crate::{
    EffectTarget,
    effects::image_grid,
    gpu::{self, Resource, UniformWriter},
};
use bae_gpu::wgpu;
use bae_render::{GpuImage, RenderError};
use bae_vision::{FillMethod, FillSettings};
use std::sync::Arc;

const LABEL: &str = "bae.vision.fill";
fn kernel(
    rt: &gpu::Runtime,
    format: wgpu::TextureFormat,
    entry: &'static str,
) -> Result<Arc<gpu::Kernel>, RenderError> {
    image_grid::kernel_at(rt, format, LABEL, bae_vision::FILL_SHADER, entry, 32, 4)
}
pub(super) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for entry in ["horizontal", "seed", "propagate", "fill"] {
        kernel(rt, format, entry)?;
    }
    Ok(())
}

/// Return a premultiplied patch, transparent outside the fill region.
/// All inputs cover the same layer-space rectangle at full resolution.
pub fn content_aware_fill(
    target: &mut dyn EffectTarget,
    src: &GpuImage,
    previous: &GpuImage,
    next: &GpuImage,
    settings: FillSettings,
) -> Result<GpuImage, RenderError> {
    let rt = gpu::runtime(target.gpu());
    let [w, h] = src.texel_size();
    let a = target.allocate(src.rect, 1.)?;
    let b = target.allocate(src.rect, 1.)?;
    let mut u = UniformWriter::with_size(32);
    u.i32s(0, &[w as i32, h as i32])
        .u32(
            12,
            match settings.method {
                FillMethod::Object => 0,
                FillMethod::Surface => 1,
                FillMethod::EdgeBlend => 2,
            },
        )
        .i32(16, settings.expansion.min(64) as i32)
        .f32(20, settings.lighting);
    let mut pass = |entry: &'static str,
                    uniform: &UniformWriter,
                    field: &GpuImage,
                    dst: &GpuImage|
     -> Result<(), RenderError> {
        let k = kernel(&rt, target.format(), entry)?;
        let group = rt.bind_group(
            &k,
            &[
                Resource::Uniform,
                Resource::View(&src.view),
                Resource::View(&field.view),
                Resource::View(&previous.view),
                Resource::View(&next.view),
                Resource::View(&dst.view),
            ],
        );
        let offset = gpu::push_uniform(&rt, uniform);
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(LABEL),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &k, &group, &[offset], [w, h]);
        Ok(())
    };
    if settings.expansion > 0 {
        pass("horizontal", &u, src, &b)?;
    }
    pass(
        "seed",
        &u,
        if settings.expansion > 0 { &b } else { src },
        &a,
    )?;
    let (mut field, mut out) = (a, b);
    let mut jump = w.max(h).next_power_of_two() / 2;
    while jump > 0 {
        u.i32(8, jump as i32);
        pass("propagate", &u, &field, &out)?;
        std::mem::swap(&mut field, &mut out);
        jump /= 2;
    }
    pass("fill", &u, &field, &out)?;
    Ok(out)
}
