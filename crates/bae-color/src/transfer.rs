//! Transfer functions (tone curves): encoded code values ↔ linear light.
//!
//! Conventions, shared by the CPU reference here and the WGSL in [`crate::wgsl`]:
//!
//! * `decode` maps encoded values to linear light (EOTF for display curves,
//!   inverse OETF for camera curves); `encode` is its inverse.
//! * Encoded values are normalized code values (`cv / (2^n - 1)`), which is how
//!   decoders hand frames to the GPU and how the vendors' current formulas are
//!   written (S-Log3, LogC, V-Log, Canon Log 3 v1.2 all fold their legal-range
//!   offsets into the constants).
//! * Display curves are extended to negative values by odd symmetry
//!   (`f(-x) = -f(x)`) and continue their power segment above 1, so 32 bpc
//!   over-range values survive (AE-CMP-461). Log curves continue their linear toe
//!   segment below the cut.
//! * PQ: linear 1.0 = 100 cd/m² (SDR reference white), as in OpenColorIO's
//!   `CURVE - ST-2084_to_LINEAR`; PQ 1.0 = 10000 cd/m² = linear 100.
//! * HLG: the BT.2100 OETF; linear is scene light in [0, 1] (no OOTF).

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

/// A transfer function. `#[non_exhaustive]`: more camera curves are added in
/// later batches.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TransferFunction {
    /// Identity.
    Linear,
    /// IEC 61966-2-1 sRGB piecewise curve.
    Srgb,
    /// ITU-R BT.709 / BT.2020 camera OETF (the "HDTV (Rec. 709)" profile curve),
    /// with the continuous BT.2020 constants (α = 1.0992968, β = 0.0180540).
    Rec709,
    /// Pure power law `linear = encoded^γ` (γ = 2.2, 2.4 = BT.1886 with zero
    /// black, 1.8, Adobe RGB's 563/256 …).
    Gamma(f64),
    /// SMPTE ST 2084 perceptual quantizer (linear 1.0 = 100 cd/m²).
    Pq,
    /// ARIB STD-B67 / BT.2100 hybrid log-gamma OETF.
    Hlg,
    /// ACEScc (S-2014-003). Not exactly invertible (encoding clamps at 0),
    /// never cancelled.
    AcesCc,
    /// ACEScct (S-2016-001).
    AcesCct,
    /// Sony S-Log3.
    SLog3,
    /// ARRI LogC3 (ALEXA, SUP 3.x, EI 800).
    LogC3,
    /// ARRI LogC4 (ALEXA 35).
    LogC4,
    /// Panasonic V-Log.
    VLog,
    /// Canon Log 3 (v1.2 constants, reflection scaled by 0.9 as in the Canon
    /// ACES IDT).
    CanonLog3,
}

impl Eq for TransferFunction {}

impl Hash for TransferFunction {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        if let TransferFunction::Gamma(g) = self {
            g.to_bits().hash(state);
        }
    }
}

/// Curve constants, shared with the WGSL generator so both use identical numbers.
pub(crate) mod k {
    pub const SRGB_DECODE_CUT: f64 = 0.04045;
    pub const SRGB_ENCODE_CUT: f64 = 0.0031308;

    pub const REC709_A: f64 = 1.099_296_826_809_44;
    pub const REC709_B: f64 = 0.018_053_968_510_807;

    pub const PQ_M1: f64 = 2610.0 / 16384.0;
    pub const PQ_M2: f64 = 2523.0 / 4096.0 * 128.0;
    pub const PQ_C1: f64 = 3424.0 / 4096.0;
    pub const PQ_C2: f64 = 2413.0 / 4096.0 * 32.0;
    pub const PQ_C3: f64 = 2392.0 / 4096.0 * 32.0;
    /// 10000 cd/m² peak / 100 cd/m² reference white.
    pub const PQ_SCALE: f64 = 100.0;

    pub const HLG_A: f64 = 0.178_832_77;
    pub const HLG_B: f64 = 1.0 - 4.0 * HLG_A;
    // c = 0.5 - a * ln(4a), evaluated in f64 at first use (ln is not const).
    pub fn hlg_c() -> f64 {
        0.5 - HLG_A * (4.0 * HLG_A).ln()
    }

    pub const ACESCCT_X_BRK: f64 = 0.0078125;
    pub const ACESCCT_Y_BRK: f64 = 0.155_251_141_552_511;
    pub const ACESCCT_A: f64 = 10.540_237_741_654_5;
    pub const ACESCCT_B: f64 = 0.072_905_534_195_835_5;

    pub const SLOG3_CUT_LIN: f64 = 0.01125;
    pub const SLOG3_CUT_CV: f64 = 171.210_294_692_9;

    pub const LOGC3_CUT: f64 = 0.010591;
    pub const LOGC3_A: f64 = 5.555556;
    pub const LOGC3_B: f64 = 0.052272;
    pub const LOGC3_C: f64 = 0.247190;
    pub const LOGC3_D: f64 = 0.385537;
    pub const LOGC3_E: f64 = 5.367655;
    pub const LOGC3_F: f64 = 0.092809;

    /// LogC4 constants (a, b, c, s, t).
    pub fn logc4() -> (f64, f64, f64, f64, f64) {
        let a = (2f64.powi(18) - 16.0) / 117.45;
        let b = (1023.0 - 95.0) / 1023.0;
        let c = 95.0 / 1023.0;
        let s = (7.0 * std::f64::consts::LN_2 * 2f64.powf(7.0 - 14.0 * c / b)) / (a * b);
        let t = (2f64.powf(14.0 * (-c / b) + 6.0) - 64.0) / a;
        (a, b, c, s, t)
    }

    pub const VLOG_CUT1: f64 = 0.01;
    pub const VLOG_CUT2: f64 = 0.181;
    pub const VLOG_B: f64 = 0.00873;
    pub const VLOG_C: f64 = 0.241514;
    pub const VLOG_D: f64 = 0.598206;

    pub const CLOG3_SLOPE: f64 = 0.367_268_45;
    pub const CLOG3_SCALE: f64 = 14.98325;
    pub const CLOG3_OFF_NEG: f64 = 0.127_839_01;
    pub const CLOG3_OFF_POS: f64 = 0.122_405_37;
    pub const CLOG3_LIN_SLOPE: f64 = 1.975_479_8;
    pub const CLOG3_LIN_OFF: f64 = 0.125_122_19;
    pub const CLOG3_CUT_NEG: f64 = 0.097_465_473;
    pub const CLOG3_CUT_POS: f64 = 0.152_778_91;
    /// Linear-domain cuts: the code-value cuts mapped through the linear
    /// segment (as colour-science does), so encode and decode switch branches at
    /// the same point.
    pub const CLOG3_LIN_CUT_POS: f64 = (CLOG3_CUT_POS - CLOG3_LIN_OFF) / CLOG3_LIN_SLOPE;
    pub const CLOG3_LIN_CUT_NEG: f64 = (CLOG3_CUT_NEG - CLOG3_LIN_OFF) / CLOG3_LIN_SLOPE;
    pub const CLOG3_REFLECTION: f64 = 0.9;
}

#[inline]
fn mirror(v: f64, f: impl Fn(f64) -> f64) -> f64 {
    if v < 0.0 { -f(-v) } else { f(v) }
}

impl TransferFunction {
    /// Every parameterless curve plus the common gammas (tests, UI lists).
    pub const ALL: [TransferFunction; 15] = [
        TransferFunction::Linear,
        TransferFunction::Srgb,
        TransferFunction::Rec709,
        TransferFunction::Gamma(2.2),
        TransferFunction::Gamma(2.4),
        TransferFunction::Gamma(1.8),
        TransferFunction::Pq,
        TransferFunction::Hlg,
        TransferFunction::AcesCc,
        TransferFunction::AcesCct,
        TransferFunction::SLog3,
        TransferFunction::LogC3,
        TransferFunction::LogC4,
        TransferFunction::VLog,
        TransferFunction::CanonLog3,
    ];

    /// Stable identifier (project files, WGSL helper names).
    pub fn id(self) -> &'static str {
        match self {
            TransferFunction::Linear => "linear",
            TransferFunction::Srgb => "srgb",
            TransferFunction::Rec709 => "rec709",
            TransferFunction::Gamma(_) => "gamma",
            TransferFunction::Pq => "pq",
            TransferFunction::Hlg => "hlg",
            TransferFunction::AcesCc => "acescc",
            TransferFunction::AcesCct => "acescct",
            TransferFunction::SLog3 => "slog3",
            TransferFunction::LogC3 => "logc3",
            TransferFunction::LogC4 => "logc4",
            TransferFunction::VLog => "vlog",
            TransferFunction::CanonLog3 => "clog3",
        }
    }

    /// `true` for the identity curve (and γ = 1).
    pub fn is_linear(self) -> bool {
        match self {
            TransferFunction::Linear => true,
            TransferFunction::Gamma(g) => g == 1.0,
            _ => false,
        }
    }

    /// `true` when `encode(decode(v)) == v` and `decode(encode(l)) == l` hold on
    /// the whole real line, so an adjacent decode/encode pair may be removed.
    pub fn is_exactly_invertible(self) -> bool {
        match self {
            TransferFunction::AcesCc => false,
            TransferFunction::Gamma(g) => g.is_finite() && g > 0.0,
            _ => true,
        }
    }

    /// `true` for logarithmic camera / grading encodings.
    pub fn is_log(self) -> bool {
        matches!(
            self,
            TransferFunction::AcesCc
                | TransferFunction::AcesCct
                | TransferFunction::SLog3
                | TransferFunction::LogC3
                | TransferFunction::LogC4
                | TransferFunction::VLog
                | TransferFunction::CanonLog3
        )
    }

    /// `true` for HDR signal encodings (PQ, HLG).
    pub fn is_hdr(self) -> bool {
        matches!(self, TransferFunction::Pq | TransferFunction::Hlg)
    }

    /// Encoded → linear.
    pub fn decode(self, v: f64) -> f64 {
        use k::*;
        match self {
            TransferFunction::Linear => v,
            TransferFunction::Srgb => mirror(v, |v| {
                if v <= SRGB_DECODE_CUT {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            }),
            TransferFunction::Rec709 => mirror(v, |v| {
                if v < 4.5 * REC709_B {
                    v / 4.5
                } else {
                    ((v + (REC709_A - 1.0)) / REC709_A).powf(1.0 / 0.45)
                }
            }),
            TransferFunction::Gamma(g) => mirror(v, |v| v.powf(g)),
            TransferFunction::Pq => mirror(v, |v| {
                let e = v.powf(1.0 / PQ_M2);
                // The denominator reaches 0 at code value ≈ 1.99 (beyond 10000
                // cd/m²); keep it positive so out-of-range input stays finite.
                ((e - PQ_C1).max(0.0) / (PQ_C2 - PQ_C3 * e).max(1e-6)).powf(1.0 / PQ_M1) * PQ_SCALE
            }),
            TransferFunction::Hlg => mirror(v, |v| {
                if v <= 0.5 {
                    v * v / 3.0
                } else {
                    (((v - hlg_c()) / HLG_A).exp() + HLG_B) / 12.0
                }
            }),
            TransferFunction::AcesCc => {
                // No HALF_MAX cap above 1.468 (the CTL clamps for half-float
                // storage; OCIO and float buffers do not).
                if v < (9.72 - 15.0) / 17.52 {
                    (2f64.powf(v * 17.52 - 9.72) - 2f64.powi(-16)) * 2.0
                } else {
                    2f64.powf(v * 17.52 - 9.72)
                }
            }
            TransferFunction::AcesCct => {
                if v <= ACESCCT_Y_BRK {
                    (v - ACESCCT_B) / ACESCCT_A
                } else {
                    2f64.powf(v * 17.52 - 9.72)
                }
            }
            TransferFunction::SLog3 => {
                if v >= SLOG3_CUT_CV / 1023.0 {
                    10f64.powf((v * 1023.0 - 420.0) / 261.5) * (0.18 + 0.01) - 0.01
                } else {
                    (v * 1023.0 - 95.0) * SLOG3_CUT_LIN / (SLOG3_CUT_CV - 95.0)
                }
            }
            TransferFunction::LogC3 => {
                if v > LOGC3_E * LOGC3_CUT + LOGC3_F {
                    (10f64.powf((v - LOGC3_D) / LOGC3_C) - LOGC3_B) / LOGC3_A
                } else {
                    (v - LOGC3_F) / LOGC3_E
                }
            }
            TransferFunction::LogC4 => {
                let (a, b, c, s, t) = logc4();
                if v >= 0.0 {
                    (2f64.powf(14.0 * (v - c) / b + 6.0) - 64.0) / a
                } else {
                    v * s + t
                }
            }
            TransferFunction::VLog => {
                if v < VLOG_CUT2 {
                    (v - 0.125) / 5.6
                } else {
                    10f64.powf((v - VLOG_D) / VLOG_C) - VLOG_B
                }
            }
            TransferFunction::CanonLog3 => {
                let c = v;
                let x = if c < CLOG3_CUT_NEG {
                    -(10f64.powf((CLOG3_OFF_NEG - c) / CLOG3_SLOPE) - 1.0) / CLOG3_SCALE
                } else if c <= CLOG3_CUT_POS {
                    (c - CLOG3_LIN_OFF) / CLOG3_LIN_SLOPE
                } else {
                    (10f64.powf((c - CLOG3_OFF_POS) / CLOG3_SLOPE) - 1.0) / CLOG3_SCALE
                };
                x * CLOG3_REFLECTION
            }
        }
    }

    /// Linear → encoded.
    pub fn encode(self, l: f64) -> f64 {
        use k::*;
        match self {
            TransferFunction::Linear => l,
            TransferFunction::Srgb => mirror(l, |l| {
                if l <= SRGB_ENCODE_CUT {
                    l * 12.92
                } else {
                    1.055 * l.powf(1.0 / 2.4) - 0.055
                }
            }),
            TransferFunction::Rec709 => mirror(l, |l| {
                if l < REC709_B {
                    l * 4.5
                } else {
                    REC709_A * l.powf(0.45) - (REC709_A - 1.0)
                }
            }),
            TransferFunction::Gamma(g) => mirror(l, |l| l.powf(1.0 / g)),
            TransferFunction::Pq => mirror(l, |l| {
                let y = (l / PQ_SCALE).powf(PQ_M1);
                ((PQ_C1 + PQ_C2 * y) / (1.0 + PQ_C3 * y)).powf(PQ_M2)
            }),
            TransferFunction::Hlg => mirror(l, |l| {
                if l <= 1.0 / 12.0 {
                    (3.0 * l).sqrt()
                } else {
                    HLG_A * (12.0 * l - HLG_B).ln() + hlg_c()
                }
            }),
            TransferFunction::AcesCc => {
                if l <= 0.0 {
                    (-16.0 + 9.72) / 17.52
                } else if l < 2f64.powi(-15) {
                    ((2f64.powi(-16) + l * 0.5).log2() + 9.72) / 17.52
                } else {
                    (l.log2() + 9.72) / 17.52
                }
            }
            TransferFunction::AcesCct => {
                if l <= ACESCCT_X_BRK {
                    ACESCCT_A * l + ACESCCT_B
                } else {
                    (l.log2() + 9.72) / 17.52
                }
            }
            TransferFunction::SLog3 => {
                if l >= SLOG3_CUT_LIN {
                    (420.0 + ((l + 0.01) / (0.18 + 0.01)).log10() * 261.5) / 1023.0
                } else {
                    (l * (SLOG3_CUT_CV - 95.0) / SLOG3_CUT_LIN + 95.0) / 1023.0
                }
            }
            TransferFunction::LogC3 => {
                if l > LOGC3_CUT {
                    LOGC3_C * (LOGC3_A * l + LOGC3_B).log10() + LOGC3_D
                } else {
                    LOGC3_E * l + LOGC3_F
                }
            }
            TransferFunction::LogC4 => {
                let (a, b, c, s, t) = logc4();
                if l >= t {
                    ((a * l + 64.0).log2() - 6.0) / 14.0 * b + c
                } else {
                    (l - t) / s
                }
            }
            TransferFunction::VLog => {
                if l < VLOG_CUT1 {
                    5.6 * l + 0.125
                } else {
                    VLOG_C * (l + VLOG_B).log10() + VLOG_D
                }
            }
            TransferFunction::CanonLog3 => {
                let x = l / CLOG3_REFLECTION;
                if x < CLOG3_LIN_CUT_NEG {
                    -CLOG3_SLOPE * (-x * CLOG3_SCALE + 1.0).log10() + CLOG3_OFF_NEG
                } else if x <= CLOG3_LIN_CUT_POS {
                    CLOG3_LIN_SLOPE * x + CLOG3_LIN_OFF
                } else {
                    CLOG3_SLOPE * (x * CLOG3_SCALE + 1.0).log10() + CLOG3_OFF_POS
                }
            }
        }
    }

    /// The encoded value range a test should sweep (`[lo, hi]`).
    pub fn encoded_test_range(self) -> (f64, f64) {
        match self {
            TransferFunction::AcesCc => (-0.35, 1.4),
            TransferFunction::AcesCct => (0.0, 1.4),
            _ if self.is_log() => (0.0, 1.0),
            _ => (-0.25, 1.25),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_on_encoded_range() {
        for tf in TransferFunction::ALL {
            let (lo, hi) = tf.encoded_test_range();
            for i in 0..=1000 {
                let v = lo + (hi - lo) * i as f64 / 1000.0;
                let back = tf.encode(tf.decode(v));
                assert!((back - v).abs() < 1e-9, "{tf:?}: {v} -> {back}");
            }
        }
    }

    #[test]
    fn anchor_points() {
        // 18% grey code values published by the vendors.
        let cases = [
            (TransferFunction::SLog3, 420.0 / 1023.0),
            (TransferFunction::LogC3, 0.391_006_832_034_084),
            (TransferFunction::LogC4, 0.278_395_836_548),
            (TransferFunction::VLog, 0.423_311_448_760_136),
            (TransferFunction::AcesCct, 0.413_588_402_492_442),
            (TransferFunction::CanonLog3, 0.343_389_370_374),
        ];
        for (tf, expected) in cases {
            let got = tf.encode(0.18);
            assert!((got - expected).abs() < 1e-6, "{tf:?}: {got} vs {expected}");
        }
        assert!((TransferFunction::Srgb.encode(1.0) - 1.0).abs() < 1e-12);
        assert!((TransferFunction::Rec709.encode(1.0) - 1.0).abs() < 1e-12);
        assert!((TransferFunction::Pq.encode(100.0) - 1.0).abs() < 1e-12);
        assert!((TransferFunction::Pq.encode(1.0) - 0.508_078_421_517_399_5).abs() < 1e-9);
        assert!((TransferFunction::Hlg.encode(1.0) - 1.0).abs() < 1e-7);
        assert!((TransferFunction::Hlg.encode(1.0 / 12.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn display_curves_are_odd() {
        for tf in [
            TransferFunction::Srgb,
            TransferFunction::Rec709,
            TransferFunction::Gamma(2.2),
            TransferFunction::Pq,
            TransferFunction::Hlg,
        ] {
            for v in [0.01, 0.2, 0.7, 1.3] {
                assert_eq!(tf.decode(-v), -tf.decode(v));
                assert_eq!(tf.encode(-v), -tf.encode(v));
            }
        }
    }
}
