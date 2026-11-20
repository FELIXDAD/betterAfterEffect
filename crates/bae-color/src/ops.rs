//! Parametric per-channel ops used by OpenColorIO configs (ExponentTransform,
//! ExponentWithLinearTransform, Log / LogAffine / LogCameraTransform,
//! CDLTransform). Formulas follow the OCIO 2 specification; every parameter is
//! per channel, so they map onto `vec3` maths on the GPU.

use std::hash::{Hash, Hasher};

/// How a power curve treats negative inputs (OCIO `style` / `negative_style`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NegativeStyle {
    /// Negative inputs become 0 (OCIO `basic`, the ExponentTransform default).
    #[default]
    Clamp,
    /// `f(-x) = -f(x)`.
    Mirror,
    /// Negative inputs pass through unchanged.
    PassThru,
    /// The linear toe segment continues below zero (ExponentWithLinear default).
    Linear,
}

/// `out = in^exponent` per channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerOp {
    /// Exponent per channel.
    pub exponent: [f64; 3],
    /// Negative handling (`Linear` behaves like `PassThru`).
    pub negative: NegativeStyle,
}

impl PowerOp {
    /// CPU evaluation.
    pub fn apply(&self, rgb: [f64; 3]) -> [f64; 3] {
        let mut o = rgb;
        for c in 0..3 {
            let (v, e) = (rgb[c], self.exponent[c]);
            o[c] = if v >= 0.0 {
                v.powf(e)
            } else {
                match self.negative {
                    NegativeStyle::Clamp => 0.0,
                    NegativeStyle::Mirror => -(-v).powf(e),
                    NegativeStyle::PassThru | NegativeStyle::Linear => v,
                }
            };
        }
        o
    }

    /// Inverse (reciprocal exponents).
    pub fn inverse(&self) -> PowerOp {
        PowerOp {
            exponent: self.exponent.map(|e| 1.0 / e),
            negative: self.negative,
        }
    }

    /// `true` when every exponent is 1.
    pub fn is_identity(&self) -> bool {
        self.exponent == [1.0; 3] && self.negative != NegativeStyle::Clamp
    }
}

/// OCIO ExponentWithLinearTransform ("moncurve"): forward (`inverse == false`)
/// decodes, `out = ((in + offset) / (1 + offset))^gamma` above the break and a
/// linear segment below it, with break and slope chosen for C1 continuity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoncurveOp {
    /// Gamma per channel (> 1).
    pub gamma: [f64; 3],
    /// Offset per channel (> 0).
    pub offset: [f64; 3],
    /// `Linear` (default) or `Mirror`.
    pub negative: NegativeStyle,
    /// `true` = encode direction.
    pub inverse: bool,
}

impl MoncurveOp {
    /// (break, slope) of the linear segment for channel `c`, in the input
    /// domain of the current direction.
    pub fn segment(&self, c: usize) -> (f64, f64) {
        let (g, o) = (self.gamma[c], self.offset[c]);
        if !self.inverse {
            let xb = o / (g - 1.0);
            let fs = ((g - 1.0) / o) * (o * g / ((g - 1.0) * (1.0 + o))).powf(g);
            (xb, fs)
        } else {
            let yb = (o * g / ((g - 1.0) * (1.0 + o))).powf(g);
            let rs = ((g - 1.0) / o).powf(g - 1.0) * ((1.0 + o) / g).powf(g);
            (yb, rs)
        }
    }

    fn eval(&self, c: usize, x: f64) -> f64 {
        let (g, o) = (self.gamma[c], self.offset[c]);
        let (brk, slope) = self.segment(c);
        if x < brk {
            x * slope
        } else if !self.inverse {
            ((x + o) / (1.0 + o)).powf(g)
        } else {
            (1.0 + o) * x.powf(1.0 / g) - o
        }
    }

    /// CPU evaluation.
    pub fn apply(&self, rgb: [f64; 3]) -> [f64; 3] {
        let mut out = rgb;
        for (c, o) in out.iter_mut().enumerate() {
            let v = rgb[c];
            *o = if self.negative == NegativeStyle::Mirror && v < 0.0 {
                -self.eval(c, -v)
            } else {
                self.eval(c, v)
            };
        }
        out
    }

    /// The other direction.
    pub fn inverse(&self) -> MoncurveOp {
        MoncurveOp {
            inverse: !self.inverse,
            ..*self
        }
    }
}

/// OCIO Log / LogAffine / LogCamera transform. Forward (`inverse == false`)
/// maps linear to log:
///
/// ```text
/// log(x) = log_slope * log_base(max(lin_slope * x + lin_offset, FLT_MIN)) + log_offset
/// camera: x <= lin_break ? linear_slope * x + linear_offset : log(x)
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogOp {
    /// Logarithm base.
    pub base: f64,
    /// Log-side slope.
    pub log_slope: [f64; 3],
    /// Log-side offset.
    pub log_offset: [f64; 3],
    /// Linear-side slope.
    pub lin_slope: [f64; 3],
    /// Linear-side offset.
    pub lin_offset: [f64; 3],
    /// LogCamera break point (linear side); `None` for Log / LogAffine.
    pub lin_break: Option<[f64; 3]>,
    /// LogCamera toe slope; `None` = derived for slope continuity.
    pub linear_slope: Option<[f64; 3]>,
    /// `true` = log → linear.
    pub inverse: bool,
}

/// The smallest normal f32 (OCIO clamps log arguments to it).
pub const FLT_MIN: f64 = 1.175_494_350_822_287_5e-38;

impl LogOp {
    /// `log_base(x)` (OCIO LogTransform).
    pub fn log(base: f64) -> Self {
        LogOp {
            base,
            log_slope: [1.0; 3],
            log_offset: [0.0; 3],
            lin_slope: [1.0; 3],
            lin_offset: [0.0; 3],
            lin_break: None,
            linear_slope: None,
            inverse: false,
        }
    }

    fn affine(&self, c: usize, x: f64) -> f64 {
        self.log_slope[c]
            * (self.lin_slope[c] * x + self.lin_offset[c])
                .max(FLT_MIN)
                .ln()
            / self.base.ln()
            + self.log_offset[c]
    }

    fn affine_inv(&self, c: usize, y: f64) -> f64 {
        (self.base.powf((y - self.log_offset[c]) / self.log_slope[c]) - self.lin_offset[c])
            / self.lin_slope[c]
    }

    /// The camera toe for channel `c`: (linear-side break, log-side break,
    /// slope, offset); `None` without a break.
    pub fn toe(&self, c: usize) -> Option<(f64, f64, f64, f64)> {
        let brk = self.lin_break?[c];
        let slope = match self.linear_slope {
            Some(s) => s[c],
            None => {
                self.log_slope[c] * self.lin_slope[c]
                    / ((self.lin_slope[c] * brk + self.lin_offset[c]) * self.base.ln())
            }
        };
        let log_brk = self.affine(c, brk);
        Some((brk, log_brk, slope, log_brk - slope * brk))
    }

    /// CPU evaluation.
    pub fn apply(&self, rgb: [f64; 3]) -> [f64; 3] {
        let mut out = rgb;
        for (c, o) in out.iter_mut().enumerate() {
            let v = rgb[c];
            *o = match (self.toe(c), self.inverse) {
                (Some((brk, _, s, off)), false) if v <= brk => s * v + off,
                (_, false) => self.affine(c, v),
                (Some((_, lbrk, s, off)), true) if v <= lbrk => (v - off) / s,
                (_, true) => self.affine_inv(c, v),
            };
        }
        out
    }

    /// The other direction.
    pub fn inverse(&self) -> LogOp {
        LogOp {
            inverse: !self.inverse,
            ..*self
        }
    }

    /// `true` when forward and inverse cancel exactly on the whole real line
    /// (only with a camera toe; plain logs clamp their argument).
    pub fn is_exactly_invertible(&self) -> bool {
        self.lin_break.is_some()
    }
}

/// ASC CDL (slope, offset, power, saturation), OCIO CDLTransform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CdlOp {
    /// Slope per channel.
    pub slope: [f64; 3],
    /// Offset per channel.
    pub offset: [f64; 3],
    /// Power per channel.
    pub power: [f64; 3],
    /// Saturation (Rec.709 luma weights).
    pub saturation: f64,
    /// ASC style (clamp to [0, 1]); `false` = OCIO `noClamp`.
    pub clamp: bool,
    /// `true` = inverse direction.
    pub inverse: bool,
}

/// Rec.709 luma weights used by the ASC CDL saturation.
pub const CDL_LUMA: [f64; 3] = [0.2126, 0.7152, 0.0722];

impl CdlOp {
    fn sat(rgb: [f64; 3], s: f64) -> [f64; 3] {
        let l = rgb[0] * CDL_LUMA[0] + rgb[1] * CDL_LUMA[1] + rgb[2] * CDL_LUMA[2];
        rgb.map(|v| l + s * (v - l))
    }

    /// CPU evaluation.
    pub fn apply(&self, rgb: [f64; 3]) -> [f64; 3] {
        let c01 = |v: f64| if self.clamp { v.clamp(0.0, 1.0) } else { v };
        if !self.inverse {
            let mut v = [0.0; 3];
            for c in 0..3 {
                let x = c01(rgb[c] * self.slope[c] + self.offset[c]);
                v[c] = if x > 0.0 { x.powf(self.power[c]) } else { x };
            }
            Self::sat(v, self.saturation).map(c01)
        } else {
            // ASC: clamp in, inverse saturation, clamp, inverse power, inverse
            // slope / offset, clamp (as OCIO; noClamp skips every clamp).
            let v = Self::sat(rgb.map(c01), 1.0 / self.saturation).map(c01);
            let mut o = [0.0; 3];
            for c in 0..3 {
                let x = if v[c] > 0.0 {
                    v[c].powf(1.0 / self.power[c])
                } else {
                    v[c]
                };
                o[c] = c01((x - self.offset[c]) / self.slope[c]);
            }
            o
        }
    }

    /// The other direction.
    pub fn inverse(&self) -> CdlOp {
        CdlOp {
            inverse: !self.inverse,
            ..*self
        }
    }
}

pub(crate) fn hash3<H: Hasher>(v: &[f64; 3], h: &mut H) {
    v.map(f64::to_bits).hash(h);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moncurve_matches_srgb_shape() {
        let m = MoncurveOp {
            gamma: [2.4; 3],
            offset: [0.055; 3],
            negative: NegativeStyle::Linear,
            inverse: false,
        };
        // Continuous at the break and close to the IEC curve.
        let (xb, fs) = m.segment(0);
        let above = ((xb + 0.055) / 1.055f64).powf(2.4);
        assert!((above - xb * fs).abs() < 1e-12);
        for v in [0.01, 0.2, 0.5, 0.9] {
            let iec = crate::TransferFunction::Srgb.decode(v);
            assert!((m.apply([v; 3])[0] - iec).abs() < 1e-4);
            let back = m.inverse().apply(m.apply([v; 3]));
            assert!((back[0] - v).abs() < 1e-12);
        }
    }

    #[test]
    fn logcamera_matches_slog3() {
        // OCIO's S-Log3 LogCamera parameters (inverse = log → linear).
        let op = LogOp {
            base: 10.0,
            log_slope: [0.255_620_723_362_659; 3],
            log_offset: [0.410_557_184_750_733; 3],
            lin_slope: [5.263_157_894_736_84; 3],
            lin_offset: [0.052_631_578_947_368; 3],
            lin_break: Some([0.01125; 3]),
            linear_slope: Some([6.621_944_237_524_86; 3]),
            inverse: false,
        };
        for l in [-0.01, 0.0, 0.005, 0.18, 1.0, 10.0] {
            let ours = op.apply([l; 3])[0];
            let reference = crate::TransferFunction::SLog3.encode(l);
            assert!(
                (ours - reference).abs() < 1e-6,
                "{l}: {ours} vs {reference}"
            );
            assert!((op.inverse().apply([ours; 3])[0] - l).abs() < 1e-9);
        }
    }

    #[test]
    fn cdl_roundtrip() {
        let op = CdlOp {
            slope: [1.1, 0.9, 1.0],
            offset: [0.01, -0.02, 0.0],
            power: [1.2, 0.8, 1.0],
            saturation: 0.9,
            clamp: true,
            inverse: false,
        };
        let v = [0.3, 0.5, 0.7];
        let back = op.inverse().apply(op.apply(v));
        for c in 0..3 {
            assert!((back[c] - v[c]).abs() < 1e-9);
        }
    }
}
