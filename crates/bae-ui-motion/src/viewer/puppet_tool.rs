//! The Puppet tools in the Composition viewer (AE's Puppet Position, Starch
//! and Overlap Pin tools, Cmd/Ctrl+P): a click on a layer adds a pin there
//! (`puppet.add_pin`; the first pin adds the Puppet effect), a drag on a pin
//! moves it, previewed live, one undo step. The selected layers' pins are
//! drawn in AE's colours: yellow Position pins, red Starch, blue Overlap.

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_model::{PrimPath, PrimType, Time, Value};
use bae_motion::query;
use bae_ui::PanelContext;
use egui::{Color32, CursorIcon, Painter, Pos2, Response, Stroke, Ui};

use crate::common::{self, Edit};
use crate::render::View;
use crate::session::SharedSession;
use crate::tools::Tool;

/// Pointer distance (points) that grabs a pin.
const HIT: f32 = 6.0;

/// The pin type a Puppet tool adds.
pub(crate) fn pin_type(tool: Tool) -> Option<&'static str> {
    Some(match tool {
        Tool::PuppetPosition => "position",
        Tool::PuppetStarch => "starch",
        Tool::PuppetBend => "bend",
        Tool::PuppetAdvanced => "advanced",
        Tool::PuppetOverlap => "overlap",
        _ => return None,
    })
}

/// A pin being dragged.
struct Drag {
    pin: PrimPath,
    /// Comp → layer space.
    to_layer: DMat4,
}

#[derive(Default)]
pub(crate) struct PuppetState {
    drag: Option<Drag>,
}

struct Handle {
    pin: PrimPath,
    world: DMat4,
    pos: Pos2,
    color: Color32,
}

fn color(kind: &str) -> Color32 {
    match kind {
        "starch" => Color32::from_rgb(235, 70, 60),
        "overlap" => Color32::from_rgb(70, 140, 255),
        _ => Color32::from_rgb(250, 215, 40),
    }
}

/// The pins of the selected layers' Puppet effects.
fn handles(session: &SharedSession, comp: &EvaluatedComp, view: &View) -> Vec<Handle> {
    let selected = session.lock().selected_layers.clone();
    let mut out = Vec::new();
    for layer in comp.layers.iter().filter(|l| selected.contains(&l.path)) {
        for e in layer
            .effects
            .iter()
            .filter(|e| e.match_name == bae_render::puppet::PUPPET_EFFECT)
        {
            for pin in e
                .children
                .iter()
                .filter(|c| c.prim_type.as_ref() == Some(&PrimType::PUPPET_PIN))
            {
                let at = match pin.get("position") {
                    Some(Value::Vec2([x, y])) => DVec3::new(*x, *y, 0.0),
                    _ => continue,
                };
                let kind = match pin.get("pin_type") {
                    Some(Value::Enum(s) | Value::String(s)) => s.as_str(),
                    _ => "position",
                };
                let w = layer.world_matrix.transform_point3(at);
                out.push(Handle {
                    pin: pin.path.clone(),
                    world: layer.world_matrix,
                    pos: view.to_screen(DVec2::new(w.x, w.y)),
                    color: color(kind),
                });
            }
        }
    }
    out
}

/// What the frame did: whether it owns the pointer and the cursor to show.
pub(crate) struct PuppetFrame {
    pub owns: bool,
    pub cursor: Option<CursorIcon>,
}

/// Mesh ▸ Show: the selected layers' Puppet meshes, deformed by their pins.
fn paint_meshes(session: &SharedSession, comp: &EvaluatedComp, view: &View, painter: &Painter) {
    use bae_effects::effects::puppet::{DENSITY, EXPANSION, Mesh, REFINEMENT, deform};
    let selected = session.lock().selected_layers.clone();
    let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(250, 215, 40, 150));
    for layer in comp.layers.iter().filter(|l| selected.contains(&l.path)) {
        let Some((min, max)) = crate::render::layer_bounds(layer) else {
            continue;
        };
        for e in layer
            .effects
            .iter()
            .filter(|e| e.match_name == bae_render::puppet::PUPPET_EFFECT)
        {
            let num = |id: u32, d: f64| e.param(id).and_then(Value::as_f64).unwrap_or(d);
            let grow = num(EXPANSION, 3.0).max(0.0);
            let mesh = Mesh::grid(min.floor() - grow, max.ceil() + grow, num(DENSITY, 10.0));
            let pins = bae_render::puppet::pins(&e.children);
            let rounds = num(REFINEMENT, 20.0).round().clamp(1.0, 20.0) as usize;
            let at: Vec<Pos2> = deform(&mesh, &pins, rounds)
                .into_iter()
                .map(|p| {
                    let w = layer.world_matrix.transform_point3(p.extend(0.0));
                    view.to_screen(DVec2::new(w.x, w.y))
                })
                .collect();
            for t in &mesh.tris {
                let [a, b, c] = t.map(|i| at[i as usize]);
                painter.line_segment([a, b], stroke);
                painter.line_segment([b, c], stroke);
                painter.line_segment([c, a], stroke);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn frame(
    state: &mut PuppetState,
    ui: &Ui,
    resp: &Response,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    comp: &EvaluatedComp,
    view: &View,
    tool: Tool,
    painter: &Painter,
    now: Time,
) -> PuppetFrame {
    let show_mesh = crate::tools::shared(cx.resources)
        .lock()
        .options
        .puppet_show_mesh;
    if show_mesh && pin_type(tool).is_some() {
        paint_meshes(session, comp, view, painter);
    }
    let handles = handles(session, comp, view);
    let dragged = state.drag.as_ref().map(|d| &d.pin);
    for h in &handles {
        let fill = if dragged == Some(&h.pin) {
            h.color
        } else {
            Color32::TRANSPARENT
        };
        painter.circle(h.pos, 4.0, Color32::from_black_alpha(90), Stroke::NONE);
        painter.circle(h.pos, 3.5, fill, Stroke::new(1.5, h.color));
    }
    let mut out = PuppetFrame {
        owns: state.drag.is_some(),
        cursor: None,
    };
    let Some(kind) = pin_type(tool) else {
        state.drag = None;
        return out;
    };
    out.owns = true;
    let near = |p: Pos2| {
        handles
            .iter()
            .filter(|h| h.pos.distance(p) <= HIT)
            .min_by(|a, b| a.pos.distance(p).total_cmp(&b.pos.distance(p)))
    };
    let pressed = ui.input(|i| i.pointer.primary_pressed()) && resp.hovered();
    let origin = ui.input(|i| i.pointer.press_origin());
    if state.drag.is_none()
        && pressed
        && let Some(p) = origin
    {
        if let Some(h) = near(p) {
            state.drag = Some(Drag {
                pin: h.pin.clone(),
                to_layer: h.world.inverse(),
            });
        } else if let Some(layer) = super::select_tool::hit_layer(cx.stage, comp, view, p)
            && let Some(l) = comp.layer(&layer)
        {
            // A new pin where the click is on the layer.
            let c = view.to_comp(p);
            let at = l
                .world_matrix
                .inverse()
                .transform_point3(DVec3::new(c.x, c.y, 0.0));
            session.lock().selected_layers = vec![layer.clone()];
            let (expansion, density) = {
                let tools = crate::tools::shared(cx.resources);
                let o = &tools.lock().options;
                (o.puppet_expansion, o.puppet_density)
            };
            cx.invoke_with(
                "puppet.add_pin",
                serde_json::json!({
                    "layer": layer,
                    "pin_type": kind,
                    "at": [at.x, at.y],
                    "time": query::to_key_time(cx.stage, &layer, now),
                    "expansion": expansion,
                    "density": density,
                }),
            );
        }
    }
    if let Some(d) = &state.drag {
        out.cursor = Some(CursorIcon::Move);
        let released = !ui.input(|i| i.pointer.primary_down());
        let value = resp.interact_pointer_pos().or(resp.hover_pos()).map(|p| {
            let c = view.to_comp(p);
            let l = d.to_layer.transform_point3(DVec3::new(c.x, c.y, 0.0));
            Value::Vec2([l.x, l.y])
        });
        let moved = resp.dragged() || resp.drag_stopped();
        match (value, released) {
            (Some(v), true) if moved => {
                common::apply_edit(cx, session, &d.pin, "position", now, Edit::Commit(v));
            }
            (Some(v), false) if moved => {
                common::apply_edit(cx, session, &d.pin, "position", now, Edit::Live(v));
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
