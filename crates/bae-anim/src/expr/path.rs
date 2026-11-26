//! Path properties in expressions: `pointOnPath`, `tangentOnPath` and
//! `normalOnPath` at a percentage of the path's arc length (AE: "to ensure
//! uniform speed along the path"). Measured in AE 26: a closed path's 0 and 1
//! are its first vertex, the tangent has length 1 and the normal is the tangent
//! turned to `[-y, x]`.

use bae_model::BezierPath;

type P = [f64; 2];

/// Gauss–Legendre nodes and weights on [-1, 1] (8 points).
const GL: [(f64, f64); 8] = [
    (-0.960_289_856_497_536_3, 0.101_228_536_290_376_26),
    (-0.796_666_477_413_626_7, 0.222_381_034_453_374_47),
    (-0.525_532_409_916_329, 0.313_706_645_877_887_3),
    (-0.183_434_642_495_649_8, 0.362_683_783_378_362),
    (0.183_434_642_495_649_8, 0.362_683_783_378_362),
    (0.525_532_409_916_329, 0.313_706_645_877_887_3),
    (0.796_666_477_413_626_7, 0.222_381_034_453_374_47),
    (0.960_289_856_497_536_3, 0.101_228_536_290_376_26),
];

/// One cubic segment's control points.
struct Seg([P; 4]);

impl Seg {
    fn point(&self, u: f64) -> P {
        let [a, b, c, d] = self.0;
        let v = 1.0 - u;
        let (k0, k1, k2, k3) = (v * v * v, 3.0 * v * v * u, 3.0 * v * u * u, u * u * u);
        [
            k0 * a[0] + k1 * b[0] + k2 * c[0] + k3 * d[0],
            k0 * a[1] + k1 * b[1] + k2 * c[1] + k3 * d[1],
        ]
    }

    fn deriv(&self, u: f64) -> P {
        let [a, b, c, d] = self.0;
        let v = 1.0 - u;
        let (k0, k1, k2) = (3.0 * v * v, 6.0 * v * u, 3.0 * u * u);
        [
            k0 * (b[0] - a[0]) + k1 * (c[0] - b[0]) + k2 * (d[0] - c[0]),
            k0 * (b[1] - a[1]) + k1 * (c[1] - b[1]) + k2 * (d[1] - c[1]),
        ]
    }

    fn speed(&self, u: f64) -> f64 {
        let d = self.deriv(u);
        d[0].hypot(d[1])
    }

    /// Arc length from 0 to `u` (8-point Gauss–Legendre on 8 sub-intervals).
    fn length_to(&self, u: f64) -> f64 {
        const PARTS: usize = 8;
        let h = u / PARTS as f64;
        (0..PARTS)
            .map(|i| {
                let (a, b) = (i as f64 * h, (i + 1) as f64 * h);
                let (mid, half) = ((a + b) / 2.0, (b - a) / 2.0);
                GL.iter()
                    .map(|(x, w)| w * self.speed(mid + half * x))
                    .sum::<f64>()
                    * half
            })
            .sum()
    }

    /// The parameter where the arc length from 0 is `s`.
    fn param_at(&self, s: f64, len: f64) -> f64 {
        if len <= 0.0 {
            return 0.0;
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        let mut u = (s / len).clamp(0.0, 1.0);
        for _ in 0..32 {
            let f = self.length_to(u) - s;
            if f.abs() < 1e-10 {
                break;
            }
            if f > 0.0 {
                hi = u;
            } else {
                lo = u;
            }
            let d = self.speed(u);
            let next = u - f / d;
            // Newton when it stays inside the bracket, else bisection.
            u = if d > 1e-12 && next > lo && next < hi {
                next
            } else {
                (lo + hi) / 2.0
            };
        }
        u
    }
}

fn segments(p: &BezierPath) -> Vec<Seg> {
    let n = p.vertices.len();
    let count = if p.closed { n } else { n.saturating_sub(1) };
    let get = |v: &[P], i: usize| v.get(i).copied().unwrap_or([0.0; 2]);
    (0..count)
        .map(|i| {
            let j = (i + 1) % n;
            let (a, d) = (p.vertices[i], p.vertices[j]);
            let out = get(&p.out_tangents, i);
            let inn = get(&p.in_tangents, j);
            Seg([
                a,
                [a[0] + out[0], a[1] + out[1]],
                [d[0] + inn[0], d[1] + inn[1]],
                d,
            ])
        })
        .collect()
}

/// The point and unit tangent at `pct` (0–1) of `p`'s arc length.
pub(crate) fn at(p: &BezierPath, pct: f64) -> Option<(P, P)> {
    let segs = segments(p);
    if segs.is_empty() {
        return p.vertices.first().map(|v| (*v, [0.0, 0.0]));
    }
    let lens: Vec<f64> = segs.iter().map(|s| s.length_to(1.0)).collect();
    let total: f64 = lens.iter().sum();
    let mut s = pct.clamp(0.0, 1.0) * total;
    let mut k = segs.len() - 1;
    for (i, l) in lens.iter().enumerate() {
        if s <= *l || i == segs.len() - 1 {
            k = i;
            break;
        }
        s -= l;
    }
    let seg = &segs[k];
    let u = seg.param_at(s.min(lens[k]), lens[k]);
    let point = seg.point(u);
    let mut d = seg.deriv(u);
    if d[0].hypot(d[1]) < 1e-12 {
        // A degenerate end: the chord's direction.
        let [a, .., b] = seg.0;
        d = [b[0] - a[0], b[1] - a[1]];
    }
    let len = d[0].hypot(d[1]);
    let tangent = if len > 0.0 {
        [d[0] / len, d[1] / len]
    } else {
        [0.0, 0.0]
    };
    Some((point, tangent))
}

/// AE's normal: the tangent turned to `[-y, x]`.
pub(crate) fn normal(tangent: P) -> P {
    [-tangent[1], tangent[0]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> BezierPath {
        BezierPath {
            vertices: vec![
                [100.0, 100.0],
                [300.0, 100.0],
                [300.0, 200.0],
                [100.0, 200.0],
            ],
            in_tangents: vec![[0.0; 2]; 4],
            out_tangents: vec![[0.0; 2]; 4],
            closed: true,
            feather: Vec::new(),
        }
    }

    /// AE 26 on this clockwise rectangle: pointOnPath(0.125) = 174.9986, 100;
    /// tangents (1, 0) and (0, -1); normals (0, 1) and (1, 0); 0 and 1 the
    /// first vertex.
    #[test]
    fn matches_after_effects_on_a_rectangle() {
        let p = rect();
        let close = |a: P, b: P| (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01;
        let (pt, t) = at(&p, 0.125).unwrap();
        assert!(
            close(pt, [175.0, 100.0]) && close(t, [1.0, 0.0]),
            "{pt:?} {t:?}"
        );
        assert!(close(normal(t), [0.0, 1.0]));
        let (_, t) = at(&p, 0.875).unwrap();
        assert!(
            close(t, [0.0, -1.0]) && close(normal(t), [1.0, 0.0]),
            "{t:?}"
        );
        assert!(close(at(&p, 0.0).unwrap().0, [100.0, 100.0]));
        assert!(close(at(&p, 1.0).unwrap().0, [100.0, 100.0]));
    }

    /// On a curve the percentage follows the arc length, not the parameter.
    #[test]
    fn percentage_is_arc_length_on_curves() {
        // A quarter circle (radius 100) as one cubic.
        let k = 55.228_474_983_079_34;
        let p = BezierPath {
            vertices: vec![[100.0, 0.0], [0.0, 100.0]],
            in_tangents: vec![[0.0; 2], [k, 0.0]],
            out_tangents: vec![[0.0, k], [0.0; 2]],
            closed: false,
            feather: Vec::new(),
        };
        // Halfway along the arc: 45°.
        let (pt, _) = at(&p, 0.5).unwrap();
        let r = std::f64::consts::FRAC_1_SQRT_2 * 100.0;
        assert!(
            (pt[0] - r).abs() < 0.05 && (pt[1] - r).abs() < 0.05,
            "{pt:?}"
        );
    }
}
