//! Path operators that work on kurbo paths: Trim Paths (AE-SHP-036) and the
//! geometric part of Merge Paths. Everything is in `f64`; arc lengths use kurbo's
//! adaptive arc-length with [`ARCLEN_ACCURACY`].

use kurbo::{BezPath, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point};

use crate::content::{TrimMode, TrimPaths};

/// Arc-length accuracy in pixels.
pub const ARCLEN_ACCURACY: f64 = 1e-4;

/// One sub-path as segments (the closing segment of a closed sub-path included).
#[derive(Clone, Debug)]
struct SubPath {
    segs: Vec<PathSeg>,
    lens: Vec<f64>,
    closed: bool,
}

impl SubPath {
    fn len(&self) -> f64 {
        self.lens.iter().sum()
    }
}

fn subpaths(path: &BezPath) -> Vec<SubPath> {
    let mut out: Vec<SubPath> = Vec::new();
    let mut last = Point::ZERO;
    let mut start = Point::ZERO;
    for el in path.elements() {
        let seg = match *el {
            PathEl::MoveTo(p) => {
                out.push(SubPath {
                    segs: Vec::new(),
                    lens: Vec::new(),
                    closed: false,
                });
                last = p;
                start = p;
                continue;
            }
            PathEl::LineTo(p) => PathSeg::Line(kurbo::Line::new(last, p)),
            PathEl::QuadTo(a, p) => PathSeg::Quad(kurbo::QuadBez::new(last, a, p)),
            PathEl::CurveTo(a, b, p) => PathSeg::Cubic(kurbo::CubicBez::new(last, a, b, p)),
            PathEl::ClosePath => {
                if let Some(sp) = out.last_mut() {
                    sp.closed = true;
                    if last != start {
                        let seg = PathSeg::Line(kurbo::Line::new(last, start));
                        sp.lens.push(seg.arclen(ARCLEN_ACCURACY));
                        sp.segs.push(seg);
                    }
                }
                last = start;
                continue;
            }
        };
        last = seg.end();
        if out.is_empty() {
            out.push(SubPath {
                segs: Vec::new(),
                lens: Vec::new(),
                closed: false,
            });
        }
        if let Some(sp) = out.last_mut() {
            sp.lens.push(seg.arclen(ARCLEN_ACCURACY));
            sp.segs.push(seg);
        }
    }
    out
}

/// Total arc length of a path (all sub-paths, closing segments included).
pub fn path_length(path: &BezPath) -> f64 {
    subpaths(path).iter().map(SubPath::len).sum()
}

/// The normalised trim ranges for Start/End/Offset, lottie-web's rules:
/// `None` = keep the input unchanged (full range), `Some([])` = draw nothing,
/// otherwise one or two `(from, to)` fractions of the total length.
pub fn trim_ranges(trim: &TrimPaths) -> Option<Vec<(f64, f64)>> {
    let mut o = trim.offset.rem_euclid(360.0) / 360.0;
    if !o.is_finite() {
        o = 0.0;
    }
    let mut s = (trim.start / 100.0).clamp(0.0, 1.0) + o;
    let mut e = (trim.end / 100.0).clamp(0.0, 1.0) + o;
    if s > e {
        std::mem::swap(&mut s, &mut e);
    }
    s = (s * 10000.0).round() / 10000.0;
    e = (e * 10000.0).round() / 10000.0;
    if s == e {
        return Some(Vec::new());
    }
    if s == 0.0 && e == 1.0 {
        return None;
    }
    Some(if e <= 1.0 {
        vec![(s, e)]
    } else if s >= 1.0 {
        vec![(s - 1.0, e - 1.0)]
    } else {
        vec![(s, 1.0), (0.0, e - 1.0)]
    })
}

/// Applies Trim Paths to a list of shapes (each the output of one path item, in
/// Timeline order). Returns the trimmed shapes, same count and order.
pub fn trim_shapes(shapes: &[BezPath], trim: &TrimPaths) -> Vec<BezPath> {
    let Some(ranges) = trim_ranges(trim) else {
        return shapes.to_vec();
    };
    if ranges.is_empty() {
        return vec![BezPath::new(); shapes.len()];
    }
    match trim.mode {
        TrimMode::Simultaneously => shapes
            .iter()
            .map(|shape| {
                let subs = subpaths(shape);
                let total: f64 = subs.iter().map(SubPath::len).sum();
                let abs: Vec<(f64, f64)> =
                    ranges.iter().map(|(a, b)| (a * total, b * total)).collect();
                extract(&subs, &abs, true)
            })
            .collect(),
        TrimMode::Individually => {
            let all: Vec<Vec<SubPath>> = shapes.iter().map(subpaths).collect();
            let lens: Vec<f64> = all
                .iter()
                .map(|s| s.iter().map(SubPath::len).sum())
                .collect();
            let total: f64 = lens.iter().sum();
            let mut offset = 0.0;
            all.iter()
                .zip(&lens)
                .map(|(subs, len)| {
                    let local: Vec<(f64, f64)> = ranges
                        .iter()
                        .filter_map(|(a, b)| {
                            let from = (a * total - offset).max(0.0);
                            let to = (b * total - offset).min(*len);
                            (to > from).then_some((from, to))
                        })
                        .collect();
                    offset += len;
                    // Joining through vertex 0 only applies within one shape.
                    let join = ranges.len() == 2 && local.len() == 2;
                    extract(subs, &local, join)
                })
                .collect()
        }
    }
}

/// The parts of `subs` between the absolute lengths in `ranges` (in order). When
/// `join_wrap` and the ranges are `[(s, end), (0, e)]` on a shape whose last
/// sub-path is closed, the two pieces form one open path through vertex 0.
fn extract(subs: &[SubPath], ranges: &[(f64, f64)], join_wrap: bool) -> BezPath {
    let mut out = BezPath::new();
    let total: f64 = subs.iter().map(SubPath::len).sum();
    let wrap = join_wrap
        && ranges.len() == 2
        && subs.len() == 1
        && subs[0].closed
        && (ranges[0].1 - total).abs() < 1e-9
        && ranges[1].0 == 0.0;
    for (i, &(from, to)) in ranges.iter().enumerate() {
        let continue_from_previous = wrap && i == 1;
        append_range(subs, from, to, &mut out, continue_from_previous);
    }
    out
}

/// Appends the part of `subs` in `[from, to]` (absolute lengths). Each sub-path
/// that is touched contributes one open piece; with `continuing`, the first piece
/// continues the current sub-path of `out` instead of starting a new one.
fn append_range(subs: &[SubPath], from: f64, to: f64, out: &mut BezPath, mut continuing: bool) {
    let mut base = 0.0;
    for sp in subs {
        let sp_len = sp.len();
        let (a, b) = ((from - base).max(0.0), (to - base).min(sp_len));
        base += sp_len;
        if b <= a {
            continue;
        }
        let mut seg_base = 0.0;
        let mut started = continuing;
        for (seg, &seg_len) in sp.segs.iter().zip(&sp.lens) {
            let (lo, hi) = (a.max(seg_base), b.min(seg_base + seg_len));
            let local = lo - seg_base;
            seg_base += seg_len;
            if hi <= lo {
                continue;
            }
            let t0 = param_at(seg, local, seg_len);
            let t1 = param_at(seg, local + (hi - lo), seg_len);
            let piece = seg.subsegment(t0..t1);
            if !started {
                out.move_to(piece.start());
                started = true;
            }
            push_seg(out, &piece);
        }
        continuing = false;
    }
}

fn param_at(seg: &PathSeg, len: f64, seg_len: f64) -> f64 {
    if seg_len <= 0.0 || len <= 0.0 {
        0.0
    } else if len >= seg_len {
        1.0
    } else {
        seg.inv_arclen(len, ARCLEN_ACCURACY)
    }
}

fn push_seg(out: &mut BezPath, seg: &PathSeg) {
    match *seg {
        PathSeg::Line(l) => out.line_to(l.p1),
        PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
        PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
    }
}

/// Merge Paths in `merge` mode: all shapes as one compound path (sub-paths kept,
/// the paint's fill rule decides overlaps).
pub fn merge_concat(shapes: &[BezPath]) -> BezPath {
    let mut out = BezPath::new();
    for s in shapes {
        out.extend(s.iter());
    }
    out
}

#[cfg(test)]
mod tests {
    use kurbo::Shape;

    use super::*;

    fn line(len: f64) -> BezPath {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((len, 0.0));
        p
    }

    fn square() -> BezPath {
        kurbo::Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1)
    }

    fn trim(start: f64, end: f64, offset: f64) -> TrimPaths {
        TrimPaths {
            start,
            end,
            offset,
            mode: TrimMode::Simultaneously,
        }
    }

    #[test]
    fn ranges_follow_lottie_rules() {
        assert_eq!(trim_ranges(&trim(0.0, 100.0, 0.0)), None);
        assert_eq!(trim_ranges(&trim(30.0, 30.0, 0.0)), Some(vec![]));
        assert_eq!(trim_ranges(&trim(80.0, 20.0, 0.0)), Some(vec![(0.2, 0.8)]));
        assert_eq!(
            trim_ranges(&trim(50.0, 100.0, 90.0)),
            Some(vec![(0.75, 1.0), (0.0, 0.25)])
        );
        assert_eq!(
            trim_ranges(&trim(0.0, 50.0, -180.0)),
            Some(vec![(0.5, 1.0)])
        );
        assert_eq!(trim_ranges(&trim(-20.0, 150.0, 0.0)), None);
    }

    #[test]
    fn trims_by_arc_length() {
        let out = trim_shapes(&[line(100.0)], &trim(25.0, 75.0, 0.0));
        assert_eq!(out[0].elements().len(), 2);
        assert!((path_length(&out[0]) - 50.0).abs() < 1e-6);
        assert_eq!(out[0].elements()[0], PathEl::MoveTo(Point::new(25.0, 0.0)));
        let curve = kurbo::Circle::new((0.0, 0.0), 50.0).to_path(0.01);
        let full = path_length(&curve);
        let half = trim_shapes(std::slice::from_ref(&curve), &trim(0.0, 50.0, 0.0));
        assert!((path_length(&half[0]) - full / 2.0).abs() < 1e-3);
    }

    #[test]
    fn wrap_around_joins_through_vertex_zero() {
        // Offset 90° on a closed square: [0.75, 1] + [0, 0.25] → one open path.
        let out = trim_shapes(&[square()], &trim(50.0, 100.0, 90.0));
        let moves = out[0]
            .elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count();
        assert_eq!(moves, 1);
        assert!((path_length(&out[0]) - 20.0).abs() < 1e-6);
        assert!(!out[0].elements().contains(&PathEl::ClosePath));
    }

    #[test]
    fn simultaneously_vs_individually() {
        let shapes = [line(100.0), line(300.0)];
        let sim = trim_shapes(&shapes, &trim(0.0, 50.0, 0.0));
        assert!((path_length(&sim[0]) - 50.0).abs() < 1e-6);
        assert!((path_length(&sim[1]) - 150.0).abs() < 1e-6);
        let ind = trim_shapes(
            &shapes,
            &TrimPaths {
                mode: TrimMode::Individually,
                ..trim(0.0, 50.0, 0.0)
            },
        );
        // First 200 of 400: all of the first line and 100 of the second.
        assert!((path_length(&ind[0]) - 100.0).abs() < 1e-6);
        assert!((path_length(&ind[1]) - 100.0).abs() < 1e-6);
        let tail = trim_shapes(
            &shapes,
            &TrimPaths {
                mode: TrimMode::Individually,
                ..trim(50.0, 100.0, 0.0)
            },
        );
        assert!(tail[0].is_empty());
        assert!((path_length(&tail[1]) - 200.0).abs() < 1e-6);
    }

    #[test]
    fn multiple_subpaths_concatenate() {
        let mut two = line(100.0);
        two.move_to((0.0, 10.0));
        two.line_to((100.0, 10.0));
        let out = trim_shapes(&[two], &trim(25.0, 75.0, 0.0));
        assert!((path_length(&out[0]) - 100.0).abs() < 1e-6);
        assert_eq!(crate::path::subpath_count(&out[0]), 2);
    }

    #[test]
    fn empty_range_draws_nothing() {
        let out = trim_shapes(&[line(10.0)], &trim(40.0, 40.0, 0.0));
        assert!(out[0].is_empty());
    }
}
