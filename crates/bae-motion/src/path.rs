//! Path vertex editing (AE-SHP-044) on mask paths and shape Path attributes:
//! `path.set_vertices` (the Pen tool adding vertices, Convert Vertex, dragging
//! vertices and handles), `path.insert_vertex` (Add Vertex tool) and
//! `path.delete_vertex` (Delete Vertex tool). Inserting and deleting change every
//! keyframe of an animated path so the vertex count stays the same in all of them,
//! as AE does.
//!
//! The geometry helpers are shared with the Composition viewer's pen tools.

use bae_model::{BezierPath, Command, CommandError, PrimPath, PrimType, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::keys::{SetAt, SetAtParams};
use crate::query;
use crate::util::{ensure_unlocked, fail, write_attr};

/// The attribute holding the path of a mask or shape Path prim.
pub const PATH_ATTR: &str = "path";

/// Center of a drawn Pen path, including cubic extrema. A point has no segments.
pub fn drawing_center(path: &BezierPath) -> [f64; 2] {
    use bae_vector::kurbo::Shape;
    if let [vertex] = path.vertices.as_slice()
        && !path.closed
    {
        return *vertex;
    }
    let c = bae_vector::path::to_bez_path(path).bounding_box().center();
    [c.x, c.y]
}

type P = [f64; 2];

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn scale(a: P, s: f64) -> P {
    [a[0] * s, a[1] * s]
}

fn lerp(a: P, b: P, t: f64) -> P {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn len(a: P) -> f64 {
    a[0].hypot(a[1])
}

/// Number of segments (a closed path has one more, back to vertex 0).
pub fn segment_count(p: &BezierPath) -> usize {
    let n = p.vertices.len();
    match (n, p.closed) {
        (0 | 1, _) => 0,
        (n, true) => n,
        (n, false) => n - 1,
    }
}

/// Control points of segment `seg` (vertex `seg` to the next one), absolute.
pub fn segment(p: &BezierPath, seg: usize) -> [P; 4] {
    let n = p.vertices.len();
    let j = (seg + 1) % n;
    [
        p.vertices[seg],
        add(p.vertices[seg], p.out_tangents[seg]),
        add(p.vertices[j], p.in_tangents[j]),
        p.vertices[j],
    ]
}

/// Point at parameter `t` of segment `seg`.
pub fn point_at(p: &BezierPath, seg: usize, t: f64) -> P {
    let [a, b, c, d] = segment(p, seg);
    let ab = lerp(a, b, t);
    let bc = lerp(b, c, t);
    let cd = lerp(c, d, t);
    lerp(lerp(ab, bc, t), lerp(bc, cd, t), t)
}

/// Steps per segment when measuring an outline.
const STEPS: usize = 32;

/// The outline as a polyline: (point, arc length from vertex 0, path parameter).
fn polyline(p: &BezierPath) -> Vec<(P, f64, f64)> {
    let mut out = Vec::new();
    let segs = segment_count(p);
    if segs == 0 {
        return out;
    }
    let mut len = 0.0;
    let mut last = p.vertices[0];
    out.push((last, 0.0, 0.0));
    for seg in 0..segs {
        for k in 1..=STEPS {
            let t = k as f64 / STEPS as f64;
            let q = point_at(p, seg, t);
            len += (q[0] - last[0]).hypot(q[1] - last[1]);
            out.push((q, len, seg as f64 + t));
            last = q;
        }
    }
    out
}

/// Length of the outline (closed paths include the closing segment).
pub fn outline_length(p: &BezierPath) -> f64 {
    polyline(p).last().map_or(0.0, |l| l.1)
}

/// The path resampled to `count` corner vertices spaced evenly along its
/// outline from vertex 0 (Smart Mask Interpolation's added vertices); feather
/// points keep their place on the outline.
pub fn resample(p: &BezierPath, count: usize) -> BezierPath {
    let line = polyline(p);
    let total = line.last().map_or(0.0, |l| l.1);
    if count < 2 || total <= 0.0 {
        return p.clone();
    }
    let spacing = total / if p.closed { count } else { count - 1 } as f64;
    // Point and parameter at arc length `s`.
    let at = |s: f64| -> (P, f64) {
        let i = line.partition_point(|l| l.1 < s).clamp(1, line.len() - 1);
        let (a, b) = (line[i - 1], line[i]);
        let f = if b.1 > a.1 {
            (s - a.1) / (b.1 - a.1)
        } else {
            0.0
        };
        (lerp(a.0, b.0, f), a.2 + (b.2 - a.2) * f)
    };
    let vertices: Vec<P> = (0..count).map(|k| at(k as f64 * spacing).0).collect();
    // Arc length of a path parameter.
    let length_at = |u: f64| -> f64 {
        let i = line.partition_point(|l| l.2 < u).clamp(1, line.len() - 1);
        let (a, b) = (line[i - 1], line[i]);
        let f = if b.2 > a.2 {
            (u - a.2) / (b.2 - a.2)
        } else {
            0.0
        };
        a.1 + (b.1 - a.1) * f
    };
    let feather = p
        .feather
        .iter()
        .map(|f| {
            let pos = length_at(f64::from(f.segment) + f.t) / spacing;
            let seg = (pos.floor() as usize).min(count - 1);
            bae_model::FeatherPoint {
                segment: seg as u32,
                t: pos - seg as f64,
                ..*f
            }
        })
        .collect();
    BezierPath {
        in_tangents: vec![[0.0, 0.0]; count],
        out_tangents: vec![[0.0, 0.0]; count],
        vertices,
        closed: p.closed,
        feather,
    }
}

/// Per-vertex linear blend of two paths with the same vertex count (feather
/// points too when their counts match).
pub fn blend(a: &BezierPath, b: &BezierPath, f: f64) -> BezierPath {
    let mix =
        |x: &[P], y: &[P]| -> Vec<P> { x.iter().zip(y).map(|(u, v)| lerp(*u, *v, f)).collect() };
    let mix1 = |x: f64, y: f64| x + (y - x) * f;
    BezierPath {
        vertices: mix(&a.vertices, &b.vertices),
        in_tangents: mix(&a.in_tangents, &b.in_tangents),
        out_tangents: mix(&a.out_tangents, &b.out_tangents),
        closed: a.closed,
        feather: if a.feather.len() == b.feather.len() {
            a.feather
                .iter()
                .zip(&b.feather)
                .map(|(u, v)| bae_model::FeatherPoint {
                    t: mix1(u.t, v.t),
                    radius: mix1(u.radius, v.radius),
                    tension: mix1(u.tension, v.tension),
                    corner_angle: mix1(u.corner_angle, v.corner_angle),
                    ..*u
                })
                .collect()
        } else {
            a.feather.clone()
        },
    }
}

/// Insert a vertex at parameter `t` of segment `seg`, splitting the cubic (de
/// Casteljau), so the outline does not change. The new vertex is `seg + 1`.
pub fn split(p: &BezierPath, seg: usize, t: f64) -> BezierPath {
    let mut out = p.clone();
    if seg >= segment_count(p) {
        return out;
    }
    let t = t.clamp(0.0, 1.0);
    let n = p.vertices.len();
    let j = (seg + 1) % n;
    let [a, b, c, d] = segment(p, seg);
    let ab = lerp(a, b, t);
    let bc = lerp(b, c, t);
    let cd = lerp(c, d, t);
    let abc = lerp(ab, bc, t);
    let bcd = lerp(bc, cd, t);
    let m = lerp(abc, bcd, t);
    out.out_tangents[seg] = sub(ab, a);
    out.in_tangents[j] = sub(cd, d);
    out.vertices.insert(seg + 1, m);
    out.in_tangents.insert(seg + 1, sub(abc, m));
    out.out_tangents.insert(seg + 1, sub(bcd, m));
    // Feather points on later segments move up by one.
    for f in &mut out.feather {
        if f.segment as usize > seg {
            f.segment += 1;
        }
    }
    out
}

/// Remove vertex `i` (its neighbours keep their handles).
pub fn remove(p: &BezierPath, i: usize) -> BezierPath {
    let mut out = p.clone();
    if i < out.vertices.len() {
        out.vertices.remove(i);
        out.in_tangents.remove(i);
        out.out_tangents.remove(i);
        out.feather.retain(|f| f.segment as usize != i);
        for f in &mut out.feather {
            if f.segment as usize > i {
                f.segment -= 1;
            }
        }
    }
    out
}

/// Whether vertex `i` has no handles (a corner point).
pub fn is_corner(p: &BezierPath, i: usize) -> bool {
    len(p.in_tangents[i]) < 1e-9 && len(p.out_tangents[i]) < 1e-9
}

/// Convert Vertex tool click on a smooth vertex: it becomes a corner (no
/// handles). A click on a corner does nothing; dragging pulls out handles
/// (AE 26, R1).
pub fn to_corner(p: &BezierPath, i: usize) -> BezierPath {
    let mut out = p.clone();
    if i < out.vertices.len() {
        out.in_tangents[i] = [0.0, 0.0];
        out.out_tangents[i] = [0.0, 0.0];
    }
    out
}

/// RotoBezier tangents (AE-SHP-045, measured in AE 26 by R1): each vertex's
/// handles lie along the line from the previous to the next vertex, each a third
/// of its own adjacent segment's chord. End points of open paths stay corners
/// (待核).
pub fn roto_bezier(p: &BezierPath) -> BezierPath {
    let mut out = p.clone();
    let n = p.vertices.len();
    for i in 0..n {
        let has_prev = i > 0 || p.closed;
        let has_next = i + 1 < n || p.closed;
        out.in_tangents[i] = [0.0, 0.0];
        out.out_tangents[i] = [0.0, 0.0];
        if n < 3 && !p.closed || !has_prev || !has_next {
            continue;
        }
        let v = p.vertices[i];
        let a = p.vertices[(i + n - 1) % n];
        let b = p.vertices[(i + 1) % n];
        let d = sub(b, a);
        let l = len(d);
        if l < 1e-9 {
            continue;
        }
        let u = scale(d, 1.0 / l);
        out.in_tangents[i] = scale(u, -len(sub(v, a)) / 3.0);
        out.out_tangents[i] = scale(u, len(sub(b, v)) / 3.0);
    }
    out
}

/// The editable path prim at `path`: a mask or a shape Path.
fn require_path_prim(tx: &Transaction<'_>, path: &PrimPath) -> Result<(), CommandError> {
    let ty = tx
        .stage()
        .prim(path)
        .and_then(|v| v.prim_type().cloned())
        .ok_or_else(|| fail(format!("{path} does not exist")))?;
    if ty != PrimType::MASK && ty != PrimType::PATH {
        return Err(fail(format!("{path} is not a mask or a shape Path")));
    }
    ensure_unlocked(tx.stage(), [path])
}

/// Apply `f` to the static value and every keyframe of `path`'s Path.
fn edit_all(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    f: impl Fn(&BezierPath) -> Result<BezierPath, CommandError>,
) -> Result<(), CommandError> {
    require_path_prim(tx, path)?;
    let mut spec = query::attr_spec(tx.stage(), path, PATH_ATTR)
        .cloned()
        .ok_or_else(|| fail(format!("{path} has no path yet")))?;
    let apply = |v: &mut Value| -> Result<(), CommandError> {
        if let Value::Bezier(b) = v {
            *b = f(b)?;
        }
        Ok(())
    };
    if let Some(v) = spec.value.as_mut() {
        apply(v)?;
    }
    for k in spec.keyframes.values_mut() {
        apply(&mut k.value)?;
    }
    write_attr(tx, path, PATH_ATTR, spec)
}

// ---------------------------------------------------------------------------

pub struct SetVertices;

#[derive(Deserialize, JsonSchema)]
pub struct SetVerticesParams {
    /// The mask or shape Path prim.
    pub path: PrimPath,
    /// Keyframe time in flicks (layer time) when the path is animated.
    #[serde(default)]
    pub time: Option<Time>,
    /// The new path (layer space for masks, group space for shape paths).
    #[serde(flatten)]
    pub shape: BezierPath,
}

impl Command for SetVertices {
    type Params = SetVerticesParams;
    type Output = ();
    const ID: &'static str = "path.set_vertices";
    const TITLE: &'static str = "Edit Path";
    const DESCRIPTION: &'static str = "Replace the vertices and tangents of a mask path or shape Path (what drawing with the Pen tool, Convert Vertex and dragging vertices or handles do): a keyframe at `time` when the path is animated, the static value otherwise.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_path_prim(tx, &p.path)?;
        if p.shape.vertices.is_empty() || !p.shape.is_consistent() {
            return Err(fail(
                "a path needs vertices with matching in/out tangent arrays",
            ));
        }
        // Closing an open mask drawn with the Pen (mode None) makes it Add
        // (AE 26, R1).
        let is_mask = tx
            .stage()
            .prim(&p.path)
            .and_then(|v| v.prim_type().cloned())
            == Some(PrimType::MASK);
        let was_open = matches!(
            tx.stage().prim(&p.path).and_then(|v| query::static_value(&v, PATH_ATTR)),
            Some(Value::Bezier(ref b)) if !b.closed
        );
        let mode_none = tx
            .stage()
            .prim(&p.path)
            .and_then(|v| query::static_value(&v, "mode"))
            .and_then(|v| v.as_str().map(str::to_owned))
            .as_deref()
            == Some("none");
        let closes = is_mask && was_open && p.shape.closed && mode_none;
        let path = p.path.clone();
        SetAt.run(
            tx,
            SetAtParams {
                auto_key: false,
                path: p.path,
                attr: PATH_ATTR.into(),
                time: p.time.unwrap_or(Time::ZERO),
                value: Value::Bezier(p.shape),
            },
        )?;
        if closes {
            crate::util::set(tx, &path, "mode", Value::Enum("add".into()))?;
        }
        Ok(())
    }
}

pub struct InsertVertex;

#[derive(Deserialize, JsonSchema)]
pub struct InsertVertexParams {
    /// The mask or shape Path prim.
    pub path: PrimPath,
    /// Segment to split (0 = from vertex 0 to vertex 1).
    pub segment: usize,
    /// Where on the segment, 0–1 (default 0.5). The outline does not change.
    #[serde(default)]
    pub t: Option<f64>,
}

/// Output of `path.insert_vertex`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InsertVertexOutput {
    /// Index of the new vertex.
    pub index: usize,
}

impl Command for InsertVertex {
    type Params = InsertVertexParams;
    type Output = InsertVertexOutput;
    const ID: &'static str = "path.insert_vertex";
    const TITLE: &'static str = "Add Vertex";
    const DESCRIPTION: &'static str = "Add Vertex tool: split a segment of a mask path or shape Path, in every keyframe, without changing its outline.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<InsertVertexOutput, CommandError> {
        let t = p.t.unwrap_or(0.5);
        let seg = p.segment;
        edit_all(tx, &p.path, |b| {
            if seg >= segment_count(b) {
                return Err(fail(format!("the path has no segment {seg}")));
            }
            Ok(split(b, seg, t))
        })?;
        Ok(InsertVertexOutput { index: seg + 1 })
    }
}

pub struct DeleteVertex;

#[derive(Deserialize, JsonSchema)]
pub struct DeleteVertexParams {
    /// The mask or shape Path prim.
    pub path: PrimPath,
    /// The vertex to remove.
    pub index: usize,
}

impl Command for DeleteVertex {
    type Params = DeleteVertexParams;
    type Output = ();
    const ID: &'static str = "path.delete_vertex";
    const TITLE: &'static str = "Delete Vertex";
    const DESCRIPTION: &'static str = "Delete Vertex tool: remove a vertex of a mask path or shape Path, in every keyframe. The last vertex cannot be removed (delete the mask or shape instead).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let i = p.index;
        edit_all(tx, &p.path, |b| {
            if i >= b.vertices.len() {
                return Err(fail(format!("the path has no vertex {i}")));
            }
            if b.vertices.len() < 2 {
                return Err(fail("cannot delete the only vertex of a path"));
            }
            Ok(remove(b, i))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> BezierPath {
        BezierPath {
            vertices: vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
            in_tangents: vec![[0.0, 0.0]; 4],
            out_tangents: vec![[0.0, 0.0]; 4],
            closed: true,
            feather: Vec::new(),
        }
    }

    #[test]
    fn split_keeps_the_outline() {
        let mut p = square();
        p.out_tangents[0] = [30.0, -20.0];
        p.in_tangents[1] = [-30.0, -20.0];
        let before: Vec<P> = (0..=10).map(|i| point_at(&p, 0, i as f64 / 10.0)).collect();
        let q = split(&p, 0, 0.5);
        assert_eq!(q.vertices.len(), 5);
        assert_eq!(q.vertices[1], point_at(&p, 0, 0.5));
        // First half and second half trace the same curve.
        for i in 0..=5 {
            let t = i as f64 / 5.0;
            let a = point_at(&q, 0, t);
            let b = before[i];
            assert!(len(sub(a, b)) < 1e-9, "{a:?} {b:?}");
            let a = point_at(&q, 1, t);
            let b = before[i + 5];
            assert!(len(sub(a, b)) < 1e-9, "{a:?} {b:?}");
        }
        // The closing segment of a closed path splits too.
        let r = split(&square(), 3, 0.5);
        assert_eq!(r.vertices[4], [0.0, 50.0]);
    }

    #[test]
    fn corners_and_smooth_points() {
        let p = square();
        assert!(is_corner(&p, 1));
        let r = roto_bezier(&p);
        // Along prev → next (from (0,0) to (100,100)), a third of each chord.
        let d = 100.0 / 3.0 / 2f64.sqrt();
        assert!(len(sub(r.out_tangents[1], [d, d])) < 1e-9);
        assert!(len(sub(r.in_tangents[1], [-d, -d])) < 1e-9);
        assert_eq!(to_corner(&r, 1).in_tangents[1], [0.0, 0.0]);
        assert!((0..4).all(|i| !is_corner(&r, i)));
        let mut open = p.clone();
        open.closed = false;
        let r = roto_bezier(&open);
        assert!(is_corner(&r, 0) && is_corner(&r, 3) && !is_corner(&r, 1));
    }
}
