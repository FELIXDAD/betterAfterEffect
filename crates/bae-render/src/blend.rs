//! Blend modes, track matte modes and mask modes.
//!
//! Tokens are the lowercase snake_case form of the ExtendScript enum constants, the
//! convention of the AE interchange format (`docs/interop/bae-interchange.md`) and
//! of `Value::Enum` attributes in the project (`blend_mode`, `track_matte`, mask
//! `mode`). The menu order and maths follow `docs/spec/ae/compositing-3d-color.md`
//! (AE-CMP-011 … AE-CMP-034).

use std::fmt;

/// An AE layer blending mode (all 38 entries of AE's menu, in menu order).
///
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u32)]
pub enum BlendMode {
    /// 正常
    #[default]
    Normal = 0,
    /// 溶解
    Dissolve,
    /// 动态抖动溶解
    DancingDissolve,
    /// 变暗
    Darken,
    /// 相乘
    Multiply,
    /// 颜色加深
    ColorBurn,
    /// 经典颜色加深
    ClassicColorBurn,
    /// 线性加深
    LinearBurn,
    /// 深色
    DarkerColor,
    /// 相加
    Add,
    /// 变亮
    Lighten,
    /// 滤色
    Screen,
    /// 颜色减淡
    ColorDodge,
    /// 经典颜色减淡
    ClassicColorDodge,
    /// 线性减淡
    LinearDodge,
    /// 浅色
    LighterColor,
    /// 叠加
    Overlay,
    /// 柔光
    SoftLight,
    /// 强光
    HardLight,
    /// 线性光
    LinearLight,
    /// 亮光
    VividLight,
    /// 点光
    PinLight,
    /// 实色混合
    HardMix,
    /// 差值
    Difference,
    /// 经典差值
    ClassicDifference,
    /// 排除
    Exclusion,
    /// 相减
    Subtract,
    /// 相除
    Divide,
    /// 色相
    Hue,
    /// 饱和度
    Saturation,
    /// 颜色
    Color,
    /// 明度
    Luminosity,
    /// 模板 Alpha
    StencilAlpha,
    /// 模板亮度
    StencilLuma,
    /// 轮廓 Alpha
    SilhouetteAlpha,
    /// 轮廓亮度
    SilhouetteLuma,
    /// Alpha 添加
    AlphaAdd,
    /// 冷光预乘
    LuminescentPremul,
}

impl BlendMode {
    /// Every mode in AE menu order.
    pub const ALL: [BlendMode; 38] = [
        BlendMode::Normal,
        BlendMode::Dissolve,
        BlendMode::DancingDissolve,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::ColorBurn,
        BlendMode::ClassicColorBurn,
        BlendMode::LinearBurn,
        BlendMode::DarkerColor,
        BlendMode::Add,
        BlendMode::Lighten,
        BlendMode::Screen,
        BlendMode::ColorDodge,
        BlendMode::ClassicColorDodge,
        BlendMode::LinearDodge,
        BlendMode::LighterColor,
        BlendMode::Overlay,
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::LinearLight,
        BlendMode::VividLight,
        BlendMode::PinLight,
        BlendMode::HardMix,
        BlendMode::Difference,
        BlendMode::ClassicDifference,
        BlendMode::Exclusion,
        BlendMode::Subtract,
        BlendMode::Divide,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
        BlendMode::StencilAlpha,
        BlendMode::StencilLuma,
        BlendMode::SilhouetteAlpha,
        BlendMode::SilhouetteLuma,
        BlendMode::AlphaAdd,
        BlendMode::LuminescentPremul,
    ];

    /// The project token, e.g. `"linear_dodge"`.
    pub const fn token(self) -> &'static str {
        match self {
            BlendMode::Normal => "normal",
            BlendMode::Dissolve => "dissolve",
            BlendMode::DancingDissolve => "dancing_dissolve",
            BlendMode::Darken => "darken",
            BlendMode::Multiply => "multiply",
            BlendMode::ColorBurn => "color_burn",
            BlendMode::ClassicColorBurn => "classic_color_burn",
            BlendMode::LinearBurn => "linear_burn",
            BlendMode::DarkerColor => "darker_color",
            BlendMode::Add => "add",
            BlendMode::Lighten => "lighten",
            BlendMode::Screen => "screen",
            BlendMode::ColorDodge => "color_dodge",
            BlendMode::ClassicColorDodge => "classic_color_dodge",
            BlendMode::LinearDodge => "linear_dodge",
            BlendMode::LighterColor => "lighter_color",
            BlendMode::Overlay => "overlay",
            BlendMode::SoftLight => "soft_light",
            BlendMode::HardLight => "hard_light",
            BlendMode::LinearLight => "linear_light",
            BlendMode::VividLight => "vivid_light",
            BlendMode::PinLight => "pin_light",
            BlendMode::HardMix => "hard_mix",
            BlendMode::Difference => "difference",
            BlendMode::ClassicDifference => "classic_difference",
            BlendMode::Exclusion => "exclusion",
            BlendMode::Subtract => "subtract",
            BlendMode::Divide => "divide",
            BlendMode::Hue => "hue",
            BlendMode::Saturation => "saturation",
            BlendMode::Color => "color",
            BlendMode::Luminosity => "luminosity",
            BlendMode::StencilAlpha => "stencil_alpha",
            BlendMode::StencilLuma => "stencil_luma",
            BlendMode::SilhouetteAlpha => "silhouette_alpha",
            BlendMode::SilhouetteLuma => "silhouette_luma",
            BlendMode::AlphaAdd => "alpha_add",
            BlendMode::LuminescentPremul => "luminescent_premul",
        }
    }

    /// Parses a project token. Also accepts ExtendScript's misspelt
    /// `silhouete_alpha` (`BlendingMode.SILHOUETE_ALPHA`).
    pub fn from_token(token: &str) -> Option<BlendMode> {
        if token == "silhouete_alpha" {
            return Some(BlendMode::SilhouetteAlpha);
        }
        BlendMode::ALL.into_iter().find(|m| m.token() == token)
    }

    /// Whether the GPU compositor implements this menu entry.
    pub const fn is_implemented(self) -> bool {
        true
    }

    /// The mode used by the compositor.
    pub const fn effective(self) -> BlendMode {
        self
    }

    pub(crate) const fn is_stencil(self) -> bool {
        matches!(self, Self::StencilAlpha | Self::StencilLuma)
    }

    pub(crate) const fn source_over_empty(self) -> bool {
        !matches!(
            self,
            Self::Dissolve
                | Self::DancingDissolve
                | Self::StencilAlpha
                | Self::StencilLuma
                | Self::SilhouetteAlpha
                | Self::SilhouetteLuma
                | Self::LuminescentPremul
        )
    }

    /// FNV-1a over the UTF-8 layer path, independent of the target architecture.
    pub(crate) fn dissolve_seed(self, layer: &str, comp: &crate::input::RenderComp) -> u32 {
        match self {
            Self::Dissolve | Self::DancingDissolve => {
                let seed = layer.bytes().fold(2166136261u32, |hash, byte| {
                    (hash ^ u32::from(byte)).wrapping_mul(16777619)
                });
                if self == Self::DancingDissolve {
                    seed ^ comp.frame_rate.time_to_frame(comp.time) as u32
                } else {
                    seed
                }
            }
            _ => 0,
        }
    }

    /// Shader index of the mode (stable: the `repr(u32)` discriminant).
    pub const fn index(self) -> u32 {
        self as u32
    }
}

impl fmt::Display for BlendMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.token())
    }
}

/// How a layer uses its track matte (AE 23.0+ model: Alpha / Luma × Normal / Inverted).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum TrackMatteMode {
    /// Alpha 遮罩
    Alpha = 1,
    /// Alpha 反转遮罩
    AlphaInverted = 2,
    /// 亮度遮罩
    Luma = 3,
    /// 亮度反转遮罩
    LumaInverted = 4,
}

impl TrackMatteMode {
    /// The project token (`alpha`, `alpha_inverted`, `luma`, `luma_inverted`).
    pub const fn token(self) -> &'static str {
        match self {
            TrackMatteMode::Alpha => "alpha",
            TrackMatteMode::AlphaInverted => "alpha_inverted",
            TrackMatteMode::Luma => "luma",
            TrackMatteMode::LumaInverted => "luma_inverted",
        }
    }

    /// Parses a token; `no_track_matte` (and unknown tokens) give `None`.
    pub fn from_token(token: &str) -> Option<TrackMatteMode> {
        match token {
            "alpha" => Some(TrackMatteMode::Alpha),
            "alpha_inverted" => Some(TrackMatteMode::AlphaInverted),
            "luma" => Some(TrackMatteMode::Luma),
            "luma_inverted" => Some(TrackMatteMode::LumaInverted),
            _ => None,
        }
    }

    /// True for the luma variants.
    pub const fn is_luma(self) -> bool {
        matches!(self, TrackMatteMode::Luma | TrackMatteMode::LumaInverted)
    }

    /// True for the inverted variants.
    pub const fn is_inverted(self) -> bool {
        matches!(
            self,
            TrackMatteMode::AlphaInverted | TrackMatteMode::LumaInverted
        )
    }
}

/// How a mask combines with the masks above it (AE-CMP-038).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MaskMode {
    /// 无
    None,
    /// 相加
    #[default]
    Add,
    /// 相减
    Subtract,
    /// 交集
    Intersect,
    /// 变亮
    Lighten,
    /// 变暗
    Darken,
    /// 差值
    Difference,
}

impl MaskMode {
    /// The project token.
    pub const fn token(self) -> &'static str {
        match self {
            MaskMode::None => "none",
            MaskMode::Add => "add",
            MaskMode::Subtract => "subtract",
            MaskMode::Intersect => "intersect",
            MaskMode::Lighten => "lighten",
            MaskMode::Darken => "darken",
            MaskMode::Difference => "difference",
        }
    }

    /// Parses a token.
    pub fn from_token(token: &str) -> Option<MaskMode> {
        [
            MaskMode::None,
            MaskMode::Add,
            MaskMode::Subtract,
            MaskMode::Intersect,
            MaskMode::Lighten,
            MaskMode::Darken,
            MaskMode::Difference,
        ]
        .into_iter()
        .find(|m| m.token() == token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_round_trip() {
        for (i, mode) in BlendMode::ALL.into_iter().enumerate() {
            assert!(mode.is_implemented());
            assert_eq!(mode.effective(), mode);
            assert_eq!(mode.index() as usize, i, "{mode} index matches menu order");
            assert_eq!(BlendMode::from_token(mode.token()), Some(mode));
        }
        assert_eq!(
            BlendMode::from_token("silhouete_alpha"),
            Some(BlendMode::SilhouetteAlpha)
        );
        assert_eq!(BlendMode::from_token("bogus"), None);
        assert_eq!(BlendMode::Screen.effective(), BlendMode::Screen);
        for m in [
            TrackMatteMode::Alpha,
            TrackMatteMode::AlphaInverted,
            TrackMatteMode::Luma,
            TrackMatteMode::LumaInverted,
        ] {
            assert_eq!(TrackMatteMode::from_token(m.token()), Some(m));
        }
        assert_eq!(TrackMatteMode::from_token("no_track_matte"), None);
        assert_eq!(MaskMode::from_token("intersect"), Some(MaskMode::Intersect));
    }
}
