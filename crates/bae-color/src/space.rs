//! Colour spaces and the built-in library.
//!
//! Every space knows how to reach the **reference space**, scene-linear
//! ACES2065-1 (AP0 primaries, ACES white) — the same connection space as the
//! OpenColorIO ACES configs, so built-in spaces and spaces read from a
//! `config.ocio` mix freely. A conversion `a → b` is `a.to_reference()` followed
//! by `b.from_reference()`, merged: two D65 spaces adapted with the same method
//! meet with an exact identity in the middle, so e.g. sRGB → Display P3 is a
//! plain colorimetric matrix with no white-point round trip left in it.

use std::borrow::Cow;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::ColorError;
use crate::math::{self, Affine};
use crate::primaries::{Adaptation, Chromaticities, white};
use crate::transfer::TransferFunction;
use crate::transform::{ColorOp, ColorTransform};

/// What kind of values a space holds (UI grouping, view-transform defaults,
/// AE's scene-referred compensation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    /// Linear light (lin sRGB, ACEScg, ACES2065-1, XYZ).
    SceneLinear,
    /// Logarithmic camera or grading encoding (S-Log3, LogC, ACEScct …).
    Log,
    /// SDR display / video encoding (sRGB, Rec.709, gamma 2.2, Display P3 …).
    Sdr,
    /// HDR signal encoding (PQ, HLG).
    Hdr,
    /// Not colour (normal maps, mattes, AE "Preserve RGB"): never converted.
    Data,
}

/// How a space is defined.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ColorSpaceKind {
    /// RGB primaries + white + transfer function. Reaches ACES2065-1 through
    /// `adaptation` between its white and the ACES white.
    Rgb {
        /// Primaries and white point.
        gamut: Chromaticities,
        /// Tone curve.
        transfer: TransferFunction,
        /// Chromatic adaptation to the ACES white.
        adaptation: Adaptation,
    },
    /// Defined by explicit transforms (OCIO colour spaces, XYZ, LUT-based spaces).
    #[serde(skip)]
    Transform {
        /// This space → ACES2065-1.
        to_reference: ColorTransform,
        /// ACES2065-1 → this space; `None` for one-way (input-only) spaces.
        from_reference: Option<ColorTransform>,
    },
    /// Data: conversions to or from it are the identity.
    Data,
}

/// A colour space.
///
/// Project files refer to spaces by [`ColorSpace::id`]; resolve ids with
/// [`builtin::by_id`] (or an OCIO config for `ocio:` ids).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColorSpace {
    /// Stable identifier, e.g. `srgb`, `acescg`, `ocio:ACEScct`.
    pub id: Cow<'static, str>,
    /// Display name (English; the UI localizes built-in ids through i18n).
    pub name: Cow<'static, str>,
    /// Value kind.
    pub encoding: Encoding,
    /// Definition.
    pub kind: ColorSpaceKind,
}

impl Eq for ColorSpace {}

impl Hash for ColorSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.encoding.hash(state);
        match &self.kind {
            ColorSpaceKind::Rgb {
                gamut,
                transfer,
                adaptation,
            } => {
                0u8.hash(state);
                gamut.bits().hash(state);
                transfer.hash(state);
                adaptation.hash(state);
            }
            ColorSpaceKind::Transform {
                to_reference,
                from_reference,
            } => {
                1u8.hash(state);
                to_reference.hash(state);
                from_reference.hash(state);
            }
            ColorSpaceKind::Data => 2u8.hash(state),
        }
    }
}

impl ColorSpace {
    /// An RGB space (const so the built-in library can be `const`).
    pub const fn rgb(
        id: &'static str,
        name: &'static str,
        encoding: Encoding,
        gamut: Chromaticities,
        transfer: TransferFunction,
    ) -> Self {
        ColorSpace {
            id: Cow::Borrowed(id),
            name: Cow::Borrowed(name),
            encoding,
            kind: ColorSpaceKind::Rgb {
                gamut,
                transfer,
                adaptation: Adaptation::Bradford,
            },
        }
    }

    /// An RGB space reaching the ACES white with a specific adaptation (camera
    /// vendors' IDT matrices: CAT02 for Sony, ARRI and Canon).
    pub const fn rgb_adapted(
        id: &'static str,
        name: &'static str,
        encoding: Encoding,
        gamut: Chromaticities,
        transfer: TransferFunction,
        adaptation: Adaptation,
    ) -> Self {
        ColorSpace {
            id: Cow::Borrowed(id),
            name: Cow::Borrowed(name),
            encoding,
            kind: ColorSpaceKind::Rgb {
                gamut,
                transfer,
                adaptation,
            },
        }
    }

    /// An RGB space with an owned id (custom / detected spaces).
    pub fn custom_rgb(
        id: impl Into<String>,
        name: impl Into<String>,
        gamut: Chromaticities,
        transfer: TransferFunction,
    ) -> Self {
        let encoding = encoding_for(transfer);
        ColorSpace {
            id: Cow::Owned(id.into()),
            name: Cow::Owned(name.into()),
            encoding,
            kind: ColorSpaceKind::Rgb {
                gamut,
                transfer,
                adaptation: Adaptation::Bradford,
            },
        }
    }

    /// The same space with a different tone curve (keeps primaries and white).
    /// Used for AE's "Linearize Working Space" and "Interpret As Linear Light".
    /// Returns `None` for spaces that are not primaries + curve.
    pub fn with_transfer(&self, transfer: TransferFunction) -> Option<ColorSpace> {
        let ColorSpaceKind::Rgb {
            gamut, adaptation, ..
        } = &self.kind
        else {
            return None;
        };
        let suffix = match transfer {
            t if t.is_linear() => "linear".to_string(),
            TransferFunction::Gamma(g) => format!("gamma{g}"),
            t => t.id().to_string(),
        };
        Some(ColorSpace {
            id: Cow::Owned(format!("{}~{}", self.id, suffix)),
            name: Cow::Owned(format!("{} ({})", self.name, suffix)),
            encoding: encoding_for(transfer),
            kind: ColorSpaceKind::Rgb {
                gamut: *gamut,
                transfer,
                adaptation: *adaptation,
            },
        })
    }

    /// The linear-light version of this space (`self` if it already is linear).
    pub fn linearized(&self) -> Option<ColorSpace> {
        match &self.kind {
            ColorSpaceKind::Rgb { transfer, .. } if transfer.is_linear() => Some(self.clone()),
            ColorSpaceKind::Rgb { .. } => self.with_transfer(TransferFunction::Linear),
            _ if self.encoding == Encoding::SceneLinear => Some(self.clone()),
            _ => None,
        }
    }

    /// Primaries and white, for RGB spaces.
    pub fn gamut(&self) -> Option<Chromaticities> {
        match &self.kind {
            ColorSpaceKind::Rgb { gamut, .. } => Some(*gamut),
            _ => None,
        }
    }

    /// Tone curve, for RGB spaces.
    pub fn transfer(&self) -> Option<TransferFunction> {
        match &self.kind {
            ColorSpaceKind::Rgb { transfer, .. } => Some(*transfer),
            _ => None,
        }
    }

    /// `true` for data spaces.
    pub fn is_data(&self) -> bool {
        matches!(self.kind, ColorSpaceKind::Data)
    }

    /// `true` when the space holds linear light.
    pub fn is_linear(&self) -> bool {
        self.encoding == Encoding::SceneLinear
    }

    /// Linear RGB of this space → ACES2065-1 (the matrix part only).
    fn linear_to_aces(gamut: &Chromaticities, adaptation: Adaptation) -> Affine {
        let ap0 = Chromaticities::ACES_AP0;
        let cat = adaptation.matrix(gamut.white, ap0.white);
        Affine::from_mat3(math::mul(
            &ap0.xyz_to_rgb(),
            &math::mul(&cat, &gamut.rgb_to_xyz()),
        ))
    }

    /// This space → ACES2065-1. Data spaces return the identity.
    pub fn to_reference(&self) -> ColorTransform {
        match &self.kind {
            ColorSpaceKind::Rgb {
                gamut,
                transfer,
                adaptation,
            } => ColorTransform::from_ops([
                ColorOp::Decode(*transfer),
                ColorOp::Matrix(Self::linear_to_aces(gamut, *adaptation)),
            ]),
            ColorSpaceKind::Transform { to_reference, .. } => to_reference.clone(),
            ColorSpaceKind::Data => ColorTransform::identity(),
        }
    }

    /// ACES2065-1 → this space.
    pub fn from_reference(&self) -> Result<ColorTransform, ColorError> {
        match &self.kind {
            ColorSpaceKind::Rgb {
                gamut,
                transfer,
                adaptation,
            } => {
                let m = Self::linear_to_aces(gamut, *adaptation)
                    .inverse()
                    .ok_or(ColorError::NotInvertible("degenerate primaries"))?;
                Ok(ColorTransform::from_ops([
                    ColorOp::Matrix(m),
                    ColorOp::Encode(*transfer),
                ]))
            }
            ColorSpaceKind::Transform {
                to_reference,
                from_reference,
            } => match from_reference {
                Some(t) => Ok(t.clone()),
                None => to_reference
                    .inverse()
                    .map_err(|_| ColorError::OneWay(self.id.to_string())),
            },
            ColorSpaceKind::Data => Ok(ColorTransform::identity()),
        }
    }

    /// The merged transform `self → dst`. Conversions from or to a data space are
    /// the identity (numbers preserved), as are conversions between equal spaces.
    pub fn transform_to(&self, dst: &ColorSpace) -> Result<ColorTransform, ColorError> {
        if self.is_data() || dst.is_data() || self.same_definition(dst) {
            return Ok(ColorTransform::identity());
        }
        Ok(self.to_reference().then(&dst.from_reference()?))
    }

    /// `true` when both spaces have the same definition (ids may differ).
    pub fn same_definition(&self, other: &ColorSpace) -> bool {
        self.kind == other.kind
    }
}

/// [`ColorSpace::transform_to`] as a free function.
pub fn convert(src: &ColorSpace, dst: &ColorSpace) -> Result<ColorTransform, ColorError> {
    src.transform_to(dst)
}

fn encoding_for(transfer: TransferFunction) -> Encoding {
    if transfer.is_linear() {
        Encoding::SceneLinear
    } else if transfer.is_log() {
        Encoding::Log
    } else if transfer.is_hdr() {
        Encoding::Hdr
    } else {
        Encoding::Sdr
    }
}

/// The built-in colour space library.
pub mod builtin {
    use super::*;
    use TransferFunction as T;

    /// sRGB (IEC 61966-2-1).
    pub const SRGB: ColorSpace = ColorSpace::rgb(
        "srgb",
        "sRGB",
        Encoding::Sdr,
        Chromaticities::REC709,
        T::Srgb,
    );
    /// Linear sRGB / Rec.709 primaries.
    pub const SRGB_LINEAR: ColorSpace = ColorSpace::rgb(
        "srgb-linear",
        "Linear sRGB (Rec.709)",
        Encoding::SceneLinear,
        Chromaticities::REC709,
        T::Linear,
    );
    /// Rec.709 with the BT.709 camera curve (AE "HDTV (Rec. 709)").
    pub const REC709: ColorSpace = ColorSpace::rgb(
        "rec709",
        "Rec.709 (HDTV)",
        Encoding::Sdr,
        Chromaticities::REC709,
        T::Rec709,
    );
    /// Rec.709 primaries, pure gamma 2.4 (BT.1886 with zero black; AE "Rec.709 Gamma 2.4").
    pub const REC709_GAMMA24: ColorSpace = ColorSpace::rgb(
        "rec709-gamma24",
        "Rec.709 Gamma 2.4 (BT.1886)",
        Encoding::Sdr,
        Chromaticities::REC709,
        T::Gamma(2.4),
    );
    /// Rec.709 primaries, pure gamma 2.2.
    pub const GAMMA22: ColorSpace = ColorSpace::rgb(
        "gamma22-rec709",
        "Gamma 2.2 (Rec.709)",
        Encoding::Sdr,
        Chromaticities::REC709,
        T::Gamma(2.2),
    );
    /// Rec.709 primaries, pure gamma 1.8 (AE "Legacy Macintosh RGB").
    pub const GAMMA18: ColorSpace = ColorSpace::rgb(
        "gamma18-rec709",
        "Gamma 1.8 (Rec.709)",
        Encoding::Sdr,
        Chromaticities::REC709,
        T::Gamma(1.8),
    );
    /// Display P3 (P3-D65 primaries, sRGB curve).
    pub const DISPLAY_P3: ColorSpace = ColorSpace::rgb(
        "display-p3",
        "Display P3",
        Encoding::Sdr,
        Chromaticities::P3_D65,
        T::Srgb,
    );
    /// Linear P3-D65.
    pub const P3_D65_LINEAR: ColorSpace = ColorSpace::rgb(
        "p3-d65-linear",
        "Linear P3-D65",
        Encoding::SceneLinear,
        Chromaticities::P3_D65,
        T::Linear,
    );
    /// DCI-P3 (theatrical white, gamma 2.6).
    pub const DCI_P3: ColorSpace = ColorSpace::rgb(
        "dci-p3",
        "DCI-P3",
        Encoding::Sdr,
        Chromaticities::DCI_P3,
        T::Gamma(2.6),
    );
    /// Rec.2020 with the BT.2020 camera curve.
    pub const REC2020: ColorSpace = ColorSpace::rgb(
        "rec2020",
        "Rec.2020",
        Encoding::Sdr,
        Chromaticities::REC2020,
        T::Rec709,
    );
    /// Linear Rec.2020.
    pub const REC2020_LINEAR: ColorSpace = ColorSpace::rgb(
        "rec2020-linear",
        "Linear Rec.2020",
        Encoding::SceneLinear,
        Chromaticities::REC2020,
        T::Linear,
    );
    /// Rec.2020 primaries, pure gamma 2.4 (BT.1886; AE "Rec.2020 Gamma 2.4",
    /// and how AE reads BT.2020-tagged video).
    pub const REC2020_GAMMA24: ColorSpace = ColorSpace::rgb(
        "rec2020-gamma24",
        "Rec.2020 Gamma 2.4 (BT.1886)",
        Encoding::Sdr,
        Chromaticities::REC2020,
        T::Gamma(2.4),
    );
    /// Rec.2100 PQ (linear 1.0 = 100 cd/m²).
    pub const REC2100_PQ: ColorSpace = ColorSpace::rgb(
        "rec2100-pq",
        "Rec.2100 PQ",
        Encoding::Hdr,
        Chromaticities::REC2020,
        T::Pq,
    );
    /// Rec.2100 HLG (scene light, no OOTF).
    pub const REC2100_HLG: ColorSpace = ColorSpace::rgb(
        "rec2100-hlg",
        "Rec.2100 HLG",
        Encoding::Hdr,
        Chromaticities::REC2020,
        T::Hlg,
    );
    /// ACEScg (AP1 linear).
    pub const ACESCG: ColorSpace = ColorSpace::rgb(
        "acescg",
        "ACEScg",
        Encoding::SceneLinear,
        Chromaticities::ACES_AP1,
        T::Linear,
    );
    /// ACES2065-1 (AP0 linear) — the reference space.
    pub const ACES2065_1: ColorSpace = ColorSpace::rgb(
        "aces2065-1",
        "ACES2065-1",
        Encoding::SceneLinear,
        Chromaticities::ACES_AP0,
        T::Linear,
    );
    /// ACEScct.
    pub const ACESCCT: ColorSpace = ColorSpace::rgb(
        "acescct",
        "ACEScct",
        Encoding::Log,
        Chromaticities::ACES_AP1,
        T::AcesCct,
    );
    /// ACEScc.
    pub const ACESCC: ColorSpace = ColorSpace::rgb(
        "acescc",
        "ACEScc",
        Encoding::Log,
        Chromaticities::ACES_AP1,
        T::AcesCc,
    );
    /// Adobe RGB (1998), gamma 563/256.
    pub const ADOBE_RGB: ColorSpace = ColorSpace::rgb(
        "adobe-rgb",
        "Adobe RGB (1998)",
        Encoding::Sdr,
        Chromaticities::ADOBE_RGB,
        T::Gamma(563.0 / 256.0),
    );
    /// Sony S-Log3 / S-Gamut3.
    pub const SLOG3_SGAMUT3: ColorSpace = ColorSpace::rgb_adapted(
        "slog3-sgamut3",
        "Sony S-Log3 S-Gamut3",
        Encoding::Log,
        Chromaticities::S_GAMUT3,
        T::SLog3,
        Adaptation::Cat02,
    );
    /// Sony S-Log3 / S-Gamut3.Cine.
    pub const SLOG3_SGAMUT3_CINE: ColorSpace = ColorSpace::rgb_adapted(
        "slog3-sgamut3cine",
        "Sony S-Log3 S-Gamut3.Cine",
        Encoding::Log,
        Chromaticities::S_GAMUT3_CINE,
        T::SLog3,
        Adaptation::Cat02,
    );
    /// ARRI LogC3 (EI 800) / ARRI Wide Gamut 3.
    pub const LOGC3_AWG3: ColorSpace = ColorSpace::rgb_adapted(
        "logc3-awg3",
        "ARRI LogC3 (EI800) Wide Gamut 3",
        Encoding::Log,
        Chromaticities::ARRI_WIDE_GAMUT_3,
        T::LogC3,
        Adaptation::Cat02,
    );
    /// ARRI LogC4 / ARRI Wide Gamut 4.
    pub const LOGC4_AWG4: ColorSpace = ColorSpace::rgb_adapted(
        "logc4-awg4",
        "ARRI LogC4 Wide Gamut 4",
        Encoding::Log,
        Chromaticities::ARRI_WIDE_GAMUT_4,
        T::LogC4,
        Adaptation::Cat02,
    );
    /// Panasonic V-Log / V-Gamut.
    pub const VLOG_VGAMUT: ColorSpace = ColorSpace::rgb(
        "vlog-vgamut",
        "Panasonic V-Log V-Gamut",
        Encoding::Log,
        Chromaticities::V_GAMUT,
        T::VLog,
    );
    /// Canon Log 3 / Cinema Gamut.
    pub const CLOG3_CINEMA_GAMUT: ColorSpace = ColorSpace::rgb_adapted(
        "clog3-cinemagamut",
        "Canon Log 3 Cinema Gamut",
        Encoding::Log,
        Chromaticities::CINEMA_GAMUT,
        T::CanonLog3,
        Adaptation::Cat02,
    );
    /// Data: no conversion ("Preserve RGB", "Raw").
    pub const RAW: ColorSpace = ColorSpace {
        id: Cow::Borrowed("raw"),
        name: Cow::Borrowed("Raw (no conversion)"),
        encoding: Encoding::Data,
        kind: ColorSpaceKind::Data,
    };

    /// CIE XYZ with D65 white (display connection space of the ACES configs).
    pub fn cie_xyz_d65() -> ColorSpace {
        let ap0 = Chromaticities::ACES_AP0;
        let to = math::mul(
            &ap0.xyz_to_rgb(),
            &Adaptation::Bradford.matrix(white::D65, ap0.white),
        );
        let to = Affine::from_mat3(to);
        ColorSpace {
            id: Cow::Borrowed("cie-xyz-d65"),
            name: Cow::Borrowed("CIE XYZ D65"),
            encoding: Encoding::SceneLinear,
            kind: ColorSpaceKind::Transform {
                to_reference: ColorTransform::matrix(to),
                from_reference: to.inverse().map(ColorTransform::matrix),
            },
        }
    }

    /// Every constant built-in space.
    pub const ALL: [ColorSpace; 26] = [
        SRGB,
        SRGB_LINEAR,
        REC709,
        REC709_GAMMA24,
        GAMMA22,
        GAMMA18,
        DISPLAY_P3,
        P3_D65_LINEAR,
        DCI_P3,
        REC2020,
        REC2020_LINEAR,
        REC2020_GAMMA24,
        REC2100_PQ,
        REC2100_HLG,
        ACESCG,
        ACES2065_1,
        ACESCCT,
        ACESCC,
        ADOBE_RGB,
        SLOG3_SGAMUT3,
        SLOG3_SGAMUT3_CINE,
        LOGC3_AWG3,
        LOGC4_AWG4,
        VLOG_VGAMUT,
        CLOG3_CINEMA_GAMUT,
        RAW,
    ];

    /// The working spaces AE offers that BAE supports (ARCHITECTURE §6).
    pub const WORKING_SPACES: [ColorSpace; 8] = [
        SRGB,
        SRGB_LINEAR,
        REC709,
        REC709_GAMMA24,
        ACESCG,
        REC2020_LINEAR,
        DISPLAY_P3,
        P3_D65_LINEAR,
    ];

    /// Looks a built-in space up by id (also accepts a few common aliases and
    /// the `<id>~linear` / `<id>~<curve>` forms produced by
    /// [`ColorSpace::with_transfer`]).
    pub fn by_id(id: &str) -> Option<ColorSpace> {
        if let Some((base, curve)) = id.split_once('~') {
            let base = by_id(base)?;
            let tf = if curve == "linear" {
                T::Linear
            } else if let Some(g) = curve.strip_prefix("gamma") {
                T::Gamma(g.parse().ok().filter(|g: &f64| g.is_finite() && *g > 0.0)?)
            } else {
                T::ALL
                    .into_iter()
                    .find(|t| t.id() == curve && !matches!(t, T::Gamma(_)))?
            };
            return base.with_transfer(tf);
        }
        let id = match id {
            "lin_srgb" | "linear-srgb" | "lin_rec709" => "srgb-linear",
            "aces" | "aces2065" => "aces2065-1",
            "p3-d65" | "displayp3" => "display-p3",
            "bt1886" => "rec709-gamma24",
            other => other,
        };
        if id == "cie-xyz-d65" {
            return Some(cie_xyz_d65());
        }
        ALL.into_iter().find(|s| s.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::builtin::*;
    use super::*;

    #[test]
    fn ids_are_unique_and_resolve() {
        for (i, s) in ALL.iter().enumerate() {
            assert_eq!(by_id(&s.id).as_ref(), Some(s));
            for t in &ALL[i + 1..] {
                assert_ne!(s.id, t.id);
            }
        }
        assert_eq!(
            by_id("srgb~linear").unwrap().gamut(),
            Some(Chromaticities::REC709)
        );
        assert!(by_id("nope").is_none());
    }

    #[test]
    fn d65_to_d65_has_no_adaptation_left() {
        // sRGB linear → P3 linear must equal the direct colorimetric matrix.
        let t = SRGB_LINEAR.transform_to(&P3_D65_LINEAR).unwrap();
        let m = t.as_matrix().expect("single matrix");
        let direct = math::mul(
            &Chromaticities::P3_D65.xyz_to_rgb(),
            &Chromaticities::REC709.rgb_to_xyz(),
        );
        for i in 0..3 {
            for j in 0..3 {
                assert!((m.m[i][j] - direct[i][j]).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn srgb_to_acescg_matches_published_matrix() {
        // OCIO ACES studio config, "Linear Rec.709 (sRGB)" → ACEScg (Bradford).
        let t = SRGB_LINEAR.transform_to(&ACESCG).unwrap();
        let m = t.as_matrix().unwrap().m;
        let expected = [
            [0.613_097_402_2, 0.339_523_146_3, 0.047_379_451_5],
            [0.070_193_722_2, 0.916_353_871_5, 0.013_452_406_3],
            [0.020_615_593_3, 0.109_569_768_9, 0.869_814_637_8],
        ];
        for i in 0..3 {
            for j in 0..3 {
                assert!((m[i][j] - expected[i][j]).abs() < 1e-6, "{m:?}");
            }
        }
    }

    #[test]
    fn same_space_and_data_are_identity() {
        assert!(SRGB.transform_to(&SRGB).unwrap().is_identity());
        assert!(SRGB.transform_to(&RAW).unwrap().is_identity());
        assert!(RAW.transform_to(&ACESCG).unwrap().is_identity());
        // Same definition, different id.
        assert!(
            by_id("lin_srgb")
                .unwrap()
                .transform_to(&SRGB_LINEAR)
                .unwrap()
                .is_identity()
        );
    }

    #[test]
    fn every_pair_roundtrips() {
        let samples = [
            [0.18, 0.18, 0.18],
            [0.9, 0.4, 0.1],
            [0.05, 0.6, 0.8],
            [0.0, 0.0, 0.0],
        ];
        let mut spaces: Vec<ColorSpace> = ALL.to_vec();
        spaces.push(cie_xyz_d65());
        for a in &spaces {
            for b in &spaces {
                if a.transfer() == Some(TransferFunction::AcesCc)
                    || b.transfer() == Some(TransferFunction::AcesCc)
                {
                    continue;
                }
                let there = a.transform_to(b).unwrap();
                let back = b.transform_to(a).unwrap();
                for s in samples {
                    // Start from encoded values of `a` that are in range.
                    let v = a
                        .from_reference()
                        .unwrap()
                        .apply_f64(SRGB_LINEAR.transform_to(&ACES2065_1).unwrap().apply_f64(s));
                    let r = back.apply_f64(there.apply_f64(v));
                    for i in 0..3 {
                        assert!(
                            (r[i] - v[i]).abs() < 1e-9 * v[i].abs().max(1.0),
                            "{} -> {}: {v:?} {r:?}",
                            a.id,
                            b.id
                        );
                    }
                }
            }
        }
    }
}
