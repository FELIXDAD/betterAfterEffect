//! Helpers shared by the Rust effects.

use std::sync::Arc;

use bae_gpu::wgpu;
use bae_render::{GpuImage, ImageAlpha, RenderError};

use crate::gpu::{self, Binding, KernelKey, KernelSource, Resource, UniformWriter};
use crate::target::EffectTarget;

const SOURCE: &str = include_str!("shaders/common.wgsl");

/// `input` as a premultiplied image: returned as is when it already is one,
/// converted on the GPU when it is straight or opaque.
pub(crate) fn premultiplied(
    target: &mut dyn EffectTarget,
    input: &GpuImage,
) -> Result<GpuImage, RenderError> {
    let mode = match input.alpha {
        ImageAlpha::Premultiplied => return Ok(input.clone()),
        ImageAlpha::Straight => 0,
        ImageAlpha::Opaque => 1,
    };
    let rt = gpu::runtime(target.gpu());
    let format = target.format();
    let fmt = crate::blur::format_name(format)?;
    let key = KernelKey {
        owner: Arc::from("bae.effects/common"),
        entry: Arc::from("to_premultiplied"),
        variant: crate::gpu::format_key(format),
    };
    let kernel = rt.kernel(&key, || {
        Ok(KernelSource {
            label: "bae.effects.common",
            wgsl: SOURCE.replace("WORKING_FORMAT", fmt),
            entry: "to_premultiplied",
            bindings: vec![
                (0, Binding::Uniform(32)),
                (1, Binding::Texture),
                (2, Binding::Storage(format)),
            ],
            workgroup: [8, 8, 1],
        })
    })?;
    let out = target.allocate(input.rect, input.pixel_scale)?;
    let mut w = UniformWriter::with_size(32);
    let r = input.rect;
    w.i32s(0, &[r.x0, r.y0, r.x1, r.y1]).u32(16, mode);
    let offset = gpu::push_uniform(&rt, &w);
    let group = rt.bind_group(
        &kernel,
        &[
            Resource::Uniform,
            Resource::View(&input.view),
            Resource::View(&out.view),
        ],
    );
    let mut pass = target
        .encoder()
        .begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("bae.effects.premultiply"),
            timestamp_writes: None,
        });
    gpu::dispatch(&mut pass, &kernel, &group, &[offset], r.size());
    drop(pass);
    Ok(out)
}
