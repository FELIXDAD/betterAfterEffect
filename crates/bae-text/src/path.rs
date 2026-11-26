//! Text on a path (AE: Text > Path Options with a mask as the Path): every
//! character of a laid-out text moves to the point of the mask path its
//! position along the line maps to, turned with the path when Perpendicular To
//! Path is on. Lines after the first keep their distance from the first line's
//! baseline (it becomes the path). Alignment uses the margins: left text starts
//! at First Margin, right text ends at Last Margin, centered text centers
//! between them; Force Alignment spreads the characters from one margin to the
//! other. Beyond the ends of an open path characters continue along the end
//! tangents; on a closed path they wrap around.

use bae_anim::TextPathOptions;
use bae_model::BezierPath;
use kurbo::{Affine, Point, Vec2};

use crate::animator::CharacterStyle;
use crate::layout::TextLayout;

/// Steps per Bézier segment when measuring the path.
const STEPS: usize = 32;

/// A path flattened for arc-length lookups.
struct Measured {
    points: Vec<Point>,
    /// Arc length at each point.
    lengths: Vec<f64>,
    closed: bool,
}

impl Measured {
    fn new(shape: &BezierPath, reverse: bool) -> Option<Self> {
        let n = shape.vertices.len();
        if n < 2 {
            return None;
        }
        let v = |i: usize| Point::new(shape.vertices[i][0], shape.vertices[i][1]);
        let tan = |t: &[[f64; 2]], i: usize| t.get(i).map_or(Vec2::ZERO, |t| Vec2::new(t[0], t[1]));
        let segments = if shape.closed { n } else { n - 1 };
        let mut points = vec![v(0)];
        for s in 0..segments {
            let (a, b) = (s, (s + 1) % n);
            let p0 = v(a);
            let p1 = p0 + tan(&shape.out_tangents, a);
            let p3 = v(b);
            let p2 = p3 + tan(&shape.in_tangents, b);
            let cubic = kurbo::CubicBez::new(p0, p1, p2, p3);
            for k in 1..=STEPS {
                points.push(kurbo::ParamCurve::eval(&cubic, k as f64 / STEPS as f64));
            }
        }
        if reverse {
            points.reverse();
        }
        let mut lengths = Vec::with_capacity(points.len());
        let mut total = 0.0;
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                total += (*p - points[i - 1]).hypot();
            }
            lengths.push(total);
        }
        (total > 1e-9).then_some(Measured {
            points,
            lengths,
            closed: shape.closed,
        })
    }

    fn length(&self) -> f64 {
        *self.lengths.last().unwrap_or(&0.0)
    }

    /// The point at arc length `s` and the path's direction there.
    fn at(&self, s: f64) -> (Point, Vec2) {
        let len = self.length();
        let s = if self.closed { s.rem_euclid(len) } else { s };
        let n = self.points.len();
        let seg = |i: usize| (self.points[i], self.points[i + 1]);
        let dir = |(a, b): (Point, Point)| {
            let d = b - a;
            let l = d.hypot();
            if l > 1e-12 {
                d / l
            } else {
                Vec2::new(1.0, 0.0)
            }
        };
        if s <= 0.0 {
            let d = dir(seg(0));
            return (self.points[0] + d * s, d);
        }
        if s >= len {
            let d = dir(seg(n - 2));
            return (self.points[n - 1] + d * (s - len), d);
        }
        let i = self.lengths.partition_point(|&l| l <= s).clamp(1, n - 1) - 1;
        let (a, b) = seg(i);
        let span = self.lengths[i + 1] - self.lengths[i];
        let t = if span > 1e-12 {
            (s - self.lengths[i]) / span
        } else {
            0.0
        };
        (a.lerp(b, t), dir((a, b)))
    }
}

/// Puts `layout`'s characters on `shape` (layer space) as AE's Path Options
/// say. `justification` is the document's paragraph alignment.
pub fn place_on_path(
    layout: &mut TextLayout,
    shape: &BezierPath,
    options: &TextPathOptions,
    justification: &str,
) {
    let Some(path) = Measured::new(shape, options.reverse) else {
        return;
    };
    if layout.chars.is_empty() || layout.lines.is_empty() {
        return;
    }
    if layout.styles.len() != layout.chars.len() {
        layout.styles = layout
            .chars
            .iter()
            .map(|c| CharacterStyle {
                transform: Affine::IDENTITY,
                paint: c.paint,
                opacity: 1.0,
            })
            .collect();
    }
    let len = path.length();
    let first = options.first_margin;
    let last = len - options.last_margin;
    let baseline = layout.lines[0].baseline;
    for line in &layout.lines {
        let chars = line.chars.clone();
        if chars.is_empty() {
            continue;
        }
        let centers: Vec<f64> = layout.chars[chars.clone()]
            .iter()
            .map(|c| c.origin.x + c.advance / 2.0)
            .collect();
        let start = match justification {
            "center" | "justify_last_center" => (first + last) / 2.0 - line.width / 2.0,
            "right" | "justify_last_right" => last - line.width,
            _ => first,
        };
        let count = centers.len();
        for (k, i) in chars.enumerate() {
            let c = &layout.chars[i];
            let s = if options.force_alignment && count > 1 {
                let a = first + layout.chars[line.chars.start].advance / 2.0;
                let b = last - layout.chars[line.chars.end - 1].advance / 2.0;
                a + (b - a) * k as f64 / (count - 1) as f64
            } else {
                start + (centers[k] - line.x)
            };
            let (p, dir) = path.at(s);
            let angle = if options.perpendicular {
                dir.y.atan2(dir.x)
            } else {
                0.0
            };
            let pivot = Vec2::new(c.origin.x + c.advance / 2.0, baseline);
            let place =
                Affine::translate(p.to_vec2()) * Affine::rotate(angle) * Affine::translate(-pivot);
            let style = &mut layout.styles[i];
            style.transform = place * style.transform;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_path(len: f64) -> BezierPath {
        BezierPath {
            vertices: vec![[0.0, 0.0], [len, 0.0]],
            in_tangents: vec![[0.0, 0.0]; 2],
            out_tangents: vec![[0.0, 0.0]; 2],
            closed: false,
            feather: Vec::new(),
        }
    }

    #[test]
    fn measures_and_samples_paths() {
        let m = Measured::new(&line_path(100.0), false).unwrap();
        assert!((m.length() - 100.0).abs() < 1e-6);
        let (p, d) = m.at(25.0);
        assert!((p.x - 25.0).abs() < 1e-6 && d.x > 0.99);
        // Past the end: along the end tangent.
        assert!((m.at(120.0).0.x - 120.0).abs() < 1e-6);
        let r = Measured::new(&line_path(100.0), true).unwrap();
        assert!((r.at(0.0).0.x - 100.0).abs() < 1e-6, "reversed");
        // A vertical path turns characters by 90°.
        let mut up = line_path(0.0);
        up.vertices[1] = [0.0, 50.0];
        let (_, d) = Measured::new(&up, false).unwrap().at(10.0);
        assert!((d.y.atan2(d.x) - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }
}
