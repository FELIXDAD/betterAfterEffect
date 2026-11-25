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

// The outer format match selects a specialized row operation; only component
// conversion and alpha math run inside the pixel loop.
trait Texel {
    const SIZE: usize;
    fn read(src: &[u8]) -> [f32; 4];
    fn write(dst: &mut [u8], rgba: [f32; 4]);
}

struct Float32;
impl Texel for Float32 {
    const SIZE: usize = 16;
    fn read(src: &[u8]) -> [f32; 4] {
        std::array::from_fn(|k| f32::from_le_bytes(src[4 * k..4 * k + 4].try_into().unwrap()))
    }
    fn write(dst: &mut [u8], rgba: [f32; 4]) {
        for (out, v) in dst.as_chunks_mut::<4>().0.iter_mut().zip(rgba) {
            out.copy_from_slice(&v.to_le_bytes());
        }
    }
}

struct Float16;
impl Texel for Float16 {
    const SIZE: usize = 8;
    fn read(src: &[u8]) -> [f32; 4] {
        std::array::from_fn(|k| half::f16::from_le_bytes([src[2 * k], src[2 * k + 1]]).to_f32())
    }
    fn write(dst: &mut [u8], rgba: [f32; 4]) {
        for (out, v) in dst.as_chunks_mut::<2>().0.iter_mut().zip(rgba) {
            out.copy_from_slice(&half::f16::from_f32(v).to_le_bytes());
        }
    }
}

struct Unorm8<const BGRA: bool>;
impl<const BGRA: bool> Texel for Unorm8<BGRA> {
    const SIZE: usize = 4;
    fn read(src: &[u8]) -> [f32; 4] {
        let mut px = std::array::from_fn(|k| f32::from(src[k]) / 255.0);
        if BGRA {
            px.swap(0, 2);
        }
        px
    }
    fn write(dst: &mut [u8], mut rgba: [f32; 4]) {
        if BGRA {
            rgba.swap(0, 2);
        }
        dst.copy_from_slice(&rgba.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8));
    }
}
type Rgba8 = Unorm8<false>;
type Bgra8 = Unorm8<true>;

fn convert_row<S: Texel, D: Texel>(src: &[u8], dst: &mut [u8], alpha: ImageAlpha) {
    for (input, output) in src.chunks_exact(S::SIZE).zip(dst.chunks_exact_mut(D::SIZE)) {
        let mut px = S::read(input);
        match alpha {
            ImageAlpha::Premultiplied => {}
            ImageAlpha::Opaque => px[3] = 1.0,
            ImageAlpha::Straight => {
                let a = px[3];
                for c in &mut px[..3] {
                    *c *= a;
                }
            }
        }
        D::write(output, px);
    }
}

fn copy_row(src: &[u8], dst: &mut [u8], _: ImageAlpha) {
    dst.copy_from_slice(&src[..dst.len()]);
}
type Row = fn(&[u8], &mut [u8], ImageAlpha);

/// Whether CPU plugins can read and write the working format.
pub fn working_supported(format: wgpu::TextureFormat) -> bool {
    use wgpu::TextureFormat::*;
    matches!(
        format,
        Rgba16Float | Rgba32Float | Rgba8Unorm | Rgba8UnormSrgb | Bgra8Unorm | Bgra8UnormSrgb
    )
}

fn working_to<D: Texel>(working: wgpu::TextureFormat) -> Row {
    use wgpu::TextureFormat::*;
    match working {
        Rgba16Float => convert_row::<Float16, D>,
        Rgba32Float => convert_row::<Float32, D>,
        Rgba8Unorm | Rgba8UnormSrgb => convert_row::<Rgba8, D>,
        Bgra8Unorm | Bgra8UnormSrgb => convert_row::<Bgra8, D>,
        _ => unreachable!("unsupported CPU working format"),
    }
}

fn to_working_row<S: Texel>(working: wgpu::TextureFormat) -> Row {
    use wgpu::TextureFormat::*;
    match working {
        Rgba16Float => convert_row::<S, Float16>,
        Rgba32Float => convert_row::<S, Float32>,
        Rgba8Unorm | Rgba8UnormSrgb => convert_row::<S, Rgba8>,
        Bgra8Unorm | Bgra8UnormSrgb => convert_row::<S, Bgra8>,
        _ => unreachable!("unsupported CPU working format"),
    }
}

fn same_format(working: wgpu::TextureFormat, format: BaePixelFormat) -> bool {
    use wgpu::TextureFormat::*;
    matches!(
        (working, format),
        (Rgba32Float, BAE_PIXEL_FORMAT_RGBA_F32_PREMUL)
            | (Rgba8Unorm | Rgba8UnormSrgb, BAE_PIXEL_FORMAT_RGBA_U8_PREMUL)
    )
}

/// Converts working-format rows into aligned plugin storage.
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
    use rayon::prelude::*;
    if width == 0 || height == 0 {
        return;
    }
    let convert = if matches!(alpha, ImageAlpha::Premultiplied) && same_format(working, format) {
        copy_row
    } else if format == BAE_PIXEL_FORMAT_RGBA_U8_PREMUL {
        working_to::<Rgba8>(working)
    } else {
        working_to::<Float32>(working)
    };
    let bytes = width as usize * bytes_per_pixel(format);
    dst.par_chunks_mut(dst_row)
        .take(height as usize)
        .enumerate()
        .for_each(|(y, row)| {
            convert(&src[y * src_row..], &mut row[..bytes], alpha);
        });
}

/// Writes tightly packed working-format rows into the reusable upload buffer.
pub fn to_working(
    src: &[u8],
    src_row: usize,
    format: BaePixelFormat,
    [width, height]: [u32; 2],
    working: wgpu::TextureFormat,
    dst: &mut Vec<u8>,
) {
    use rayon::prelude::*;
    let row_bytes = width as usize * working.block_copy_size(None).unwrap() as usize;
    dst.resize(row_bytes * height as usize, 0);
    if row_bytes == 0 {
        return;
    }
    let convert = if same_format(working, format) {
        copy_row
    } else if format == BAE_PIXEL_FORMAT_RGBA_U8_PREMUL {
        to_working_row::<Rgba8>(working)
    } else {
        to_working_row::<Float32>(working)
    };
    dst.par_chunks_mut(row_bytes)
        .enumerate()
        .for_each(|(y, row)| {
            convert(&src[y * src_row..], row, ImageAlpha::Premultiplied);
        });
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
    fn copy_and_swizzle_respect_roi_stride() {
        use wgpu::TextureFormat::{Bgra8Unorm, Rgba8Unorm};
        let mut src = [0; 128];
        src[4..8].copy_from_slice(&[12, 34, 56, 78]);
        src[68..72].copy_from_slice(&[90, 87, 65, 43]);
        let mut upload = Vec::with_capacity(32);
        let storage = upload.as_ptr();
        to_working(
            &src[4..],
            64,
            BAE_PIXEL_FORMAT_RGBA_U8_PREMUL,
            [1, 2],
            Rgba8Unorm,
            &mut upload,
        );
        assert_eq!(upload, [12, 34, 56, 78, 90, 87, 65, 43]);
        to_working(
            &src[4..],
            64,
            BAE_PIXEL_FORMAT_RGBA_U8_PREMUL,
            [1, 2],
            Bgra8Unorm,
            &mut upload,
        );
        assert_eq!(upload, [56, 34, 12, 78, 65, 87, 90, 43]);
        assert_eq!(storage, upload.as_ptr());
    }

    #[test]
    fn working_roundtrip() {
        let f = wgpu::TextureFormat::Rgba16Float;
        let px = [0.25f32, 0.5, 0.75, 0.5];
        let mut src = vec![0u8; 8];
        Float16::write(&mut src, px);
        let fmt = BAE_PIXEL_FORMAT_RGBA_F32_PREMUL;
        let mut cpu = vec![0u8; 64];
        from_working(&src, 8, f, ImageAlpha::Straight, [1, 1], fmt, &mut cpu, 64);
        let mut back = Vec::new();
        to_working(
            &cpu,
            64,
            fmt,
            [1, 1],
            wgpu::TextureFormat::Rgba32Float,
            &mut back,
        );
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
