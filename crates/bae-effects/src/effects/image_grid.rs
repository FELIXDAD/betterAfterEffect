//! Coordinate and pipeline helpers for layer-space GPU effects.

use std::sync::Arc;

use bae_gpu::wgpu;
use bae_render::{EffectParams, GpuImage, IRect, ImageAlpha, RenderError};

use crate::gpu::{self, Binding, Kernel, KernelKey, KernelSource, Runtime, UniformWriter};

pub(super) const CHANNEL_MATH: &str = include_str!("../shaders/channel_math.wgsl");

pub(super) fn rect4(r: IRect) -> [i32; 4] {
    [r.x0, r.y0, r.x1, r.y1]
}

pub(super) fn grid_scale(params: &EffectParams<'_>, input: &GpuImage) -> f64 {
    if params.layer_bounds.is_empty() {
        input.pixel_scale
    } else {
        1.0 / params.scale
    }
}

pub(super) fn layer_rect(params: &EffectParams<'_>) -> IRect {
    let [x0, y0, x1, y1] = rect4(params.layer_bounds).map(|p| f64::from(p) * params.scale);
    IRect::new(
        x0.floor() as i32,
        y0.floor() as i32,
        x1.ceil() as i32,
        y1.ceil() as i32,
    )
}

pub(super) fn pixel_rect(rect: IRect, scale: f64) -> IRect {
    let [x0, y0, x1, y1] = rect4(rect).map(|p| (f64::from(p) * scale).round() as i32);
    IRect::new(x0, y0, x1, y1)
}

// The shaders use render-grid coordinates. A source can be at a different
// density and have a different origin (e.g. a footage texture at native size).
pub(super) fn image_coords(w: &mut UniformWriter, at: usize, image: &GpuImage, grid: f64) {
    let [width, height] = image.texel_size();
    w.f32(at, (f64::from(image.rect.x0) / grid) as f32)
        .f32(at + 4, (f64::from(image.rect.y0) / grid) as f32)
        .f32(at + 8, (grid / image.pixel_scale) as f32)
        .u32(
            at + 12,
            match image.alpha {
                ImageAlpha::Premultiplied => 0,
                ImageAlpha::Straight => 1,
                ImageAlpha::Opaque => 2,
            },
        )
        .u32(at + 16, width)
        .u32(at + 20, height);
}

pub(super) fn kernel(
    rt: &Runtime,
    format: wgpu::TextureFormat,
    label: &'static str,
    source: &str,
    uniform_size: u32,
) -> Result<Arc<Kernel>, RenderError> {
    kernel_with_inputs(rt, format, label, source, uniform_size, 2)
}

pub(super) fn kernel_with_inputs(
    rt: &Runtime,
    format: wgpu::TextureFormat,
    label: &'static str,
    source: &str,
    uniform_size: u32,
    input_count: u32,
) -> Result<Arc<Kernel>, RenderError> {
    let key = KernelKey {
        owner: Arc::from(label),
        entry: Arc::from("main"),
        variant: gpu::format_key(format),
    };
    rt.kernel(&key, || {
        let format_name = crate::blur::format_name(format)?;
        Ok(KernelSource {
            label,
            wgsl: [
                CHANNEL_MATH,
                include_str!("../shaders/image_grid.wgsl"),
                source,
            ]
            .concat()
            .replace("WORKING_FORMAT", format_name),
            entry: "main",
            bindings: std::iter::once((0, Binding::Uniform(uniform_size)))
                .chain((1..=input_count).map(|slot| (slot, Binding::Texture)))
                .chain(std::iter::once((input_count + 1, Binding::Storage(format))))
                .collect(),
            workgroup: [8, 8, 1],
        })
    })
}
