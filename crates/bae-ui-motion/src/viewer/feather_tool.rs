//! The Mask Feather tool (AE-CMP-043): a press on a mask path of a selected
//! layer adds a variable-width feather point there and the drag sets its radius
//! (outward or inward); dragging a point changes its radius; Alt+click deletes it.
//! Each gesture is one `attr.set` of the mask's `feather_points`.

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec3};
use bae_model::{BezierPath, PrimPath, PrimType, Stage};
use bae_ui::PanelContext;
use egui::{Painter, Pos2, Response, Stroke, Ui};
use serde_json::json;

use super::overlay::{self, EditablePath, PathHit};
use crate::render::View;
use crate::session::SharedSession;
use crate::tools::Tool;

/// Pick radius of a feather point (screen points).
const PICK: f32 = 6.0;

#[derive(Default)]
pub(super) struct FeatherState {
    drag: Option<Drag>,
}

struct Drag {
    mask: PrimPath,
    path: BezierPath,
    to_comp: DMat4,
    points: Vec<[f32; 2]>,
    index: usize,
}

fn points_of(stage: &Stage, mask: &PrimPath) -> Vec<[f32; 2]> {
    bae_render::feather_points(
        stage
            .prim(mask)
            .and_then(|v| v.attr("feather_points").and_then(|s| s.value.clone())),
    )
}

/// Where a feather point's handle is on screen.
fn handle(path: &BezierPath, to_comp: &DMat4, p: [f32; 2], view: &View) -> Option<Pos2> {
    let (at, n) = bae_render::mask::path_frame(path, f64::from(p[0]))?;
    let r = f64::from(p[1]);
    Some(overlay::to_screen(
        view,
        to_comp,
        [at[0] + n[0] * r, at[1] + n[1] * r],
    ))
}

fn commit(cx: &mut PanelContext<'_>, mask: &PrimPath, points: &[[f32; 2]]) {
    let flat: Vec<f64> = points
        .iter()
        .flat_map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect();
    cx.invoke_with(
        "attr.set",
        json!({"path": mask, "attr": "feather_points", "value": {"type": "float_list", "value": flat}}),
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn frame(
    state: &mut FeatherState,
    ui: &Ui,
    resp: &Response,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    comp: &EvaluatedComp,
    view: &View,
    tool: Tool,
    painter: &Painter,
) {
    if tool != Tool::MaskFeather {
        state.drag = None;
        return;
    }
    let stage = cx.stage;
    let selected = session.lock().selected_layers.clone();
    let paths: Vec<EditablePath> = overlay::editable_paths(stage, comp, &selected, None)
        .into_iter()
        .filter(|p| stage.prim(&p.prim).is_some_and(|v| v.is_a(&PrimType::MASK)))
        .collect();
    // Feather boundaries and points of the selected layers' masks.
    for p in &paths {
        let points = points_of(stage, &p.prim);
        let stroke = Stroke::new(1.0, p.color.gamma_multiply(0.6));
        for line in bae_render::mask::feather_offsets(&p.path, &points, &[0.0, 1.0]) {
            let screen: Vec<Pos2> = line
                .iter()
                .map(|q| overlay::to_screen(view, &p.to_comp, [q.x, q.y]))
                .collect();
            painter.add(egui::Shape::closed_line(screen, stroke));
        }
        for fp in &points {
            if let Some(h) = handle(&p.path, &p.to_comp, *fp, view) {
                painter.circle_stroke(h, 3.5, Stroke::new(1.0, p.color));
            }
        }
    }
    let (pressed, released, alt) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.modifiers.alt,
        )
    });
    let hover = resp.hover_pos();
    if pressed
        && resp.hovered()
        && let Some(pos) = hover
    {
        let on_point = paths.iter().find_map(|p| {
            let points = points_of(stage, &p.prim);
            points
                .iter()
                .position(|fp| {
                    handle(&p.path, &p.to_comp, *fp, view).is_some_and(|h| h.distance(pos) <= PICK)
                })
                .map(|i| (p, points, i))
        });
        if let Some((p, mut points, index)) = on_point {
            if alt {
                points.remove(index);
                commit(cx, &p.prim, &points);
            } else {
                state.drag = Some(Drag {
                    mask: p.prim.clone(),
                    path: p.path.clone(),
                    to_comp: p.to_comp,
                    points,
                    index,
                });
            }
        } else if let Some(PathHit::Segment { path, segment, t }) = overlay::hit(&paths, view, pos)
        {
            let p = &paths[path];
            let mut points = points_of(stage, &p.prim);
            points.push([(segment as f64 + t) as f32, 0.0]);
            state.drag = Some(Drag {
                mask: p.prim.clone(),
                path: p.path.clone(),
                to_comp: p.to_comp,
                index: points.len() - 1,
                points,
            });
        }
    }
    if let Some(d) = state.drag.as_mut() {
        // The radius follows the pointer along the path's normal at the point.
        if let Some(pos) = resp.interact_pointer_pos().or(hover)
            && let Some((at, n)) =
                bae_render::mask::path_frame(&d.path, f64::from(d.points[d.index][0]))
        {
            let c = view.to_comp(pos);
            let q = d
                .to_comp
                .inverse()
                .transform_point3(DVec3::new(c.x, c.y, 0.0));
            d.points[d.index][1] = ((q.x - at[0]) * n[0] + (q.y - at[1]) * n[1]) as f32;
        }
        if let Some(h) = handle(&d.path, &d.to_comp, d.points[d.index], view) {
            painter.circle_filled(h, 3.5, bae_ui::theme::color::ACCENT);
        }
    }
    if released && let Some(d) = state.drag.take() {
        commit(cx, &d.mask, &d.points);
    }
}
