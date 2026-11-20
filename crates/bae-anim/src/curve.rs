//! Keyframe interpolation with AE semantics (AE-ANI-030 … AE-ANI-054).
//!
//! A [`Curve`] is the compiled form of an attribute's keyframes: values decoded to
//! numeric components, spatial segments with arc-length tables, roving spans. Compile
//! once (the [`Evaluator`](crate::Evaluator) caches curves) and sample many times.
//!
//! Times passed to [`Curve::sample`] are in the keyframes' own time base (for layer
//! attributes: layer time, see [`crate::time`]).

use std::collections::BTreeMap;

use bae_model::{BezierPath, Ease, FeatherPoint, Interpolation, Keyframe, Time, Value};

use crate::bezier::{SpatialSegment, cubic, solve_monotonic_x};

/// How a multi-dimensional value is interpolated over time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CurveKind {
    /// Each dimension has its own temporal ease (AE `TwoD`/`ThreeD` properties such as
    /// Scale). One-dimensional values, colours and paths are also this kind.
    PerDimension,
    /// One temporal ease drives the distance along a Bezier motion path (AE
    /// `TwoD_SPATIAL`/`ThreeD_SPATIAL`: Position, Anchor Point, Point of Interest,
    /// point effect parameters).
    Spatial,
}

/// AE's influence range (percent → fraction).
const MIN_INFLUENCE: f64 = 0.001;
/// Influence AE uses for the Linear side of a mixed Linear/Bezier segment.
const LINEAR_INFLUENCE: f64 = 1.0 / 6.0;

#[derive(Clone, Debug)]
enum Comps {
    /// Numeric components (float, vec2, vec3, color, float list).
    Num(Vec<f64>),
    /// Mask / shape path; interpolated vertex by vertex with one shared ease.
    Path(BezierPath),
    /// Anything else: only hold interpolation makes sense.
    Discrete,
}

#[derive(Clone, Debug)]
struct Key {
    time: Time,
    value: Value,
    comps: Comps,
    interp_in: Interpolation,
    interp_out: Interpolation,
    ease_in: Vec<Ease>,
    ease_out: Vec<Ease>,
    spatial_in: [f64; 3],
    spatial_out: [f64; 3],
    roving: bool,
}

#[derive(Clone, Debug)]
struct Span {
    /// First and last key index of the span (all keys strictly between are roving).
    first: usize,
    last: usize,
    /// Cumulative arc length at each key of the span (`len = last - first + 1`).
    cumulative: Vec<f64>,
}

/// Compiled keyframes of one attribute.
#[derive(Clone, Debug)]
pub struct Curve {
    keys: Vec<Key>,
    kind: CurveKind,
    /// Spatial curves: segment `i` joins key `i` and `i + 1`.
    segments: Vec<SpatialSegment>,
    /// Spatial curves: span index for each segment.
    span_of: Vec<usize>,
    spans: Vec<Span>,
    /// Clamp the result to this range (Opacity, …), applied after interpolation.
    clamp: Option<(f64, f64)>,
}

/// One side of a segment's temporal ease, after AE's Linear and clamping rules.
#[derive(Clone, Copy, Debug)]
struct Handle {
    linear: bool,
    speed: f64,
    influence: f64,
}

impl Curve {
    /// Compile keyframes. Returns `None` when there are none.
    ///
    /// `kind` only matters for `vec2`/`vec3` values; spatial curves need it to be
    /// [`CurveKind::Spatial`] (see [`crate::schema::curve_kind`]).
    pub fn new(keyframes: &BTreeMap<Time, Keyframe>, kind: CurveKind) -> Option<Curve> {
        if keyframes.is_empty() {
            return None;
        }
        let n = keyframes.len();
        let keys: Vec<Key> = keyframes
            .iter()
            .enumerate()
            .map(|(i, (t, k))| Key {
                time: *t,
                comps: decompose(&k.value),
                value: k.value.clone(),
                interp_in: k.interp_in,
                interp_out: k.interp_out,
                ease_in: k.ease_in.clone(),
                ease_out: k.ease_out.clone(),
                spatial_in: k.spatial_in.unwrap_or_default(),
                spatial_out: k.spatial_out.unwrap_or_default(),
                // The first and last keys cannot rove; neither can Hold keys.
                roving: k.flags.roving
                    && i > 0
                    && i + 1 < n
                    && k.interp_in != Interpolation::Hold
                    && k.interp_out != Interpolation::Hold,
            })
            .collect();
        let spatial = kind == CurveKind::Spatial
            && keys
                .iter()
                .all(|k| matches!(k.value, Value::Vec2(_) | Value::Vec3(_)));
        let mut curve = Curve {
            keys,
            kind: if spatial {
                CurveKind::Spatial
            } else {
                CurveKind::PerDimension
            },
            segments: Vec::new(),
            span_of: Vec::new(),
            spans: Vec::new(),
            clamp: None,
        };
        if spatial {
            curve.build_spatial();
        }
        Some(curve)
    }

    /// Clamp every component of the result to `[min, max]` after interpolation (AE
    /// clamps Opacity to 0–100 even when a Bezier ease overshoots).
    pub fn with_clamp(mut self, min: f64, max: f64) -> Self {
        self.clamp = Some((min, max));
        self
    }

    pub fn kind(&self) -> CurveKind {
        self.kind
    }

    /// Time of the first and last keyframe.
    pub fn time_range(&self) -> (Time, Time) {
        (self.keys[0].time, self.keys[self.keys.len() - 1].time)
    }

    /// Number of keyframes.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Always false: a curve has at least one keyframe.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    fn build_spatial(&mut self) {
        let n = self.keys.len();
        for i in 0..n.saturating_sub(1) {
            let (a, b) = (&self.keys[i], &self.keys[i + 1]);
            let p0 = vec3_of(&a.comps);
            let p3 = vec3_of(&b.comps);
            let p1 = add3(p0, a.spatial_out);
            let p2 = add3(p3, b.spatial_in);
            self.segments.push(SpatialSegment::new(p0, p1, p2, p3));
        }
        // Spans: maximal runs whose inner keys are roving.
        let mut first = 0;
        for i in 1..n {
            if !self.keys[i].roving {
                let mut cumulative = vec![0.0];
                let mut acc = 0.0;
                for s in &self.segments[first..i] {
                    acc += s.length();
                    cumulative.push(acc);
                }
                let idx = self.spans.len();
                self.spans.push(Span {
                    first,
                    last: i,
                    cumulative,
                });
                self.span_of.extend(std::iter::repeat_n(idx, i - first));
                first = i;
            }
        }
    }

    /// Index `i` of the segment `[keys[i], keys[i+1])` containing `t`, or `Err` with the
    /// key to return when `t` is outside the keyframe range.
    fn locate(&self, t: Time) -> Result<usize, usize> {
        let n = self.keys.len();
        if n == 1 || t <= self.keys[0].time {
            return Err(0);
        }
        if t >= self.keys[n - 1].time {
            return Err(n - 1);
        }
        // Last key with time <= t.
        let i = self.keys.partition_point(|k| k.time <= t) - 1;
        Ok(i)
    }

    /// Value at time `t` (in the keyframes' time base).
    pub fn sample(&self, t: Time) -> Value {
        let i = match self.locate(t) {
            Ok(i) => i,
            Err(k) => return self.finish(self.keys[k].value.clone()),
        };
        let (a, b) = (&self.keys[i], &self.keys[i + 1]);
        if t == a.time {
            return self.finish(a.value.clone());
        }
        // Hold: an outgoing Hold keeps the start value; an incoming Hold (with a
        // non-Hold outgoing side) jumps to the end value right after the start key
        // (measured in AE 26).
        if a.interp_out == Interpolation::Hold {
            return self.finish(a.value.clone());
        }
        if b.interp_in == Interpolation::Hold {
            return self.finish(b.value.clone());
        }
        if self.kind == CurveKind::Spatial {
            return self.finish(self.sample_spatial(i, t));
        }
        let x = (t - a.time).as_seconds();
        let dt = (b.time - a.time).as_seconds();
        let v = match (&a.comps, &b.comps) {
            (Comps::Num(va), Comps::Num(vb)) if va.len() == vb.len() => {
                if matches!(a.value, Value::Color(_)) {
                    progress_lerp(va, vb, color_distance(va, vb), dt, x, a, b)
                } else {
                    let out: Vec<f64> = (0..va.len())
                        .map(|d| {
                            let ho = handle(&a.ease_out, d, a.interp_out, va[d], vb[d], dt);
                            let hi = handle(&b.ease_in, d, b.interp_in, va[d], vb[d], dt);
                            ease_1d(dt, x, va[d], vb[d], ho, hi)
                        })
                        .collect();
                    recompose(&a.value, &out)
                }
            }
            (Comps::Path(pa), Comps::Path(pb)) if same_topology(pa, pb) => {
                let ho = handle(&a.ease_out, 0, a.interp_out, 0.0, 1.0, dt);
                let hi = handle(&b.ease_in, 0, b.interp_in, 0.0, 1.0, dt);
                let p = ease_1d(dt, x, 0.0, 1.0, ho, hi);
                Value::Bezier(lerp_path(pa, pb, p))
            }
            // Mismatched dimensions or discrete values hold.
            _ => a.value.clone(),
        };
        self.finish(v)
    }

    fn sample_spatial(&self, seg: usize, t: Time) -> Value {
        let span = &self.spans[self.span_of[seg]];
        let (a, b) = (&self.keys[span.first], &self.keys[span.last]);
        let total = *span.cumulative.last().expect("span");
        let dt = (b.time - a.time).as_seconds();
        let x = (t - a.time).as_seconds();
        let s = if total <= 0.0 {
            0.0
        } else {
            let ho = spatial_handle(&a.ease_out, a.interp_out, total, dt);
            let hi = spatial_handle(&b.ease_in, b.interp_in, total, dt);
            ease_1d(dt, x, 0.0, total, ho, hi)
        };
        // Segment within the span holding distance s.
        let rel = span
            .cumulative
            .partition_point(|c| *c <= s)
            .clamp(1, span.cumulative.len() - 1)
            - 1;
        let k = span.first + rel;
        let p = self.segments[k].point_at_length(s - span.cumulative[rel]);
        match self.keys[k].value {
            Value::Vec2(_) => Value::Vec2([p[0], p[1]]),
            _ => Value::Vec3(p),
        }
    }

    fn finish(&self, v: Value) -> Value {
        match (self.clamp, v) {
            (Some((lo, hi)), Value::Float(x)) => Value::Float(x.clamp(lo, hi)),
            (Some((lo, hi)), Value::Vec2(x)) => Value::Vec2(x.map(|c| c.clamp(lo, hi))),
            (Some((lo, hi)), Value::Vec3(x)) => Value::Vec3(x.map(|c| c.clamp(lo, hi))),
            (_, v) => v,
        }
    }
}

/// Temporal ease handle for one dimension of a non-spatial segment.
fn handle(eases: &[Ease], d: usize, interp: Interpolation, v0: f64, v1: f64, dt: f64) -> Handle {
    if interp != Interpolation::Bezier {
        // Linear side: speed is the segment's average rate, influence 1/6 (AE-ANI-031,
        // verified against AE 26 on a Linear → Bezier segment).
        return Handle {
            linear: true,
            speed: (v1 - v0) / dt,
            influence: LINEAR_INFLUENCE,
        };
    }
    let e = eases
        .get(d)
        .or_else(|| eases.first())
        .copied()
        .unwrap_or_default();
    Handle {
        linear: false,
        speed: e.speed,
        influence: (e.influence / 100.0).clamp(MIN_INFLUENCE, 1.0),
    }
}

/// Temporal ease handle of a spatial span: speeds are path speeds (px/s ≥ 0).
fn spatial_handle(eases: &[Ease], interp: Interpolation, length: f64, dt: f64) -> Handle {
    if interp != Interpolation::Bezier {
        return Handle {
            linear: true,
            speed: length / dt,
            influence: LINEAR_INFLUENCE,
        };
    }
    let e = eases.first().copied().unwrap_or_default();
    let speed = e.speed.abs();
    let mut influence = (e.influence / 100.0).clamp(MIN_INFLUENCE, 1.0);
    // The handle may not reach past the end of the path (AE-ANI-035).
    if speed * influence * dt > length && speed > 0.0 {
        influence = (length / (speed * dt)).max(MIN_INFLUENCE);
    }
    Handle {
        linear: false,
        speed,
        influence,
    }
}

/// AE temporal Bezier (AE-ANI-033) on one dimension: `x` seconds into a segment of
/// `dt` seconds from `y0` to `y1`.
fn ease_1d(dt: f64, x: f64, y0: f64, y1: f64, out: Handle, inn: Handle) -> f64 {
    if dt <= 0.0 {
        return y1;
    }
    if out.linear && inn.linear {
        return y0 + (y1 - y0) * (x / dt);
    }
    let x1 = out.influence * dt;
    let x2 = dt - inn.influence * dt;
    let y1c = y0 + out.speed * out.influence * dt;
    let y2c = y1 - inn.speed * inn.influence * dt;
    let u = solve_monotonic_x(x1, x2, dt, x);
    cubic(y0, y1c, y2c, y1, u)
}

/// Shared-ease interpolation (colours): one curve over the distance between the two
/// values, then every component moves by the same fraction.
fn progress_lerp(va: &[f64], vb: &[f64], dist: f64, dt: f64, x: f64, a: &Key, b: &Key) -> Value {
    let p = if dist <= 0.0 {
        0.0
    } else {
        let ho = handle(&a.ease_out, 0, a.interp_out, 0.0, dist, dt);
        let hi = handle(&b.ease_in, 0, b.interp_in, 0.0, dist, dt);
        ease_1d(dt, x, 0.0, dist, ho, hi) / dist
    };
    let out: Vec<f64> = va.iter().zip(vb).map(|(a, b)| a + (b - a) * p).collect();
    recompose(&a.value, &out)
}

/// Distance used for colour ease speeds: AE shows colour speeds in 8-bit units per
/// second. UNVERIFIED (AE-ANI-034): Euclidean length of the 0–255 RGBA difference.
fn color_distance(va: &[f64], vb: &[f64]) -> f64 {
    va.iter()
        .zip(vb)
        .map(|(a, b)| ((b - a) * 255.0).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn decompose(v: &Value) -> Comps {
    match v {
        Value::Float(x) => Comps::Num(vec![*x]),
        Value::Vec2(x) => Comps::Num(x.to_vec()),
        Value::Vec3(x) => Comps::Num(x.to_vec()),
        Value::Color(x) => Comps::Num(x.to_vec()),
        Value::FloatList(x) => Comps::Num(x.clone()),
        // Times (time remap) interpolate in seconds, so speeds are seconds per second.
        Value::Time(t) => Comps::Num(vec![t.as_seconds()]),
        Value::Bezier(p) => Comps::Path(p.clone()),
        _ => Comps::Discrete,
    }
}

fn recompose(template: &Value, c: &[f64]) -> Value {
    match template {
        Value::Float(_) => Value::Float(c[0]),
        Value::Vec2(_) => Value::Vec2([c[0], c[1]]),
        Value::Vec3(_) => Value::Vec3([c[0], c[1], c[2]]),
        Value::Color(_) => Value::Color([c[0], c[1], c[2], c[3]]),
        Value::FloatList(_) => Value::FloatList(c.to_vec()),
        Value::Time(_) => Value::Time(Time::from_seconds(c[0])),
        other => other.clone(),
    }
}

fn vec3_of(c: &Comps) -> [f64; 3] {
    match c {
        Comps::Num(v) => [
            v.first().copied().unwrap_or(0.0),
            v.get(1).copied().unwrap_or(0.0),
            v.get(2).copied().unwrap_or(0.0),
        ],
        _ => [0.0; 3],
    }
}

fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn same_topology(a: &BezierPath, b: &BezierPath) -> bool {
    a.vertices.len() == b.vertices.len()
        && a.in_tangents.len() == b.in_tangents.len()
        && a.out_tangents.len() == b.out_tangents.len()
}

fn lerp_path(a: &BezierPath, b: &BezierPath, p: f64) -> BezierPath {
    let lerp = |x: &[[f64; 2]], y: &[[f64; 2]]| -> Vec<[f64; 2]> {
        x.iter()
            .zip(y)
            .map(|(u, v)| [u[0] + (v[0] - u[0]) * p, u[1] + (v[1] - u[1]) * p])
            .collect()
    };
    BezierPath {
        vertices: lerp(&a.vertices, &b.vertices),
        in_tangents: lerp(&a.in_tangents, &b.in_tangents),
        out_tangents: lerp(&a.out_tangents, &b.out_tangents),
        closed: a.closed,
        feather: if a.feather.len() == b.feather.len() {
            a.feather
                .iter()
                .zip(&b.feather)
                .map(|(u, v)| FeatherPoint {
                    t: u.t + (v.t - u.t) * p,
                    radius: u.radius + (v.radius - u.radius) * p,
                    tension: u.tension + (v.tension - u.tension) * p,
                    corner_angle: u.corner_angle + (v.corner_angle - u.corner_angle) * p,
                    ..*u
                })
                .collect()
        } else {
            a.feather.clone()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::KeyframeFlags;

    fn secs(s: f64) -> Time {
        Time::from_seconds(s)
    }

    fn bez(v: impl Into<Value>, ease: Vec<Ease>) -> Keyframe {
        Keyframe {
            interp_in: Interpolation::Bezier,
            interp_out: Interpolation::Bezier,
            ease_in: ease.clone(),
            ease_out: ease,
            ..Keyframe::linear(v)
        }
    }

    fn keys(list: Vec<(f64, Keyframe)>) -> BTreeMap<Time, Keyframe> {
        list.into_iter().map(|(t, k)| (secs(t), k)).collect()
    }

    fn f(v: Value) -> f64 {
        v.as_f64().unwrap()
    }

    #[test]
    fn linear_hold_and_outside_range() {
        let c = Curve::new(
            &keys(vec![
                (0.0, Keyframe::linear(0.0)),
                (1.0, Keyframe::hold(10.0)),
                (2.0, Keyframe::linear(20.0)),
            ]),
            CurveKind::PerDimension,
        )
        .unwrap();
        assert_eq!(f(c.sample(secs(-1.0))), 0.0);
        assert_eq!(f(c.sample(secs(0.5))), 5.0);
        assert_eq!(f(c.sample(secs(1.0))), 10.0);
        assert_eq!(f(c.sample(secs(1.999))), 10.0);
        assert_eq!(f(c.sample(secs(2.0))), 20.0);
        assert_eq!(f(c.sample(secs(9.0))), 20.0);
    }

    #[test]
    fn easy_ease_matches_ae() {
        // AE 26: rotation 0 → 360 over 2 s with Easy Ease, sampled at 1/60 s.
        let e = vec![Ease::default()];
        let c = Curve::new(
            &keys(vec![(0.0, bez(0.0, e.clone())), (2.0, bez(360.0, e))]),
            CurveKind::PerDimension,
        )
        .unwrap();
        let v = f(c.sample(Time(11_760_000)));
        assert!((v - 0.074_583_333_333_33).abs() < 1e-9, "{v}");
    }

    #[test]
    fn spatial_straight_linear_is_constant_speed() {
        let k = |v: [f64; 3]| Keyframe {
            spatial_in: Some([0.0; 3]),
            spatial_out: Some([0.0; 3]),
            ..Keyframe::linear(v)
        };
        let c = Curve::new(
            &keys(vec![(0.0, k([0.0, 0.0, 0.0])), (1.0, k([100.0, 0.0, 0.0]))]),
            CurveKind::Spatial,
        )
        .unwrap();
        assert_eq!(c.kind(), CurveKind::Spatial);
        assert_eq!(c.sample(secs(0.25)), Value::Vec3([25.0, 0.0, 0.0]));
    }

    #[test]
    fn roving_span_is_one_ease() {
        // Three collinear keys, the middle one roving: speed is continuous through it,
        // so the middle key's own time does not matter.
        let k = |v: f64, roving: bool| Keyframe {
            spatial_in: Some([0.0; 3]),
            spatial_out: Some([0.0; 3]),
            flags: KeyframeFlags {
                roving,
                ..Default::default()
            },
            ..Keyframe::linear([v, 0.0, 0.0])
        };
        let a = Curve::new(
            &keys(vec![
                (0.0, k(0.0, false)),
                (0.2, k(50.0, true)),
                (1.0, k(100.0, false)),
            ]),
            CurveKind::Spatial,
        )
        .unwrap();
        assert_eq!(a.sample(secs(0.5)), Value::Vec3([50.0, 0.0, 0.0]));
        assert_eq!(a.sample(secs(0.1)), Value::Vec3([10.0, 0.0, 0.0]));
    }

    #[test]
    fn clamp_after_interpolation() {
        let c = Curve::new(
            &keys(vec![
                (
                    0.0,
                    bez(
                        0.0,
                        vec![Ease {
                            speed: 0.0,
                            influence: 50.0,
                        }],
                    ),
                ),
                (
                    1.0,
                    bez(
                        100.0,
                        vec![Ease {
                            speed: -400.0,
                            influence: 50.0,
                        }],
                    ),
                ),
            ]),
            CurveKind::PerDimension,
        )
        .unwrap()
        .with_clamp(0.0, 100.0);
        let max = (0..=100)
            .map(|i| f(c.sample(secs(i as f64 / 100.0))))
            .fold(f64::MIN, f64::max);
        assert_eq!(max, 100.0);
    }
}
