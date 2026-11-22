//! Colour transforms: an ordered list of [`ColorOp`]s that is kept merged.
//!
//! "Adjacent transforms merge into one": [`ColorTransform::then`] concatenates
//! two transforms and re-optimizes the seam, so a render-graph chain such as
//! `footage → ACES2065-1 → working → display` collapses to the minimal op list
//! (for example S-Log3 decode + one 3×3 matrix + sRGB encode). The rules are
//! exact, never approximations:
//!
//! * consecutive matrices multiply into one (in f64);
//! * identity matrices and linear curves disappear;
//! * `decode(f)` followed by `encode(f)` (and the reverse) cancel when `f` is
//!   exactly invertible on the whole real line ([`TransferFunction::is_exactly_invertible`]);
//! * pure gamma curves fold into one gamma;
//! * consecutive clamps intersect.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rustc_hash::FxHasher;

use crate::ColorError;
use crate::lut::{Lut1d, Lut3d};
use crate::math::Affine;
use crate::ops::{CdlOp, LogOp, MoncurveOp, PowerOp, hash3};
use crate::transfer::TransferFunction;

/// Matrix coefficients closer than this to the identity are treated as identity.
/// Far below f32 resolution, far above f64 round-off of adaptation round trips.
pub const IDENTITY_EPSILON: f64 = 1e-10;

/// One step of a colour transform. `#[non_exhaustive]`: more ops (OCIO fixed
/// functions, ACES output transforms) are added later.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ColorOp {
    /// `rgb = m * rgb + offset`.
    Matrix(Affine),
    /// Encoded → linear.
    Decode(TransferFunction),
    /// Linear → encoded.
    Encode(TransferFunction),
    /// Per-channel 1D LUT.
    Lut1d(Arc<Lut1d>),
    /// Exact inverse of a per-channel 1D LUT (binary search, as OCIO).
    Lut1dInverse(Arc<Lut1d>),
    /// 3D LUT.
    Lut3d(Arc<Lut3d>),
    /// Per-channel clamp to `[min, max]` (either bound may be infinite).
    Clamp {
        /// Lower bound.
        min: f64,
        /// Upper bound.
        max: f64,
    },
    /// Per-channel power (OCIO ExponentTransform).
    Power(PowerOp),
    /// Power with a linear toe (OCIO ExponentWithLinearTransform).
    Moncurve(MoncurveOp),
    /// Log / LogAffine / LogCamera.
    Log(LogOp),
    /// ASC CDL.
    Cdl(CdlOp),
}

impl ColorOp {
    /// Applies the op on the CPU (f64 reference).
    pub fn apply(&self, rgb: [f64; 3]) -> [f64; 3] {
        match self {
            ColorOp::Matrix(a) => a.apply(rgb),
            ColorOp::Decode(tf) => rgb.map(|v| tf.decode(v)),
            ColorOp::Encode(tf) => rgb.map(|v| tf.encode(v)),
            ColorOp::Lut1d(l) => l.eval(rgb),
            ColorOp::Lut1dInverse(l) => l.eval_inverse(rgb),
            ColorOp::Lut3d(l) => l.eval(rgb),
            ColorOp::Clamp { min, max } => rgb.map(|v| v.max(*min).min(*max)),
            ColorOp::Power(p) => p.apply(rgb),
            ColorOp::Moncurve(m) => m.apply(rgb),
            ColorOp::Log(l) => l.apply(rgb),
            ColorOp::Cdl(c) => c.apply(rgb),
        }
    }

    /// The inverse op.
    pub fn inverse(&self) -> Result<ColorOp, ColorError> {
        Ok(match self {
            ColorOp::Matrix(a) => ColorOp::Matrix(
                a.inverse()
                    .ok_or(ColorError::NotInvertible("singular matrix"))?,
            ),
            ColorOp::Decode(tf) => ColorOp::Encode(*tf),
            ColorOp::Encode(tf) => ColorOp::Decode(*tf),
            ColorOp::Lut1d(l) => ColorOp::Lut1dInverse(l.clone()),
            ColorOp::Lut1dInverse(l) => ColorOp::Lut1d(l.clone()),
            ColorOp::Lut3d(_) => return Err(ColorError::NotInvertible("3D LUT")),
            ColorOp::Clamp { min, max } => ColorOp::Clamp {
                min: *min,
                max: *max,
            },
            ColorOp::Power(p) => ColorOp::Power(p.inverse()),
            ColorOp::Moncurve(m) => ColorOp::Moncurve(m.inverse()),
            ColorOp::Log(l) => ColorOp::Log(l.inverse()),
            ColorOp::Cdl(c) => ColorOp::Cdl(c.inverse()),
        })
    }

    /// Hashes what determines the generated WGSL (op kinds, curve kinds, LUT
    /// interpolation) but not the numbers that travel in uniforms or textures.
    pub(crate) fn hash_structure<H: Hasher>(&self, h: &mut H) {
        std::mem::discriminant(self).hash(h);
        match self {
            ColorOp::Decode(tf) | ColorOp::Encode(tf) => std::mem::discriminant(tf).hash(h),
            ColorOp::Lut3d(l) => l.interpolation.hash(h),
            ColorOp::Power(p) => p.negative.hash(h),
            ColorOp::Moncurve(m) => (m.negative, m.inverse).hash(h),
            ColorOp::Log(l) => (l.lin_break.is_some(), l.inverse).hash(h),
            ColorOp::Cdl(c) => (c.clamp, c.inverse).hash(h),
            ColorOp::Matrix(_)
            | ColorOp::Lut1d(_)
            | ColorOp::Lut1dInverse(_)
            | ColorOp::Clamp { .. } => {}
        }
    }

    /// Hashes everything, including numbers and LUT contents.
    pub(crate) fn hash_content<H: Hasher>(&self, h: &mut H) {
        self.hash_structure(h);
        match self {
            ColorOp::Matrix(a) => {
                for row in &a.m {
                    row.map(f64::to_bits).hash(h);
                }
                a.offset.map(f64::to_bits).hash(h);
            }
            ColorOp::Decode(tf) | ColorOp::Encode(tf) => tf.hash(h),
            ColorOp::Lut1d(l) | ColorOp::Lut1dInverse(l) => l.hash_data(h),
            ColorOp::Lut3d(l) => l.hash_data(h),
            ColorOp::Clamp { min, max } => {
                min.to_bits().hash(h);
                max.to_bits().hash(h);
            }
            ColorOp::Power(p) => hash3(&p.exponent, h),
            ColorOp::Moncurve(m) => {
                hash3(&m.gamma, h);
                hash3(&m.offset, h);
            }
            ColorOp::Log(l) => {
                l.base.to_bits().hash(h);
                for v in [l.log_slope, l.log_offset, l.lin_slope, l.lin_offset] {
                    hash3(&v, h);
                }
                if let Some(b) = l.lin_break {
                    hash3(&b, h);
                }
                if let Some(s) = l.linear_slope {
                    hash3(&s, h);
                }
            }
            ColorOp::Cdl(c) => {
                for v in [c.slope, c.offset, c.power] {
                    hash3(&v, h);
                }
                c.saturation.to_bits().hash(h);
            }
        }
    }

    fn is_noop(&self) -> bool {
        match self {
            ColorOp::Matrix(a) => a.is_identity(IDENTITY_EPSILON),
            ColorOp::Decode(tf) | ColorOp::Encode(tf) => tf.is_linear(),
            ColorOp::Clamp { min, max } => *min == f64::NEG_INFINITY && *max == f64::INFINITY,
            ColorOp::Power(p) => p.is_identity(),
            ColorOp::Lut1d(_)
            | ColorOp::Lut1dInverse(_)
            | ColorOp::Lut3d(_)
            | ColorOp::Moncurve(_)
            | ColorOp::Log(_)
            | ColorOp::Cdl(_) => false,
        }
    }
}

/// Tries to merge `b` into `a` (a runs first). `Some(None)`: both cancel;
/// `Some(Some(op))`: replaced by one op; `None`: no rule applies.
fn merge(a: &ColorOp, b: &ColorOp) -> Option<Option<ColorOp>> {
    use ColorOp::*;
    use TransferFunction::Gamma;
    let keep = |op: ColorOp| Some(if op.is_noop() { None } else { Some(op) });
    match (a, b) {
        (Matrix(x), Matrix(y)) => keep(Matrix(x.then(y))),
        (Decode(f), Encode(g)) | (Encode(f), Decode(g)) if f == g && f.is_exactly_invertible() => {
            Some(None)
        }
        (Moncurve(x), Moncurve(y)) if *y == x.inverse() => Some(None),
        (Log(x), Log(y)) if *y == x.inverse() && x.is_exactly_invertible() => Some(None),
        (Power(x), Power(y))
            if x.negative == y.negative && x.negative != crate::ops::NegativeStyle::Clamp =>
        {
            let e = [0, 1, 2].map(|c| x.exponent[c] * y.exponent[c]);
            keep(Power(PowerOp {
                exponent: e,
                negative: x.negative,
            }))
        }
        // x^p then x^(1/q) = x^(p/q); all curves are odd-mirrored, so this is exact.
        (Decode(Gamma(p)), Encode(Gamma(q))) => keep(Decode(Gamma(p / q))),
        (Encode(Gamma(p)), Decode(Gamma(q))) => keep(Decode(Gamma(q / p))),
        (Decode(Gamma(p)), Decode(Gamma(q))) => keep(Decode(Gamma(p * q))),
        (Encode(Gamma(p)), Encode(Gamma(q))) => keep(Encode(Gamma(p * q))),
        (Clamp { min: a0, max: a1 }, Clamp { min: b0, max: b1 }) => {
            let (min, max) = (a0.max(*b0), a1.min(*b1));
            if min <= max {
                keep(Clamp { min, max })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// An optimized chain of colour ops. Cheap to clone (LUTs are shared).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColorTransform {
    ops: Vec<ColorOp>,
}

impl ColorTransform {
    /// The identity transform.
    pub const fn identity() -> Self {
        ColorTransform { ops: Vec::new() }
    }

    /// Builds a transform from ops (optimized).
    pub fn from_ops(ops: impl IntoIterator<Item = ColorOp>) -> Self {
        let mut t = ColorTransform::identity();
        for op in ops {
            t.push(op);
        }
        t
    }

    /// A single matrix.
    pub fn matrix(m: Affine) -> Self {
        Self::from_ops([ColorOp::Matrix(m)])
    }

    /// The ops, in application order.
    pub fn ops(&self) -> &[ColorOp] {
        &self.ops
    }

    /// Appends an op, merging it with the tail where possible.
    pub fn push(&mut self, op: ColorOp) {
        if op.is_noop() {
            return;
        }
        let mut pending = Some(op);
        while let Some(op) = pending.take() {
            match self.ops.last().and_then(|last| merge(last, &op)) {
                Some(merged) => {
                    self.ops.pop();
                    // The merged op may now merge with the new tail.
                    pending = merged;
                }
                None => self.ops.push(op),
            }
        }
    }

    /// `self` followed by `next`, merged at the seam.
    pub fn then(&self, next: &ColorTransform) -> ColorTransform {
        let mut out = self.clone();
        for op in &next.ops {
            out.push(op.clone());
        }
        out
    }

    /// The inverse transform.
    pub fn inverse(&self) -> Result<ColorTransform, ColorError> {
        let mut out = ColorTransform::identity();
        for op in self.ops.iter().rev() {
            out.push(op.inverse()?);
        }
        Ok(out)
    }

    /// `true` when the transform does nothing (skip the pass entirely).
    pub fn is_identity(&self) -> bool {
        self.ops.is_empty()
    }

    /// `true` when the transform is a pure 3×3 matrix without offset (or
    /// identity): it commutes with alpha premultiplication, so premultiplied
    /// pixels need no unpremultiply.
    pub fn is_linear(&self) -> bool {
        self.ops
            .iter()
            .all(|op| matches!(op, ColorOp::Matrix(a) if a.offset == [0.0; 3]))
    }

    /// The single matrix, when the transform is exactly one matrix op (or
    /// identity). Lets a consumer fold it into its own maths.
    pub fn as_matrix(&self) -> Option<Affine> {
        match self.ops.as_slice() {
            [] => Some(Affine::IDENTITY),
            [ColorOp::Matrix(a)] => Some(*a),
            _ => None,
        }
    }

    /// CPU reference evaluation in f64.
    pub fn apply_f64(&self, rgb: [f64; 3]) -> [f64; 3] {
        self.ops.iter().fold(rgb, |c, op| op.apply(c))
    }

    /// CPU evaluation of one f32 RGB triple.
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let o = self.apply_f64(rgb.map(f64::from));
        o.map(|v| v as f32)
    }

    /// CPU evaluation of premultiplied RGBA pixels in place (unpremultiplies
    /// around non-linear ops; alpha 0 pixels are left untouched).
    pub fn apply_premultiplied(&self, pixels: &mut [[f32; 4]]) {
        if self.is_identity() {
            return;
        }
        let linear = self.is_linear();
        for p in pixels {
            let a = p[3];
            if linear {
                let o = self.apply([p[0], p[1], p[2]]);
                p[..3].copy_from_slice(&o);
            } else if a > 0.0 {
                let o = self.apply([p[0] / a, p[1] / a, p[2] / a]);
                p[0] = o[0] * a;
                p[1] = o[1] * a;
                p[2] = o[2] * a;
            }
        }
    }

    /// Hash of the shader structure: transforms with equal keys generate the
    /// same WGSL (for the same prefix) and differ only in uniform / texture
    /// data, so a render graph can reuse one pipeline. Use it in pipeline cache
    /// keys.
    pub fn structure_key(&self) -> u64 {
        let mut h = FxHasher::default();
        self.ops.len().hash(&mut h);
        for op in &self.ops {
            op.hash_structure(&mut h);
        }
        h.finish()
    }

    /// Hash of everything that affects the output pixels (frame cache keys).
    pub fn content_hash(&self) -> u64 {
        let mut h = FxHasher::default();
        self.ops.len().hash(&mut h);
        for op in &self.ops {
            op.hash_content(&mut h);
        }
        h.finish()
    }
}

impl Hash for ColorTransform {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.content_hash().hash(state);
    }
}

impl Eq for ColorTransform {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Mat3;

    const M: Mat3 = [[0.6, 0.3, 0.1], [0.2, 0.7, 0.1], [0.05, 0.15, 0.8]];

    #[test]
    fn matrices_merge_and_cancel() {
        let a = Affine::from_mat3(M);
        let t = ColorTransform::from_ops([
            ColorOp::Decode(TransferFunction::Srgb),
            ColorOp::Matrix(a),
            ColorOp::Matrix(a.inverse().unwrap()),
            ColorOp::Encode(TransferFunction::Srgb),
        ]);
        assert!(t.is_identity(), "{t:?}");
    }

    #[test]
    fn curves_do_not_cancel_across_matrices() {
        let t = ColorTransform::from_ops([
            ColorOp::Decode(TransferFunction::Srgb),
            ColorOp::Matrix(Affine::from_mat3(M)),
            ColorOp::Encode(TransferFunction::Srgb),
        ]);
        assert_eq!(t.ops().len(), 3);
        assert!(!t.is_linear());
    }

    #[test]
    fn then_merges_the_seam() {
        let a = ColorTransform::from_ops([
            ColorOp::Decode(TransferFunction::SLog3),
            ColorOp::Matrix(Affine::from_mat3(M)),
        ]);
        let b = ColorTransform::from_ops([
            ColorOp::Matrix(Affine::scale([2.0, 2.0, 2.0])),
            ColorOp::Encode(TransferFunction::Srgb),
        ]);
        let ab = a.then(&b);
        assert_eq!(ab.ops().len(), 3);
        let v = [0.4, 0.5, 0.6];
        let direct = b.apply_f64(a.apply_f64(v));
        let merged = ab.apply_f64(v);
        for i in 0..3 {
            assert!((direct[i] - merged[i]).abs() < 1e-12);
        }
    }

    #[test]
    fn gammas_fold() {
        let t = ColorTransform::from_ops([
            ColorOp::Decode(TransferFunction::Gamma(2.4)),
            ColorOp::Encode(TransferFunction::Gamma(2.2)),
        ]);
        assert_eq!(
            t.ops(),
            &[ColorOp::Decode(TransferFunction::Gamma(2.4 / 2.2))]
        );
        let t = ColorTransform::from_ops([
            ColorOp::Decode(TransferFunction::Gamma(2.2)),
            ColorOp::Encode(TransferFunction::Gamma(2.2)),
        ]);
        assert!(t.is_identity());
    }

    #[test]
    fn acescc_never_cancels() {
        let t = ColorTransform::from_ops([
            ColorOp::Encode(TransferFunction::AcesCc),
            ColorOp::Decode(TransferFunction::AcesCc),
        ]);
        assert_eq!(t.ops().len(), 2);
    }

    #[test]
    fn inverse_roundtrips() {
        let t = ColorTransform::from_ops([
            ColorOp::Decode(TransferFunction::LogC3),
            ColorOp::Matrix(Affine::from_mat3(M)),
            ColorOp::Encode(TransferFunction::Pq),
        ]);
        let inv = t.inverse().unwrap();
        assert!(t.then(&inv).is_identity());
        let v = [0.3, 0.45, 0.6];
        let back = inv.apply_f64(t.apply_f64(v));
        for i in 0..3 {
            assert!((back[i] - v[i]).abs() < 1e-9);
        }
    }

    #[test]
    fn structure_key_ignores_numbers() {
        let a = ColorTransform::from_ops([
            ColorOp::Matrix(Affine::from_mat3(M)),
            ColorOp::Encode(TransferFunction::Gamma(2.2)),
        ]);
        let b = ColorTransform::from_ops([
            ColorOp::Matrix(Affine::scale([0.5, 1.0, 2.0])),
            ColorOp::Encode(TransferFunction::Gamma(2.4)),
        ]);
        assert_eq!(a.structure_key(), b.structure_key());
        assert_ne!(a.content_hash(), b.content_hash());
    }
}
