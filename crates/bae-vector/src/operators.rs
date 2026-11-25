//! AE shape operators evaluated in the containing group's coordinate system.

use crate::{
    Composite, MergeMode,
    ops::merge_concat,
    path::{append_bezier, from_bez_path, point, to_bez_path, xy},
};
use bae_model::BezierPath;
use kurbo::{
    Affine, BezPath, Cap, Join, ParamCurve, ParamCurveArclen, ParamCurveDeriv, Point, Stroke, Vec2,
};
use linesweeper::{BinaryOp, FillRule, binary_op};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Repeater {
    pub copies: f64,
    pub offset: f64,
    pub order: Composite,
    pub anchor: [f64; 2],
    pub position: [f64; 2],
    pub scale: [f64; 2],
    pub rotation: f64,
    pub start_opacity: f64,
    pub end_opacity: f64,
}

impl Repeater {
    pub(crate) fn transform(&self, index: usize) -> Affine {
        let n = index as f64 + self.offset;
        let anchor = point(self.anchor).to_vec2();
        let scale = |s: f64| {
            let s = s / 100.0;
            // AE alternates reflection by copy index, independent of Offset.
            if s == 0.0 && n < 0.0 {
                return 0.0;
            }
            let magnitude = s.abs().powf(n);
            if s < 0.0 && index % 2 == 1 {
                -magnitude
            } else {
                magnitude
            }
        };
        Affine::translate(point(self.position).to_vec2() * n + anchor)
            * Affine::rotate(self.rotation.to_radians() * n)
            * Affine::scale_non_uniform(scale(self.scale[0]), scale(self.scale[1]))
            * Affine::translate(-anchor)
    }

    pub(crate) fn opacity(&self, index: usize) -> f64 {
        let t = index as f64 / self.copies.ceil().max(1.0);
        (self.start_opacity + (self.end_opacity - self.start_opacity) * t)
            * (self.copies - index as f64).min(1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Offset {
    pub amount: f64,
    pub join: Join,
    pub miter_limit: f64,
    pub copies: f64,
    pub copy_offset: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wiggle {
    pub size: f64,
    pub detail: f64,
    pub smooth: bool,
    pub frequency: f64,
    pub correlation: f64,
    pub temporal_phase: f64,
    pub spatial_phase: f64,
    pub seed: i32,
}

fn boolean(a: &BezPath, b: &BezPath, op: BinaryOp) -> BezPath {
    let mut result = BezPath::new();
    if let Ok(contours) = binary_op(a, b, FillRule::NonZero, op) {
        for contour in contours.contours() {
            result.extend(contour.path.iter());
        }
    }
    result
}

pub(crate) fn merge(paths: &[BezPath], mode: MergeMode) -> BezPath {
    let op = match mode {
        MergeMode::Merge => return merge_concat(paths),
        MergeMode::Add => BinaryOp::Union,
        MergeMode::Subtract => BinaryOp::Difference,
        MergeMode::Intersect => BinaryOp::Intersection,
        MergeMode::Exclude => BinaryOp::Xor,
    };
    let closed = paths.iter().map(|path| {
        let mut result = BezPath::new();
        for mut part in from_bez_path(path) {
            part.closed = true;
            append_bezier(&part, &mut result);
        }
        result
    });
    closed
        .reduce(|a, b| boolean(&a, &b, op))
        .unwrap_or_default()
}

impl Offset {
    pub(crate) fn apply(&self, path: &BezPath) -> BezPath {
        let mut out = BezPath::new();
        for copy in 0..self.copies.ceil() as usize {
            let d = self.amount * (copy as f64 + self.copy_offset);
            if d == 0.0 {
                out.extend(path.iter());
                continue;
            }
            let pen = Stroke::new(2.0 * d.abs())
                .with_join(self.join)
                .with_miter_limit(self.miter_limit)
                .with_caps(Cap::Butt);
            let mut closed = BezPath::new();
            let mut open = BezPath::new();
            for part in from_bez_path(path) {
                append_bezier(&part, if part.closed { &mut closed } else { &mut open });
            }
            if !open.is_empty() {
                out.extend(kurbo::stroke(open, &pen, &Default::default(), 0.02));
            }
            if closed.is_empty() {
                continue;
            }
            let border = kurbo::stroke(closed.iter(), &pen, &Default::default(), 0.02);
            let expanded = boolean(
                &closed,
                &border,
                if d > 0.0 {
                    BinaryOp::Union
                } else {
                    BinaryOp::Difference
                },
            );
            out.extend(expanded.iter());
        }
        out
    }
}

pub(crate) fn pucker(p: &mut BezierPath, amount: f64) {
    let center =
        p.vertices.iter().map(|&v| point(v).to_vec2()).sum::<Vec2>() / p.vertices.len() as f64;
    let k = amount / 100.0;
    for ((v, tin), tout) in p
        .vertices
        .iter_mut()
        .zip(&mut p.in_tangents)
        .zip(&mut p.out_tangents)
    {
        let shift = (center - point(*v).to_vec2()) * k;
        *v = xy(point(*v) + shift);
        for tangent in [tin, tout] {
            *tangent = xy(point(*tangent).to_vec2() * (1.0 + k) - 2.0 * shift);
        }
    }
}

fn vertex(path: &mut BezierPath, v: Point, tin: Vec2, tout: Vec2) {
    path.vertices.push(xy(v));
    path.in_tangents.push(xy(tin));
    path.out_tangents.push(xy(tout));
}

pub(crate) fn round(p: &BezierPath, radius: f64) -> BezierPath {
    let mut out = BezierPath {
        closed: p.closed,
        ..Default::default()
    };
    let n = p.vertices.len();
    for i in 0..n {
        let v = point(p.vertices[i]);
        if radius <= 0.0
            || (!p.closed && (i == 0 || i + 1 == n))
            || p.in_tangents[i] != [0.0; 2]
            || p.out_tangents[i] != [0.0; 2]
        {
            vertex(
                &mut out,
                v,
                point(p.in_tangents[i]).to_vec2(),
                point(p.out_tangents[i]).to_vec2(),
            );
            continue;
        }
        let cut = |j| {
            let d = point(p.vertices[j]) - v;
            d * (radius / d.hypot()).min(0.5)
        };
        let a = cut((i + n - 1) % n);
        let b = cut((i + 1) % n);
        vertex(&mut out, v + a, Vec2::ZERO, -0.5519 * a);
        vertex(&mut out, v + b, -0.5519 * b, Vec2::ZERO);
    }
    out
}

// Value noise with smooth interpolation in time and along the contour.
fn noise(x: f64, y: f64, seed: i32) -> f64 {
    let lattice = |i: i32, j: i32| {
        let mut bits =
            (i as u32).wrapping_mul(0x9e3779b9) ^ (j as u32).wrapping_mul(0x85ebca6b) ^ seed as u32;
        bits = (bits ^ bits >> 16).wrapping_mul(0x7feb352d);
        bits = (bits ^ bits >> 15).wrapping_mul(0x846ca68b);
        (bits ^ bits >> 16) as f64 / u32::MAX as f64 * 2.0 - 1.0
    };
    let (i, j) = (x.floor() as i32, y.floor() as i32);
    let mix = |a, b, t: f64| a + (b - a) * t * t * (3.0 - 2.0 * t);
    let fx = x - x.floor();
    mix(
        mix(lattice(i, j), lattice(i.wrapping_add(1), j), fx),
        mix(
            lattice(i, j.wrapping_add(1)),
            lattice(i.wrapping_add(1), j.wrapping_add(1)),
            fx,
        ),
        y - y.floor(),
    )
}

/// Zig Zag and Wiggle Paths place Detail intermediate points on each segment,
/// independent of its length. At original corners AE uses the neighbour chord.
fn ridges(
    p: &BezierPath,
    detail: f64,
    smooth: bool,
    displacement: impl Fn(usize) -> f64,
) -> BezierPath {
    let steps = detail.round().clamp(0.0, 100.0) as usize + 1;
    let mut out = BezierPath {
        closed: p.closed,
        ..Default::default()
    };
    let n = p.vertices.len();
    for (k, seg) in to_bez_path(p).segments().enumerate() {
        let length = seg.arclen(0.01);
        if length == 0.0 {
            continue;
        }
        let here = seg.start();
        let next = seg.end();
        let prev = point(
            p.vertices[if k == 0 && !p.closed {
                0
            } else {
                (k + n - 1) % n
            }],
        );
        let chord = here.distance(next);
        let last_open = !p.closed && k + 2 == n;
        for j in 0..steps + usize::from(last_open) {
            let t = seg.inv_arclen(length * j as f64 / steps as f64, 0.01);
            let direction = if j == 0 && prev != next {
                next - prev
            } else if j == steps {
                next - here
            } else {
                seg.to_cubic().deriv().eval(t).to_vec2()
            };
            let d = if direction.hypot2() > 0.0 {
                direction.normalize()
            } else {
                Vec2::ZERO
            };
            let tip = seg.eval(t) + Vec2::new(-d.y, d.x) * displacement(k * steps + j);
            let handle = if smooth {
                d / (2 * steps) as f64
            } else {
                Vec2::ZERO
            };
            let incoming = if j == 0 { prev.distance(here) } else { chord };
            vertex(&mut out, tip, -handle * incoming, handle * chord);
        }
    }
    out
}

impl Wiggle {
    pub(crate) fn apply(&self, p: &BezierPath, time: f64) -> BezierPath {
        if self.size == 0.0 {
            return p.clone();
        }
        let time = time * self.frequency + self.temporal_phase / 360.0;
        ridges(p, self.detail, self.smooth, |i| {
            let phase = i as f64 * (1.0 - self.correlation / 100.0) + self.spatial_phase / 360.0;
            0.5 * self.size * noise(time, phase, self.seed)
        })
    }
}

pub(crate) fn zigzag(p: &BezierPath, size: f64, detail: f64, smooth: bool) -> BezierPath {
    if size == 0.0 {
        return p.clone();
    }
    ridges(p, detail, smooth, |i| if i % 2 == 0 { size } else { -size })
}

pub(crate) fn twist(p: &BezierPath, angle: f64, center: Point) -> BezierPath {
    if angle == 0.0 {
        return p.clone();
    }
    use kurbo::Shape;
    let path = to_bez_path(p);
    let bounds = path.bounding_box();
    let pivot = bounds.center() + center.to_vec2();
    let radius = p
        .vertices
        .iter()
        .map(|&v| point(v).distance(pivot))
        .fold(0.0, f64::max);
    if radius == 0.0 {
        return p.clone();
    }
    let warp = |v: Point| {
        let delta = v - pivot;
        let rotation = angle.to_radians() * (1.0 - delta.hypot() / radius);
        pivot + (Affine::rotate(rotation) * delta.to_point()).to_vec2()
    };
    let mut result = BezPath::new();
    for seg in path.segments() {
        let cubic = seg.to_cubic();
        let length = seg.arclen(0.02);
        if result.is_empty() {
            result.move_to(warp(cubic.p0));
        }
        // In the AE angle grid the sampling density doubles at 50 and 100
        // degrees. Scale the same spacing with segment length.
        let density = 2.0_f64.powf((angle.abs() / 50.0).log2().ceil().max(0.0));
        let n = (length / 40.0 * density).ceil() as usize;
        if n <= 1 {
            result.curve_to(warp(cubic.p1), warp(cubic.p2), warp(cubic.p3));
            continue;
        }
        // AE samples at evenly spaced Bezier parameters, including on straight
        // segments (their two controls coincide with the endpoints).

        let points = (0..=n)
            .map(|i| warp(cubic.eval(i as f64 / n as f64)))
            .collect::<Vec<_>>();
        let mut prev_handle = warp(cubic.subsegment(0.0..1.0 / n as f64).p1);
        for i in 1..=n {
            let here = points[i];
            let (incoming, outgoing) = if i == n {
                (
                    warp(cubic.subsegment((n - 1) as f64 / n as f64..1.0).p2),
                    here,
                )
            } else {
                let d = points[i + 1] - points[i - 1];
                let t = if d.hypot2() == 0.0 {
                    Vec2::ZERO
                } else {
                    d.normalize() / 3.0
                };
                (
                    here - t * here.distance(points[i - 1]),
                    here + t * here.distance(points[i + 1]),
                )
            };
            result.curve_to(prev_handle, incoming, here);
            prev_handle = outgoing;
        }
    }
    if p.closed && !result.is_empty() {
        result.close_path();
    }
    from_bez_path(&result).pop().unwrap_or_default()
}

/// Model attribute for an AE parameter matchName, using the AE bridge's namespace.
pub fn parameter_attr(match_name: &str) -> String {
    let prefix = match match_name {
        "ADBE Vector Repeater Anchor"
        | "ADBE Vector Repeater Position"
        | "ADBE Vector Repeater Scale"
        | "ADBE Vector Repeater Rotation"
        | "ADBE Vector Repeater Opacity 1"
        | "ADBE Vector Repeater Opacity 2" => "ae:ADBE_Vector_Repeater_Transform:",
        _ => "ae:",
    };
    format!("{prefix}{}", match_name.replace(' ', "_"))
}
