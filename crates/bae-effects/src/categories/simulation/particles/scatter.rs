use crate::effects::image_grid;
use crate::gpu::{self, Resource, UniformWriter};
use crate::{EffectTarget, params::Params};
use bae_gpu::wgpu;
use bae_render::{GpuImage, IRect, RenderError};
use std::sync::Arc;

fn kernel(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<Arc<gpu::Kernel>, RenderError> {
    image_grid::kernel_with_inputs(
        rt,
        format,
        "bae.effects.scatterize",
        include_str!("scatter.wgsl"),
        80,
        1,
    )
}

pub(super) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    kernel(rt, format)?;
    Ok(())
}

pub(super) fn render(
    target: &mut dyn EffectTarget,
    p: &Params<'_>,
    input: &GpuImage,
    roi: IRect,
) -> Result<GpuImage, RenderError> {
    if p.f(1) == 0. && p.f(2) == 0. && p.f(3) == 0. {
        return Ok(input.clone());
    }
    let grid = image_grid::grid_scale(p.p, input);
    let layer = if p.p.layer_bounds.is_empty() {
        image_grid::pixel_rect(input.rect, 1. / grid)
    } else {
        image_grid::layer_rect(p.p)
    };
    let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
    let rt = gpu::runtime(target.gpu());
    let pipeline = kernel(&rt, target.format())?;
    let mut u = UniformWriter::with_size(80);
    image_grid::image_coords(&mut u, 0, input, grid);
    u.i32s(32, &image_grid::rect4(roi))
        .i32s(48, &image_grid::rect4(layer));
    u.f32s(
        64,
        &[
            (p.f(1) / grid) as f32,
            p.f(2).to_radians() as f32,
            p.f(3).to_radians() as f32,
        ],
    )
    .u32(76, p.popup(4) as u32);
    let group = rt.bind_group(
        &pipeline,
        &[
            Resource::Uniform,
            Resource::View(&input.view),
            Resource::View(&out.view),
        ],
    );
    let offset = gpu::push_uniform(&rt, &u);
    let mut pass = target.encoder().begin_compute_pass(&Default::default());
    gpu::dispatch(&mut pass, &pipeline, &group, &[offset], roi.size());
    drop(pass);
    Ok(out)
}
