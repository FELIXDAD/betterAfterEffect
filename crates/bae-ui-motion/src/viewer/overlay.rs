//! Viewer overlays drawn over the image: mask paths of the selected layers (the
//! renderer does not draw masks yet; AE draws the outlines in the mask color
//! anyway), and helpers to draw Bezier paths given in a layer's space.

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_model::{BezierPath, PrimPath, Stage, Value};
use bae_motion::query;
use egui::epaint::CubicBezierShape;
use egui::{Color32, Painter, Pos2, Shape, Stroke};

use crate::render::View;

/// Screen position of a point in a layer's space.
pub(super) fn to_screen(view: &View, m: &DMat4, p: [f64; 2]) -> Pos2 {
    let w = m.transform_point3(DVec3::new(p[0], p[1], 0.0));
    view.to_screen(DVec2::new(w.x, w.y))
}

/// The segments of `path` (in the space `m` maps to comp space) as screen shapes.
pub(crate) fn path_shapes(path: &BezierPath, m: &DMat4, view: &View, stroke: Stroke) -> Vec<Shape> {
    let n = path.vertices.len();
    if n == 0 || !path.is_consistent() {
        return Vec::new();
    }
    let segments = if path.closed { n } else { n - 1 };
    let add = |a: [f64; 2], b: [f64; 2]| [a[0] + b[0], a[1] + b[1]];
    (0..segments)
        .map(|i| {
            let j = (i + 1) % n;
            let pts = [
                to_screen(view, m, path.vertices[i]),
                to_screen(view, m, add(path.vertices[i], path.out_tangents[i])),
                to_screen(view, m, add(path.vertices[j], path.in_tangents[j])),
                to_screen(view, m, path.vertices[j]),
            ];
            Shape::CubicBezier(CubicBezierShape::from_points_stroke(
                pts,
                false,
                Color32::TRANSPARENT,
                stroke,
            ))
        })
        .collect()
}

/// A mask's outline color (its `color` attribute).
pub(crate) fn mask_color(stage: &Stage, mask: &PrimPath) -> Color32 {
    let c = stage
        .prim(mask)
        .and_then(|v| query::static_value(&v, "color"))
        .and_then(|v| match v {
            Value::Color(c) => Some(c),
            _ => None,
        })
        .unwrap_or([1.0, 1.0, 0.0, 1.0]);
    let u = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(u(c[0]), u(c[1]), u(c[2]))
}

/// Mask outlines of the selected layers.
pub(super) fn masks(
    painter: &Painter,
    stage: &Stage,
    comp: &EvaluatedComp,
    view: &View,
    selected: &[PrimPath],
) {
    for l in comp.layers.iter().filter(|l| selected.contains(&l.path)) {
        for m in &l.masks {
            let stroke = Stroke::new(1.0, mask_color(stage, &m.path));
            painter.extend(path_shapes(&m.shape, &l.world_matrix, view, stroke));
        }
    }
}

/// A path the pen and vertex tools can edit: a mask of a selected layer, or a
/// Bezier Path in a selected shape layer.
#[derive(Clone, Debug)]
pub(crate) struct EditablePath {
    /// The mask or shape Path prim.
    pub prim: PrimPath,
    /// Its value at the current time, in its own space.
    pub path: BezierPath,
    /// Path space → comp space.
    pub to_comp: DMat4,
    pub color: Color32,
}

impl EditablePath {
    pub fn screen(&self, view: &View, i: usize) -> Pos2 {
        to_screen(view, &self.to_comp, self.path.vertices[i])
    }

    /// Comp space → path space.
    pub fn to_local(&self, p: DVec2) -> [f64; 2] {
        let v = self
            .to_comp
            .inverse()
            .transform_point3(DVec3::new(p.x, p.y, 0.0));
        [v.x, v.y]
    }
}

/// Editable paths of the selected layers, topmost layer first: masks, then
/// (with `shapes`, the settings their colors follow) the Bezier paths of shape
/// layers.
pub(crate) fn editable_paths(
    stage: &Stage,
    comp: &EvaluatedComp,
    selected: &[PrimPath],
    shapes: Option<super::prefs::ViewerPrefs>,
) -> Vec<EditablePath> {
    let mut out = Vec::new();
    for l in comp.layers.iter().filter(|l| selected.contains(&l.path)) {
        for m in &l.masks {
            out.push(EditablePath {
                prim: m.path.clone(),
                path: m.shape.clone(),
                to_comp: l.world_matrix,
                color: mask_color(stage, &m.path),
            });
        }
        if let Some(prefs) = shapes
            && l.kind == bae_model::LayerKind::Shape
        {
            let label = stage
                .prim(&l.path)
                .and_then(|v| query::static_value(&v, "label"))
                .and_then(|v| v.as_f64())
                .map_or(8, |v| v as usize);
            let color = prefs.handle_color(bae_ui::theme::label_color(label));
            shape_paths(&l.contents, &l.world_matrix, color, &mut out);
        }
    }
    out
}

fn shape_paths(
    items: &[bae_anim::EvaluatedPrim],
    m: &DMat4,
    color: Color32,
    out: &mut Vec<EditablePath>,
) {
    use bae_model::PrimType;
    for item in items {
        match item.prim_type.as_ref() {
            Some(t) if *t == PrimType::PATH => {
                if let Some(Value::Bezier(b)) = item.get("path") {
                    out.push(EditablePath {
                        prim: item.path.clone(),
                        path: b.clone(),
                        to_comp: *m,
                        color,
                    });
                }
            }
            Some(t) if *t == PrimType::SHAPE_GROUP => {
                let gm = *m * crate::render::group_matrix(item);
                shape_paths(&item.children, &gm, color, out);
            }
            // A group's `contents` folder (or an untyped container).
            Some(t) if *t == PrimType::FOLDER => shape_paths(&item.children, m, color, out),
            Some(_) => {}
            None => shape_paths(&item.children, m, color, out),
        }
    }
}

/// Vertex squares (AE: hollow; `filled` ones are selected).
pub(super) fn vertices(painter: &Painter, view: &View, p: &EditablePath, filled: Option<usize>) {
    for i in 0..p.path.vertices.len() {
        let r = egui::Rect::from_center_size(p.screen(view, i), egui::vec2(view.point, view.point));
        if filled == Some(i) {
            painter.rect_filled(r, 0.0, p.color);
        } else {
            painter.rect_filled(r, 0.0, Color32::from_black_alpha(160));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.color), egui::StrokeKind::Middle);
        }
    }
}

/// What the pointer is over on an editable path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PathHit {
    Vertex { path: usize, index: usize },
    Segment { path: usize, segment: usize, t: f64 },
}

/// Pick radius of vertices and segments, points.
const VERTEX_PICK: f32 = 5.0;
const SEGMENT_PICK: f32 = 4.0;

/// The vertex (first) or segment of `paths` under `pos`.
pub(crate) fn hit(paths: &[EditablePath], view: &View, pos: Pos2) -> Option<PathHit> {
    for (pi, p) in paths.iter().enumerate() {
        for i in 0..p.path.vertices.len() {
            if p.screen(view, i).distance(pos) <= VERTEX_PICK {
                return Some(PathHit::Vertex { path: pi, index: i });
            }
        }
    }
    let mut best: Option<(f32, PathHit)> = None;
    for (pi, p) in paths.iter().enumerate() {
        if !p.path.is_consistent() {
            continue;
        }
        for seg in 0..bae_motion::path::segment_count(&p.path) {
            let at = |t: f64| {
                to_screen(
                    view,
                    &p.to_comp,
                    bae_motion::path::point_at(&p.path, seg, t),
                )
            };
            const N: usize = 32;
            let (mut bt, mut bd) = (0.0, f32::MAX);
            for k in 0..=N {
                let t = k as f64 / N as f64;
                let d = at(t).distance(pos);
                if d < bd {
                    (bt, bd) = (t, d);
                }
            }
            // Refine around the best sample.
            let (mut lo, mut hi) = (
                (bt - 1.0 / N as f64).max(0.0),
                (bt + 1.0 / N as f64).min(1.0),
            );
            for _ in 0..20 {
                let a = lo + (hi - lo) / 3.0;
                let b = hi - (hi - lo) / 3.0;
                if at(a).distance(pos) < at(b).distance(pos) {
                    hi = b;
                } else {
                    lo = a;
                }
            }
            let t = (lo + hi) / 2.0;
            let d = at(t).distance(pos);
            if d <= SEGMENT_PICK && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((
                    d,
                    PathHit::Segment {
                        path: pi,
                        segment: seg,
                        t,
                    },
                ));
            }
        }
    }
    best.map(|(_, h)| h)
}
