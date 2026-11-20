//! Pixel worlds and the CPU-node conversion contract.
//!
//! AE effects read and write *worlds* (`PF_EffectWorld` / `PF_LayerDef`):
//! a pointer, `rowbytes`, width/height, an extent hint, world flags and a
//! pixel aspect ratio (SDK guide, "Effect Basics › PF_EffectWorld"). CPU
//! worlds store four channels per pixel in memory order **A, R, G, B** at one
//! of three depths:
//!
//! | depth | channel type | white | notes |
//! |---|---|---|---|
//! | 8 bpc  | `u8`  | 255   | |
//! | 16 bpc | `u16` | 32768 | *not* 65535; values above 32768 are invalid |
//! | 32 bpc | `f32` | 1.0   | over-range and negative values are legal |
//!
//! Pixels are premultiplied in the working colour space, exactly like BAE's
//! working buffers, so the host never (un)premultiplies; it only swizzles and
//! rescales. GPU worlds (`PF_Cmd_SMART_RENDER_GPU`) are 128-bit float in
//! memory order **B, G, R, A** inside a linear device *buffer* ([`crate::gpu`]).
//!
//! BAE's render graph keeps RGBA16F (optionally RGBA32F) premultiplied
//! textures. The fast path converts on the GPU ([`crate::shaders`]) straight
//! into a mappable staging buffer laid out as an AE world, so the plug-in
//! reads the mapped memory in place and the CPU does no per-pixel work. The
//! kernels here are the reference implementation and the CPU fallback; the
//! GPU shader is tested against them.
//!
//! Precision note: an RGBA16F working buffer carries ~11 significant bits, so
//! a 16 bpc AE effect sees 16-bit values quantised to half-float precision.
//! Lossless 16 bpc chains need the RGBA32F working mode (request R04-03).

use crate::error::{HostError, Result};
use crate::geometry::{LRect, RationalScale};

/// Channel depth of a CPU world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PixelDepth {
    /// 8 bits per channel, 0..=255.
    U8,
    /// 16 bits per channel, 0..=32768.
    U16,
    /// 32-bit float per channel.
    F32,
}

/// White value of a 16 bpc AE channel.
pub const MAX_CHAN16: u16 = 32768;
/// White value of an 8 bpc AE channel.
pub const MAX_CHAN8: u8 = 255;

impl PixelDepth {
    /// Bytes per channel.
    pub const fn bytes_per_channel(self) -> usize {
        match self {
            PixelDepth::U8 => 1,
            PixelDepth::U16 => 2,
            PixelDepth::F32 => 4,
        }
    }

    /// Bytes per 4-channel pixel.
    pub const fn bytes_per_pixel(self) -> usize {
        4 * self.bytes_per_channel()
    }

    /// Bits per channel as reported in SmartFX `bitdepth` fields.
    pub const fn bits(self) -> u32 {
        8 * self.bytes_per_channel() as u32
    }
}

/// Memory order of the four channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChannelOrder {
    /// A, R, G, B — every CPU AE world.
    Argb,
    /// B, G, R, A — AE GPU worlds (128-bit float).
    Bgra,
    /// R, G, B, A — BAE textures.
    Rgba,
}

impl ChannelOrder {
    /// Position of (r, g, b, a) within a pixel.
    pub const fn positions(self) -> [usize; 4] {
        match self {
            ChannelOrder::Argb => [1, 2, 3, 0],
            ChannelOrder::Bgra => [2, 1, 0, 3],
            ChannelOrder::Rgba => [0, 1, 2, 3],
        }
    }
}

/// Description of a world the host hands to a plug-in (everything except the
/// data pointer, which only the P3 FFI shim touches).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldDesc {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Bytes from the start of one row to the next (≥ width × bpp).
    pub rowbytes: usize,
    /// Channel depth.
    pub depth: PixelDepth,
    /// Channel order.
    pub order: ChannelOrder,
    /// Where this buffer's (0,0) sits in layer coordinates. SmartFX buffers
    /// cover the negotiated result rect, so their origin is that rect's
    /// top-left; legacy buffers may be expanded by upstream effects.
    pub origin_x: i32,
    /// See `origin_x`.
    pub origin_y: i32,
    /// Smallest rect (buffer coordinates) known to contain all non-zero alpha.
    pub extent_hint: LRect,
    /// The plug-in may write the pixels (true for outputs).
    pub writable: bool,
    /// Pixel aspect ratio of the content.
    pub pixel_aspect: RationalScale,
}

/// Row alignment BAE uses for host-allocated worlds. 256 bytes is wgpu's
/// required `bytes_per_row` alignment for buffer↔texture copies, so a GPU
/// staging buffer can be handed to the plug-in *as* its world without any
/// repacking. AE effects must honour arbitrary rowbytes (SDK guide,
/// "PF_EffectWorld › Rowbytes"), so this is always legal.
pub const HOST_ROW_ALIGN: usize = 256;

/// Round `width × bpp` up to `align` (a power of two).
pub fn aligned_rowbytes(width: u32, depth: PixelDepth, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    let raw = width as usize * depth.bytes_per_pixel();
    (raw + align - 1) & !(align - 1)
}

impl WorldDesc {
    /// A host-allocated CPU world of `rect` (layer coordinates) at `depth`,
    /// with rows aligned to [`HOST_ROW_ALIGN`].
    pub fn for_rect(rect: LRect, depth: PixelDepth, writable: bool) -> WorldDesc {
        WorldDesc {
            width: rect.width(),
            height: rect.height(),
            rowbytes: aligned_rowbytes(rect.width(), depth, HOST_ROW_ALIGN),
            depth,
            order: ChannelOrder::Argb,
            origin_x: rect.left,
            origin_y: rect.top,
            extent_hint: LRect::from_xywh(0, 0, rect.width() as i32, rect.height() as i32),
            writable,
            pixel_aspect: RationalScale::ONE,
        }
    }

    /// Total bytes the buffer must provide.
    pub fn byte_len(&self) -> usize {
        if self.height == 0 {
            return 0;
        }
        self.rowbytes * (self.height as usize - 1)
            + self.width as usize * self.depth.bytes_per_pixel()
    }

    /// Check internal consistency and that a buffer of `len` bytes suffices.
    pub fn validate(&self, len: usize) -> Result<()> {
        if self.rowbytes < self.width as usize * self.depth.bytes_per_pixel() {
            return Err(HostError::BadBuffer {
                reason: "rowbytes smaller than a row",
            });
        }
        if !self.rowbytes.is_multiple_of(self.depth.bytes_per_channel()) {
            return Err(HostError::BadBuffer {
                reason: "rowbytes not a multiple of the channel size",
            });
        }
        if len < self.byte_len() {
            return Err(HostError::BadBuffer {
                reason: "buffer shorter than height × rowbytes",
            });
        }
        Ok(())
    }

    /// Row stride in channel elements (u8/u16/f32 units).
    pub fn stride_elems(&self) -> usize {
        self.rowbytes / self.depth.bytes_per_channel()
    }
}

// ---------------------------------------------------------------------------
// Half floats (BAE RGBA16F texels), implemented here to keep the crate
// dependency-free. Round-to-nearest-even, IEEE 754 binary16.
// ---------------------------------------------------------------------------

/// An IEEE 754 binary16 value stored as raw bits (one RGBA16F channel).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct F16(pub u16);

/// Convert half-float bits to `f32` (exact).
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h & 0x8000) as u32) << 16;
    let exp = ((h >> 10) & 0x1f) as u32;
    let man = (h & 0x3ff) as u32;
    let bits = match exp {
        0 if man == 0 => sign,
        0 => {
            // Subnormal: man × 2^-24, exact in f32.
            let v = man as f32 * (1.0 / 16_777_216.0);
            return if sign != 0 { -v } else { v };
        }
        31 => sign | 0x7f80_0000 | (man << 13),
        _ => sign | ((exp + 112) << 23) | (man << 13),
    };
    f32::from_bits(bits)
}

/// Convert `f32` to half-float bits, rounding to nearest even. NaN maps to a
/// quiet NaN, overflow to infinity.
pub fn f32_to_f16(x: f32) -> u16 {
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32;
    let man = b & 0x7f_ffff;
    if exp == 0xff {
        return if man == 0 {
            sign | 0x7c00
        } else {
            sign | 0x7e00
        };
    }
    let e = exp - 112; // rebias 127 → 15
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = man | 0x80_0000;
        let shift = (14 - e) as u32;
        let mut q = m >> shift;
        let rem = m & ((1 << shift) - 1);
        let half = 1 << (shift - 1);
        if rem > half || (rem == half && (q & 1) == 1) {
            q += 1;
        }
        return sign | q as u16;
    }
    let mut h = ((e as u32) << 10) | (man >> 13);
    let rem = man & 0x1fff;
    if rem > 0x1000 || (rem == 0x1000 && (h & 1) == 1) {
        h += 1;
    }
    sign | h as u16
}

fn f16_lut() -> &'static [f32] {
    static LUT: std::sync::OnceLock<Box<[f32]>> = std::sync::OnceLock::new();
    LUT.get_or_init(|| (0..=u16::MAX).map(f16_to_f32).collect())
}

/// A BAE-side texel channel type (half or single float).
pub trait BaeSample: Copy + Send + Sync {
    /// Decode to `f32`.
    fn load(self, lut: &[f32]) -> f32;
    /// Encode from `f32`.
    fn store(v: f32) -> Self;
}

impl BaeSample for F16 {
    #[inline(always)]
    fn load(self, lut: &[f32]) -> f32 {
        lut[self.0 as usize]
    }
    #[inline(always)]
    fn store(v: f32) -> Self {
        F16(f32_to_f16(v))
    }
}

impl BaeSample for f32 {
    #[inline(always)]
    fn load(self, _lut: &[f32]) -> f32 {
        self
    }
    #[inline(always)]
    fn store(v: f32) -> Self {
        v
    }
}

/// Mutable view of a world's pixel memory, typed by depth.
pub enum WorldBufMut<'a> {
    /// 8 bpc channels.
    U8(&'a mut [u8]),
    /// 16 bpc channels.
    U16(&'a mut [u16]),
    /// 32 bpc channels.
    F32(&'a mut [f32]),
}

/// Read-only view of a world's pixel memory, typed by depth.
#[derive(Clone, Copy)]
pub enum WorldBuf<'a> {
    /// 8 bpc channels.
    U8(&'a [u8]),
    /// 16 bpc channels.
    U16(&'a [u16]),
    /// 32 bpc channels.
    F32(&'a [f32]),
}

/// How out-of-range values are treated when *encoding* float worlds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatPolicy {
    /// Pass values through unchanged (AE semantics for 32 bpc).
    Passthrough,
    /// Replace NaN with 0 (defensive default: protects plug-ins from NaNs
    /// produced upstream; infinities are kept).
    ZeroNan,
}

#[inline(always)]
fn to_u8(v: f32) -> u8 {
    // NaN survives `clamp`, and the saturating `as` cast maps NaN to 0.
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[inline(always)]
fn to_u16(v: f32) -> u16 {
    (v.clamp(0.0, 1.0) * 32768.0 + 0.5) as u16
}

#[inline(always)]
fn sanitize(v: f32, p: FloatPolicy) -> f32 {
    match p {
        FloatPolicy::Passthrough => v,
        FloatPolicy::ZeroNan => {
            if v.is_nan() {
                0.0
            } else {
                v
            }
        }
    }
}

/// Encode a premultiplied RGBA region (`src`, row stride `src_stride` in
/// channel elements) into an AE world (`dst` described by `desc`).
/// Rows `rows` (relative to the world) are processed, which lets callers split
/// work across threads ([`encode_parallel`]).
pub fn encode_rows<S: BaeSample>(
    src: &[S],
    src_stride: usize,
    desc: &WorldDesc,
    dst: &mut WorldBufMut<'_>,
    rows: core::ops::Range<usize>,
    policy: FloatPolicy,
) {
    let lut = f16_lut();
    let w = desc.width as usize;
    let ds = desc.stride_elems();
    let [pr, pg, pb, pa] = desc.order.positions();
    for (i, y) in rows.enumerate() {
        let s = &src[y * src_stride..y * src_stride + 4 * w];
        macro_rules! row {
            ($buf:expr, $conv:expr) => {{
                let d = &mut $buf[i * ds..i * ds + 4 * w];
                for (dp, sp) in d.chunks_exact_mut(4).zip(s.chunks_exact(4)) {
                    dp[pr] = $conv(sp[0].load(lut));
                    dp[pg] = $conv(sp[1].load(lut));
                    dp[pb] = $conv(sp[2].load(lut));
                    dp[pa] = $conv(sp[3].load(lut));
                }
            }};
        }
        match dst {
            WorldBufMut::U8(b) => row!(b, to_u8),
            WorldBufMut::U16(b) => row!(b, to_u16),
            WorldBufMut::F32(b) => row!(b, |v| sanitize(v, policy)),
        }
    }
}

/// Decode rows of an AE world back into premultiplied RGBA samples. 16 bpc
/// values above 32768 (invalid) are clamped to white.
pub fn decode_rows<S: BaeSample>(
    src: WorldBuf<'_>,
    desc: &WorldDesc,
    dst: &mut [S],
    dst_stride: usize,
    rows: core::ops::Range<usize>,
) {
    let w = desc.width as usize;
    let ss = desc.stride_elems();
    let [pr, pg, pb, pa] = desc.order.positions();
    for (i, y) in rows.enumerate() {
        let d = &mut dst[i * dst_stride..i * dst_stride + 4 * w];
        macro_rules! row {
            ($buf:expr, $conv:expr) => {{
                let s = &$buf[y * ss..y * ss + 4 * w];
                for (dp, sp) in d.chunks_exact_mut(4).zip(s.chunks_exact(4)) {
                    dp[0] = S::store($conv(sp[pr]));
                    dp[1] = S::store($conv(sp[pg]));
                    dp[2] = S::store($conv(sp[pb]));
                    dp[3] = S::store($conv(sp[pa]));
                }
            }};
        }
        match src {
            WorldBuf::U8(b) => row!(b, |v: u8| v as f32 * (1.0 / 255.0)),
            WorldBuf::U16(b) => row!(b, |v: u16| v.min(MAX_CHAN16) as f32 * (1.0 / 32768.0)),
            WorldBuf::F32(b) => row!(b, |v: f32| v),
        }
    }
}

fn check_dst(desc: &WorldDesc, dst: &WorldBufMut<'_>) -> Result<()> {
    let (depth, len) = match dst {
        WorldBufMut::U8(b) => (PixelDepth::U8, b.len()),
        WorldBufMut::U16(b) => (PixelDepth::U16, b.len() * 2),
        WorldBufMut::F32(b) => (PixelDepth::F32, b.len() * 4),
    };
    if depth != desc.depth {
        return Err(HostError::BadBuffer {
            reason: "buffer type does not match world depth",
        });
    }
    desc.validate(len)
}

fn check_src(src_len: usize, src_stride: usize, desc: &WorldDesc) -> Result<()> {
    if src_stride < 4 * desc.width as usize {
        return Err(HostError::BadBuffer {
            reason: "source stride smaller than a row",
        });
    }
    if desc.height > 0
        && src_len < src_stride * (desc.height as usize - 1) + 4 * desc.width as usize
    {
        return Err(HostError::BadBuffer {
            reason: "source shorter than height × stride",
        });
    }
    Ok(())
}

/// Encode a whole world on the calling thread (validated).
pub fn encode<S: BaeSample>(
    src: &[S],
    src_stride: usize,
    desc: &WorldDesc,
    mut dst: WorldBufMut<'_>,
    policy: FloatPolicy,
) -> Result<()> {
    check_dst(desc, &dst)?;
    check_src(src.len(), src_stride, desc)?;
    encode_rows(
        src,
        src_stride,
        desc,
        &mut dst,
        0..desc.height as usize,
        policy,
    );
    Ok(())
}

/// Decode a whole world on the calling thread (validated).
pub fn decode<S: BaeSample>(
    src: WorldBuf<'_>,
    desc: &WorldDesc,
    dst: &mut [S],
    dst_stride: usize,
) -> Result<()> {
    let (depth, len) = match src {
        WorldBuf::U8(b) => (PixelDepth::U8, b.len()),
        WorldBuf::U16(b) => (PixelDepth::U16, b.len() * 2),
        WorldBuf::F32(b) => (PixelDepth::F32, b.len() * 4),
    };
    if depth != desc.depth {
        return Err(HostError::BadBuffer {
            reason: "buffer type does not match world depth",
        });
    }
    desc.validate(len)?;
    check_src(dst.len(), dst_stride, desc)?;
    decode_rows(src, desc, dst, dst_stride, 0..desc.height as usize);
    Ok(())
}

/// Encode using `threads` scoped threads (row bands). This is what the CPU
/// fallback path costs; the production host would use BAE's render thread
/// pool instead of spawning.
pub fn encode_parallel<S: BaeSample>(
    src: &[S],
    src_stride: usize,
    desc: &WorldDesc,
    dst: WorldBufMut<'_>,
    policy: FloatPolicy,
    threads: usize,
) -> Result<()> {
    check_dst(desc, &dst)?;
    check_src(src.len(), src_stride, desc)?;
    let h = desc.height as usize;
    let threads = threads.clamp(1, h.max(1));
    let band = h.div_ceil(threads);
    let ds = desc.stride_elems();
    std::thread::scope(|scope| {
        macro_rules! split {
            ($buf:expr, $variant:ident) => {{
                for (i, chunk) in $buf.chunks_mut(band * ds).enumerate() {
                    let start = i * band;
                    let end = (start + band).min(h);
                    scope.spawn(move || {
                        let mut d = WorldBufMut::$variant(chunk);
                        encode_rows(src, src_stride, desc, &mut d, start..end, policy);
                    });
                }
            }};
        }
        match dst {
            WorldBufMut::U8(b) => split!(b, U8),
            WorldBufMut::U16(b) => split!(b, U16),
            WorldBufMut::F32(b) => split!(b, F32),
        }
    });
    Ok(())
}

/// Decode using `threads` scoped threads (row bands).
pub fn decode_parallel<S: BaeSample>(
    src: WorldBuf<'_>,
    desc: &WorldDesc,
    dst: &mut [S],
    dst_stride: usize,
    threads: usize,
) -> Result<()> {
    // Validate once with the sequential checker's rules.
    let (depth, len) = match src {
        WorldBuf::U8(b) => (PixelDepth::U8, b.len()),
        WorldBuf::U16(b) => (PixelDepth::U16, b.len() * 2),
        WorldBuf::F32(b) => (PixelDepth::F32, b.len() * 4),
    };
    if depth != desc.depth {
        return Err(HostError::BadBuffer {
            reason: "buffer type does not match world depth",
        });
    }
    desc.validate(len)?;
    check_src(dst.len(), dst_stride, desc)?;
    let h = desc.height as usize;
    let threads = threads.clamp(1, h.max(1));
    let band = h.div_ceil(threads);
    std::thread::scope(|scope| {
        for (i, chunk) in dst.chunks_mut(band * dst_stride).enumerate() {
            let start = i * band;
            let end = (start + band).min(h);
            scope.spawn(move || decode_rows(src, desc, chunk, dst_stride, start..end));
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f16_roundtrip_all_values() {
        for h in 0..=u16::MAX {
            let f = f16_to_f32(h);
            if f.is_nan() {
                assert!(f16_to_f32(f32_to_f16(f)).is_nan());
                continue;
            }
            assert_eq!(f32_to_f16(f), h, "bits {h:#06x}");
        }
    }

    #[test]
    fn f16_rounding() {
        assert_eq!(f32_to_f16(1.0), 0x3c00);
        assert_eq!(f32_to_f16(65504.0), 0x7bff);
        assert_eq!(f32_to_f16(65520.0), 0x7c00); // tie → even → overflow
        assert_eq!(f32_to_f16(0.1), 0x2e66);
        assert_eq!(f32_to_f16(2f32.powi(-24)), 0x0001);
        assert_eq!(f32_to_f16(2f32.powi(-25)), 0x0000); // tie → even (0)
        assert_eq!(f32_to_f16(3.0 * 2f32.powi(-26)), 0x0001);
        assert_eq!(f32_to_f16(-2.0), 0xc000);
        assert_eq!(f32_to_f16(f32::INFINITY), 0x7c00);
        assert_eq!(f32_to_f16(1e-30), 0);
    }

    fn sample_rgba(w: usize, h: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let a = ((x + y) % 7) as f32 / 6.0;
                v.extend_from_slice(&[
                    a * (x % 5) as f32 / 4.0,
                    a * 0.5,
                    a * (y % 3) as f32 / 2.0,
                    a,
                ]);
            }
        }
        v
    }

    #[test]
    fn layout_and_rowbytes() {
        let d = WorldDesc::for_rect(LRect::from_xywh(-3, 4, 1000, 2), PixelDepth::U16, true);
        assert_eq!(d.rowbytes, 8192);
        assert_eq!((d.origin_x, d.origin_y), (-3, 4));
        assert_eq!(d.byte_len(), 8192 + 8000);
        assert!(d.validate(d.byte_len()).is_ok());
        assert!(d.validate(d.byte_len() - 1).is_err());
        assert_eq!(ChannelOrder::Argb.positions(), [1, 2, 3, 0]);
    }

    #[test]
    fn argb8_exact_and_ordered() {
        let (w, h) = (13, 5);
        let src = sample_rgba(w, h);
        let desc = WorldDesc::for_rect(
            LRect::from_xywh(0, 0, w as i32, h as i32),
            PixelDepth::U8,
            true,
        );
        let mut buf = vec![0u8; desc.byte_len()];
        encode(
            &src,
            4 * w,
            &desc,
            WorldBufMut::U8(&mut buf),
            FloatPolicy::ZeroNan,
        )
        .unwrap();
        // Pixel (1,0): rgba = src[4..8]; memory must be A,R,G,B.
        let p = &buf[4..8];
        assert_eq!(p[0], to_u8(src[7]));
        assert_eq!(p[1], to_u8(src[4]));
        assert_eq!(p[3], to_u8(src[6]));
        // 8-bit values survive a trip through half floats exactly.
        let mut half = vec![F16(0); w * h * 4];
        decode(WorldBuf::U8(&buf), &desc, &mut half, 4 * w).unwrap();
        let mut buf2 = vec![0u8; desc.byte_len()];
        encode(
            &half,
            4 * w,
            &desc,
            WorldBufMut::U8(&mut buf2),
            FloatPolicy::ZeroNan,
        )
        .unwrap();
        assert_eq!(buf, buf2);
    }

    #[test]
    fn argb16_scale_is_32768() {
        let desc = WorldDesc::for_rect(LRect::from_xywh(0, 0, 1, 1), PixelDepth::U16, true);
        let mut buf = vec![0u16; desc.byte_len() / 2];
        encode(
            &[1.0f32, 0.5, 0.0, 2.0],
            4,
            &desc,
            WorldBufMut::U16(&mut buf),
            FloatPolicy::ZeroNan,
        )
        .unwrap();
        assert_eq!(&buf[..4], &[32768, 32768, 16384, 0]);
        // Invalid >32768 values clamp to white on decode.
        let mut out = [0f32; 4];
        decode(WorldBuf::U16(&[40000, 32768, 0, 16384]), &desc, &mut out, 4).unwrap();
        assert_eq!(out, [1.0, 0.0, 0.5, 1.0]);
    }

    #[test]
    fn argb16_through_f32_is_lossless_but_f16_is_not() {
        let w = 4096usize;
        let desc = WorldDesc::for_rect(LRect::from_xywh(0, 0, w as i32, 1), PixelDepth::U16, true);
        let world: Vec<u16> = (0..w * 4).map(|i| (i * 8 % 32769) as u16).collect();
        let mut f = vec![0f32; w * 4];
        decode(WorldBuf::U16(&world), &desc, &mut f, 4 * w).unwrap();
        let mut back = vec![0u16; w * 4];
        encode(
            &f,
            4 * w,
            &desc,
            WorldBufMut::U16(&mut back),
            FloatPolicy::Passthrough,
        )
        .unwrap();
        assert_eq!(world, back);
        let mut half = vec![F16(0); w * 4];
        decode(WorldBuf::U16(&world), &desc, &mut half, 4 * w).unwrap();
        encode(
            &half,
            4 * w,
            &desc,
            WorldBufMut::U16(&mut back),
            FloatPolicy::Passthrough,
        )
        .unwrap();
        let max_err = world
            .iter()
            .zip(&back)
            .map(|(a, b)| (*a as i32 - *b as i32).abs())
            .max()
            .unwrap();
        assert!(
            max_err > 0 && max_err <= 8,
            "half-float quantisation error {max_err}"
        );
    }

    #[test]
    fn float_world_policies_and_gpu_order() {
        let mut desc = WorldDesc::for_rect(LRect::from_xywh(0, 0, 1, 1), PixelDepth::F32, true);
        let mut buf = vec![0f32; desc.byte_len() / 4];
        encode(
            &[f32::NAN, 2.5, -1.0, 1.0],
            4,
            &desc,
            WorldBufMut::F32(&mut buf),
            FloatPolicy::ZeroNan,
        )
        .unwrap();
        assert_eq!(&buf[..4], &[1.0, 0.0, 2.5, -1.0]); // A,R,G,B
        desc.order = ChannelOrder::Bgra;
        encode(
            &[0.1f32, 0.2, 0.3, 0.4],
            4,
            &desc,
            WorldBufMut::F32(&mut buf),
            FloatPolicy::ZeroNan,
        )
        .unwrap();
        assert_eq!(&buf[..4], &[0.3, 0.2, 0.1, 0.4]); // B,G,R,A
    }

    #[test]
    fn parallel_matches_sequential() {
        let (w, h) = (257, 61);
        let src: Vec<F16> = sample_rgba(w, h).into_iter().map(F16::store).collect();
        for depth in [PixelDepth::U8, PixelDepth::U16, PixelDepth::F32] {
            let desc = WorldDesc::for_rect(LRect::from_xywh(0, 0, w as i32, h as i32), depth, true);
            let n = desc.byte_len() / depth.bytes_per_channel();
            macro_rules! check {
                ($t:ty, $v:ident) => {{
                    let mut a = vec![<$t>::default(); n];
                    let mut b = vec![<$t>::default(); n];
                    encode(
                        &src,
                        4 * w,
                        &desc,
                        WorldBufMut::$v(&mut a),
                        FloatPolicy::ZeroNan,
                    )
                    .unwrap();
                    encode_parallel(
                        &src,
                        4 * w,
                        &desc,
                        WorldBufMut::$v(&mut b),
                        FloatPolicy::ZeroNan,
                        7,
                    )
                    .unwrap();
                    assert_eq!(a, b);
                    let mut da = vec![F16(0); w * h * 4];
                    let mut db = vec![F16(0); w * h * 4];
                    decode(WorldBuf::$v(&a), &desc, &mut da, 4 * w).unwrap();
                    decode_parallel(WorldBuf::$v(&a), &desc, &mut db, 4 * w, 5).unwrap();
                    assert_eq!(da, db);
                }};
            }
            match depth {
                PixelDepth::U8 => check!(u8, U8),
                PixelDepth::U16 => check!(u16, U16),
                PixelDepth::F32 => check!(f32, F32),
            }
        }
    }

    #[test]
    fn mismatched_buffers_are_rejected() {
        let desc = WorldDesc::for_rect(LRect::from_xywh(0, 0, 8, 8), PixelDepth::U16, true);
        let mut small = vec![0u8; 16];
        assert!(
            encode(
                &[0f32; 256],
                32,
                &desc,
                WorldBufMut::U8(&mut small),
                FloatPolicy::ZeroNan
            )
            .is_err()
        );
        let mut ok = vec![0u16; desc.byte_len() / 2];
        assert!(
            encode(
                &[0f32; 10],
                32,
                &desc,
                WorldBufMut::U16(&mut ok),
                FloatPolicy::ZeroNan
            )
            .is_err()
        );
    }
}
