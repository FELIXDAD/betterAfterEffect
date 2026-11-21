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
pub(super) fn path_shapes(path: &BezierPath, m: &DMat4, view: &View, stroke: Stroke) -> Vec<Shape> {
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
pub(super) fn mask_color(stage: &Stage, mask: &PrimPath) -> Color32 {
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
