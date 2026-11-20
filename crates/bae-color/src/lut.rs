//! 1D and 3D lookup tables (data and CPU evaluation).

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

/// How a 3D LUT is interpolated. Both modes exist on the GPU too.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    /// Trilinear (OpenColorIO's default for 3D LUTs).
    #[default]
    Linear,
    /// Tetrahedral (smoother, OCIO `interpolation: tetrahedral`).
    Tetrahedral,
}

/// A per-channel 1D LUT: `entries[i]` is the output for input
/// `domain_min + i / (len - 1) * (domain_max - domain_min)`, linearly
/// interpolated and clamped at both ends.
#[derive(Clone, Debug, PartialEq)]
pub struct Lut1d {
    /// Output RGB per entry (at least 2 entries).
    pub entries: Vec<[f32; 3]>,
    /// Input value of the first entry, per channel.
    pub domain_min: [f32; 3],
    /// Input value of the last entry, per channel.
    pub domain_max: [f32; 3],
}

impl Lut1d {
    /// Identity LUT with `len` entries on `[0, 1]`.
    pub fn identity(len: usize) -> Self {
        let len = len.max(2);
        let entries = (0..len)
            .map(|i| {
                let x = i as f32 / (len - 1) as f32;
                [x, x, x]
            })
            .collect();
        Lut1d {
            entries,
            domain_min: [0.0; 3],
            domain_max: [1.0; 3],
        }
    }

    /// Evaluates one channel.
    pub fn eval_channel(&self, c: usize, x: f64) -> f64 {
        let n = self.entries.len();
        if n == 0 {
            return x;
        }
        if n == 1 {
            return self.entries[0][c] as f64;
        }
        let lo = self.domain_min[c] as f64;
        let hi = self.domain_max[c] as f64;
        let t = if hi > lo {
            ((x - lo) / (hi - lo)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let p = t * (n - 1) as f64;
        let i0 = (p.floor() as usize).min(n - 1);
        let i1 = (i0 + 1).min(n - 1);
        let f = p - i0 as f64;
        let a = self.entries[i0][c] as f64;
        let b = self.entries[i1][c] as f64;
        a + (b - a) * f
    }

    /// Evaluates RGB.
    pub fn eval(&self, rgb: [f64; 3]) -> [f64; 3] {
        [
            self.eval_channel(0, rgb[0]),
            self.eval_channel(1, rgb[1]),
            self.eval_channel(2, rgb[2]),
        ]
    }

    /// Numerical inverse: a new LUT sampling the inverse of each (monotonic)
    /// channel on the output range, with `len` entries. Flat or decreasing
    /// segments are handled by taking the first matching entry.
    pub fn inverse(&self, len: usize) -> Lut1d {
        let len = len.max(2);
        let mut out = vec![[0.0f32; 3]; len];
        let mut dmin = [0.0f32; 3];
        let mut dmax = [1.0f32; 3];
        for c in 0..3 {
            let col: Vec<f64> = self.entries.iter().map(|e| e[c] as f64).collect();
            let lo = col.iter().cloned().fold(f64::INFINITY, f64::min);
            let hi = col.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            dmin[c] = lo as f32;
            dmax[c] = hi as f32;
            let n = col.len();
            let increasing = col.last() >= col.first();
            for (i, slot) in out.iter_mut().enumerate() {
                let y = lo + (hi - lo) * i as f64 / (len - 1) as f64;
                // Find the segment containing y.
                let mut x = 0.0;
                for s in 0..n.saturating_sub(1) {
                    let (a, b) = if increasing {
                        (col[s], col[s + 1])
                    } else {
                        (col[s + 1], col[s])
                    };
                    let (sa, sb) = if increasing { (s, s + 1) } else { (s + 1, s) };
                    if (y >= a && y <= b) || s == n - 2 {
                        let f = if b > a {
                            ((y - a) / (b - a)).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        let pa = sa as f64 / (n - 1) as f64;
                        let pb = sb as f64 / (n - 1) as f64;
                        x = pa + (pb - pa) * f;
                        break;
                    }
                }
                let lo_in = self.domain_min[c] as f64;
                let hi_in = self.domain_max[c] as f64;
                slot[c] = (lo_in + x * (hi_in - lo_in)) as f32;
            }
        }
        Lut1d {
            entries: out,
            domain_min: dmin,
            domain_max: dmax,
        }
    }

    pub(crate) fn hash_data<H: Hasher>(&self, h: &mut H) {
        self.entries.len().hash(h);
        for e in &self.entries {
            e.map(f32::to_bits).hash(h);
        }
        self.domain_min.map(f32::to_bits).hash(h);
        self.domain_max.map(f32::to_bits).hash(h);
    }
}

/// A 3D LUT with `size³` entries. Entry `(r, g, b)` is at index
/// `r + g * size + b * size²` (red fastest, as in .cube files).
#[derive(Clone, Debug, PartialEq)]
pub struct Lut3d {
    /// Grid points per axis (≥ 2).
    pub size: usize,
    /// Output RGB, `size³` entries.
    pub data: Vec<[f32; 3]>,
    /// Input value mapped to grid index 0, per channel.
    pub domain_min: [f32; 3],
    /// Input value mapped to grid index `size - 1`, per channel.
    pub domain_max: [f32; 3],
    /// Interpolation.
    pub interpolation: Interpolation,
}

impl Lut3d {
    /// Identity LUT.
    pub fn identity(size: usize) -> Self {
        let size = size.max(2);
        let mut data = Vec::with_capacity(size * size * size);
        let s = (size - 1) as f32;
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    data.push([r as f32 / s, g as f32 / s, b as f32 / s]);
                }
            }
        }
        Lut3d {
            size,
            data,
            domain_min: [0.0; 3],
            domain_max: [1.0; 3],
            interpolation: Interpolation::Linear,
        }
    }

    /// Builds a LUT by sampling `f` on the grid.
    pub fn from_fn(size: usize, mut f: impl FnMut([f64; 3]) -> [f64; 3]) -> Self {
        let mut lut = Self::identity(size);
        for e in &mut lut.data {
            let o = f([e[0] as f64, e[1] as f64, e[2] as f64]);
            *e = [o[0] as f32, o[1] as f32, o[2] as f32];
        }
        lut
    }

    fn at(&self, r: usize, g: usize, b: usize) -> [f64; 3] {
        let e = self.data[r + g * self.size + b * self.size * self.size];
        [e[0] as f64, e[1] as f64, e[2] as f64]
    }

    /// Evaluates RGB (inputs clamped to the domain).
    pub fn eval(&self, rgb: [f64; 3]) -> [f64; 3] {
        let n = self.size;
        if n < 2 || self.data.len() != n * n * n {
            return rgb;
        }
        let mut i0 = [0usize; 3];
        let mut i1 = [0usize; 3];
        let mut f = [0.0f64; 3];
        for c in 0..3 {
            let lo = self.domain_min[c] as f64;
            let hi = self.domain_max[c] as f64;
            let t = if hi > lo {
                ((rgb[c] - lo) / (hi - lo)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let p = t * (n - 1) as f64;
            i0[c] = (p.floor() as usize).min(n - 2);
            i1[c] = i0[c] + 1;
            f[c] = p - i0[c] as f64;
        }
        let c = |r: bool, g: bool, b: bool| {
            self.at(
                if r { i1[0] } else { i0[0] },
                if g { i1[1] } else { i0[1] },
                if b { i1[2] } else { i0[2] },
            )
        };
        let lerp = |a: [f64; 3], b: [f64; 3], t: f64| {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]
        };
        match self.interpolation {
            Interpolation::Linear => {
                let c00 = lerp(c(false, false, false), c(true, false, false), f[0]);
                let c10 = lerp(c(false, true, false), c(true, true, false), f[0]);
                let c01 = lerp(c(false, false, true), c(true, false, true), f[0]);
                let c11 = lerp(c(false, true, true), c(true, true, true), f[0]);
                lerp(lerp(c00, c10, f[1]), lerp(c01, c11, f[1]), f[2])
            }
            Interpolation::Tetrahedral => tetrahedral(f, c),
        }
    }

    pub(crate) fn hash_data<H: Hasher>(&self, h: &mut H) {
        self.size.hash(h);
        for e in &self.data {
            e.map(f32::to_bits).hash(h);
        }
        self.domain_min.map(f32::to_bits).hash(h);
        self.domain_max.map(f32::to_bits).hash(h);
        self.interpolation.hash(h);
    }
}

/// Tetrahedral interpolation of the unit cube with corner lookup `c`.
fn tetrahedral(f: [f64; 3], c: impl Fn(bool, bool, bool) -> [f64; 3]) -> [f64; 3] {
    let [fr, fg, fb] = f;
    let c000 = c(false, false, false);
    let c111 = c(true, true, true);
    // Pick the tetrahedron by the ordering of the fractions; result =
    // c000 + w1*(A - c000) + w2*(B - A) + w3*(c111 - B).
    let (a, b, w1, w2, w3) = if fr > fg {
        if fg > fb {
            (c(true, false, false), c(true, true, false), fr, fg, fb)
        } else if fr > fb {
            (c(true, false, false), c(true, false, true), fr, fb, fg)
        } else {
            (c(false, false, true), c(true, false, true), fb, fr, fg)
        }
    } else if fb > fg {
        (c(false, false, true), c(false, true, true), fb, fg, fr)
    } else if fb > fr {
        (c(false, true, false), c(false, true, true), fg, fb, fr)
    } else {
        (c(false, true, false), c(true, true, false), fg, fr, fb)
    };
    let mut out = [0.0; 3];
    for i in 0..3 {
        out[i] = c000[i] + w1 * (a[i] - c000[i]) + w2 * (b[i] - a[i]) + w3 * (c111[i] - b[i]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_luts_are_identity() {
        let l1 = Lut1d::identity(17);
        let mut l3 = Lut3d::identity(9);
        for v in [[0.0, 0.5, 1.0], [0.123, 0.777, 0.31], [0.9, 0.01, 0.4]] {
            let o = l1.eval(v);
            for c in 0..3 {
                assert!((o[c] - v[c]).abs() < 1e-6);
            }
            for interp in [Interpolation::Linear, Interpolation::Tetrahedral] {
                l3.interpolation = interp;
                let o = l3.eval(v);
                for c in 0..3 {
                    assert!((o[c] - v[c]).abs() < 1e-6, "{interp:?} {v:?} {o:?}");
                }
            }
        }
    }

    #[test]
    fn lut1d_inverse() {
        let mut l = Lut1d::identity(256);
        for e in &mut l.entries {
            *e = e.map(|x| x.powf(2.2));
        }
        let inv = l.inverse(1024);
        for x in [0.05, 0.3, 0.6, 0.95] {
            let y = l.eval_channel(0, x);
            assert!((inv.eval_channel(0, y) - x).abs() < 2e-3);
        }
    }
}
