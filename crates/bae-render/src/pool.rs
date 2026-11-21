//! Texture pool: reuses working textures within and across frames.
//!
//! Sizes are bucketed (multiples of 64 texels) so slightly different ROIs reuse
//! the same allocation. A texture is free when no [`GpuImage`] clone refers to it
//! any more (each image carries a lease token; the pool checks the token's
//! reference count), so the executor frees intermediates simply by dropping them,
//! and a frame kept in the frame cache stays allocated.

use std::sync::Arc;

use bae_gpu::wgpu;

use crate::error::RenderError;
use crate::geom::IRect;
use crate::source::{GpuImage, ImageAlpha};

/// Usage of every pooled texture.
pub const POOL_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
    .union(wgpu::TextureUsages::TEXTURE_BINDING)
    .union(wgpu::TextureUsages::STORAGE_BINDING)
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST);

const BUCKET: u32 = 64;
/// Frames an idle texture survives.
const MAX_IDLE_FRAMES: u64 = 120;

struct Slot {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
    size: [u32; 2],
    token: Arc<()>,
    last_used: u64,
    bytes: u64,
}

impl Slot {
    fn is_free(&self) -> bool {
        Arc::strong_count(&self.token) == 1
    }
}

/// Pool statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PoolStats {
    /// Textures allocated (free or in use).
    pub textures: usize,
    /// Bytes allocated.
    pub bytes: u64,
    /// Bytes of textures currently in use.
    pub bytes_in_use: u64,
    /// Textures created since the pool was made.
    pub created: u64,
}

/// The pool.
pub struct TexturePool {
    slots: Vec<Slot>,
    frame: u64,
    idle_budget: u64,
    created: u64,
}

impl std::fmt::Debug for TexturePool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TexturePool")
            .field("stats", &self.stats())
            .finish()
    }
}

fn bucket(v: u32) -> u32 {
    v.max(1).div_ceil(BUCKET) * BUCKET
}

fn bytes_per_texel(format: wgpu::TextureFormat) -> u64 {
    u64::from(format.block_copy_size(None).unwrap_or(8))
}

impl TexturePool {
    /// A pool keeping at most `idle_budget` bytes of unused textures.
    pub fn new(idle_budget: u64) -> Self {
        Self {
            slots: Vec::new(),
            frame: 0,
            idle_budget,
            created: 0,
        }
    }

    /// Statistics.
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            textures: self.slots.len(),
            bytes: self.slots.iter().map(|s| s.bytes).sum(),
            bytes_in_use: self
                .slots
                .iter()
                .filter(|s| !s.is_free())
                .map(|s| s.bytes)
                .sum(),
            created: self.created,
        }
    }

    /// An image covering `rect` (pixels of its space) at `pixel_scale` pixels
    /// per texel. The texture may be larger than needed; contents are undefined.
    pub fn acquire(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        rect: IRect,
        pixel_scale: f64,
    ) -> Result<GpuImage, RenderError> {
        let limit = device.limits().max_texture_dimension_2d;
        let [w, h] = crate::source::texels_for(rect, pixel_scale);
        let (w, h) = (w.max(1), h.max(1));
        if w > limit || h > limit {
            return Err(RenderError::TooLarge {
                width: w,
                height: h,
                limit,
            });
        }
        let size = [bucket(w).min(limit), bucket(h).min(limit)];
        let frame = self.frame;
        let slot = match self
            .slots
            .iter_mut()
            .find(|s| s.format == format && s.size == size && s.is_free())
        {
            Some(slot) => slot,
            None => {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("bae-render.pool"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    // Mask formats (R16F, RG16F) have no storage access in core WebGPU.
                    usage: POOL_USAGE
                        & format
                            .guaranteed_format_features(device.features())
                            .allowed_usages,
                    view_formats: &[],
                });
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                self.created += 1;
                self.slots.push(Slot {
                    texture,
                    view,
                    format,
                    size,
                    token: Arc::new(()),
                    last_used: frame,
                    bytes: u64::from(size[0]) * u64::from(size[1]) * bytes_per_texel(format),
                });
                self.slots.last_mut().expect("just pushed")
            }
        };
        slot.last_used = frame;
        Ok(GpuImage::pooled(
            slot.texture.clone(),
            slot.view.clone(),
            rect,
            ImageAlpha::Premultiplied,
            pixel_scale,
            slot.token.clone(),
        ))
    }

    /// Ends a frame: drops textures idle for too long and idle textures beyond
    /// the idle budget (least recently used first).
    pub fn end_frame(&mut self) {
        self.frame += 1;
        let frame = self.frame;
        self.slots
            .retain(|s| !s.is_free() || frame - s.last_used <= MAX_IDLE_FRAMES);
        let mut idle: u64 = self
            .slots
            .iter()
            .filter(|s| s.is_free())
            .map(|s| s.bytes)
            .sum();
        while idle > self.idle_budget {
            let Some((i, _)) = self
                .slots
                .iter()
                .enumerate()
                .filter(|(_, s)| s.is_free())
                .min_by_key(|(_, s)| s.last_used)
            else {
                break;
            };
            idle -= self.slots.swap_remove(i).bytes;
        }
    }

    /// Drops every free texture.
    pub fn trim(&mut self) {
        self.slots.retain(|s| !s.is_free());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets() {
        assert_eq!(bucket(1), 64);
        assert_eq!(bucket(64), 64);
        assert_eq!(bucket(65), 128);
        assert_eq!(bucket(1920), 1920);
        assert_eq!(bucket(1080), 1088);
    }
}
