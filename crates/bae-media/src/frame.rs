//! Frame delivery types: what a decoder hands out.
//!
//! * [`GpuFrame`] — the normal result: an `Rgba16Float` texture on the shared
//!   device (the working format, ARCHITECTURE §5), alpha premultiplied unless the
//!   request said otherwise. Color values stay in the source encoding (no
//!   linearization); [`GpuFrame::color`] says what that encoding is.
//! * [`CpuFrame`] — tightly packed RGBA pixels in memory, for thumbnails,
//!   packaging, plugins' CPU nodes and tests. Decoders produce it for still
//!   images; [`MediaGpu::upload`](crate::MediaGpu::upload) turns it into a
//!   [`GpuFrame`].

use bae_gpu::wgpu;

use crate::meta::{AlphaMode, ColorInfo, PixelAspect};

/// The GPU working format every [`GpuFrame`] uses.
pub const WORKING_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Memory layout of a [`CpuFrame`]: four interleaved channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// 8-bit unsigned normalized, R G B A.
    Rgba8,
    /// 8-bit unsigned normalized, B G R A (CoreVideo / Media Foundation readback).
    Bgra8,
    /// 16-bit unsigned normalized (native endian), R G B A.
    Rgba16,
    /// IEEE half float (native endian), R G B A.
    Rgba16Float,
    /// IEEE single float (native endian), R G B A.
    Rgba32Float,
}

impl PixelFormat {
    /// Bytes per pixel.
    pub const fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::Rgba8 | Self::Bgra8 => 4,
            Self::Rgba16 | Self::Rgba16Float => 8,
            Self::Rgba32Float => 16,
        }
    }

    /// The texture format a frame of this layout is uploaded as before it is
    /// converted to [`WORKING_FORMAT`]. 16-bit unorm goes through `Rgba16Uint`
    /// (core WebGPU; `Rgba16Unorm` needs an optional feature).
    pub const fn upload_format(self) -> wgpu::TextureFormat {
        match self {
            Self::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
            Self::Bgra8 => wgpu::TextureFormat::Bgra8Unorm,
            Self::Rgba16 => wgpu::TextureFormat::Rgba16Uint,
            Self::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
            Self::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
        }
    }
}

/// Decoded pixels in CPU memory, rows tightly packed.
#[derive(Clone, Debug, PartialEq)]
pub struct CpuFrame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Layout of [`Self::data`].
    pub format: PixelFormat,
    /// Pixel bytes, `width * height * bytes_per_pixel` long, top row first.
    pub data: Vec<u8>,
    /// Alpha interpretation of the pixels.
    pub alpha: AlphaMode,
    /// Color encoding of the pixels.
    pub color: ColorInfo,
    /// Pixel aspect ratio.
    pub pixel_aspect: PixelAspect,
}

impl CpuFrame {
    /// Bytes per row.
    pub fn bytes_per_row(&self) -> u32 {
        self.width * self.format.bytes_per_pixel()
    }

    /// Expected length of [`Self::data`].
    pub fn expected_len(&self) -> usize {
        self.bytes_per_row() as usize * self.height as usize
    }

    /// The bytes of pixel (`x`, `y`).
    pub fn pixel(&self, x: u32, y: u32) -> &[u8] {
        let bpp = self.format.bytes_per_pixel() as usize;
        let start = y as usize * self.bytes_per_row() as usize + x as usize * bpp;
        &self.data[start..start + bpp]
    }

    /// Pixel (`x`, `y`) as normalized floats (for tests and thumbnails).
    pub fn pixel_f32(&self, x: u32, y: u32) -> [f32; 4] {
        let p = self.pixel(x, y);
        match self.format {
            PixelFormat::Rgba8 => std::array::from_fn(|i| f32::from(p[i]) / 255.0),
            PixelFormat::Bgra8 => {
                let [b, g, r, a] = std::array::from_fn(|i| f32::from(p[i]) / 255.0);
                [r, g, b, a]
            }
            PixelFormat::Rgba16 => std::array::from_fn(|i| {
                f32::from(u16::from_ne_bytes([p[2 * i], p[2 * i + 1]])) / 65535.0
            }),
            PixelFormat::Rgba16Float => {
                std::array::from_fn(|i| half::f16::from_ne_bytes([p[2 * i], p[2 * i + 1]]).to_f32())
            }
            PixelFormat::Rgba32Float => std::array::from_fn(|i| {
                f32::from_ne_bytes([p[4 * i], p[4 * i + 1], p[4 * i + 2], p[4 * i + 3]])
            }),
        }
    }
}

/// A decoded frame on the GPU, in [`WORKING_FORMAT`].
///
/// Cheap to clone (the texture is reference counted); the
/// [`FrameCache`](crate::FrameCache) hands out clones.
#[derive(Clone, Debug)]
pub struct GpuFrame {
    /// `Rgba16Float`, usage `TEXTURE_BINDING | STORAGE_BINDING | COPY_SRC | COPY_DST`.
    pub texture: wgpu::Texture,
    /// Alpha of the texture: [`AlphaMode::Premultiplied`] (default),
    /// [`AlphaMode::Straight`] (if requested) or [`AlphaMode::Opaque`].
    pub alpha: AlphaMode,
    /// Color encoding of the texels (the source's; not linearized).
    pub color: ColorInfo,
    /// Pixel aspect ratio of the source.
    pub pixel_aspect: PixelAspect,
}

impl GpuFrame {
    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.texture.width()
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.texture.height()
    }

    /// A default view of the texture.
    pub fn view(&self) -> wgpu::TextureView {
        self.texture
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// GPU memory used by the texture, in bytes (for cache budgeting).
    pub fn byte_size(&self) -> u64 {
        u64::from(self.width()) * u64::from(self.height()) * 8
    }
}

/// What alpha the caller wants in the delivered frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlphaOutput {
    /// Premultiplied (associated) alpha — what the compositor works in.
    #[default]
    Premultiplied,
    /// Straight alpha, as AE effects that ask for unpremultiplied input expect.
    Straight,
}

/// Which frame to decode and how to deliver it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FrameRequest {
    /// Frame index, 0-based from the source's first frame (see [`crate::time`]).
    pub index: i64,
    /// Alpha of the delivered frame.
    pub alpha_output: AlphaOutput,
    /// Overrides the source's alpha tag (AE "Interpret Footage → Alpha").
    pub alpha_override: Option<AlphaMode>,
    /// Reserved for proxies / reduced-resolution preview: 1 = full size, 2 = half,
    /// 4 = quarter. Decoders may ignore it; check the delivered frame size.
    pub downscale: u8,
}

impl FrameRequest {
    /// Full-resolution, premultiplied frame `index`.
    pub fn new(index: i64) -> Self {
        FrameRequest {
            index,
            alpha_output: AlphaOutput::Premultiplied,
            alpha_override: None,
            downscale: 1,
        }
    }

    /// Same request with an alpha interpretation override.
    pub fn with_alpha_override(mut self, alpha: AlphaMode) -> Self {
        self.alpha_override = Some(alpha);
        self
    }

    /// Same request delivering straight alpha.
    pub fn straight(mut self) -> Self {
        self.alpha_output = AlphaOutput::Straight;
        self
    }
}
