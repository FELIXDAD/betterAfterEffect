//! The Mask Feather tool (AE-CMP-043): a press on a mask path of a selected
//! layer adds a variable-width feather point there and the drag sets its radius
//! (outward, or inward past the path); dragging a point changes its radius;
//! Alt+click deletes it. Feather points are part of the mask path (they animate
//! with its keyframes), so each gesture is one `path.set_vertices`.

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec3};
use bae_model::{BezierPath, FeatherPoint, PrimPath, PrimType};
use bae_motion::query::CompInfo;
use bae_ui::PanelContext;
use egui::{Painter, Pos2, Response, Stroke, Ui};

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
    index: usize,
}

/// Where a feather point's handle is on screen.
fn handle(path: &BezierPath, to_comp: &DMat4, f: &FeatherPoint, view: &View) -> Option<Pos2> {
    let (at, n) = bae_render::mask::path_frame(path, f64::from(f.segment) + f.t)?;
    let r = if f.inner {
        -f.radius.abs()
    } else {
        f.radius.abs()
    };
    Some(overlay::to_screen(
        view,
        to_comp,
        [at[0] + n[0] * r, at[1] + n[1] * r],
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn frame(
    state: &mut FeatherState,
    ui: &Ui,
    resp: &Response,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
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
        let stroke = Stroke::new(1.0, p.color.gamma_multiply(0.6));
        for line in bae_render::mask::feather_offsets(&p.path, &[0.0, 1.0]) {
            let screen: Vec<Pos2> = line
                .iter()
                .map(|q| overlay::to_screen(view, &p.to_comp, [q.x, q.y]))
                .collect();
            painter.add(egui::Shape::closed_line(screen, stroke));
        }
        for f in &p.path.feather {
            if let Some(h) = handle(&p.path, &p.to_comp, f, view) {
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
            p.path
                .feather
                .iter()
                .position(|f| {
                    handle(&p.path, &p.to_comp, f, view).is_some_and(|h| h.distance(pos) <= PICK)
                })
                .map(|i| (p, i))
        });
        if let Some((p, index)) = on_point {
            if alt {
                let mut path = p.path.clone();
                path.feather.remove(index);
                super::path_edit::set_vertices(cx, session, info, &p.prim, &path);
            } else {
                state.drag = Some(Drag {
                    mask: p.prim.clone(),
                    path: p.path.clone(),
                    to_comp: p.to_comp,
                    index,
                });
            }
        } else if let Some(PathHit::Segment { path, segment, t }) = overlay::hit(&paths, view, pos)
        {
            let p = &paths[path];
            let mut shape = p.path.clone();
            shape.feather.push(FeatherPoint {
                segment: segment as u32,
                t,
                ..Default::default()
            });
            state.drag = Some(Drag {
                mask: p.prim.clone(),
                to_comp: p.to_comp,
                index: shape.feather.len() - 1,
                path: shape,
            });
        }
    }
    if let Some(d) = state.drag.as_mut() {
        // The radius follows the pointer along the path's normal at the point;
        // past the path it becomes an inner feather.
        let f = d.path.feather[d.index];
        if let Some(pos) = resp.interact_pointer_pos().or(hover)
            && let Some((at, n)) = bae_render::mask::path_frame(&d.path, f64::from(f.segment) + f.t)
        {
            let c = view.to_comp(pos);
            let q = d
                .to_comp
                .inverse()
                .transform_point3(DVec3::new(c.x, c.y, 0.0));
            let r = (q.x - at[0]) * n[0] + (q.y - at[1]) * n[1];
            let fp = &mut d.path.feather[d.index];
            fp.radius = r.abs();
            fp.inner = r < 0.0;
        }
        if let Some(h) = handle(&d.path, &d.to_comp, &d.path.feather[d.index], view) {
            painter.circle_filled(h, 3.5, bae_ui::theme::color::ACCENT);
        }
    }
    if released && let Some(d) = state.drag.take() {
        super::path_edit::set_vertices(cx, session, info, &d.mask, &d.path);
    }
}
