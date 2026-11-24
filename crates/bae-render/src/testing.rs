//! Headless entry points for testing layer sources, effects and plugin hosts
//! outside this crate (05 vector/text, 07 effects, 19 plugins), on the shared
//! GPU device without a renderer or a window.
//!
//! ```no_run
//! # use std::sync::Arc;
//! # fn demo(gpu: Arc<bae_gpu::GpuContext>, source: &dyn bae_render::LayerSource) -> Result<(), bae_render::RenderError> {
//! use bae_render::{IRect, SourceRequest, testing};
//! let image = testing::render_source(&gpu, source, &SourceRequest {
//!     roi: IRect::new(-50, -50, 50, 50),
//!     scale: 2.0,
//!     time: bae_model::Time::ZERO,
//! })?;
//! let pixels = testing::read_pixels(&gpu, &image)?; // premultiplied f32 RGBA, texel order
//! # Ok(()) }
//! ```

use std::sync::Arc;

use bae_gpu::{GpuContext, wgpu};

use crate::effect::{Effect, EffectParams};
use crate::error::RenderError;
use crate::geom::IRect;
use crate::pool::TexturePool;
use crate::source::{EncodeContext, GpuImage, ImageAlpha, LayerSource, SourceRequest};

/// The working format of tests (the renderer's default).
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

struct Allocator {
    pool: TexturePool,
    gpu: Arc<GpuContext>,
}

impl crate::source::ImageAllocator for Allocator {
    fn allocate(&mut self, rect: IRect, pixel_scale: f64) -> Result<GpuImage, RenderError> {
        self.pool
            .acquire(self.gpu.device(), FORMAT, rect, pixel_scale)
    }
}

/// Runs `f` with an [`EncodeContext`] over a fresh texture pool and command
/// encoder, then submits the encoded work. Use it to call
/// [`LayerSource::render`] / [`Effect::render`] (or your own encoding code)
/// exactly as the renderer does.
pub fn with_encode_context<R>(
    gpu: &Arc<GpuContext>,
    f: impl FnOnce(&mut EncodeContext<'_>) -> Result<R, RenderError>,
) -> Result<R, RenderError> {
    let mut alloc = Allocator {
        pool: TexturePool::new(0),
        gpu: gpu.clone(),
    };
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bae-render.testing"),
        });
    let result = {
        let mut ctx = EncodeContext {
            gpu,
            encoder: &mut encoder,
            format: FORMAT,
            allocator: &mut alloc,
            effect_inputs: &[],
            effect_masks: &[],
        };
        f(&mut ctx)
    };
    gpu.queue().submit([encoder.finish()]);
    result
}

/// Renders `source` for `request`.
pub fn render_source(
    gpu: &Arc<GpuContext>,
    source: &dyn LayerSource,
    request: &SourceRequest,
) -> Result<GpuImage, RenderError> {
    with_encode_context(gpu, |ctx| source.render(ctx, request))
}

/// Applies `effect` to `input`, producing at least `output_roi` (pixels of the
/// input's space). Build `params` with [`EffectParams::new`] and set the fields
/// the effect reads.
pub fn render_effect(
    gpu: &Arc<GpuContext>,
    effect: &dyn Effect,
    params: &EffectParams<'_>,
    input: &GpuImage,
    output_roi: IRect,
) -> Result<GpuImage, RenderError> {
    with_encode_context(gpu, |ctx| effect.render(ctx, params, input, output_roi))
}

/// Uploads premultiplied RGBA (`rect` texels at `pixel_scale`, row-major,
/// `texel_size` long) as a working-format image, e.g. an effect input.
pub fn upload(
    gpu: &Arc<GpuContext>,
    rect: IRect,
    pixel_scale: f64,
    pixels: &[[f32; 4]],
) -> Result<GpuImage, RenderError> {
    let [w, h] = crate::source::texels_for(rect, pixel_scale);
    if pixels.len() != (w * h) as usize {
        return Err(RenderError::Other(format!(
            "upload: {} pixels for {w}×{h} texels",
            pixels.len()
        )));
    }
    let data: Vec<u8> = pixels
        .iter()
        .flat_map(|p| p.map(|v| half::f16::from_f32(v).to_bits()))
        .flat_map(u16::to_le_bytes)
        .collect();
    let texture = bae_gpu::transfer::create_texture_with_data(
        gpu,
        &wgpu::TextureDescriptor {
            label: Some("bae-render.testing.upload"),
            size: wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: crate::pool::POOL_USAGE,
            view_formats: &[],
        },
        &data,
    )?;
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Ok(GpuImage::new(
        texture,
        view,
        rect,
        ImageAlpha::Premultiplied,
        pixel_scale,
    ))
}

/// Reads an image back as RGBA `f32` in texel order (`texel_size` texels, row
/// major), exactly as stored: premultiplied for working images, straight for
/// [`ImageAlpha::Straight`] sources. Supports RGBA16F, RGBA32F and RGBA8 (unorm).
pub fn read_pixels(gpu: &Arc<GpuContext>, image: &GpuImage) -> Result<Vec<[f32; 4]>, RenderError> {
    let data = bae_gpu::transfer::read_texture(gpu, &image.texture)?;
    let [w, h] = image.texel_size();
    let bpp = (data.bytes_per_row / data.width.max(1)) as usize;
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h.min(data.height) {
        let row = data.row(y);
        for x in 0..w.min(data.width) as usize {
            let p = &row[x * bpp..(x + 1) * bpp];
            out.push(match data.format {
                wgpu::TextureFormat::Rgba16Float => std::array::from_fn(|c| {
                    half::f16::from_bits(u16::from_le_bytes([p[2 * c], p[2 * c + 1]])).to_f32()
                }),
                wgpu::TextureFormat::Rgba32Float => std::array::from_fn(|c| {
                    f32::from_le_bytes([p[4 * c], p[4 * c + 1], p[4 * c + 2], p[4 * c + 3]])
                }),
                wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {
                    std::array::from_fn(|c| f32::from(p[c]) / 255.0)
                }
                other => {
                    return Err(RenderError::Other(format!(
                        "read_pixels: unsupported {other:?}"
                    )));
                }
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use bae_model::Time;

    use super::*;

    /// A source that fills its request with a colour (allocating like a real one).
    struct Flat;

    impl LayerSource for Flat {
        fn size(&self) -> [u32; 2] {
            [8, 8]
        }

        fn bounds(&self) -> IRect {
            IRect::new(-4, -4, 4, 4)
        }

        fn content_hash(&self, _: Time) -> u64 {
            0
        }

        fn render(
            &self,
            ctx: &mut EncodeContext<'_>,
            req: &SourceRequest,
        ) -> Result<GpuImage, RenderError> {
            let image = ctx.allocate(req.roi, 1.0 / req.scale)?;
            ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &image.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.5,
                            g: 0.25,
                            b: 0.0,
                            a: 0.5,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            Ok(image)
        }
    }

    #[test]
    fn renders_sources_and_round_trips_pixels() {
        let Some(gpu) = bae_gpu::testing::shared_context() else {
            return;
        };
        let request = SourceRequest {
            roi: IRect::new(-4, -4, 4, 4),
            scale: 2.0,
            time: Time::ZERO,
        };
        let image = render_source(&gpu, &Flat, &request).unwrap();
        assert_eq!(image.rect, IRect::new(-4, -4, 4, 4));
        assert_eq!(image.texel_size(), [16, 16]);
        let px = read_pixels(&gpu, &image).unwrap();
        assert_eq!(px.len(), 256);
        assert_eq!(px[0], [0.5, 0.25, 0.0, 0.5]);

        let input: Vec<[f32; 4]> = (0..12).map(|i| [i as f32 / 16.0, 0.0, 0.0, 1.0]).collect();
        let up = upload(&gpu, IRect::new(10, 20, 14, 23), 1.0, &input).unwrap();
        assert_eq!(read_pixels(&gpu, &up).unwrap(), input);
    }
}
