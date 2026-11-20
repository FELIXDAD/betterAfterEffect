//! Parametric shape paths with AE's exact vertex order (AE-SHP-010..012).
//!
//! The vertex order, start vertex and direction decide where Trim Paths, dashes and
//! Offset Paths start, so they follow lottie-web's construction (which reproduces
//! AE's) exactly. The arc constant lives in one place, [`ELLIPSE_KAPPA`].

use bae_model::BezierPath;

/// Handle length of a quarter arc as a fraction of the radius (lottie-web's
/// `roundCorner`, used for ellipses and rounded corners). AE-SHP-011: AE may use
/// 0.55228 instead; switch here once verified.
pub const ELLIPSE_KAPPA: f64 = 0.5519;

/// Path direction (AE "Reverse Path Direction", AE-SHP-014).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Clockwise on screen (y down) for parametric shapes.
    #[default]
    Normal,
    /// Counter-clockwise; Bezier paths are traversed backwards.
    Reversed,
}

impl Direction {
    /// From the project token (`normal` / `reversed`); unknown tokens are normal.
    pub fn from_token(token: &str) -> Self {
        match token {
            "reversed" => Direction::Reversed,
            _ => Direction::Normal,
        }
    }
}

/// Polystar type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StarType {
    #[default]
    Star,
    Polygon,
}

/// Rectangle parameters (AE-SHP-010), group space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectShape {
    pub size: [f64; 2],
    /// Centre.
    pub position: [f64; 2],
    /// Corner radius in pixels.
    pub roundness: f64,
}

impl Default for RectShape {
    fn default() -> Self {
        Self {
            size: [100.0, 100.0],
            position: [0.0, 0.0],
            roundness: 0.0,
        }
    }
}

/// Ellipse parameters (AE-SHP-011), group space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EllipseShape {
    /// Full width and height.
    pub size: [f64; 2],
    /// Centre.
    pub position: [f64; 2],
}

impl Default for EllipseShape {
    fn default() -> Self {
        Self {
            size: [100.0, 100.0],
            position: [0.0, 0.0],
        }
    }
}

/// Polystar parameters (AE-SHP-012), group space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StarShape {
    pub star_type: StarType,
    /// Number of points (floored, as lottie-web; AE's fractional points are
    /// UNVERIFIED).
    pub points: f64,
    /// Centre.
    pub position: [f64; 2],
    /// Degrees, clockwise.
    pub rotation: f64,
    /// Star only.
    pub inner_radius: f64,
    pub outer_radius: f64,
    /// Percent, star only.
    pub inner_roundness: f64,
    /// Percent.
    pub outer_roundness: f64,
}

impl Default for StarShape {
    fn default() -> Self {
        Self {
            star_type: StarType::Star,
            points: 5.0,
            position: [0.0, 0.0],
            rotation: 0.0,
            inner_radius: 50.0,
            outer_radius: 100.0,
            inner_roundness: 0.0,
            outer_roundness: 0.0,
        }
    }
}

/// Builds a closed path from `(vertex, in_tip, out_tip)` triples (absolute tips).
fn closed_from_tips(triples: &[([f64; 2], [f64; 2], [f64; 2])]) -> BezierPath {
    let rel = |tip: [f64; 2], v: [f64; 2]| [tip[0] - v[0], tip[1] - v[1]];
    BezierPath {
        vertices: triples.iter().map(|t| t.0).collect(),
        in_tangents: triples.iter().map(|t| rel(t.1, t.0)).collect(),
        out_tangents: triples.iter().map(|t| rel(t.2, t.0)).collect(),
        closed: true,
        feather: Vec::new(),
    }
}

impl RectShape {
    /// The rectangle as AE generates it: 4 vertices starting at the top-right
    /// corner, or 8 when rounded, clockwise for [`Direction::Normal`].
    pub fn to_path(&self, direction: Direction) -> BezierPath {
        let [cx, cy] = self.position;
        let hw = self.size[0] / 2.0;
        let hh = self.size[1] / 2.0;
        // AE clamps the radius to half the shorter side (also for negative sizes).
        let r = self.roundness.min(hw.abs()).min(hh.abs()).max(0.0);
        let c = r * (1.0 - ELLIPSE_KAPPA);
        let (l, rt, t, b) = (cx - hw, cx + hw, cy - hh, cy + hh);
        // (vertex, in tip, out tip)
        let tri = |v: [f64; 2], i: [f64; 2], o: [f64; 2]| (v, i, o);
        let pts = match (direction, r > 0.0) {
            (Direction::Normal, true) => vec![
                tri([rt, t + r], [rt, t + c], [rt, t + r]),
                tri([rt, b - r], [rt, b - r], [rt, b - c]),
                tri([rt - r, b], [rt - c, b], [rt - r, b]),
                tri([l + r, b], [l + r, b], [l + c, b]),
                tri([l, b - r], [l, b - c], [l, b - r]),
                tri([l, t + r], [l, t + r], [l, t + c]),
                tri([l + r, t], [l + c, t], [l + r, t]),
                tri([rt - r, t], [rt - r, t], [rt - c, t]),
            ],
            (Direction::Normal, false) => [[rt, t], [rt, b], [l, b], [l, t]]
                .into_iter()
                .map(|v| tri(v, v, v))
                .collect(),
            (Direction::Reversed, true) => vec![
                tri([rt, t + r], [rt, t + r], [rt, t + c]),
                tri([rt - r, t], [rt - c, t], [rt - r, t]),
                tri([l + r, t], [l + r, t], [l + c, t]),
                tri([l, t + r], [l, t + c], [l, t + r]),
                tri([l, b - r], [l, b - r], [l, b - c]),
                tri([l + r, b], [l + c, b], [l + r, b]),
                tri([rt - r, b], [rt - r, b], [rt - c, b]),
                tri([rt, b - r], [rt, b - c], [rt, b - r]),
            ],
            (Direction::Reversed, false) => [[rt, t], [l, t], [l, b], [rt, b]]
                .into_iter()
                .map(|v| tri(v, v, v))
                .collect(),
        };
        closed_from_tips(&pts)
    }
}

impl EllipseShape {
    /// Four vertices starting at 12 o'clock, clockwise for [`Direction::Normal`].
    pub fn to_path(&self, direction: Direction) -> BezierPath {
        let [cx, cy] = self.position;
        let rx = self.size[0] / 2.0;
        let ry = self.size[1] / 2.0;
        let (kx, ky) = (rx * ELLIPSE_KAPPA, ry * ELLIPSE_KAPPA);
        // `s` mirrors x for the reversed direction.
        let s = match direction {
            Direction::Normal => 1.0,
            Direction::Reversed => -1.0,
        };
        let pts = [
            (
                [cx, cy - ry],
                [cx - s * kx, cy - ry],
                [cx + s * kx, cy - ry],
            ),
            (
                [cx + s * rx, cy],
                [cx + s * rx, cy - ky],
                [cx + s * rx, cy + ky],
            ),
            (
                [cx, cy + ry],
                [cx + s * kx, cy + ry],
                [cx - s * kx, cy + ry],
            ),
            (
                [cx - s * rx, cy],
                [cx - s * rx, cy + ky],
                [cx - s * rx, cy - ky],
            ),
        ];
        closed_from_tips(&pts)
    }
}

impl StarShape {
    /// Vertices start straight up (rotated by `rotation`), alternating outer and
    /// inner for stars; clockwise for [`Direction::Normal`].
    pub fn to_path(&self, direction: Direction) -> BezierPath {
        let n = self.points.floor().max(0.0) as usize;
        if n == 0 {
            return BezierPath {
                closed: true,
                ..BezierPath::default()
            };
        }
        let dir = match direction {
            Direction::Normal => 1.0,
            Direction::Reversed => -1.0,
        };
        let star = self.star_type == StarType::Star;
        let count = if star { 2 * n } else { n };
        let step = std::f64::consts::TAU / count as f64;
        let mut angle = (-90.0 + self.rotation).to_radians();
        let [px, py] = self.position;
        let mut pts = Vec::with_capacity(count);
        for i in 0..count {
            let outer = !star || i % 2 == 0;
            let (radius, roundness) = if outer {
                (self.outer_radius, self.outer_roundness / 100.0)
            } else {
                (self.inner_radius, self.inner_roundness / 100.0)
            };
            // Handle length at 100 % roundness: 2πr / (4n) (lottie-web).
            let seg = std::f64::consts::TAU * radius / (4.0 * n as f64);
            let (x, y) = (radius * angle.cos(), radius * angle.sin());
            let len = (x * x + y * y).sqrt();
            let (ox, oy) = if len == 0.0 {
                (0.0, 0.0)
            } else {
                (y / len, -x / len)
            };
            let h = seg * roundness * dir;
            let v = [px + x, py + y];
            let in_tip = [v[0] + ox * h, v[1] + oy * h];
            let out_tip = [v[0] - ox * h, v[1] - oy * h];
            pts.push((v, in_tip, out_tip));
            angle += step * dir;
        }
        closed_from_tips(&pts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn sharp_rect_starts_top_right_clockwise() {
        let p = RectShape::default().to_path(Direction::Normal);
        assert_eq!(
            p.vertices,
            vec![[50.0, -50.0], [50.0, 50.0], [-50.0, 50.0], [-50.0, -50.0]]
        );
        assert!(p.closed);
        assert!(p.in_tangents.iter().all(|t| *t == [0.0, 0.0]));
        let r = RectShape::default().to_path(Direction::Reversed);
        assert_eq!(
            r.vertices,
            vec![[50.0, -50.0], [-50.0, -50.0], [-50.0, 50.0], [50.0, 50.0]]
        );
    }

    #[test]
    fn rounded_rect_matches_ae_construction() {
        let rect = RectShape {
            size: [200.0, 100.0],
            position: [10.0, 20.0],
            roundness: 80.0, // clamped to 50
        };
        let p = rect.to_path(Direction::Normal);
        assert_eq!(p.vertices.len(), 8);
        // v0 = (Rt, T+R) with an in-tangent towards the top-right corner.
        assert_eq!(p.vertices[0], [110.0, 20.0]);
        let handle = 50.0 * ELLIPSE_KAPPA;
        assert!(close(p.in_tangents[0], [0.0, -handle]));
        assert_eq!(p.out_tangents[0], [0.0, 0.0]);
        // v1 = (Rt, B-R) leaves towards the bottom-right corner.
        assert_eq!(p.vertices[1], [110.0, 20.0]);
        assert!(close(p.out_tangents[1], [0.0, handle]));
        // Reversed starts at the same vertex and goes up first.
        let r = rect.to_path(Direction::Reversed);
        assert_eq!(r.vertices[0], [110.0, 20.0]);
        assert!(close(r.out_tangents[0], [0.0, -handle]));
        assert_eq!(r.vertices[1], [60.0, -30.0]);
    }

    #[test]
    fn ellipse_starts_at_twelve() {
        let p = EllipseShape::default().to_path(Direction::Normal);
        assert_eq!(p.vertices[0], [0.0, -50.0]);
        assert_eq!(p.vertices[1], [50.0, 0.0]);
        let k = 50.0 * ELLIPSE_KAPPA;
        assert!(close(p.out_tangents[0], [k, 0.0]));
        assert!(close(p.in_tangents[1], [0.0, -k]));
        let r = EllipseShape::default().to_path(Direction::Reversed);
        assert_eq!(r.vertices[1], [-50.0, 0.0]);
        assert!(close(r.out_tangents[0], [-k, 0.0]));
    }

    #[test]
    fn star_and_polygon_vertices() {
        let star = StarShape::default().to_path(Direction::Normal);
        assert_eq!(star.vertices.len(), 10);
        assert!(close(star.vertices[0], [0.0, -100.0]));
        // Second vertex: inner radius, 36° clockwise from 12 o'clock.
        let a = (-90.0f64 + 36.0).to_radians();
        assert!(close(star.vertices[1], [50.0 * a.cos(), 50.0 * a.sin()]));
        let poly = StarShape {
            star_type: StarType::Polygon,
            points: 4.0,
            outer_roundness: 100.0,
            ..StarShape::default()
        };
        let p = poly.to_path(Direction::Normal);
        assert_eq!(p.vertices.len(), 4);
        // Full roundness: handle = 2πr/(4n) along the clockwise tangent.
        let h = std::f64::consts::TAU * 100.0 / 16.0;
        assert!(close(p.out_tangents[0], [h, 0.0]));
        assert!(close(p.in_tangents[0], [-h, 0.0]));
        let reversed = poly.to_path(Direction::Reversed);
        assert!(close(reversed.vertices[1], [-100.0, 0.0]));
        assert!(close(reversed.out_tangents[0], [-h, 0.0]));
    }

    #[test]
    fn degenerate_parameters() {
        let none = StarShape {
            points: 0.5,
            ..StarShape::default()
        };
        assert!(none.to_path(Direction::Normal).vertices.is_empty());
        let zero = StarShape {
            outer_radius: 0.0,
            outer_roundness: 50.0,
            ..StarShape::default()
        };
        let p = zero.to_path(Direction::Normal);
        assert_eq!(p.vertices[0], [0.0, 0.0]);
        assert_eq!(p.out_tangents[0], [0.0, 0.0]);
        let flat = RectShape {
            size: [0.0, 10.0],
            ..RectShape::default()
        };
        assert_eq!(flat.to_path(Direction::Normal).vertices.len(), 4);
    }
}
