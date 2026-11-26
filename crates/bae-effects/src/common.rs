//! Helpers shared by the Rust effects.

use std::sync::Arc;

use bae_gpu::wgpu;
use bae_render::{GpuImage, IRect, ImageAlpha, RenderError};

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

/// `image` under the shared Gaussian (repeat edges) of `radius` space pixels,
/// run in texture coordinates and placed back on the layer.
pub(crate) fn blurred(
    target: &mut dyn EffectTarget,
    image: &GpuImage,
    radius: f64,
) -> Result<GpuImage, RenderError> {
    let sigma = radius * 0.5 / image.pixel_scale;
    if crate::blur::radius(sigma) == 0 {
        return Ok(image.clone());
    }
    let mut local = premultiplied(target, image)?;
    let [w, h] = local.texel_size();
    local.rect = IRect::from_size(w, h);
    local.pixel_scale = 1.;
    let mut result = crate::blur::plan_gaussian(
        local.rect,
        [sigma; 2],
        crate::blur::Edge::Repeat,
        local.rect,
    )
    .encode(target, &local)?;
    result.rect = image.rect;
    result.pixel_scale = image.pixel_scale;
    Ok(result)
}
