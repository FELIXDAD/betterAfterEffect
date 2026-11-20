//! Cubic Bezier helpers: the temporal ease solve (time → value) and arc-length
//! parameterisation of spatial segments.

/// Solve `x(u) = x` on `[0, 1]` for a cubic Bezier whose x control points are
/// `0, x1, x2, x3` and whose x is monotonic (AE guarantees this for influences in
/// `(0, 1]`). Returns `u`.
pub(crate) fn solve_monotonic_x(x1: f64, x2: f64, x3: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= x3 {
        return 1.0;
    }
    let bx = |u: f64| {
        let v = 1.0 - u;
        3.0 * v * v * u * x1 + 3.0 * v * u * u * x2 + u * u * u * x3
    };
    let dbx = |u: f64| {
        let v = 1.0 - u;
        3.0 * v * v * x1 + 6.0 * v * u * (x2 - x1) + 3.0 * u * u * (x3 - x2)
    };
    // Newton from the linear guess, falling back to bisection whenever a step leaves
    // the bracket or the slope vanishes.
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    let mut u = x / x3;
    let tol = 1e-14 * x3.max(1e-300);
    for _ in 0..64 {
        let f = bx(u) - x;
        if f.abs() <= tol {
            return u;
        }
        if f < 0.0 {
            lo = u;
        } else {
            hi = u;
        }
        let d = dbx(u);
        let next = if d > 1e-300 { u - f / d } else { f64::NAN };
        u = if next > lo && next < hi {
            next
        } else {
            0.5 * (lo + hi)
        };
        if hi - lo <= 1e-16 {
            break;
        }
    }
    u
}

/// Evaluate a 1-D cubic Bezier with control values `y0..y3` at `u`.
#[inline]
pub(crate) fn cubic(y0: f64, y1: f64, y2: f64, y3: f64, u: f64) -> f64 {
    let v = 1.0 - u;
    v * v * v * y0 + 3.0 * v * v * u * y1 + 3.0 * v * u * u * y2 + u * u * u * y3
}

/// Derivative of a 1-D cubic Bezier at `u`.
#[inline]
pub(crate) fn cubic_deriv(y0: f64, y1: f64, y2: f64, y3: f64, u: f64) -> f64 {
    let v = 1.0 - u;
    3.0 * v * v * (y1 - y0) + 6.0 * v * u * (y2 - y1) + 3.0 * u * u * (y3 - y2)
}

/// A 3-D cubic Bezier segment of a spatial property (2-D values use z = 0), with an
/// arc-length table so that distance → point lookups are cheap during playback.
#[derive(Clone, Debug)]
pub(crate) struct SpatialSegment {
    p: [[f64; 3]; 4],
    /// Straight segment (both tangents zero): arc length is linear in `u`.
    straight: bool,
    /// Cumulative arc length at `u = i / N` (`N = table.len() - 1`).
    table: Vec<f64>,
}

/// Sub-intervals of the arc-length table. Each one is integrated with 8-point
/// Gauss–Legendre, which is exact to ~1e-12 px for any non-degenerate cubic
/// segment of on-screen size.
const ARC_INTERVALS: usize = 32;

const GL_X: [f64; 8] = [
    -0.960_289_856_497_536_3,
    -0.796_666_477_413_626_7,
    -0.525_532_409_916_329,
    -0.183_434_642_495_649_8,
    0.183_434_642_495_649_8,
    0.525_532_409_916_329,
    0.796_666_477_413_626_7,
    0.960_289_856_497_536_3,
];
const GL_W: [f64; 8] = [
    0.101_228_536_290_376_26,
    0.222_381_034_453_374_47,
    0.313_706_645_877_887_3,
    0.362_683_783_378_362,
    0.362_683_783_378_362,
    0.313_706_645_877_887_3,
    0.222_381_034_453_374_47,
    0.101_228_536_290_376_26,
];

impl SpatialSegment {
    /// `p0` start value, `p1`/`p2` absolute control points, `p3` end value.
    pub(crate) fn new(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3], p3: [f64; 3]) -> Self {
        let straight = p1 == p0 && p2 == p3;
        let mut seg = SpatialSegment {
            p: [p0, p1, p2, p3],
            straight,
            table: Vec::new(),
        };
        if !straight {
            let mut table = Vec::with_capacity(ARC_INTERVALS + 1);
            let mut acc = 0.0;
            table.push(0.0);
            for i in 0..ARC_INTERVALS {
                let a = i as f64 / ARC_INTERVALS as f64;
                let b = (i + 1) as f64 / ARC_INTERVALS as f64;
                acc += seg.integrate(a, b);
                table.push(acc);
            }
            seg.table = table;
        }
        seg
    }

    pub(crate) fn length(&self) -> f64 {
        if self.straight {
            dist(self.p[0], self.p[3])
        } else {
            *self.table.last().expect("table")
        }
    }

    pub(crate) fn point(&self, u: f64) -> [f64; 3] {
        let p = &self.p;
        std::array::from_fn(|d| cubic(p[0][d], p[1][d], p[2][d], p[3][d], u))
    }

    /// Unnormalised tangent at `u`.
    pub(crate) fn derivative(&self, u: f64) -> [f64; 3] {
        let p = &self.p;
        std::array::from_fn(|d| cubic_deriv(p[0][d], p[1][d], p[2][d], p[3][d], u))
    }

    fn speed(&self, u: f64) -> f64 {
        let d = self.derivative(u);
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }

    fn integrate(&self, a: f64, b: f64) -> f64 {
        let half = 0.5 * (b - a);
        let mid = 0.5 * (a + b);
        GL_X.iter()
            .zip(GL_W)
            .map(|(x, w)| w * self.speed(mid + half * x))
            .sum::<f64>()
            * half
    }

    /// Curve parameter at arc length `s` (clamped to the segment).
    pub(crate) fn param_at_length(&self, s: f64) -> f64 {
        let len = self.length();
        if len <= 0.0 || s <= 0.0 {
            return 0.0;
        }
        if s >= len {
            return 1.0;
        }
        if self.straight {
            return s / len;
        }
        // Interval containing s, then Newton on u with the interval as a bracket.
        let i = match self
            .table
            .binary_search_by(|v| v.partial_cmp(&s).expect("finite"))
        {
            Ok(i) => return i as f64 / ARC_INTERVALS as f64,
            Err(i) => i - 1,
        };
        let a = i as f64 / ARC_INTERVALS as f64;
        let b = (i + 1) as f64 / ARC_INTERVALS as f64;
        let base = self.table[i];
        let span = self.table[i + 1] - base;
        let (mut lo, mut hi) = (a, b);
        let mut u = if span > 0.0 {
            a + (b - a) * (s - base) / span
        } else {
            a
        };
        for _ in 0..32 {
            let f = base + self.integrate(a, u) - s;
            if f.abs() < 1e-11 {
                break;
            }
            if f < 0.0 {
                lo = u;
            } else {
                hi = u;
            }
            let d = self.speed(u);
            let next = if d > 1e-12 { u - f / d } else { f64::NAN };
            u = if next > lo && next < hi {
                next
            } else {
                0.5 * (lo + hi)
            };
            if hi - lo < 1e-15 {
                break;
            }
        }
        u
    }

    /// Point at arc length `s` along the segment.
    pub(crate) fn point_at_length(&self, s: f64) -> [f64; 3] {
        if self.straight {
            let len = self.length();
            if len <= 0.0 {
                return self.p[0];
            }
            let f = (s / len).clamp(0.0, 1.0);
            let (a, b) = (self.p[0], self.p[3]);
            return std::array::from_fn(|d| a[d] + (b[d] - a[d]) * f);
        }
        self.point(self.param_at_length(s))
    }
}

pub(crate) fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_linear_x() {
        // Control points at 1/3 and 2/3 make x(u) = u.
        for x in [0.0, 0.1, 0.5, 0.77, 1.0] {
            assert!((solve_monotonic_x(1.0 / 3.0, 2.0 / 3.0, 1.0, x) - x).abs() < 1e-12);
        }
    }

    #[test]
    fn solve_extreme_influences() {
        for (x1, x2) in [(0.001, 0.999), (1.0, 0.0), (0.8, 0.2), (0.001, 0.0)] {
            for i in 0..=100 {
                let x = i as f64 / 100.0;
                let u = solve_monotonic_x(x1, x2, 1.0, x);
                let back = cubic(0.0, x1, x2, 1.0, u);
                assert!((back - x).abs() < 1e-10, "{x1} {x2} {x}: {back}");
            }
        }
    }

    #[test]
    fn arc_length_of_quarter_circle_approximation() {
        // Standard cubic approximation of a quarter circle of radius 100.
        let k = 0.552_284_749_830_793_4 * 100.0;
        let seg = SpatialSegment::new(
            [100.0, 0.0, 0.0],
            [100.0, k, 0.0],
            [k, 100.0, 0.0],
            [0.0, 100.0, 0.0],
        );
        // Known length of this approximation: 157.0796... * (1 + ~1e-4).
        assert!((seg.length() - 157.08).abs() < 0.03, "{}", seg.length());
        // point_at_length is the inverse of the cumulative length.
        for i in 0..=20 {
            let s = seg.length() * i as f64 / 20.0;
            let u = seg.param_at_length(s);
            let back = seg.integrate(0.0, u);
            assert!((back - s).abs() < 1e-8, "{s} {back}");
        }
    }

    #[test]
    fn straight_segment() {
        let seg = SpatialSegment::new([0.0; 3], [0.0; 3], [30.0, 40.0, 0.0], [30.0, 40.0, 0.0]);
        assert_eq!(seg.length(), 50.0);
        assert_eq!(seg.point_at_length(25.0), [15.0, 20.0, 0.0]);
    }
}
