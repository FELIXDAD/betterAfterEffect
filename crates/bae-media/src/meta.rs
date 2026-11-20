//! Media metadata: what a file *is*, read without decoding pixels.
//!
//! Color tags are reported, not applied: interpreting them (and the user's
//! "Interpret Footage" overrides) is `bae-color`'s job (workstream 06). Tags
//! follow ITU-T H.273 (CICP) code points, the vocabulary shared by PNG `cICP`,
//! H.264 / HEVC VUI, CoreVideo and Media Foundation.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::time::{FrameRate, Time, Timecode};

/// What kind of media a source is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaKind {
    /// A single image; every frame index returns the same picture.
    Still,
    /// Numbered image files (`name_0001.png …`), one file per frame.
    ImageSequence,
    /// A movie file (video track, optionally audio).
    Movie,
    /// Audio only (reserved: decoding is a later PR).
    Audio,
}

/// Color primaries (H.273 table 2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum ColorPrimaries {
    /// Not tagged; `bae-color` applies its default rule (sRGB / Rec.709).
    #[default]
    Unspecified,
    /// BT.709 / sRGB (1).
    Bt709,
    /// BT.470 System M (4).
    Bt470M,
    /// BT.601 625-line / PAL (5).
    Bt601Pal,
    /// BT.601 525-line / NTSC, SMPTE 170M (6, 7).
    Bt601Ntsc,
    /// Generic film (8).
    Film,
    /// BT.2020 / BT.2100 (9).
    Bt2020,
    /// CIE 1931 XYZ (10).
    Xyz,
    /// DCI-P3 theatrical white (11).
    DciP3,
    /// Display P3, D65 white (12).
    DisplayP3,
    /// ACES AP0 (EXR files written by ACES tools; no CICP code point).
    AcesAp0,
    /// ACES AP1 / ACEScg (no CICP code point).
    AcesAp1,
    /// Explicit chromaticities (EXR `chromaticities`, PNG `cHRM`).
    Custom(Chromaticities),
}

/// CIE xy chromaticities of the primaries and the white point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chromaticities {
    /// Red primary (x, y).
    pub red: [f32; 2],
    /// Green primary (x, y).
    pub green: [f32; 2],
    /// Blue primary (x, y).
    pub blue: [f32; 2],
    /// White point (x, y).
    pub white: [f32; 2],
}

impl Chromaticities {
    /// BT.709 / sRGB primaries with D65.
    pub const BT709: Chromaticities = Chromaticities {
        red: [0.64, 0.33],
        green: [0.30, 0.60],
        blue: [0.15, 0.06],
        white: [0.3127, 0.3290],
    };
    /// ACES AP0 with the ACES white point.
    pub const ACES_AP0: Chromaticities = Chromaticities {
        red: [0.7347, 0.2653],
        green: [0.0, 1.0],
        blue: [0.0001, -0.0770],
        white: [0.32168, 0.33767],
    };
    /// ACES AP1 (ACEScg) with the ACES white point.
    pub const ACES_AP1: Chromaticities = Chromaticities {
        red: [0.713, 0.293],
        green: [0.165, 0.830],
        blue: [0.128, 0.044],
        white: [0.32168, 0.33767],
    };
    /// BT.2020 with D65.
    pub const BT2020: Chromaticities = Chromaticities {
        red: [0.708, 0.292],
        green: [0.170, 0.797],
        blue: [0.131, 0.046],
        white: [0.3127, 0.3290],
    };
    /// Display P3 (P3 primaries, D65).
    pub const DISPLAY_P3: Chromaticities = Chromaticities {
        red: [0.680, 0.320],
        green: [0.265, 0.690],
        blue: [0.150, 0.060],
        white: [0.3127, 0.3290],
    };

    fn approx_eq(&self, other: &Chromaticities) -> bool {
        let pairs = [
            (self.red, other.red),
            (self.green, other.green),
            (self.blue, other.blue),
            (self.white, other.white),
        ];
        pairs
            .iter()
            .all(|(a, b)| (a[0] - b[0]).abs() < 0.002 && (a[1] - b[1]).abs() < 0.002)
    }
}

impl ColorPrimaries {
    /// From an H.273 `ColourPrimaries` code point.
    pub fn from_cicp(code: u8) -> Self {
        match code {
            1 => Self::Bt709,
            4 => Self::Bt470M,
            5 => Self::Bt601Pal,
            6 | 7 => Self::Bt601Ntsc,
            8 => Self::Film,
            9 => Self::Bt2020,
            10 => Self::Xyz,
            11 => Self::DciP3,
            12 => Self::DisplayP3,
            _ => Self::Unspecified,
        }
    }

    /// The H.273 code point (2 = unspecified for values without one).
    pub fn to_cicp(self) -> u8 {
        match self {
            Self::Bt709 => 1,
            Self::Bt470M => 4,
            Self::Bt601Pal => 5,
            Self::Bt601Ntsc => 6,
            Self::Film => 8,
            Self::Bt2020 => 9,
            Self::Xyz => 10,
            Self::DciP3 => 11,
            Self::DisplayP3 => 12,
            Self::Unspecified | Self::AcesAp0 | Self::AcesAp1 | Self::Custom(_) => 2,
        }
    }

    /// Recognizes well-known chromaticities, otherwise keeps them as `Custom`.
    pub fn from_chromaticities(c: Chromaticities) -> Self {
        if c.approx_eq(&Chromaticities::BT709) {
            Self::Bt709
        } else if c.approx_eq(&Chromaticities::ACES_AP0) {
            Self::AcesAp0
        } else if c.approx_eq(&Chromaticities::ACES_AP1) {
            Self::AcesAp1
        } else if c.approx_eq(&Chromaticities::BT2020) {
            Self::Bt2020
        } else if c.approx_eq(&Chromaticities::DISPLAY_P3) {
            Self::DisplayP3
        } else {
            Self::Custom(c)
        }
    }
}

/// Transfer characteristics (H.273 table 3), plus a pure power gamma.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum TransferFunction {
    /// Not tagged.
    #[default]
    Unspecified,
    /// BT.709 / BT.601 / BT.2020 camera OETF (1, 6, 14, 15).
    Bt709,
    /// IEC 61966-2-1 sRGB (13).
    Srgb,
    /// Linear light (8). EXR is linear by convention.
    Linear,
    /// SMPTE ST 2084 PQ (16).
    Pq,
    /// ARIB STD-B67 HLG (18).
    Hlg,
    /// Pure power law with this decoding exponent (PNG `gAMA` = 1 / exponent,
    /// H.273 4 = 2.2, 5 = 2.8).
    Gamma(f32),
}

impl TransferFunction {
    /// From an H.273 `TransferCharacteristics` code point.
    pub fn from_cicp(code: u8) -> Self {
        match code {
            1 | 6 | 14 | 15 => Self::Bt709,
            4 => Self::Gamma(2.2),
            5 => Self::Gamma(2.8),
            8 => Self::Linear,
            13 => Self::Srgb,
            16 => Self::Pq,
            18 => Self::Hlg,
            _ => Self::Unspecified,
        }
    }

    /// The H.273 code point (2 = unspecified for values without one).
    pub fn to_cicp(self) -> u8 {
        match self {
            Self::Bt709 => 1,
            Self::Gamma(g) if (g - 2.2).abs() < 0.01 => 4,
            Self::Gamma(g) if (g - 2.8).abs() < 0.01 => 5,
            Self::Linear => 8,
            Self::Srgb => 13,
            Self::Pq => 16,
            Self::Hlg => 18,
            Self::Unspecified | Self::Gamma(_) => 2,
        }
    }
}

/// Y'CbCr ↔ R'G'B' matrix (H.273 table 4). Only meaningful for YUV-coded video;
/// the decoder applies it, so GPU frames are always RGB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MatrixCoefficients {
    /// RGB / not YUV-coded (0).
    Identity,
    /// Not tagged (2); decoders assume BT.709 for HD and BT.601 for SD.
    #[default]
    Unspecified,
    /// BT.709 (1).
    Bt709,
    /// BT.601 (5, 6).
    Bt601,
    /// BT.2020 non-constant luminance (9).
    Bt2020Ncl,
    /// BT.2020 constant luminance (10).
    Bt2020Cl,
}

impl MatrixCoefficients {
    /// From an H.273 `MatrixCoefficients` code point.
    pub fn from_cicp(code: u8) -> Self {
        match code {
            0 => Self::Identity,
            1 => Self::Bt709,
            5 | 6 => Self::Bt601,
            9 => Self::Bt2020Ncl,
            10 => Self::Bt2020Cl,
            _ => Self::Unspecified,
        }
    }

    /// The H.273 code point.
    pub fn to_cicp(self) -> u8 {
        match self {
            Self::Identity => 0,
            Self::Bt709 => 1,
            Self::Unspecified => 2,
            Self::Bt601 => 6,
            Self::Bt2020Ncl => 9,
            Self::Bt2020Cl => 10,
        }
    }
}

/// Code-value range of the source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColorRange {
    /// 0–255 (images, full-range video).
    #[default]
    Full,
    /// 16–235 / 64–940 "video" range. Decoders expand it, so GPU frames are full range.
    Limited,
}

/// The color tags of a source, as found in the file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ColorInfo {
    /// Primaries.
    pub primaries: ColorPrimaries,
    /// Transfer function.
    pub transfer: TransferFunction,
    /// YUV matrix (video).
    pub matrix: MatrixCoefficients,
    /// Code-value range.
    pub range: ColorRange,
    /// Embedded ICC profile, if any. Not serialized (can be large); its presence
    /// is enough for the project file, `bae-color` re-reads it from the source.
    #[serde(skip)]
    pub icc_profile: Option<Arc<[u8]>>,
}

impl ColorInfo {
    /// sRGB-tagged RGB.
    pub fn srgb() -> Self {
        ColorInfo {
            primaries: ColorPrimaries::Bt709,
            transfer: TransferFunction::Srgb,
            matrix: MatrixCoefficients::Identity,
            range: ColorRange::Full,
            icc_profile: None,
        }
    }

    /// Scene-linear BT.709 primaries (EXR without a `chromaticities` attribute).
    pub fn linear_bt709() -> Self {
        ColorInfo {
            primaries: ColorPrimaries::Bt709,
            transfer: TransferFunction::Linear,
            matrix: MatrixCoefficients::Identity,
            range: ColorRange::Full,
            icc_profile: None,
        }
    }

    /// From H.273 code points.
    pub fn from_cicp(primaries: u8, transfer: u8, matrix: u8, full_range: bool) -> Self {
        ColorInfo {
            primaries: ColorPrimaries::from_cicp(primaries),
            transfer: TransferFunction::from_cicp(transfer),
            matrix: MatrixCoefficients::from_cicp(matrix),
            range: if full_range {
                ColorRange::Full
            } else {
                ColorRange::Limited
            },
            icc_profile: None,
        }
    }

    /// `true` when the transfer function is high dynamic range (PQ / HLG) or
    /// scene-linear.
    pub fn is_hdr_or_linear(&self) -> bool {
        matches!(
            self.transfer,
            TransferFunction::Pq | TransferFunction::Hlg | TransferFunction::Linear
        )
    }
}

/// How the alpha channel relates to the color channels (AE: Interpret Footage →
/// Alpha).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlphaMode {
    /// No alpha channel; the image is opaque.
    #[default]
    Opaque,
    /// Straight (unassociated) alpha: color is independent of alpha (PNG, WebP,
    /// most ProRes 4444).
    Straight,
    /// Premultiplied (associated) alpha with a black matte (EXR, renders).
    Premultiplied,
    /// The source has an alpha channel but it is to be ignored (AE "Ignore").
    Ignore,
}

impl AlphaMode {
    /// `true` if the source carries an alpha channel worth keeping.
    pub fn has_alpha(self) -> bool {
        matches!(self, Self::Straight | Self::Premultiplied)
    }
}

/// Pixel aspect ratio as a rational (width of a pixel / its height).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PixelAspect {
    /// Numerator.
    pub num: u32,
    /// Denominator.
    pub den: u32,
}

impl PixelAspect {
    /// Square pixels.
    pub const SQUARE: PixelAspect = PixelAspect { num: 1, den: 1 };

    /// `num / den`, reduced; zero components fall back to square.
    pub fn new(num: u32, den: u32) -> Self {
        if num == 0 || den == 0 {
            return Self::SQUARE;
        }
        let g = gcd(num, den);
        PixelAspect {
            num: num / g,
            den: den / g,
        }
    }

    /// From a float ratio (EXR `pixelAspectRatio`), with up to 1/10000 precision.
    pub fn from_f32(ratio: f32) -> Self {
        if !ratio.is_finite() || ratio <= 0.0 || (ratio - 1.0).abs() < 1e-4 {
            return Self::SQUARE;
        }
        Self::new((ratio * 10_000.0).round() as u32, 10_000)
    }

    /// The ratio as a float.
    pub fn as_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den.max(1))
    }

    /// `true` for square pixels.
    pub fn is_square(self) -> bool {
        self.num == self.den
    }
}

impl Default for PixelAspect {
    fn default() -> Self {
        Self::SQUARE
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

/// Numeric type of the stored samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SampleType {
    /// Unsigned integer of `bit_depth` bits (8, 10, 12, 16).
    UInt,
    /// IEEE half float.
    Half,
    /// IEEE single float.
    Float,
}

/// Description of the picture of a source (stills, sequences and video tracks).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VideoInfo {
    /// Width in stored pixels.
    pub width: u32,
    /// Height in stored pixels.
    pub height: u32,
    /// Pixel aspect ratio.
    pub pixel_aspect: PixelAspect,
    /// Native frame rate. `None` for stills; for sequences it is the import rate
    /// (AE default 30 fps, configurable) since files carry none.
    pub frame_rate: Option<FrameRate>,
    /// Number of frames. `None` for stills (any frame index is valid).
    pub frame_count: Option<u64>,
    /// Bits per sample as stored (8, 10, 12, 16, 32).
    pub bit_depth: u8,
    /// Sample type as stored.
    pub sample_type: SampleType,
    /// Color tags.
    pub color: ColorInfo,
    /// Alpha interpretation.
    pub alpha: AlphaMode,
    /// Codec / compression name (`"png"`, `"h264"`, `"prores4444"`, `"exr/piz"` …).
    pub codec: String,
}

impl VideoInfo {
    /// Duration, if the source has a frame rate and a frame count.
    pub fn duration(&self) -> Option<Time> {
        let rate = self.frame_rate?;
        let count = i64::try_from(self.frame_count?).ok()?;
        Some(rate.frame_to_time(count))
    }
}

/// Audio stream description (reserved: audio decoding with symphonia lands in a
/// later PR, see the workstream doc).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioInfo {
    /// Samples per second.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: u16,
    /// Length in sample frames, if known.
    pub frame_count: Option<u64>,
    /// Codec name (`"aac"`, `"pcm_s24le"` …).
    pub codec: String,
}

/// Information about one image-sequence source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SequenceInfo {
    /// File-name pattern with `#` per padded digit, e.g. `shot_####.exr`.
    pub pattern: String,
    /// First frame number on disk (maps to frame index 0).
    pub first: i64,
    /// Last frame number on disk.
    pub last: i64,
    /// Frame numbers inside `first..=last` that have no file.
    pub missing: Vec<i64>,
}

/// Everything known about a media file after probing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    /// The file that was opened (for sequences: the first frame).
    pub path: PathBuf,
    /// Still, sequence, movie or audio.
    pub kind: MediaKind,
    /// Format id of the [`MediaFormat`](crate::MediaFormat) that opened it.
    pub format: String,
    /// The picture (absent for audio-only files).
    pub video: Option<VideoInfo>,
    /// Audio streams (reserved; empty until audio decoding lands).
    pub audio: Vec<AudioInfo>,
    /// Start timecode (EXR `timeCode`, QuickTime `tmcd`), if present.
    pub timecode: Option<Timecode>,
    /// Sequence details, for [`MediaKind::ImageSequence`].
    pub sequence: Option<SequenceInfo>,
}

impl MediaInfo {
    /// Attribute values for the project's `Asset` prim, named as in
    /// `bae_model::schema` (`kind`, `width`, `height`, `frame_rate`,
    /// `duration`, `pixel_aspect`, `alpha_mode`, `has_video`, `has_audio`,
    /// `sequence:first_frame`, `sequence:pattern`). Importers write them through
    /// model commands. Attributes the file does not determine are left out
    /// (stills have no rate or duration, opaque sources no alpha mode);
    /// `file:*` (paths, fingerprint) belong to the importer and `color_space` to
    /// `bae-color`'s naming.
    pub fn asset_attributes(&self) -> Vec<(&'static str, bae_model::Value)> {
        use bae_model::Value;
        let mut attrs = vec![(
            "kind",
            Value::Enum(
                if self.kind == MediaKind::ImageSequence {
                    "sequence"
                } else {
                    "footage"
                }
                .into(),
            ),
        )];
        attrs.push(("has_video", Value::Bool(self.video.is_some())));
        attrs.push(("has_audio", Value::Bool(!self.audio.is_empty())));
        if let Some(video) = &self.video {
            attrs.push(("width", Value::Int(i64::from(video.width))));
            attrs.push(("height", Value::Int(i64::from(video.height))));
            attrs.push(("pixel_aspect", Value::Float(video.pixel_aspect.as_f64())));
            if self.kind != MediaKind::Still {
                if let Some(rate) = video.frame_rate {
                    attrs.push(("frame_rate", Value::FrameRate(rate)));
                }
                if let Some(duration) = video.duration() {
                    attrs.push(("duration", Value::Time(duration)));
                }
            }
            let alpha = match video.alpha {
                AlphaMode::Straight => Some("straight"),
                AlphaMode::Premultiplied => Some("premultiplied"),
                AlphaMode::Ignore => Some("ignore"),
                AlphaMode::Opaque => None,
            };
            if let Some(alpha) = alpha {
                attrs.push(("alpha_mode", Value::Enum(alpha.into())));
            }
        }
        if let Some(sequence) = &self.sequence {
            attrs.push(("sequence:first_frame", Value::Int(sequence.first)));
            attrs.push(("sequence:pattern", Value::String(sequence.pattern.clone())));
        }
        attrs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cicp_round_trip() {
        for code in [1_u8, 4, 5, 6, 8, 9, 10, 11, 12] {
            assert_eq!(ColorPrimaries::from_cicp(code).to_cicp(), code);
        }
        for code in [1_u8, 4, 5, 8, 13, 16, 18] {
            assert_eq!(TransferFunction::from_cicp(code).to_cicp(), code);
        }
        for code in [0_u8, 1, 2, 6, 9, 10] {
            assert_eq!(MatrixCoefficients::from_cicp(code).to_cicp(), code);
        }
        let hlg = ColorInfo::from_cicp(9, 18, 9, false);
        assert_eq!(hlg.primaries, ColorPrimaries::Bt2020);
        assert_eq!(hlg.range, ColorRange::Limited);
        assert!(hlg.is_hdr_or_linear());
    }

    #[test]
    fn chromaticities_are_recognized() {
        assert_eq!(
            ColorPrimaries::from_chromaticities(Chromaticities::ACES_AP1),
            ColorPrimaries::AcesAp1
        );
        let odd = Chromaticities {
            white: [0.33, 0.33],
            ..Chromaticities::BT709
        };
        assert_eq!(
            ColorPrimaries::from_chromaticities(odd),
            ColorPrimaries::Custom(odd)
        );
    }

    #[test]
    fn pixel_aspect() {
        assert_eq!(PixelAspect::new(10, 11), PixelAspect { num: 10, den: 11 });
        assert_eq!(PixelAspect::new(40, 44), PixelAspect::new(10, 11));
        assert!(PixelAspect::from_f32(1.0).is_square());
        assert_eq!(PixelAspect::from_f32(2.0), PixelAspect::new(2, 1));
        assert!(PixelAspect::new(0, 5).is_square());
    }

    #[test]
    fn asset_attributes_match_the_schema() {
        use bae_model::{PrimType, schema};
        let info = MediaInfo {
            path: "shot_0001.exr".into(),
            kind: MediaKind::ImageSequence,
            format: "exr".into(),
            video: Some(VideoInfo {
                width: 2048,
                height: 858,
                pixel_aspect: PixelAspect::new(2, 1),
                frame_rate: Some(FrameRate::FPS_24),
                frame_count: Some(48),
                bit_depth: 16,
                sample_type: SampleType::Half,
                color: ColorInfo::linear_bt709(),
                alpha: AlphaMode::Premultiplied,
                codec: "exr/piz".into(),
            }),
            audio: Vec::new(),
            timecode: None,
            sequence: Some(SequenceInfo {
                pattern: "shot_####.exr".into(),
                first: 1,
                last: 48,
                missing: Vec::new(),
            }),
        };
        let attrs = info.asset_attributes();
        assert_eq!(attrs.len(), 11);
        for (name, value) in &attrs {
            let def = schema::attr_def(&PrimType::ASSET, name)
                .unwrap_or_else(|| panic!("{name} is not an Asset attribute"));
            assert_eq!(def.value_type, value.value_type(), "{name}");
            if !def.options.is_empty() {
                let bae_model::Value::Enum(token) = value else {
                    unreachable!()
                };
                assert!(def.options.contains(&token.as_str()), "{name} = {token}");
            }
        }
        let get = |n: &str| {
            attrs
                .iter()
                .find(|(name, _)| *name == n)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(
            get("duration"),
            Some(bae_model::Value::Time(Time::from_flicks(2 * 705_600_000)))
        );
        assert_eq!(
            get("alpha_mode"),
            Some(bae_model::Value::Enum("premultiplied".into()))
        );
    }

    #[test]
    fn info_serializes() {
        let info = VideoInfo {
            width: 4,
            height: 2,
            pixel_aspect: PixelAspect::SQUARE,
            frame_rate: Some(FrameRate::FPS_24),
            frame_count: Some(48),
            bit_depth: 8,
            sample_type: SampleType::UInt,
            color: ColorInfo::srgb(),
            alpha: AlphaMode::Straight,
            codec: "png".into(),
        };
        assert_eq!(info.duration(), Some(Time::from_flicks(2 * 705_600_000)));
        let json = serde_json::to_string(&info).unwrap();
        let back: VideoInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, info);
    }
}
