//! The Puppet tools in the Composition viewer (AE's Puppet Position, Starch
//! and Overlap Pin tools, Cmd/Ctrl+P): a click on a layer adds a pin there
//! (`puppet.add_pin`; the first pin adds the Puppet effect); a click on a pin
//! selects it (Shift: adds or removes it), a drag moves the selected pins
//! together, previewed live, one undo step; Delete removes them. A selected
//! Bend or Advanced pin shows AE's ring: drag the ring to rotate, the square
//! on it to scale (live, one undo step). The selected
//! layers' pins are drawn in AE's colours: yellow Position pins, red Starch,
//! blue Overlap; selected pins are filled.

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_model::{PrimPath, PrimType, Time, Value};
use bae_motion::query;
use bae_ui::PanelContext;
use egui::{Color32, CursorIcon, Painter, Pos2, Response, Stroke, Ui};

use crate::render::View;
use crate::session::{Scrub, SharedSession};
use crate::tools::Tool;

/// Pointer distance (points) that grabs a pin.
const HIT: f32 = 6.0;
/// Radius (points) of a Bend / Advanced pin's rotation ring.
const RING: f32 = 24.0;

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

/// The selected pins being dragged.
struct Drag {
    /// Where the press was (comp space).
    start: DVec2,
    /// Each pin, where it was (layer pixels), comp → its layer's space; the
    /// grabbed pin first.
    pins: Vec<(PrimPath, DVec2, DMat4)>,
}

/// A Bend / Advanced pin's ring or square being dragged.
struct Turn {
    pin: PrimPath,
    centre: Pos2,
    /// Rotating (else scaling).
    rotate: bool,
    /// The pin's value and the pointer's angle / distance at the press.
    start: f64,
    from: f32,
}

#[derive(Default)]
pub(crate) struct PuppetState {
    drag: Option<Drag>,
    turn: Option<Turn>,
    /// Selected pins.
    selected: Vec<PrimPath>,
}

impl PuppetState {
    /// Delete: removes the selected pins (one undo step); `false` with none.
    pub(crate) fn delete_selected(&mut self, cx: &mut PanelContext<'_>) -> bool {
        let pins: Vec<PrimPath> = std::mem::take(&mut self.selected)
            .into_iter()
            .filter(|p| cx.stage.has_prim(p))
            .collect();
        if pins.is_empty() {
            return false;
        }
        cx.invoke_with("puppet.delete_pins", serde_json::json!({ "pins": pins }));
        true
    }

    pub(crate) fn has_selection(&self) -> bool {
        !self.selected.is_empty()
    }
}

struct Handle {
    pin: PrimPath,
    /// Bend or Advanced: rotation (degrees) and scale (percent).
    turn: Option<(f64, f64)>,
    world: DMat4,
    /// Layer pixels.
    at: DVec2,
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

/// The pins of the `selected` layers' Puppet effects.
fn handles(selected: &[PrimPath], comp: &EvaluatedComp, view: &View) -> Vec<Handle> {
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
                let turn = matches!(kind, "bend" | "advanced").then(|| {
                    (
                        pin.f64("rotation").unwrap_or(0.0),
                        pin.f64("scale").unwrap_or(100.0),
                    )
                });
                out.push(Handle {
                    pin: pin.path.clone(),
                    turn,
                    world: layer.world_matrix,
                    at: at.truncate(),
                    pos: view.to_screen(DVec2::new(w.x, w.y)),
                    color: color(kind),
                });
            }
        }
    }
    out
}

/// Shows (`commit` false) or commits the pins' new `attr` values: one value drag
/// with the others as its peers, one `attr.set_many` at the end (one undo
/// step), a keyframe at `now` where Position is animated.
fn edit_pins(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    attr: &str,
    values: &[(PrimPath, Value)],
    now: Time,
    commit: bool,
) {
    let Some(((pin, value), peers)) = values.split_first() else {
        return;
    };
    let auto_key = session.lock().auto_key;
    session.lock().scrub = Some(Scrub {
        auto_key,
        path: pin.clone(),
        attr: attr.to_owned(),
        value: value.clone(),
        peers: peers.to_vec(),
        time: now,
        committed: commit.then(|| (cx.stage.revision(), std::time::Instant::now())),
    });
    if commit {
        let edits: Vec<serde_json::Value> = values
            .iter()
            .map(|(p, v)| {
                serde_json::json!({
                    "path": p,
                    "attr": attr,
                    "time": query::to_key_time(cx.stage, p, now),
                    "value": v,
                })
            })
            .collect();
        cx.invoke_with("attr.set_many", serde_json::json!({ "edits": edits }));
    }
}

/// Stores the outlines the renderer traced for Puppet effects that have none
/// yet (AE's Create Puppet Mesh, one undo step): from then on every render
/// builds the same mesh.
fn store_traces(ui: &Ui, cx: &mut PanelContext<'_>, comp: &EvaluatedComp) {
    use bae_effects::effects::puppet::{OUTLINES, take_trace, tracing};
    for e in comp
        .layers
        .iter()
        .flat_map(|l| &l.effects)
        .filter(|e| e.match_name == bae_render::puppet::PUPPET_EFFECT)
    {
        let stored = matches!(e.param(OUTLINES), Some(Value::FloatList(f)) if !f.is_empty());
        if !stored && let Some(outlines) = take_trace(&e.path) {
            cx.invoke_with(
                "puppet.set_mesh",
                serde_json::json!({ "effect": e.path, "outlines": outlines }),
            );
        }
    }
    if tracing() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
    }
}

/// What the frame did: whether it owns the pointer and the cursor to show.
pub(crate) struct PuppetFrame {
    pub owns: bool,
    pub cursor: Option<CursorIcon>,
}

/// Mesh ▸ Show: the `selected` layers' Puppet meshes, deformed by their pins.
fn paint_meshes(selected: &[PrimPath], comp: &EvaluatedComp, view: &View, painter: &Painter) {
    use bae_effects::effects::puppet::{DENSITY, EXPANSION, OUTLINES, REFINEMENT, deform, mesh_of};
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
            let bounds = bae_render::IRect::new(
                min.x.floor() as i32,
                min.y.floor() as i32,
                max.x.ceil() as i32,
                max.y.ceil() as i32,
            );
            let mesh = mesh_of(
                e.param(OUTLINES),
                bounds,
                num(EXPANSION, 3.0),
                num(DENSITY, 10.0),
            );
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

/// One frame of the Puppet tools over `comp` seen through `view`, showing
/// the pins of `selected` (the Composition viewer: the selected layers; the
/// Layer panel: its layer).
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
    selected: &[PrimPath],
) -> PuppetFrame {
    store_traces(ui, cx, comp);
    let show_mesh = crate::tools::shared(cx.resources)
        .lock()
        .options
        .puppet_show_mesh;
    if show_mesh && pin_type(tool).is_some() {
        paint_meshes(selected, comp, view, painter);
    }
    let handles = handles(selected, comp, view);
    state
        .selected
        .retain(|p| handles.iter().any(|h| h.pin == *p));
    for h in &handles {
        let fill = if state.selected.contains(&h.pin) {
            h.color
        } else {
            Color32::TRANSPARENT
        };
        painter.circle(h.pos, 4.0, Color32::from_black_alpha(90), Stroke::NONE);
        painter.circle(h.pos, 3.5, fill, Stroke::new(1.5, h.color));
    }
    // The ring of the one selected Bend / Advanced pin, its square at the
    // pin's rotation.
    let ringed = match &state.selected[..] {
        [one] => handles.iter().find(|h| h.pin == *one && h.turn.is_some()),
        _ => None,
    };
    let square = ringed.map(|h| {
        let (rotation, _) = h.turn.unwrap_or_default();
        let a = rotation.to_radians() as f32;
        h.pos + egui::vec2(a.cos(), a.sin()) * RING
    });
    if let (Some(h), Some(sq)) = (ringed, square) {
        painter.circle_stroke(h.pos, RING, Stroke::new(1.0, h.color));
        painter.rect_filled(
            egui::Rect::from_center_size(sq, egui::vec2(6.0, 6.0)),
            0.0,
            h.color,
        );
    }
    let mut out = PuppetFrame {
        owns: state.drag.is_some() || state.turn.is_some(),
        cursor: None,
    };
    let Some(kind) = pin_type(tool) else {
        state.drag = None;
        state.turn = None;
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
    // The ring and its square come before the pins.
    if state.drag.is_none()
        && state.turn.is_none()
        && pressed
        && let (Some(p), Some(h), Some(sq)) = (origin, ringed, square)
    {
        let (rotation, scale) = h.turn.unwrap_or((0.0, 100.0));
        let d = p - h.pos;
        if p.distance(sq) <= HIT {
            state.turn = Some(Turn {
                pin: h.pin.clone(),
                centre: h.pos,
                rotate: false,
                start: scale,
                from: d.length().max(1.0),
            });
        } else if (d.length() - RING).abs() <= 4.0 {
            state.turn = Some(Turn {
                pin: h.pin.clone(),
                centre: h.pos,
                rotate: true,
                start: rotation,
                from: d.y.atan2(d.x),
            });
        }
    }
    if state.drag.is_none()
        && state.turn.is_none()
        && pressed
        && let Some(p) = origin
    {
        let shift = ui.input(|i| i.modifiers.shift);
        if let Some(h) = near(p) {
            let on = state.selected.contains(&h.pin);
            if shift && on {
                state.selected.retain(|s| *s != h.pin);
            } else if shift {
                state.selected.push(h.pin.clone());
            } else if !on {
                state.selected = vec![h.pin.clone()];
            }
            if state.selected.contains(&h.pin) {
                // The grabbed pin first, then the other selected ones.
                let mut order = vec![h];
                order.extend(
                    handles
                        .iter()
                        .filter(|o| o.pin != h.pin && state.selected.contains(&o.pin)),
                );
                state.drag = Some(Drag {
                    start: view.to_comp(p),
                    pins: order
                        .into_iter()
                        .map(|o| (o.pin.clone(), o.at, o.world.inverse()))
                        .collect(),
                });
            }
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
            state.selected.clear();
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
        } else if !shift {
            state.selected.clear();
        }
    }
    if let Some(t) = &state.turn {
        out.cursor = Some(CursorIcon::Grabbing);
        let released = !ui.input(|i| i.pointer.primary_down());
        let moved = resp.dragged() || resp.drag_stopped();
        if moved && let Some(p) = resp.interact_pointer_pos().or(resp.hover_pos()) {
            let d = p - t.centre;
            let (attr, value) = if t.rotate {
                // Clockwise on screen is positive, as AE's Rotation.
                let delta = f64::from(d.y.atan2(d.x) - t.from).to_degrees();
                ("rotation", t.start + delta)
            } else {
                ("scale", (t.start * f64::from(d.length() / t.from)).max(1.0))
            };
            let values = [(t.pin.clone(), Value::Float(value))];
            edit_pins(cx, session, attr, &values, now, released);
        }
        if released {
            state.turn = None;
        }
    }
    if let Some(d) = &state.drag {
        out.cursor = Some(CursorIcon::Move);
        let released = !ui.input(|i| i.pointer.primary_down());
        let moved = resp.dragged() || resp.drag_stopped();
        if moved && let Some(p) = resp.interact_pointer_pos().or(resp.hover_pos()) {
            // Each pin moves by the pointer's move in its own layer's space.
            let c = view.to_comp(p);
            let layer_of = |m: &DMat4, q: DVec2| m.transform_point3(q.extend(0.0)).truncate();
            let values: Vec<(PrimPath, Value)> = d
                .pins
                .iter()
                .map(|(pin, at, m)| {
                    let v = *at + layer_of(m, c) - layer_of(m, d.start);
                    (pin.clone(), Value::Vec2(v.to_array()))
                })
                .collect();
            edit_pins(cx, session, "position", &values, now, released);
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
