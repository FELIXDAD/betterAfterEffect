//! OCIO `BuiltinTransform` styles, expanded into bae-color ops.
//!
//! Matrices are derived from the chromaticities with the adaptation OCIO
//! documents in the style name (`_BFD` = Bradford; camera styles use their
//! vendor's CAT). Curves OCIO bakes into LUTs (PQ, Canon Log 3) use the exact
//! analytic formulas here. ACES output transforms (RRT / ODT, ACES 2.0) are not
//! implemented yet and return [`ColorError::Unsupported`].

use crate::ColorError;
use crate::math::{self, Affine};
use crate::ops::{MoncurveOp, NegativeStyle, PowerOp};
use crate::primaries::{Adaptation, Chromaticities, rgb_to_rgb, white};
use crate::transfer::TransferFunction;
use crate::transform::{ColorOp, ColorTransform};

/// Sony VENICE S-Gamut3.
const VENICE_S_GAMUT3: Chromaticities = Chromaticities {
    red: [0.740_464, 0.279_364],
    green: [0.089_241, 0.893_638],
    blue: [0.110_488, -0.052_579],
    white: white::D65,
};
/// Sony VENICE S-Gamut3.Cine.
const VENICE_S_GAMUT3_CINE: Chromaticities = Chromaticities {
    red: [0.775_901, 0.274_502],
    green: [0.188_682, 0.828_683],
    blue: [0.101_337, -0.089_187],
    white: white::D65,
};

fn to_ap0(src: &Chromaticities, cat: Adaptation) -> ColorOp {
    ColorOp::Matrix(Affine::from_mat3(rgb_to_rgb(src, &Chromaticities::ACES_AP0, cat)))
}

/// CIE XYZ (D65) → linear RGB of `dst`, adapting D65 to the target white with Bradford.
fn xyz_to(dst: &Chromaticities) -> ColorOp {
    let cat = Adaptation::Bradford.matrix(white::D65, dst.white);
    ColorOp::Matrix(Affine::from_mat3(math::mul(&dst.xyz_to_rgb(), &cat)))
}

fn ap_to_xyz(src: &Chromaticities) -> ColorOp {
    let cat = Adaptation::Bradford.matrix(src.white, white::D65);
    ColorOp::Matrix(Affine::from_mat3(math::mul(&cat, &src.rgb_to_xyz())))
}

fn power_inv(gamma: f64, mirror: bool) -> ColorOp {
    ColorOp::Power(
        PowerOp {
            exponent: [gamma; 3],
            negative: if mirror { NegativeStyle::Mirror } else { NegativeStyle::Clamp },
        }
        .inverse(),
    )
}

fn srgb_moncurve_inv(mirror: bool) -> ColorOp {
    ColorOp::Moncurve(MoncurveOp {
        gamma: [2.4; 3],
        offset: [0.055; 3],
        negative: if mirror { NegativeStyle::Mirror } else { NegativeStyle::Linear },
        inverse: true,
    })
}

/// Expands a builtin style (forward direction).
pub fn expand(style: &str) -> Result<ColorTransform, ColorError> {
    use Chromaticities as C;
    use TransferFunction as T;
    let ops: Vec<ColorOp> = match style {
        "IDENTITY" => vec![],
        "UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD" => vec![ap_to_xyz(&C::ACES_AP0)],
        "UTILITY - ACES-AP1_to_CIE-XYZ-D65_BFD" => vec![ap_to_xyz(&C::ACES_AP1)],
        "UTILITY - ACES-AP1_to_LINEAR-REC709_BFD" => {
            vec![ColorOp::Matrix(Affine::from_mat3(rgb_to_rgb(&C::ACES_AP1, &C::REC709, Adaptation::Bradford)))]
        }
        "UTILITY - ACES-AP0_to_LINEAR-REC709_BFD" => {
            vec![ColorOp::Matrix(Affine::from_mat3(rgb_to_rgb(&C::ACES_AP0, &C::REC709, Adaptation::Bradford)))]
        }
        "ACEScg_to_ACES2065-1" => vec![to_ap0(&C::ACES_AP1, Adaptation::Bradford)],
        "ACEScct_to_ACES2065-1" => vec![ColorOp::Decode(T::AcesCct), to_ap0(&C::ACES_AP1, Adaptation::Bradford)],
        "ACEScc_to_ACES2065-1" => vec![ColorOp::Decode(T::AcesCc), to_ap0(&C::ACES_AP1, Adaptation::Bradford)],
        "ARRI_ALEXA-LOGC-EI800-AWG_to_ACES2065-1" => {
            vec![ColorOp::Decode(T::LogC3), to_ap0(&C::ARRI_WIDE_GAMUT_3, Adaptation::Cat02)]
        }
        "ARRI_LOGC4_to_ACES2065-1" => vec![ColorOp::Decode(T::LogC4), to_ap0(&C::ARRI_WIDE_GAMUT_4, Adaptation::Cat02)],
        "SONY_SLOG3-SGAMUT3_to_ACES2065-1" => vec![ColorOp::Decode(T::SLog3), to_ap0(&C::S_GAMUT3, Adaptation::Cat02)],
        "SONY_SLOG3-SGAMUT3.CINE_to_ACES2065-1" => {
            vec![ColorOp::Decode(T::SLog3), to_ap0(&C::S_GAMUT3_CINE, Adaptation::Cat02)]
        }
        "SONY_SLOG3-SGAMUT3-VENICE_to_ACES2065-1" => {
            vec![ColorOp::Decode(T::SLog3), to_ap0(&VENICE_S_GAMUT3, Adaptation::Cat02)]
        }
        "SONY_SLOG3-SGAMUT3.CINE-VENICE_to_ACES2065-1" => {
            vec![ColorOp::Decode(T::SLog3), to_ap0(&VENICE_S_GAMUT3_CINE, Adaptation::Cat02)]
        }
        "PANASONIC_VLOG-VGAMUT_to_ACES2065-1" => vec![ColorOp::Decode(T::VLog), to_ap0(&C::V_GAMUT, Adaptation::Bradford)],
        "CANON_CLOG3-CGAMUT_to_ACES2065-1" => {
            vec![ColorOp::Decode(T::CanonLog3), to_ap0(&C::CINEMA_GAMUT, Adaptation::Cat02)]
        }
        "CURVE - CANON_CLOG3_to_LINEAR" => vec![ColorOp::Decode(T::CanonLog3)],
        "CURVE - ST-2084_to_LINEAR" => vec![ColorOp::Decode(T::Pq)],
        "CURVE - LINEAR_to_ST-2084" => vec![ColorOp::Encode(T::Pq)],
        "CURVE - HLG-OETF" => vec![ColorOp::Encode(T::Hlg)],
        "CURVE - HLG-OETF-INVERSE" => vec![ColorOp::Decode(T::Hlg)],
        s if s.starts_with("DISPLAY - CIE-XYZ-D65_to_") => {
            let rest = &s["DISPLAY - CIE-XYZ-D65_to_".len()..];
            let (target, mirror) = match rest.strip_suffix(" - MIRROR NEGS") {
                Some(t) => (t, true),
                None => (rest, false),
            };
            match target {
                "sRGB" => vec![xyz_to(&C::REC709), srgb_moncurve_inv(mirror)],
                "DisplayP3" => vec![xyz_to(&C::P3_D65), srgb_moncurve_inv(mirror)],
                // EDR: the sRGB curve, mirrored so values above 1 and below 0 survive.
                "DisplayP3-HDR" => vec![xyz_to(&C::P3_D65), srgb_moncurve_inv(true)],
                "REC.1886-REC.709" => vec![xyz_to(&C::REC709), power_inv(2.4, mirror)],
                "REC.1886-REC.2020" => vec![xyz_to(&C::REC2020), power_inv(2.4, mirror)],
                "G2.2-REC.709" => vec![xyz_to(&C::REC709), power_inv(2.2, mirror)],
                "G2.6-P3-D65" => vec![xyz_to(&C::P3_D65), power_inv(2.6, mirror)],
                "G2.6-P3-DCI-BFD" => vec![xyz_to(&C::DCI_P3), power_inv(2.6, mirror)],
                "G2.6-P3-D60-BFD" => vec![
                    xyz_to(&Chromaticities {
                        white: white::ACES,
                        ..C::P3_D65
                    }),
                    power_inv(2.6, mirror),
                ],
                "REC.2100-PQ" => vec![xyz_to(&C::REC2020), ColorOp::Encode(T::Pq)],
                "ST2084-P3-D65" => vec![xyz_to(&C::P3_D65), ColorOp::Encode(T::Pq)],
                _ => return Err(ColorError::Unsupported(format!("OCIO builtin `{style}`"))),
            }
        }
        _ => return Err(ColorError::Unsupported(format!("OCIO builtin `{style}`"))),
    };
    Ok(ColorTransform::from_ops(ops))
}
