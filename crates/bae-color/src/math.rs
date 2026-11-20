//! Small f64 linear algebra: 3×3 matrices and 3×4 affine maps.
//!
//! All matrix work is done in f64 so that chains of conversions (for example
//! D65 → ACES white → D65 adaptations) cancel to the identity within ~1e-15
//! before they are rounded to f32 for the GPU.

use serde::{Deserialize, Serialize};

/// Row-major 3×3 matrix: `out[i] = Σ_j m[i][j] * v[j]`.
pub type Mat3 = [[f64; 3]; 3];

/// The 3×3 identity.
pub const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// `a * b` (apply `b` first, then `a`).
pub fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

/// `m * v`.
pub fn mul_vec(m: &Mat3, v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// Determinant.
pub fn det(m: &Mat3) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Inverse, or `None` when the matrix is (numerically) singular.
pub fn inverse(m: &Mat3) -> Option<Mat3> {
    let d = det(m);
    let scale = m.iter().flatten().fold(0.0f64, |a, b| a.max(b.abs()));
    if !d.is_finite() || d.abs() <= f64::EPSILON * scale * scale * scale {
        return None;
    }
    let inv_d = 1.0 / d;
    Some([
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv_d,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_d,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_d,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv_d,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_d,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_d,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv_d,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_d,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_d,
        ],
    ])
}

/// Diagonal matrix.
pub fn diag(d: [f64; 3]) -> Mat3 {
    [[d[0], 0.0, 0.0], [0.0, d[1], 0.0], [0.0, 0.0, d[2]]]
}

/// Affine colour map `out = m * rgb + offset` (OCIO `MatrixTransform` with offset).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affine {
    /// Row-major 3×3 part.
    pub m: Mat3,
    /// Added after the matrix.
    pub offset: [f64; 3],
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    /// The identity map.
    pub const IDENTITY: Affine = Affine {
        m: IDENTITY,
        offset: [0.0; 3],
    };

    /// A pure 3×3 matrix.
    pub const fn from_mat3(m: Mat3) -> Self {
        Affine {
            m,
            offset: [0.0; 3],
        }
    }

    /// Per-channel scale.
    pub fn scale(s: [f64; 3]) -> Self {
        Self::from_mat3(diag(s))
    }

    /// `self` followed by `next`.
    pub fn then(&self, next: &Affine) -> Affine {
        let o = mul_vec(&next.m, self.offset);
        Affine {
            m: mul(&next.m, &self.m),
            offset: [
                o[0] + next.offset[0],
                o[1] + next.offset[1],
                o[2] + next.offset[2],
            ],
        }
    }

    /// Inverse map, or `None` when the matrix is singular.
    pub fn inverse(&self) -> Option<Affine> {
        let inv = inverse(&self.m)?;
        let o = mul_vec(&inv, self.offset);
        Some(Affine {
            m: inv,
            offset: [-o[0], -o[1], -o[2]],
        })
    }

    /// Applies the map.
    pub fn apply(&self, v: [f64; 3]) -> [f64; 3] {
        let o = mul_vec(&self.m, v);
        [
            o[0] + self.offset[0],
            o[1] + self.offset[1],
            o[2] + self.offset[2],
        ]
    }

    /// `true` when every coefficient is within `eps` of the identity.
    pub fn is_identity(&self, eps: f64) -> bool {
        (0..3).all(|i| {
            self.offset[i].abs() <= eps
                && (0..3).all(|j| (self.m[i][j] - IDENTITY[i][j]).abs() <= eps)
        })
    }

    /// `true` when the matrix is diagonal and there is no offset (a per-channel
    /// scale, which commutes with per-channel curves only in special cases).
    pub fn is_diagonal(&self) -> bool {
        self.offset == [0.0; 3] && (0..3).all(|i| (0..3).all(|j| i == j || self.m[i][j] == 0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_roundtrip() {
        let m = [
            [0.4124, 0.3576, 0.1805],
            [0.2126, 0.7152, 0.0722],
            [0.0193, 0.1192, 0.9505],
        ];
        let p = mul(&m, &inverse(&m).unwrap());
        assert!(Affine::from_mat3(p).is_identity(1e-12));
        assert!(inverse(&[[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [0.0, 0.0, 1.0]]).is_none());
    }

    #[test]
    fn affine_compose_and_invert() {
        let a = Affine {
            m: [[2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [1.0, 0.0, 1.0]],
            offset: [0.5, -1.0, 0.25],
        };
        let b = Affine::scale([0.5, 2.0, 4.0]);
        let v = [0.3, 0.7, -0.2];
        let ab = a.then(&b);
        let direct = b.apply(a.apply(v));
        let merged = ab.apply(v);
        for i in 0..3 {
            assert!((direct[i] - merged[i]).abs() < 1e-14);
        }
        assert!(a.then(&a.inverse().unwrap()).is_identity(1e-14));
    }
}
