//! Effect point controls (AE): the Point / 3D Point parameters of the selected
//! effects of the selected layers drawn as crosshairs in the layer's label
//! color; the Selection tool drags them (one undo step per drag).

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_model::effect::{ParamKind, param_attr};
use bae_model::{PrimPath, Time, Value};
use bae_motion::query;
use bae_ui::PanelContext;
use bae_ui::theme;
use egui::{Color32, CursorIcon, Painter, Pos2, Response, Stroke, Ui, vec2};

use crate::common::{self, Edit};
use crate::render::View;
use crate::session::SharedSession;
use crate::tools::Tool;

use super::prefs::ViewerPrefs;

/// Pointer distance (points) that grabs a crosshair.
const HIT: f32 = 6.0;

/// A crosshair being dragged.
pub(crate) struct PointDrag {
    effect: PrimPath,
    id: u32,
    /// A 3D point's z (kept while dragging in x / y).
    z: Option<f64>,
    /// Comp → layer space.
    to_layer: DMat4,
}

#[derive(Default)]
pub(crate) struct EffectPoints {
    drag: Option<PointDrag>,
}

struct Handle {
    effect: PrimPath,
    id: u32,
    z: Option<f64>,
    world: DMat4,
    pos: Pos2,
    color: Color32,
}

/// What the frame did: whether it owns the pointer (the Selection tool must
/// not also move the layer) and the cursor to show.
pub(crate) struct PointsFrame {
    pub owns: bool,
    pub cursor: Option<CursorIcon>,
}

fn handles(
    cx: &PanelContext<'_>,
    session: &SharedSession,
    comp: &EvaluatedComp,
    view: &View,
    prefs: &ViewerPrefs,
) -> Vec<Handle> {
    let (layers, effects) = {
        let s = session.lock();
        (s.selected_layers.clone(), s.selected_effects.clone())
    };
    if effects.is_empty() {
        return Vec::new();
    }
    let Some(registries) = crate::effects::registries(cx.resources) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for layer in comp.layers.iter().filter(|l| layers.contains(&l.path)) {
        let label = cx
            .stage
            .prim(&layer.path)
            .and_then(|v| query::static_value(&v, "label"))
            .and_then(|v| v.as_f64())
            .map_or(8, |v| v as usize);
        let color = prefs.handle_color(theme::label_color(label));
        for e in layer.effects.iter().filter(|e| effects.contains(&e.path)) {
            let Some(def) = registries.def(&e.match_name) else {
                continue;
            };
            for param in &def.params {
                if param.flags.hidden
                    || !matches!(param.kind, ParamKind::Point | ParamKind::Point3d)
                {
                    continue;
                }
                let (x, y, z) = match e.param(param.id).or(param.default.as_ref()) {
                    Some(Value::Vec2([x, y])) => (*x, *y, None),
                    Some(Value::Vec3([x, y, z])) => (*x, *y, Some(*z)),
                    _ => continue,
                };
                let w = layer
                    .world_matrix
                    .transform_point3(DVec3::new(x, y, z.unwrap_or(0.0)));
                out.push(Handle {
                    effect: e.path.clone(),
                    id: param.id,
                    z,
                    world: layer.world_matrix,
                    pos: view.to_screen(DVec2::new(w.x, w.y)),
                    color,
                });
            }
        }
    }
    out
}

/// AE's effect point: a small circle with a cross, over a dark halo.
fn paint(painter: &Painter, pos: Pos2, color: Color32) {
    let r = 4.5;
    let arm = 5.0;
    for (width, c) in [(3.0, Color32::from_black_alpha(140)), (1.0, color)] {
        let s = Stroke::new(width, c);
        painter.circle_stroke(pos, r, s);
        painter.line_segment([pos - vec2(arm, 0.0), pos + vec2(arm, 0.0)], s);
        painter.line_segment([pos - vec2(0.0, arm), pos + vec2(0.0, arm)], s);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn frame(
    state: &mut EffectPoints,
    ui: &Ui,
    resp: &Response,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    comp: &EvaluatedComp,
    view: &View,
    tool: Tool,
    painter: &Painter,
    prefs: &ViewerPrefs,
    now: Time,
) -> PointsFrame {
    // View Options ▸ Effect Controls off: no crosshairs, nothing to drag.
    if !session.lock().layer_controls.effect_controls {
        state.drag = None;
        return PointsFrame {
            owns: false,
            cursor: None,
        };
    }
    if tool == Tool::Selection
        && let Some(hit) = crypto_pick(ui, resp, cx, session, comp, view, now)
    {
        return hit;
    }
    let handles = handles(cx, session, comp, view, prefs);
    for h in &handles {
        paint(painter, h.pos, h.color);
    }
    let mut out = PointsFrame {
        owns: state.drag.is_some(),
        cursor: None,
    };
    if tool != Tool::Selection {
        state.drag = None;
        return out;
    }
    let near = |p: Pos2| {
        handles
            .iter()
            .filter(|h| h.pos.distance(p) <= HIT)
            .min_by(|a, b| a.pos.distance(p).total_cmp(&b.pos.distance(p)))
    };
    if state.drag.is_none() {
        let pressed = ui.input(|i| i.pointer.primary_pressed());
        let origin = ui.input(|i| i.pointer.press_origin());
        if let Some(h) = origin.filter(|_| resp.hovered()).and_then(near)
            && (pressed || resp.drag_started_by(egui::PointerButton::Primary))
        {
            state.drag = Some(PointDrag {
                effect: h.effect.clone(),
                id: h.id,
                z: h.z,
                to_layer: h.world.inverse(),
            });
        }
    }
    if let Some(d) = &state.drag {
        out.owns = true;
        out.cursor = Some(CursorIcon::Move);
        let attr = param_attr(d.id);
        let released = !ui.input(|i| i.pointer.primary_down());
        let value = resp.interact_pointer_pos().or(resp.hover_pos()).map(|p| {
            let c = view.to_comp(p);
            let l = d.to_layer.transform_point3(DVec3::new(c.x, c.y, 0.0));
            match d.z {
                Some(z) => Value::Vec3([l.x, l.y, z]),
                None => Value::Vec2([l.x, l.y]),
            }
        });
        let moved = resp.dragged() || resp.drag_stopped();
        match (value, released) {
            (Some(v), true) if moved => {
                common::apply_edit(cx, session, &d.effect, &attr, now, Edit::Commit(v));
            }
            (Some(v), false) if moved => {
                common::apply_edit(cx, session, &d.effect, &attr, now, Edit::Live(v));
            }
            _ => {}
        }
        if released {
            state.drag = None;
        }
    } else if let Some(p) = resp.hover_pos()
        && near(p).is_some()
    {
        out.cursor = Some(CursorIcon::Move);
    }
    out
}

/// Pick the first-ranked ID from the displayed frame, then store that ID in
/// the effect. A moving object keeps its matte when the playhead advances.
fn crypto_pick(
    ui: &Ui,
    resp: &Response,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    comp: &EvaluatedComp,
    view: &View,
    now: Time,
) -> Option<PointsFrame> {
    use bae_effects::exr_controls::{self, CryptoSelection};
    let pos = resp.hover_pos()?;
    let (selected, frame) = {
        let s = session.lock();
        (s.selected_layers.clone(), s.channel_frame.clone()?)
    };
    if frame.comp != comp.path || frame.time != now {
        return None;
    }
    for layer in comp.layers.iter().filter(|l| selected.contains(&l.path)) {
        let Some(info) = frame.layers.get(&layer.path) else {
            continue;
        };
        let Some(effect) = layer.effects.iter().find(|e| {
            e.enabled
                && e.match_name == "Cryptomatte"
                && matches!(e.param(3), Some(Value::Bool(true) | Value::Int(1)))
        }) else {
            continue;
        };
        let mut sel: CryptoSelection = exr_controls::read(effect.param(1));
        let group = if sel.layer.is_empty() {
            info.crypto.first()
        } else {
            info.crypto.iter().find(|l| l.name == sel.layer)
        }?;
        if ui.input(|i| i.pointer.primary_pressed()) {
            let p = view.to_comp(pos);
            let inverse = layer.world_matrix.inverse();
            let q = if let Some((camera, size)) = view.camera.filter(|_| layer.switches.three_d) {
                let (origin, dir) = super::gizmo3d::ray(&camera, size, p);
                let origin = inverse.transform_point3(origin);
                let dir = inverse.transform_vector3(dir);
                let distance = -origin.z / dir.z;
                if !distance.is_finite() || distance < 0. {
                    continue;
                }
                origin + distance * dir
            } else {
                inverse.transform_point3(p.extend(0.))
            };
            if q.is_finite()
                && let Some(id) = info.pick(group, [q.x.floor() as i32, q.y.floor() as i32])
            {
                sel.layer = group.name.clone();
                sel.object = Some(id);
                common::apply_edit(
                    cx,
                    session,
                    &effect.path,
                    &param_attr(1),
                    now,
                    Edit::Commit(exr_controls::value(&sel)),
                );
                ui.ctx().request_repaint();
            }
        }
        return Some(PointsFrame {
            owns: true,
            cursor: Some(CursorIcon::Crosshair),
        });
    }
    None
}
