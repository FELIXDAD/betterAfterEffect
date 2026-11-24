//! Texture-to-texture compute passes. Both sampled images keep their render-space origins.
use super::*;
use crate::gpu::{
    self, Binding, Kernel, KernelKey, KernelSource, Resource, Runtime, UniformWriter,
};
use bae_gpu::wgpu;
use std::sync::Arc;

const UNIFORM_SIZE: u32 = 160;
const SOURCE: &str = include_str!("spatial.wgsl");
const ENTRIES: &[&str] = &[
    "matte_choke",
    "directional",
    "radial",
    "mosaic_rows",
    "mosaic_cells",
    "mosaic_output",
    "unsharp",
    "shadow",
    "glow_extract",
    "glow_composite",
    "emboss",
    "cartoon_smooth",
    "cartoon_output",
    "rough_output",
    "tile_collapse",
    "motion_tile",
    "radial_shadow_project",
    "radial_shadow_over",
];

fn pipeline(
    rt: &Runtime,
    format: wgpu::TextureFormat,
    entry: &'static str,
) -> Result<Arc<Kernel>, RenderError> {
    let key = KernelKey {
        owner: "bae.effects/spatial".into(),
        entry: entry.into(),
        variant: gpu::format_key(format),
    };
    rt.kernel(&key, || {
        Ok(KernelSource {
            label: "bae.effects.spatial",
            wgsl: format!("{SOURCE}\n{}", include_str!("stylize/stylize.wgsl"))
                .replace("WORKING_FORMAT", blur::format_name(format)?),
            entry,
            bindings: vec![
                (0, Binding::Uniform(UNIFORM_SIZE)),
                (1, Binding::Texture),
                (2, Binding::Texture),
                (3, Binding::Storage(format)),
            ],
            workgroup: [8, 8, 1],
        })
    })
}

pub(crate) fn prewarm(rt: &Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for &entry in ENTRIES {
        pipeline(rt, format, entry)?;
    }
    Ok(())
}

pub(super) fn dispatch(
    target: &mut dyn EffectTarget,
    entry: &'static str,
    input: &GpuImage,
    aux: &GpuImage,
    out: IRect,
    coefficients: &[[f32; 4]],
    float: bool,
) -> Result<GpuImage, RenderError> {
    let image = target.allocate(out, input.pixel_scale)?;
    dispatch_to(target, entry, input, aux, &image, coefficients, float)?;
    Ok(image)
}

pub(super) fn dispatch_to(
    target: &mut dyn EffectTarget,
    entry: &'static str,
    input: &GpuImage,
    aux: &GpuImage,
    image: &GpuImage,
    coefficients: &[[f32; 4]],
    float: bool,
) -> Result<(), RenderError> {
    let out = image.rect;
    let rt = gpu::runtime(target.gpu());
    let kernel = pipeline(&rt, target.format(), entry)?;
    let mut w = UniformWriter::with_size(UNIFORM_SIZE as usize);
    for (index, r) in [input.rect, aux.rect, out].iter().enumerate() {
        w.i32s(index * 16, &[r.x0, r.y0, r.x1, r.y1]);
    }
    w.u32(48, u32::from(float));
    for (i, row) in coefficients.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            w.f32(64 + i * 16 + j * 4, *v);
        }
    }
    let offset = gpu::push_uniform(&rt, &w);
    let group = rt.bind_group(
        &kernel,
        &[
            Resource::Uniform,
            Resource::View(&input.view),
            Resource::View(&aux.view),
            Resource::View(&image.view),
        ],
    );
    let mut pass = target
        .encoder()
        .begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(entry),
            timestamp_writes: None,
        });
    gpu::dispatch(&mut pass, &kernel, &group, &[offset], out.size());
    drop(pass);
    Ok(())
}
