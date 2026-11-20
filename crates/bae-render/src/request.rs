//! How to render a frame: resolution, region of interest, bit depth, colour.

use std::hash::{Hash, Hasher};

use bae_gpu::wgpu;

use crate::geom::IRect;

/// Preview resolution (AE: Full, Half, Third, Quarter, Custom).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Resolution {
    /// 完整
    #[default]
    Full,
    /// 二分之一
    Half,
    /// 三分之一
    Third,
    /// 四分之一
    Quarter,
    /// Every `n`th pixel (AE "Custom"); `0` and `1` mean full.
    Custom(u32),
}

impl Resolution {
    /// The downsample factor `n` (render at `1/n` of the comp size).
    pub fn factor(self) -> u32 {
        match self {
            Resolution::Full => 1,
            Resolution::Half => 2,
            Resolution::Third => 3,
            Resolution::Quarter => 4,
            Resolution::Custom(n) => n.max(1),
        }
    }

    /// `1 / factor`.
    pub fn scale(self) -> f64 {
        1.0 / f64::from(self.factor())
    }

    /// Output size for a comp of `width × height` (rounded up, at least 1×1).
    pub fn scaled_size(self, width: u32, height: u32) -> [u32; 2] {
        let n = self.factor();
        [width.div_ceil(n).max(1), height.div_ceil(n).max(1)]
    }
}

/// Project colour depth. 8 and 16 bpc make the float pipeline clamp to `[0, 1]`
/// at AE's clamping points (AE-CMP-005), so results match an 8/16-bpc AE project.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BitDepth {
    /// 8 bpc (AE default): clamp.
    #[default]
    Bpc8,
    /// 16 bpc: clamp.
    Bpc16,
    /// 32 bpc float: no clamping between steps.
    Float32,
}

impl BitDepth {
    /// True when results are clamped to `[0, 1]` after each blend.
    pub fn clamps(self) -> bool {
        !matches!(self, BitDepth::Float32)
    }
}

/// GPU format of the working buffers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WorkFormat {
    /// RGBA16F, premultiplied (default; ARCHITECTURE §5).
    #[default]
    Rgba16Float,
    /// RGBA32F, premultiplied (optional high-precision mode).
    Rgba32Float,
}

impl WorkFormat {
    /// The wgpu texture format.
    pub fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            WorkFormat::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
            WorkFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
        }
    }
}

/// How blending treats colour values. Colour *conversion* (input/working/display
/// spaces, OCIO) belongs to workstream 06 (`bae-color`); the compositor only needs
/// to know whether blending happens on encoded or linear values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendSpace {
    /// Blend the working space's encoded values (AE default, non-linear).
    #[default]
    Encoded,
    /// Blend in linear light ("Blend Colors Using 1.0 Gamma" / "Linearize Working
    /// Space"). Reserved: needs 06's transfer functions; renders as `Encoded`.
    Linear,
}

/// Colour context of a render (filled in by 06 later).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ColorContext {
    /// Blending space.
    pub blend: BlendSpace,
}

/// Why a frame is rendered. Final renders exclude guide layers (evaluator) and
/// never use draft shortcuts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RenderPurpose {
    /// Interactive preview (viewer, RAM preview).
    #[default]
    Preview,
    /// Render queue / `bae render` / agent screenshots.
    Final,
}

/// Parameters of one frame render besides the evaluated comp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderRequest {
    /// Downsampling.
    pub resolution: Resolution,
    /// Region of interest in full-resolution comp pixels; `None` = whole comp.
    /// The output covers `roi ∩ comp` (scaled by the resolution).
    pub roi: Option<IRect>,
    /// Colour depth semantics.
    pub depth: BitDepth,
    /// Working buffer format.
    pub format: WorkFormat,
    /// Colour context.
    pub color: ColorContext,
    /// Preview or final.
    pub purpose: RenderPurpose,
}

impl RenderRequest {
    /// Full resolution, whole comp, 8 bpc, RGBA16F.
    pub fn full() -> Self {
        Self::default()
    }

    /// Hash of the parameters that change the output pixels (the frame cache key).
    pub fn fingerprint(&self) -> u64 {
        let mut h = rustc_hash::FxHasher::default();
        self.resolution.hash(&mut h);
        self.roi.hash(&mut h);
        self.depth.hash(&mut h);
        self.format.hash(&mut h);
        self.color.hash(&mut h);
        self.purpose.hash(&mut h);
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_sizes() {
        assert_eq!(Resolution::Full.scaled_size(1920, 1080), [1920, 1080]);
        assert_eq!(Resolution::Half.scaled_size(1920, 1080), [960, 540]);
        assert_eq!(Resolution::Third.scaled_size(1920, 1080), [640, 360]);
        assert_eq!(Resolution::Quarter.scaled_size(1919, 1079), [480, 270]);
        assert_eq!(Resolution::Custom(0).factor(), 1);
    }

    #[test]
    fn fingerprint_changes_with_output_parameters() {
        let a = RenderRequest::full();
        let b = RenderRequest {
            resolution: Resolution::Half,
            ..a
        };
        assert_ne!(a.fingerprint(), b.fingerprint());
        assert_eq!(a.fingerprint(), RenderRequest::full().fingerprint());
    }
}
