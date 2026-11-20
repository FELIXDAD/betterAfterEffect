//! Path geometry: AE's vertex + tangent form ([`BezierPath`]) and kurbo's element
//! form ([`BezPath`]).
//!
//! The project stores paths the way AE does (`Shape.vertices`, `inTangents`,
//! `outTangents`, `closed`; tangents relative to their vertex). Everything that
//! measures, trims, strokes or draws works on [`BezPath`] in `f64`; conversion
//! to `f32` happens only at the Vello boundary.
//!
//! Segments whose two handles are both zero become straight lines, so strokes get
//! well-defined joins at corners (a cubic with zero handles has no end tangent).

use bae_model::BezierPath;
use kurbo::{BezPath, PathEl, Point, Vec2};

/// `[x, y]` as a point.
pub fn point(a: [f64; 2]) -> Point {
    Point::new(a[0], a[1])
}

/// A point or vector as `[x, y]`.
pub fn xy(p: impl Into<(f64, f64)>) -> [f64; 2] {
    let (x, y) = p.into();
    [x, y]
}

/// Absolute control points of the segment from vertex `i` to vertex `j`.
fn segment(path: &BezierPath, i: usize, j: usize) -> (Point, Point, Point, Point) {
    let v = |k: usize| point(path.vertices[k]);
    let tangent =
        |list: &[[f64; 2]], k: usize| list.get(k).map_or(Vec2::ZERO, |t| Vec2::new(t[0], t[1]));
    let p0 = v(i);
    let p3 = v(j);
    let c1 = p0 + tangent(&path.out_tangents, i);
    let c2 = p3 + tangent(&path.in_tangents, j);
    (p0, c1, c2, p3)
}

fn push_segment(out: &mut BezPath, (p0, c1, c2, p3): (Point, Point, Point, Point)) {
    if c1 == p0 && c2 == p3 {
        out.line_to(p3);
    } else {
        out.curve_to(c1, c2, p3);
    }
}

/// Appends `path` as one sub-path of `out`. Empty paths add nothing; a closed path
/// ends with its closing segment followed by `ClosePath`.
pub fn append_bezier(path: &BezierPath, out: &mut BezPath) {
    let n = path.vertices.len();
    if n == 0 {
        return;
    }
    out.move_to(point(path.vertices[0]));
    for i in 0..n - 1 {
        push_segment(out, segment(path, i, i + 1));
    }
    if path.closed {
        let closing = segment(path, n - 1, 0);
        // A straight closing segment is implied by ClosePath.
        if closing.1 != closing.0 || closing.2 != closing.3 {
            out.curve_to(closing.1, closing.2, closing.3);
        }
        out.close_path();
    }
}

/// `path` as a kurbo path (one sub-path).
pub fn to_bez_path(path: &BezierPath) -> BezPath {
    let mut out = BezPath::new();
    append_bezier(path, &mut out);
    out
}

/// Splits a kurbo path into AE paths, one per sub-path. Quadratic segments are
/// raised to cubics. A closed sub-path whose last segment returns to its start
/// point folds that segment into the closing segment (AE has no duplicate vertex).
pub fn from_bez_path(path: &BezPath) -> Vec<BezierPath> {
    let mut out = Vec::new();
    let mut cur: Option<BezierPath> = None;
    let mut last = Point::ZERO;
    let finish = |cur: &mut Option<BezierPath>, out: &mut Vec<BezierPath>| {
        if let Some(p) = cur.take() {
            out.push(p);
        }
    };
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                finish(&mut cur, &mut out);
                cur = Some(BezierPath {
                    vertices: vec![xy(p)],
                    in_tangents: vec![[0.0; 2]],
                    out_tangents: vec![[0.0; 2]],
                    closed: false,
                    feather: Vec::new(),
                });
                last = p;
            }
            PathEl::LineTo(p) => {
                if let Some(c) = cur.as_mut() {
                    push_vertex(c, p, Vec2::ZERO, Vec2::ZERO);
                }
                last = p;
            }
            PathEl::QuadTo(q, p) => {
                if let Some(c) = cur.as_mut() {
                    let c1 = last + (q - last) * (2.0 / 3.0);
                    let c2 = p + (q - p) * (2.0 / 3.0);
                    push_vertex(c, p, c1 - last, c2 - p);
                }
                last = p;
            }
            PathEl::CurveTo(c1, c2, p) => {
                if let Some(c) = cur.as_mut() {
                    push_vertex(c, p, c1 - last, c2 - p);
                }
                last = p;
            }
            PathEl::ClosePath => {
                if let Some(c) = cur.as_mut() {
                    c.closed = true;
                    let n = c.vertices.len();
                    if n > 1 && c.vertices[n - 1] == c.vertices[0] {
                        // Fold the duplicate end vertex into vertex 0.
                        c.vertices.pop();
                        let in_t = c.in_tangents.pop().unwrap_or([0.0; 2]);
                        c.out_tangents.pop();
                        c.in_tangents[0] = in_t;
                    }
                    last = point(c.vertices[0]);
                }
                finish(&mut cur, &mut out);
            }
        }
    }
    finish(&mut cur, &mut out);
    out
}

/// Appends vertex `p` reached by a segment whose handles are `out_prev` (relative
/// to the previous vertex) and `in_p` (relative to `p`).
fn push_vertex(c: &mut BezierPath, p: Point, out_prev: Vec2, in_p: Vec2) {
    if let Some(o) = c.out_tangents.last_mut() {
        *o = xy(out_prev);
    }
    c.vertices.push(xy(p));
    c.in_tangents.push(xy(in_p));
    c.out_tangents.push([0.0; 2]);
}

/// The same path traversed backwards (AE "Reverse Path Direction" on a Bezier
/// path, AE-SHP-014): vertex 0 stays first, followed by `n−1, n−2, …, 1`; each
/// vertex swaps its in and out tangents. Open paths are simply reversed.
pub fn reversed(path: &BezierPath) -> BezierPath {
    let n = path.vertices.len();
    let tangent = |list: &[[f64; 2]], k: usize| list.get(k).copied().unwrap_or([0.0; 2]);
    let order: Vec<usize> = if path.closed && n > 0 {
        std::iter::once(0).chain((1..n).rev()).collect()
    } else {
        (0..n).rev().collect()
    };
    BezierPath {
        vertices: order.iter().map(|&k| path.vertices[k]).collect(),
        in_tangents: order
            .iter()
            .map(|&k| tangent(&path.out_tangents, k))
            .collect(),
        out_tangents: order
            .iter()
            .map(|&k| tangent(&path.in_tangents, k))
            .collect(),
        closed: path.closed,
        feather: Vec::new(),
    }
}

/// Number of sub-paths (`MoveTo`s) of a kurbo path.
pub fn subpath_count(path: &BezPath) -> usize {
    path.elements()
        .iter()
        .filter(|e| matches!(e, PathEl::MoveTo(_)))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(closed: bool) -> BezierPath {
        BezierPath {
            vertices: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            in_tangents: vec![[0.0; 2]; 4],
            out_tangents: vec![[0.0; 2]; 4],
            closed,
            feather: Vec::new(),
        }
    }

    #[test]
    fn straight_segments_become_lines() {
        let p = to_bez_path(&square(true));
        let els = p.elements();
        assert_eq!(els.len(), 5);
        assert!(matches!(els[0], PathEl::MoveTo(_)));
        assert!(els[1..4].iter().all(|e| matches!(e, PathEl::LineTo(_))));
        assert_eq!(els[4], PathEl::ClosePath);
    }

    #[test]
    fn curved_closing_segment_is_kept() {
        let mut c = square(true);
        c.in_tangents[0] = [-5.0, 0.0];
        let p = to_bez_path(&c);
        let els = p.elements();
        assert!(matches!(els[4], PathEl::CurveTo(_, _, p) if p == Point::ZERO));
        assert_eq!(els[5], PathEl::ClosePath);
        // Round trip folds the closing curve back into vertex 0's in-tangent.
        let back = from_bez_path(&p);
        assert_eq!(back, vec![c]);
    }

    #[test]
    fn round_trip_open_and_closed() {
        for closed in [false, true] {
            let mut sq = square(closed);
            sq.out_tangents[1] = [0.0, 3.0];
            sq.in_tangents[2] = [2.0, -1.0];
            assert_eq!(from_bez_path(&to_bez_path(&sq)), vec![sq]);
        }
    }

    #[test]
    fn missing_tangents_count_as_zero() {
        let mut sq = square(false);
        sq.in_tangents.clear();
        sq.out_tangents.truncate(1);
        assert_eq!(to_bez_path(&sq).elements().len(), 4);
    }

    #[test]
    fn reverse_keeps_first_vertex_of_closed_paths() {
        let mut sq = square(true);
        sq.out_tangents[0] = [1.0, 0.0];
        let r = reversed(&sq);
        assert_eq!(
            r.vertices,
            vec![[0.0, 0.0], [0.0, 10.0], [10.0, 10.0], [10.0, 0.0]]
        );
        assert_eq!(r.in_tangents[0], [1.0, 0.0]);
        assert_eq!(reversed(&r), sq);
        let open = reversed(&square(false));
        assert_eq!(open.vertices[0], [0.0, 10.0]);
    }

    #[test]
    fn empty_path_draws_nothing() {
        assert!(to_bez_path(&BezierPath::default()).is_empty());
        assert_eq!(subpath_count(&BezPath::new()), 0);
    }
}
