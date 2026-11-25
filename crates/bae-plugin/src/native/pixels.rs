//! CPU images exchanged with native effects (spec §10.6): 64-byte aligned
//! buffers and conversion between the GPU working formats and the CPU formats
//! a plugin accepts (premultiplied RGBA, 32-bit float or 8-bit).

use bae_gpu::wgpu;
use bae_plugin_sdk::abi::{
    BAE_PIXEL_FORMAT_RGBA_F32_PREMUL, BAE_PIXEL_FORMAT_RGBA_U8_PREMUL, BaePixelFormat, BaeRect,
};
use bae_render::ImageAlpha;

/// Rows (and buffers) are aligned to this many bytes.
pub const ROW_ALIGN: usize = 64;

/// CPU formats the host can hand to a plugin, in the host's preference order.
pub const HOST_FORMATS: [BaePixelFormat; 2] = [
    BAE_PIXEL_FORMAT_RGBA_F32_PREMUL,
    BAE_PIXEL_FORMAT_RGBA_U8_PREMUL,
];

/// The first format of the plugin's preference list the host supports.
pub fn choose_format(plugin: &[BaePixelFormat]) -> Option<BaePixelFormat> {
    plugin.iter().copied().find(|f| HOST_FORMATS.contains(f))
}

/// Bytes per pixel of a supported CPU format.
pub fn bytes_per_pixel(format: BaePixelFormat) -> usize {
    if format == BAE_PIXEL_FORMAT_RGBA_U8_PREMUL {
        4
    } else {
        16
    }
}

/// Row stride of a `width`-pixel row.
pub fn row_bytes(format: BaePixelFormat, width: u32) -> usize {
    (width as usize * bytes_per_pixel(format)).next_multiple_of(ROW_ALIGN)
}

/// Bytes of an image covering `rect`.
pub fn image_len(format: BaePixelFormat, rect: BaeRect) -> usize {
    row_bytes(format, rect.width()) * rect.height() as usize
}

#[repr(C, align(64))]
#[derive(Clone, Copy)]
struct Block([u8; ROW_ALIGN]);

/// A 64-byte aligned, zero-initialized byte buffer.
#[derive(Default)]
pub struct AlignedBuf {
    blocks: Vec<Block>,
    len: usize,
}

impl AlignedBuf {
    /// A buffer of `len` bytes.
    pub fn with_len(len: usize) -> Self {
        let mut b = Self::default();
        b.resize(len);
        b
    }

    /// Sets the length (contents of new bytes are zero, old ones are kept).
    pub fn resize(&mut self, len: usize) {
        let blocks = len.div_ceil(ROW_ALIGN);
        if self.blocks.len() < blocks {
            self.blocks.resize(blocks, Block([0; ROW_ALIGN]));
        }
        self.len = len;
    }

    /// The bytes.
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: `blocks` holds at least `len` initialized bytes.
        unsafe { std::slice::from_raw_parts(self.blocks.as_ptr().cast::<u8>(), self.len) }
    }

    /// The bytes, writable.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: as above, and `&mut self` is exclusive.
        unsafe { std::slice::from_raw_parts_mut(self.blocks.as_mut_ptr().cast::<u8>(), self.len) }
    }
}

fn read_texel(src: &[u8], format: wgpu::TextureFormat) -> Option<[f32; 4]> {
    use wgpu::TextureFormat as F;
    Some(match format {
        F::Rgba16Float => {
            std::array::from_fn(|k| half::f16::from_le_bytes([src[2 * k], src[2 * k + 1]]).to_f32())
        }
        F::Rgba32Float => std::array::from_fn(|k| {
            f32::from_le_bytes([src[4 * k], src[4 * k + 1], src[4 * k + 2], src[4 * k + 3]])
        }),
        F::Rgba8Unorm | F::Rgba8UnormSrgb => std::array::from_fn(|k| f32::from(src[k]) / 255.0),
        F::Bgra8Unorm | F::Bgra8UnormSrgb => {
            let c = |k: usize| f32::from(src[k]) / 255.0;
            [c(2), c(1), c(0), c(3)]
        }
        _ => return None,
    })
}

fn write_texel(dst: &mut [u8], format: wgpu::TextureFormat, px: [f32; 4]) {
    use wgpu::TextureFormat as F;
    let u8_of = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    match format {
        F::Rgba16Float => {
            for (k, v) in px.iter().enumerate() {
                dst[2 * k..2 * k + 2].copy_from_slice(&half::f16::from_f32(*v).to_le_bytes());
            }
        }
        F::Rgba32Float => {
            for (k, v) in px.iter().enumerate() {
                dst[4 * k..4 * k + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        F::Bgra8Unorm | F::Bgra8UnormSrgb => {
            dst[..4].copy_from_slice(&[u8_of(px[2]), u8_of(px[1]), u8_of(px[0]), u8_of(px[3])]);
        }
        _ => {
            for (k, v) in px.iter().enumerate() {
                dst[k] = u8_of(*v);
            }
        }
    }
}

/// `true` if [`from_working`] / [`to_working`] handle `format`.
pub fn working_supported(format: wgpu::TextureFormat) -> bool {
    read_texel(&[0; 16], format).is_some()
}

/// Converts `width × height` texels of a working-format image (rows of
/// `src_row` bytes) into the plugin format `format` (rows of `dst_row` bytes),
/// premultiplying straight input and making opaque input's alpha 1.
#[allow(clippy::too_many_arguments)]
pub fn from_working(
    src: &[u8],
    src_row: usize,
    working: wgpu::TextureFormat,
    alpha: ImageAlpha,
    [width, height]: [u32; 2],
    format: BaePixelFormat,
    dst: &mut [u8],
    dst_row: usize,
) {
    let texel = working.block_copy_size(None).unwrap_or(8) as usize;
    let bpp = bytes_per_pixel(format);
    for y in 0..height as usize {
        let s = &src[y * src_row..];
        let d = &mut dst[y * dst_row..];
        for x in 0..width as usize {
            let mut px = read_texel(&s[x * texel..], working).unwrap_or_default();
            match alpha {
                ImageAlpha::Premultiplied => {}
                ImageAlpha::Straight => {
                    let a = px[3];
                    for c in &mut px[..3] {
                        *c *= a;
                    }
                }
                ImageAlpha::Opaque => px[3] = 1.0,
            }
            let out = &mut d[x * bpp..(x + 1) * bpp];
            if format == BAE_PIXEL_FORMAT_RGBA_U8_PREMUL {
                for k in 0..4 {
                    out[k] = (px[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            } else {
                for k in 0..4 {
                    out[4 * k..4 * k + 4].copy_from_slice(&px[k].to_le_bytes());
                }
            }
        }
    }
}

/// Converts `width × height` plugin-format pixels (rows of `src_row` bytes)
/// into tightly packed working-format texels.
pub fn to_working(
    src: &[u8],
    src_row: usize,
    format: BaePixelFormat,
    [width, height]: [u32; 2],
    working: wgpu::TextureFormat,
) -> Vec<u8> {
    let texel = working.block_copy_size(None).unwrap_or(8) as usize;
    let bpp = bytes_per_pixel(format);
    let mut out = vec![0u8; width as usize * height as usize * texel];
    for y in 0..height as usize {
        let s = &src[y * src_row..];
        for x in 0..width as usize {
            let p = &s[x * bpp..(x + 1) * bpp];
            let px: [f32; 4] = if format == BAE_PIXEL_FORMAT_RGBA_U8_PREMUL {
                std::array::from_fn(|k| f32::from(p[k]) / 255.0)
            } else {
                std::array::from_fn(|k| {
                    f32::from_le_bytes([p[4 * k], p[4 * k + 1], p[4 * k + 2], p[4 * k + 3]])
                })
            };
            let at = (y * width as usize + x) * texel;
            write_texel(&mut out[at..at + texel], working, px);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_aligned() {
        assert_eq!(row_bytes(BAE_PIXEL_FORMAT_RGBA_F32_PREMUL, 3), 64);
        assert_eq!(row_bytes(BAE_PIXEL_FORMAT_RGBA_U8_PREMUL, 17), 128);
        let b = AlignedBuf::with_len(100);
        assert_eq!(b.as_slice().as_ptr() as usize % ROW_ALIGN, 0);
        assert_eq!(b.as_slice().len(), 100);
    }

    #[test]
    fn working_roundtrip() {
        let f = wgpu::TextureFormat::Rgba16Float;
        let px = [0.25f32, 0.5, 0.75, 0.5];
        let mut src = vec![0u8; 8];
        write_texel(&mut src, f, px);
        let fmt = BAE_PIXEL_FORMAT_RGBA_F32_PREMUL;
        let mut cpu = vec![0u8; 64];
        from_working(&src, 8, f, ImageAlpha::Straight, [1, 1], fmt, &mut cpu, 64);
        let back = to_working(&cpu, 64, fmt, [1, 1], wgpu::TextureFormat::Rgba32Float);
        let got: Vec<f32> = back
            .chunks(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        assert_eq!(
            got,
            vec![0.125, 0.25, 0.375, 0.5],
            "premultiplied on the way in"
        );
        assert_eq!(choose_format(&[7, fmt]), Some(fmt));
        assert_eq!(choose_format(&[7]), None);
    }
}
