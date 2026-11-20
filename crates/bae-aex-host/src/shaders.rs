//! GPU-side world conversion shaders.
//!
//! [`CONVERT_WGSL`] converts a region of a premultiplied RGBA working texture
//! into a packed AE world in a storage buffer (`encode_*` entry points) and
//! back (`decode_*`). It is the recommended implementation of the 04 "CPU
//! node" conversion (spec request R04-02): conversion costs microseconds on
//! the GPU, the CPU never touches pixels, and only the bytes the plug-in
//! needs cross the bus. The crate does not depend on wgpu; the shader is
//! compiled and checked against the CPU reference by the round-trip
//! benchmark in `bench/roundtrip`.
//!
//! Binding contract (group 0): 0 = source texture (`texture_2d<f32>`),
//! 1 = destination `rgba16float` storage texture, 2 = output word buffer
//! (read-write storage), 3 = input word buffer (read-only storage),
//! 4 = [`ConvertParams`] uniform. Workgroup size 8×8.

use crate::pixel::{ChannelOrder, PixelDepth};

/// WGSL source of the conversion kernels.
pub const CONVERT_WGSL: &str = include_str!("shaders/convert.wgsl");

/// Workgroup edge length used by every entry point.
pub const WORKGROUP: u32 = 8;

/// Uniform block layout (`Params` in the shader); 32 bytes, std140/uniform
/// compatible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvertParams {
    /// Region origin inside the texture.
    pub origin: [i32; 2],
    /// Region size.
    pub size: [u32; 2],
    /// Packed-buffer row stride in u32 words (`rowbytes / 4`).
    pub row_words: u32,
}

impl ConvertParams {
    /// Serialize to the 32-byte little-endian uniform layout.
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut b = [0u8; 32];
        b[0..4].copy_from_slice(&self.origin[0].to_le_bytes());
        b[4..8].copy_from_slice(&self.origin[1].to_le_bytes());
        b[8..12].copy_from_slice(&self.size[0].to_le_bytes());
        b[12..16].copy_from_slice(&self.size[1].to_le_bytes());
        b[16..20].copy_from_slice(&self.row_words.to_le_bytes());
        b
    }
}

/// Entry point names for a world format.
pub fn entry_points(
    depth: PixelDepth,
    order: ChannelOrder,
) -> Option<(&'static str, &'static str)> {
    match (depth, order) {
        (PixelDepth::U8, ChannelOrder::Argb) => Some(("encode_argb8", "decode_argb8")),
        (PixelDepth::U16, ChannelOrder::Argb) => Some(("encode_argb16", "decode_argb16")),
        (PixelDepth::F32, ChannelOrder::Argb) => Some(("encode_argb32f", "decode_argb32f")),
        (PixelDepth::F32, ChannelOrder::Bgra) => Some(("encode_bgra128", "decode_bgra128")),
        _ => None,
    }
}

/// Number of workgroups to dispatch for a `w × h` region.
pub fn dispatch_size(w: u32, h: u32) -> (u32, u32) {
    (w.div_ceil(WORKGROUP), h.div_ceil(WORKGROUP))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_declares_all_entry_points() {
        for (d, o) in [
            (PixelDepth::U8, ChannelOrder::Argb),
            (PixelDepth::U16, ChannelOrder::Argb),
            (PixelDepth::F32, ChannelOrder::Argb),
            (PixelDepth::F32, ChannelOrder::Bgra),
        ] {
            let (e, dcd) = entry_points(d, o).unwrap();
            assert!(CONVERT_WGSL.contains(&format!("fn {e}(")), "{e}");
            assert!(CONVERT_WGSL.contains(&format!("fn {dcd}(")), "{dcd}");
        }
        assert!(entry_points(PixelDepth::U8, ChannelOrder::Bgra).is_none());
        assert_eq!(dispatch_size(1920, 1080), (240, 135));
        let p = ConvertParams {
            origin: [-1, 2],
            size: [3, 4],
            row_words: 64,
        };
        assert_eq!(&p.to_bytes()[0..4], &(-1i32).to_le_bytes());
    }
}
