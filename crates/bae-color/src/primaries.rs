//! RGB primaries, white points, RGB ↔ XYZ matrices and chromatic adaptation.

use serde::{Deserialize, Serialize};

use crate::math::{self, Mat3};

/// CIE 1931 xy chromaticity.
pub type Xy = [f64; 2];

/// Standard white points (CIE 1931 2° xy).
pub mod white {
    use super::Xy;

    /// D65 as written in sRGB / Rec.709 / Rec.2020 (four decimals).
    pub const D65: Xy = [0.3127, 0.3290];
    /// The ACES white point (≈ D60).
    pub const ACES: Xy = [0.32168, 0.33767];
    /// D50 (ICC profile connection space).
    pub const D50: Xy = [0.3457, 0.3585];
    /// DCI theatrical white.
    pub const DCI: Xy = [0.314, 0.351];
    /// CIE illuminant C (NTSC 1953, generic film).
    pub const C: Xy = [0.310, 0.316];
    /// Equal-energy white E.
    pub const E: Xy = [1.0 / 3.0, 1.0 / 3.0];
}

/// CIE xy of the three primaries and the white point of an RGB space.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chromaticities {
    /// Red primary.
    pub red: Xy,
    /// Green primary.
    pub green: Xy,
    /// Blue primary.
    pub blue: Xy,
    /// White point.
    pub white: Xy,
}

impl Chromaticities {
    /// ITU-R BT.709 / sRGB (D65).
    pub const REC709: Self = Self {
        red: [0.64, 0.33],
        green: [0.30, 0.60],
        blue: [0.15, 0.06],
        white: white::D65,
    };
    /// P3 primaries with D65 (Display P3, P3-D65).
    pub const P3_D65: Self = Self {
        red: [0.680, 0.320],
        green: [0.265, 0.690],
        blue: [0.150, 0.060],
        white: white::D65,
    };
    /// DCI-P3 (theatrical white).
    pub const DCI_P3: Self = Self {
        white: white::DCI,
        ..Self::P3_D65
    };
    /// ITU-R BT.2020 / BT.2100 (D65).
    pub const REC2020: Self = Self {
        red: [0.708, 0.292],
        green: [0.170, 0.797],
        blue: [0.131, 0.046],
        white: white::D65,
    };
    /// ACES AP0 (ACES2065-1).
    pub const ACES_AP0: Self = Self {
        red: [0.7347, 0.2653],
        green: [0.0, 1.0],
        blue: [0.0001, -0.0770],
        white: white::ACES,
    };
    /// ACES AP1 (ACEScg, ACEScc, ACEScct).
    pub const ACES_AP1: Self = Self {
        red: [0.713, 0.293],
        green: [0.165, 0.830],
        blue: [0.128, 0.044],
        white: white::ACES,
    };
    /// Adobe RGB (1998).
    pub const ADOBE_RGB: Self = Self {
        red: [0.64, 0.33],
        green: [0.21, 0.71],
        blue: [0.15, 0.06],
        white: white::D65,
    };
    /// ITU-R BT.601 625-line (PAL / SECAM, H.273 code 5).
    pub const BT601_625: Self = Self {
        red: [0.64, 0.33],
        green: [0.29, 0.60],
        blue: [0.15, 0.06],
        white: white::D65,
    };
    /// ITU-R BT.601 525-line / SMPTE 170M (NTSC, H.273 codes 6 and 7).
    pub const BT601_525: Self = Self {
        red: [0.630, 0.340],
        green: [0.310, 0.595],
        blue: [0.155, 0.070],
        white: white::D65,
    };
    /// ITU-R BT.470 System M (H.273 code 4), illuminant C.
    pub const BT470M: Self = Self {
        red: [0.67, 0.33],
        green: [0.21, 0.71],
        blue: [0.14, 0.08],
        white: white::C,
    };
    /// Generic film (H.273 code 8), illuminant C.
    pub const FILM: Self = Self {
        red: [0.681, 0.319],
        green: [0.243, 0.692],
        blue: [0.145, 0.049],
        white: white::C,
    };
    /// Sony S-Gamut3.
    pub const S_GAMUT3: Self = Self {
        red: [0.730, 0.280],
        green: [0.140, 0.855],
        blue: [0.100, -0.050],
        white: white::D65,
    };
    /// Sony S-Gamut3.Cine.
    pub const S_GAMUT3_CINE: Self = Self {
        red: [0.766, 0.275],
        green: [0.225, 0.800],
        blue: [0.089, -0.087],
        white: white::D65,
    };
    /// ARRI Wide Gamut 3 (ALEXA, LogC3).
    pub const ARRI_WIDE_GAMUT_3: Self = Self {
        red: [0.6840, 0.3130],
        green: [0.2210, 0.8480],
        blue: [0.0861, -0.1020],
        white: white::D65,
    };
    /// ARRI Wide Gamut 4 (ALEXA 35, LogC4).
    pub const ARRI_WIDE_GAMUT_4: Self = Self {
        red: [0.7347, 0.2653],
        green: [0.1424, 0.8576],
        blue: [0.0991, -0.0308],
        white: white::D65,
    };
    /// Panasonic V-Gamut.
    pub const V_GAMUT: Self = Self {
        red: [0.730, 0.280],
        green: [0.165, 0.840],
        blue: [0.100, -0.030],
        white: white::D65,
    };
    /// Canon Cinema Gamut.
    pub const CINEMA_GAMUT: Self = Self {
        red: [0.740, 0.270],
        green: [0.170, 1.140],
        blue: [0.080, -0.100],
        white: white::D65,
    };

    /// The normalized primary matrix: linear RGB → CIE XYZ (Y of white = 1).
    pub fn rgb_to_xyz(&self) -> Mat3 {
        let [r, g, b] = [
            xy_to_xyz(self.red),
            xy_to_xyz(self.green),
            xy_to_xyz(self.blue),
        ];
        let p = [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]];
        let w = xy_to_xyz(self.white);
        let s = math::mul_vec(&math::inverse(&p).unwrap_or(math::IDENTITY), w);
        math::mul(&p, &math::diag(s))
    }

    /// CIE XYZ → linear RGB.
    pub fn xyz_to_rgb(&self) -> Mat3 {
        math::inverse(&self.rgb_to_xyz()).unwrap_or(math::IDENTITY)
    }

    /// `true` when all coordinates are within `eps`.
    pub fn approx_eq(&self, other: &Self, eps: f64) -> bool {
        let a = [self.red, self.green, self.blue, self.white];
        let b = [other.red, other.green, other.blue, other.white];
        a.iter()
            .zip(b.iter())
            .all(|(p, q)| (p[0] - q[0]).abs() <= eps && (p[1] - q[1]).abs() <= eps)
    }

    /// Hashable bit pattern (for cache keys).
    pub(crate) fn bits(&self) -> [u64; 8] {
        let v = [
            self.red[0],
            self.red[1],
            self.green[0],
            self.green[1],
            self.blue[0],
            self.blue[1],
            self.white[0],
            self.white[1],
        ];
        v.map(f64::to_bits)
    }
}

/// xy → XYZ with Y = 1.
pub fn xy_to_xyz(xy: Xy) -> [f64; 3] {
    let [x, y] = xy;
    if y == 0.0 {
        return [0.0, 0.0, 0.0];
    }
    [x / y, 1.0, (1.0 - x - y) / y]
}

/// Chromatic adaptation method (von Kries-type, cone-response matrix).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Adaptation {
    /// Bradford (ICC, OCIO ACES "utility" spaces). The default.
    #[default]
    Bradford,
    /// CIECAM02 CAT02 (ACES output transforms, several camera IDTs).
    Cat02,
    /// Von Kries with Hunt-Pointer-Estevez cones.
    VonKries,
    /// Plain XYZ scaling.
    XyzScaling,
    /// No adaptation: XYZ passes through (absolute colorimetry).
    None,
}

impl Adaptation {
    fn cone_matrix(self) -> Option<Mat3> {
        match self {
            Adaptation::Bradford => Some([
                [0.8951, 0.2664, -0.1614],
                [-0.7502, 1.7135, 0.0367],
                [0.0389, -0.0685, 1.0296],
            ]),
            Adaptation::Cat02 => Some([
                [0.7328, 0.4296, -0.1624],
                [-0.7036, 1.6975, 0.0061],
                [0.0030, 0.0136, 0.9834],
            ]),
            Adaptation::VonKries => Some([
                [0.40024, 0.70760, -0.08081],
                [-0.22630, 1.16532, 0.04570],
                [0.0, 0.0, 0.91822],
            ]),
            Adaptation::XyzScaling => Some(math::IDENTITY),
            Adaptation::None => None,
        }
    }

    /// XYZ (under `src` white) → XYZ (under `dst` white).
    pub fn matrix(self, src: Xy, dst: Xy) -> Mat3 {
        let Some(cone) = self.cone_matrix() else {
            return math::IDENTITY;
        };
        if src == dst {
            return math::IDENTITY;
        }
        let inv = math::inverse(&cone).unwrap_or(math::IDENTITY);
        let s = math::mul_vec(&cone, xy_to_xyz(src));
        let d = math::mul_vec(&cone, xy_to_xyz(dst));
        let scale = math::diag([d[0] / s[0], d[1] / s[1], d[2] / s[2]]);
        math::mul(&inv, &math::mul(&scale, &cone))
    }
}

/// Linear RGB (`src`) → linear RGB (`dst`) with chromatic adaptation between
/// their white points.
pub fn rgb_to_rgb(src: &Chromaticities, dst: &Chromaticities, adaptation: Adaptation) -> Mat3 {
    let cat = adaptation.matrix(src.white, dst.white);
    math::mul(&dst.xyz_to_rgb(), &math::mul(&cat, &src.rgb_to_xyz()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &Mat3, b: &Mat3, eps: f64) -> bool {
        (0..3).all(|i| (0..3).all(|j| (a[i][j] - b[i][j]).abs() <= eps))
    }

    #[test]
    fn srgb_npm_matches_iec() {
        // IEC 61966-2-1 (computed from the primaries; the standard rounds to 4 places).
        let m = Chromaticities::REC709.rgb_to_xyz();
        let expected = [
            [0.4123907992659595, 0.357584339383878, 0.1804807884018343],
            [0.21263900587151036, 0.715168678767756, 0.07219231536073371],
            [0.01933081871559185, 0.11919477979462598, 0.9505321522496606],
        ];
        assert!(close(&m, &expected, 1e-12));
    }

    #[test]
    fn ap0_npm_matches_aces() {
        // SMPTE ST 2065-1.
        let m = Chromaticities::ACES_AP0.rgb_to_xyz();
        let expected = [
            [0.9525523959, 0.0, 0.0000936786],
            [0.3439664498, 0.7281660966, -0.0721325464],
            [0.0, 0.0, 1.0088251844],
        ];
        assert!(close(&m, &expected, 1e-9));
    }

    #[test]
    fn adaptation_roundtrip_cancels() {
        for a in [
            Adaptation::Bradford,
            Adaptation::Cat02,
            Adaptation::VonKries,
        ] {
            let there = a.matrix(white::D65, white::ACES);
            let back = a.matrix(white::ACES, white::D65);
            assert!(close(&math::mul(&back, &there), &math::IDENTITY, 1e-14));
            // White maps to white.
            let w = math::mul_vec(&there, xy_to_xyz(white::D65));
            let e = xy_to_xyz(white::ACES);
            for i in 0..3 {
                assert!((w[i] - e[i]).abs() < 1e-12);
            }
        }
    }
}
