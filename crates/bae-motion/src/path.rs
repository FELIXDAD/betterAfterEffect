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

/// Convert Vertex tool click: a smooth vertex becomes a corner (no handles), a
/// corner becomes smooth, its handles along the direction from the previous to
/// the next vertex, each a third of the adjacent segment's chord (AE-SHP-044,
/// handle length 待核).
pub fn toggle_corner(p: &BezierPath, i: usize) -> BezierPath {
    let mut out = p.clone();
    let n = p.vertices.len();
    if i >= n {
        return out;
    }
    if !is_corner(p, i) {
        out.in_tangents[i] = [0.0, 0.0];
        out.out_tangents[i] = [0.0, 0.0];
        return out;
    }
    let v = p.vertices[i];
    let prev = (i > 0 || p.closed).then(|| p.vertices[(i + n - 1) % n]);
    let next = (i + 1 < n || p.closed).then(|| p.vertices[(i + 1) % n]);
    let (dir, l_in, l_out) = match (prev, next) {
        (Some(a), Some(b)) => (sub(b, a), len(sub(v, a)), len(sub(b, v))),
        (None, Some(b)) => (sub(b, v), 0.0, len(sub(b, v))),
        (Some(a), None) => (sub(v, a), len(sub(v, a)), 0.0),
        (None, None) => return out,
    };
    let d = len(dir);
    if d < 1e-9 {
        return out;
    }
    let u = scale(dir, 1.0 / d);
    out.in_tangents[i] = scale(u, -l_in / 3.0);
    out.out_tangents[i] = scale(u, l_out / 3.0);
    out
}

/// RotoBezier tangents (AE-SHP-045): every vertex smooth, its handles parallel
/// to the line through its neighbours (Catmull-Rom, a sixth of that distance;
/// AE's formula and default 33 % tension 待核). End points of open paths stay
/// corners.
pub fn roto_bezier(p: &BezierPath) -> BezierPath {
    let mut out = p.clone();
    let n = p.vertices.len();
    for i in 0..n {
        let has_prev = i > 0 || p.closed;
        let has_next = i + 1 < n || p.closed;
        if n < 3 && !p.closed || !has_prev || !has_next {
            out.in_tangents[i] = [0.0, 0.0];
            out.out_tangents[i] = [0.0, 0.0];
            continue;
        }
        let a = p.vertices[(i + n - 1) % n];
        let b = p.vertices[(i + 1) % n];
        let t = scale(sub(b, a), 1.0 / 6.0);
        out.in_tangents[i] = scale(t, -1.0);
        out.out_tangents[i] = t;
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
        SetAt.run(
            tx,
            SetAtParams {
                path: p.path,
                attr: PATH_ATTR.into(),
                time: p.time.unwrap_or(Time::ZERO),
                value: Value::Bezier(p.shape),
            },
        )
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
        let s = toggle_corner(&p, 1);
        assert!(!is_corner(&s, 1));
        // Along prev → next (from (0,0) to (100,100)), a third of each chord.
        let d = 100.0 / 3.0 / 2f64.sqrt();
        assert!(len(sub(s.out_tangents[1], [d, d])) < 1e-9);
        assert!(len(sub(s.in_tangents[1], [-d, -d])) < 1e-9);
        assert_eq!(toggle_corner(&s, 1), p);
        let r = roto_bezier(&p);
        assert!((0..4).all(|i| !is_corner(&r, i)));
        let mut open = p.clone();
        open.closed = false;
        let r = roto_bezier(&open);
        assert!(is_corner(&r, 0) && is_corner(&r, 3) && !is_corner(&r, 1));
    }
}
