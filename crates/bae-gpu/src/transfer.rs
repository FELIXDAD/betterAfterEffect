//! CPU ↔ GPU texture transfers.
//!
//! Frames stay on the GPU (ARCHITECTURE §2.3); these helpers are for the few
//! places that need pixels on the CPU — export encoding, thumbnails, Agent
//! screenshots — and for plugin "CPU nodes" that hand pixels back to the GPU.
//!
//! * [`TextureReadback`] copies a texture into a staging buffer with WebGPU's
//!   256-byte row alignment and later strips the padding. It is asynchronous:
//!   [`TextureReadback::start`] records + submits, the result is collected with
//!   [`PendingReadback::try_finish`] (non-blocking; the event loop polls the device
//!   each frame) or [`PendingReadback::wait`]. Keep two in flight for double-buffered
//!   export.
//! * [`upload_texture`] / [`create_texture_with_data`] write tightly packed rows.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::context::{GpuContext, GpuError};

/// Bytes per row of a buffer copy must be a multiple of this (256).
pub const COPY_BYTES_PER_ROW_ALIGNMENT: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

/// Rounds `unpadded` up to [`COPY_BYTES_PER_ROW_ALIGNMENT`].
pub const fn padded_bytes_per_row(unpadded: u32) -> u32 {
    let align = COPY_BYTES_PER_ROW_ALIGNMENT;
    unpadded.div_ceil(align) * align
}

/// Bytes per pixel of an uncompressed, single-plane color format.
pub fn bytes_per_pixel(format: wgpu::TextureFormat) -> Option<u32> {
    if format.block_dimensions() != (1, 1) || format.is_multi_planar_format() {
        return None;
    }
    format.block_copy_size(Some(wgpu::TextureAspect::All))
}

/// Pixels read back to the CPU, rows tightly packed (no padding).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageData {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel format (layout of [`Self::data`]).
    pub format: wgpu::TextureFormat,
    /// `width * bytes_per_pixel`.
    pub bytes_per_row: u32,
    /// Pixel bytes, `bytes_per_row * height` long.
    pub data: Vec<u8>,
}

impl ImageData {
    /// The bytes of row `y`.
    pub fn row(&self, y: u32) -> &[u8] {
        let start = (y * self.bytes_per_row) as usize;
        &self.data[start..start + self.bytes_per_row as usize]
    }

    /// The bytes of pixel (`x`, `y`).
    pub fn pixel(&self, x: u32, y: u32) -> &[u8] {
        let bpp = (self.bytes_per_row / self.width.max(1)) as usize;
        let row = self.row(y);
        &row[x as usize * bpp..(x as usize + 1) * bpp]
    }
}

/// A region of a texture's mip level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureRegion {
    /// Mip level.
    pub mip_level: u32,
    /// Top-left corner (and array layer in `z`).
    pub origin: wgpu::Origin3d,
    /// Size (`depth_or_array_layers` must be 1).
    pub size: wgpu::Extent3d,
}

impl TextureRegion {
    /// The whole first layer of mip 0.
    pub fn full(texture: &wgpu::Texture) -> Self {
        Self {
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            size: wgpu::Extent3d {
                width: texture.width(),
                height: texture.height(),
                depth_or_array_layers: 1,
            },
        }
    }
}

/// A texture copy recorded into a staging buffer, not yet submitted.
#[derive(Debug)]
pub struct TextureReadback {
    buffer: wgpu::Buffer,
    layout: ReadbackLayout,
}

#[derive(Debug, Clone, Copy)]
struct ReadbackLayout {
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    unpadded_bytes_per_row: u32,
    padded_bytes_per_row: u32,
}

impl TextureReadback {
    /// Records a copy of `region` of `texture` (which needs `COPY_SRC` usage)
    /// into `encoder`. Submit the encoder, then call [`Self::map`].
    pub fn record(
        ctx: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
        region: TextureRegion,
    ) -> Result<Self, GpuError> {
        let format = texture.format();
        let bpp = bytes_per_pixel(format).ok_or_else(|| {
            GpuError::Unsupported(format!("readback of {format:?} (compressed or planar)"))
        })?;
        if !texture.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            return Err(GpuError::Unsupported(
                "readback needs a texture with COPY_SRC usage".into(),
            ));
        }
        if region.size.depth_or_array_layers != 1
            || region.size.width == 0
            || region.size.height == 0
        {
            return Err(GpuError::Unsupported(format!(
                "readback region {:?} must be one non-empty 2D layer",
                region.size
            )));
        }
        let unpadded = region.size.width * bpp;
        let padded = padded_bytes_per_row(unpadded);
        let buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("bae.readback"),
            size: u64::from(padded) * u64::from(region.size.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: region.mip_level,
                origin: region.origin,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(region.size.height),
                },
            },
            region.size,
        );
        Ok(Self {
            buffer,
            layout: ReadbackLayout {
                width: region.size.width,
                height: region.size.height,
                format,
                unpadded_bytes_per_row: unpadded,
                padded_bytes_per_row: padded,
            },
        })
    }

    /// Records and submits a copy of the whole texture in its own command buffer,
    /// and starts mapping. The usual entry point.
    pub fn start(ctx: &GpuContext, texture: &wgpu::Texture) -> Result<PendingReadback, GpuError> {
        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("bae.readback"),
            });
        let readback = Self::record(ctx, &mut encoder, texture, TextureRegion::full(texture))?;
        ctx.queue().submit([encoder.finish()]);
        Ok(readback.map())
    }

    /// Starts mapping the staging buffer. Call after the encoder that recorded the
    /// copy was submitted.
    pub fn map(self) -> PendingReadback {
        let status: Arc<Mutex<Option<Result<(), String>>>> = Arc::default();
        let callback_status = status.clone();
        self.buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                *callback_status.lock() = Some(result.map_err(|e| e.to_string()));
            });
        PendingReadback {
            buffer: self.buffer,
            layout: self.layout,
            status,
        }
    }
}

/// A readback whose staging buffer is being mapped.
#[derive(Debug)]
pub struct PendingReadback {
    buffer: wgpu::Buffer,
    layout: ReadbackLayout,
    status: Arc<Mutex<Option<Result<(), String>>>>,
}

impl PendingReadback {
    /// `true` once the data can be taken without blocking. Mapping only progresses
    /// when the device is polled ([`GpuContext::poll`]).
    pub fn is_ready(&self) -> bool {
        self.status.lock().is_some()
    }

    /// Returns the pixels if the mapping completed, without blocking.
    pub fn try_finish(&self) -> Option<Result<ImageData, GpuError>> {
        let status = self.status.lock().take()?;
        Some(status.map_err(GpuError::Map).and_then(|()| self.copy_out()))
    }

    /// Blocks until the GPU finished the copy and returns the pixels.
    pub fn wait(self, ctx: &GpuContext) -> Result<ImageData, GpuError> {
        loop {
            if let Some(result) = self.try_finish() {
                return result;
            }
            if let Some(info) = ctx.lost_info() {
                return Err(GpuError::DeviceLost(info));
            }
            ctx.device().poll(wgpu::PollType::wait_indefinitely())?;
        }
    }

    fn copy_out(&self) -> Result<ImageData, GpuError> {
        let layout = self.layout;
        let mut data =
            Vec::with_capacity(layout.unpadded_bytes_per_row as usize * layout.height as usize);
        {
            let view = self
                .buffer
                .get_mapped_range(..)
                .map_err(|e| GpuError::Map(e.to_string()))?;
            for row in view
                .chunks(layout.padded_bytes_per_row as usize)
                .take(layout.height as usize)
            {
                data.extend_from_slice(&row[..layout.unpadded_bytes_per_row as usize]);
            }
        }
        self.buffer.unmap();
        Ok(ImageData {
            width: layout.width,
            height: layout.height,
            format: layout.format,
            bytes_per_row: layout.unpadded_bytes_per_row,
            data,
        })
    }
}

/// Reads a whole texture back, blocking until done (thumbnails, tests, Agent
/// screenshots). Prefer [`TextureReadback::start`] on hot paths.
pub fn read_texture(ctx: &GpuContext, texture: &wgpu::Texture) -> Result<ImageData, GpuError> {
    TextureReadback::start(ctx, texture)?.wait(ctx)
}

/// Writes tightly packed pixels (`bytes_per_row = width * bpp`) into `region` of
/// `texture` (which needs `COPY_DST` usage). The queue stages the data; the
/// write is ordered before the next submission.
pub fn upload_texture(
    ctx: &GpuContext,
    texture: &wgpu::Texture,
    region: TextureRegion,
    data: &[u8],
) -> Result<(), GpuError> {
    let format = texture.format();
    let bpp = bytes_per_pixel(format).ok_or_else(|| {
        GpuError::Unsupported(format!("upload of {format:?} (compressed or planar)"))
    })?;
    let bytes_per_row = region.size.width * bpp;
    let expected = bytes_per_row as usize
        * region.size.height as usize
        * region.size.depth_or_array_layers as usize;
    if data.len() != expected {
        return Err(GpuError::Unsupported(format!(
            "upload of {} bytes into a {}×{}×{} {format:?} region (expected {expected})",
            data.len(),
            region.size.width,
            region.size.height,
            region.size.depth_or_array_layers
        )));
    }
    if !texture.usage().contains(wgpu::TextureUsages::COPY_DST) {
        return Err(GpuError::Unsupported(
            "upload needs a texture with COPY_DST usage".into(),
        ));
    }
    ctx.queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: region.mip_level,
            origin: region.origin,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_row),
            rows_per_image: Some(region.size.height),
        },
        region.size,
    );
    Ok(())
}

/// Creates a 2D texture and fills mip 0 with tightly packed `data`. `COPY_DST`
/// is added to the usage.
pub fn create_texture_with_data(
    ctx: &GpuContext,
    desc: &wgpu::TextureDescriptor<'_>,
    data: &[u8],
) -> Result<wgpu::Texture, GpuError> {
    let mut desc = desc.clone();
    desc.usage |= wgpu::TextureUsages::COPY_DST;
    let texture = ctx.device().create_texture(&desc);
    upload_texture(
        ctx,
        &texture,
        TextureRegion {
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            size: desc.size,
        },
        data,
    )?;
    Ok(texture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_padding() {
        assert_eq!(padded_bytes_per_row(0), 0);
        assert_eq!(padded_bytes_per_row(1), 256);
        assert_eq!(padded_bytes_per_row(256), 256);
        assert_eq!(padded_bytes_per_row(37 * 4), 256);
        assert_eq!(padded_bytes_per_row(37 * 8), 512);
    }

    #[test]
    fn pixel_sizes() {
        use wgpu::TextureFormat as F;
        assert_eq!(bytes_per_pixel(F::Rgba8Unorm), Some(4));
        assert_eq!(bytes_per_pixel(F::Rgba16Float), Some(8));
        assert_eq!(bytes_per_pixel(F::Rgba32Float), Some(16));
        assert_eq!(bytes_per_pixel(F::R8Unorm), Some(1));
        assert_eq!(bytes_per_pixel(F::Bc1RgbaUnorm), None);
    }
}
