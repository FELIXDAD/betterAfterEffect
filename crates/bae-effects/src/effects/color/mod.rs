//! Per-channel curves and spatial tone correction.
mod curves;
mod shadow;

use crate::gpu::{self, Binding, KernelKey, KernelSource, Resource, UniformWriter};
use crate::target::EffectTarget;
use bae_gpu::wgpu;
use bae_render::{GpuImage, RenderError};

pub(crate) use curves::Curves;
pub(crate) use shadow::ShadowHighlight;

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for entry in [
        "apply_curves",
        "histogram",
        "percentiles",
        "shadow_highlight",
    ] {
        kernel(rt, format, entry)?;
    }
    Ok(())
}

fn kernel(
    rt: &gpu::Runtime,
    format: wgpu::TextureFormat,
    entry: &'static str,
) -> Result<std::sync::Arc<gpu::Kernel>, RenderError> {
    let key = KernelKey {
        owner: "bae.effects/color".into(),
        entry: entry.into(),
        variant: gpu::format_key(format),
    };
    rt.kernel(&key, || {
        let (source, bytes, inputs) = match entry {
            "apply_curves" => (include_str!("curves.wgsl"), curves::UNIFORM_BYTES, 1),
            "histogram" | "percentiles" => (include_str!("shadow.wgsl"), shadow::UNIFORM_BYTES, 1),
            "shadow_highlight" => (include_str!("shadow.wgsl"), shadow::UNIFORM_BYTES, 4),
            _ => unreachable!(),
        };
        let mut bindings = vec![
            (0, Binding::Uniform(bytes as u32)),
            (1, Binding::Storage(format)),
        ];
        bindings.extend((0..inputs).map(|i| (i + 2, Binding::Texture)));
        Ok(KernelSource {
            label: entry,
            wgsl: source.replace("WORKING_FORMAT", crate::blur::format_name(format)?),
            entry,
            bindings,
            workgroup: [8, 8, 1],
        })
    })
}

fn dispatch(
    target: &mut dyn EffectTarget,
    entry: &'static str,
    u: &UniformWriter,
    inputs: &[&GpuImage],
    out: &GpuImage,
    size: [u32; 2],
) -> Result<(), RenderError> {
    let rt = gpu::runtime(target.gpu());
    let kernel = kernel(&rt, target.format(), entry)?;
    let mut bindings = vec![Resource::Uniform, Resource::View(&out.view)];
    bindings.extend(inputs.iter().map(|image| Resource::View(&image.view)));
    let group = rt.bind_group(&kernel, &bindings);
    let offset = gpu::push_uniform(&rt, u);
    let mut pass = target
        .encoder()
        .begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(entry),
            timestamp_writes: None,
        });
    gpu::dispatch(&mut pass, &kernel, &group, &[offset], size);
    Ok(())
}
