//! Timeline and Graph Editor interaction tests: the panel runs headless with
//! simulated pointer and keyboard input, its requests run through the command
//! registry the way the shell runs them, and every edit is checked in the project
//! and undone.

use bae_app::Resources;
use bae_model::{AuthorId, CommandRegistry, Interpolation, PrimPath, Stage, Time, Value};
use bae_motion::{KeyRef, query};
use bae_ui::{Invocation, Panel, PanelContext, PanelId, PanelRequests};
use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use serde_json::{Value as Json, json};

use crate::TimelinePanel;
use crate::session::SharedSession;

const SEC: i64 = Time::FLICKS_PER_SECOND;
const ROT: &str = "transform:rotation";
const POS: &str = "transform:position";

struct H {
    stage: Stage,
    reg: CommandRegistry,
    author: AuthorId,
    resources: Resources,
    ctx: egui::Context,
    time: f64,
    comp: PrimPath,
    layer: PrimPath,
    /// Tessellate each frame too (the perf probe measures the full CPU cost).
    tessellate: bool,
}

impl H {
    /// A 640×360, 30 fps, 10 s comp with one solid, opened in the timeline.
    fn new() -> Self {
        let mut reg = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        let mut resources = Resources::new();
        resources.insert(SharedSession::default());
        let mut h = H {
            stage: Stage::new(),
            reg,
            author: AuthorId::new("user:test").unwrap(),
            resources,
            ctx: egui::Context::default(),
            time: 0.0,
            comp: PrimPath::new("/x").unwrap(),
            layer: PrimPath::new("/x").unwrap(),
            tessellate: false,
        };
        h.comp = path(
            &h.run(
                "comp.create",
                json!({"width": 640, "height": 360, "fps": 30}),
            )["path"],
        );
        h.layer = path(&h.run("layer.create", json!({"comp": h.comp, "kind": "solid"}))["path"]);
        h.session().lock().open_comp(&h.comp);
        h
    }

    fn session(&self) -> SharedSession {
        self.resources.get::<SharedSession>().unwrap().clone()
    }

    fn run(&mut self, id: &str, params: Json) -> Json {
        self.reg
            .execute(&mut self.stage, &self.author, id, params)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .output
    }

    /// Keyframes on `attr` of the solid: (seconds, value).
    fn keys(&mut self, attr: &str, keys: &[(f64, Json)]) {
        let layer = self.layer.clone();
        self.run(
            "attr.set_animated",
            json!({"path": layer, "attr": attr, "animated": true, "time": Time::from_seconds(keys[0].0)}),
        );
        for (t, v) in keys {
            self.run(
                "attr.set_at",
                json!({"path": layer, "attr": attr, "time": Time::from_seconds(*t), "value": v}),
            );
        }
    }

    fn spec(&self, attr: &str) -> bae_model::AttrSpec {
        query::attr_spec(&self.stage, &self.layer, attr)
            .cloned()
            .unwrap_or_default()
    }

    fn key(&self, attr: &str, secs: f64) -> KeyRef {
        KeyRef::new(self.layer.clone(), attr, Time::from_seconds(secs))
    }

    /// One frame of the timeline with `events` and modifiers held.
    fn frame(
        &mut self,
        tl: &mut TimelinePanel,
        events: Vec<Event>,
        mods: Modifiers,
    ) -> PanelRequests {
        let mut requests = PanelRequests::default();
        let id = PanelId::new("timeline");
        // Modifiers first, as egui-winit sends them.
        let mut all = vec![Event::ModifiersChanged(mods)];
        all.extend(events);
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
            events: all,
            time: Some(self.time),
            predicted_dt: 1.0 / 60.0,
            ..Default::default()
        };
        self.time += 1.0 / 60.0;
        let stage = &self.stage;
        let author = &self.author;
        let resources = &self.resources;
        let mut out = self.ctx.run_ui(raw, |ui| {
            let mut cx =
                PanelContext::new(&id, "timeline", stage, author, resources, &mut requests);
            cx.focused = true;
            tl.ui(ui, &mut cx);
        });
        out.textures_delta.clear();
        if self.tessellate {
            let shapes = std::mem::take(&mut out.shapes);
            std::hint::black_box(self.ctx.tessellate(shapes, out.pixels_per_point));
        }
        requests
    }

    fn apply(&mut self, r: &PanelRequests) {
        for inv in &r.invocations {
            let params = if inv.params.is_null() {
                json!({})
            } else {
                inv.params.clone()
            };
            self.reg
                .execute(&mut self.stage, &self.author, &inv.id, params)
                .unwrap_or_else(|e| panic!("{}: {e}", inv.id));
        }
    }

    fn action(&self, tl: &mut TimelinePanel, inv: Invocation) -> PanelRequests {
        let mut requests = PanelRequests::default();
        let id = PanelId::new("timeline");
        let mut cx = PanelContext::new(
            &id,
            "timeline",
            &self.stage,
            &self.author,
            &self.resources,
            &mut requests,
        );
        cx.focused = true;
        assert!(tl.on_action(&inv, &mut cx), "{} not handled", inv.id);
        requests
    }

    fn undo(&mut self) {
        self.stage.undo(&self.author).expect("undo");
    }

    /// Press at `a`, drag to `b` in steps and release; asserts that nothing is
    /// committed before the release and returns the release frame's requests.
    fn drag(&mut self, tl: &mut TimelinePanel, a: Pos2, b: Pos2, mods: Modifiers) -> PanelRequests {
        self.frame(tl, vec![Event::PointerMoved(a)], mods);
        self.frame(tl, vec![button(a, true, mods)], mods);
        for i in 1..=6 {
            let p = a + (b - a) * (i as f32 / 6.0);
            let r = self.frame(tl, vec![Event::PointerMoved(p)], mods);
            assert!(
                r.invocations.is_empty(),
                "no command while dragging: {:?}",
                r.invocations
            );
        }
        self.frame(tl, vec![button(b, false, mods)], mods)
    }

    fn click(&mut self, tl: &mut TimelinePanel, p: Pos2, mods: Modifiers) -> PanelRequests {
        self.frame(tl, vec![Event::PointerMoved(p)], mods);
        self.frame(tl, vec![button(p, true, mods)], mods);
        self.frame(tl, vec![button(p, false, mods)], mods)
    }

    /// The Graph Editor with the solid selected (its animated properties shown).
    fn graph(&mut self) -> TimelinePanel {
        let mut tl = TimelinePanel::default();
        {
            let s = self.session();
            let mut s = s.lock();
            s.selected_layers = vec![self.layer.clone()];
            s.graph_editor = true;
        }
        self.frame(&mut tl, Vec::new(), Modifiers::NONE);
        self.frame(&mut tl, Vec::new(), Modifiers::NONE);
        tl
    }
}

fn path(v: &Json) -> PrimPath {
    PrimPath::new(v.as_str().unwrap()).unwrap()
}

fn button(pos: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    }
}

fn float(v: f64) -> Json {
    json!({"type": "float", "value": v})
}

fn vec3(x: f64, y: f64) -> Json {
    json!({"type": "vec3", "value": [x, y, 0.0]})
}

fn key_pos(tl: &mut TimelinePanel, k: &KeyRef, dim: usize) -> Pos2 {
    tl.graph_editor()
        .last
        .keys
        .iter()
        .find(|(x, d, _)| x == k && *d == dim)
        .map(|(_, _, p)| *p)
        .unwrap_or_else(|| panic!("key {k:?} not drawn"))
}

fn handle_pos(tl: &mut TimelinePanel, k: &KeyRef, out: bool) -> Pos2 {
    tl.graph_editor()
        .last
        .handles
        .iter()
        .find(|(x, d, o, _)| x == k && *d == 0 && *o == out)
        .map(|(_, _, _, p)| *p)
        .unwrap_or_else(|| panic!("handle of {k:?} not drawn"))
}

fn vaxis(tl: &mut TimelinePanel, attr: &str) -> crate::graph::VAxis {
    tl.graph_editor()
        .last
        .vaxes
        .iter()
        .find(|((_, a), _)| a == attr)
        .map(|(_, v)| *v)
        .expect("graph shown")
}

fn button_rect(tl: &mut TimelinePanel, label: &str) -> Rect {
    tl.graph_editor()
        .last
        .buttons
        .iter()
        .find(|(l, _)| l == label)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("button {label}"))
}

fn x_of(tl: &TimelinePanel, secs: f64) -> f32 {
    tl.last_axis.unwrap().x(Time::from_seconds(secs))
}

fn cmd() -> Modifiers {
    Modifiers {
        command: true,
        mac_cmd: cfg!(target_os = "macos"),
        ctrl: !cfg!(target_os = "macos"),
        ..Modifiers::NONE
    }
}

fn alt() -> Modifiers {
    Modifiers {
        alt: true,
        ..Modifiers::NONE
    }
}

// ---------------------------------------------------------------------------

#[test]
fn shift_f3_and_the_switch_toggle_the_graph_editor() {
    let mut h = H::new();
    let mut tl = TimelinePanel::default();
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    h.action(&mut tl, Invocation::new(crate::actions::ids::GRAPH_EDITOR));
    assert!(h.session().lock().graph_editor);
    h.action(&mut tl, Invocation::new(crate::actions::ids::GRAPH_EDITOR));
    assert!(!h.session().lock().graph_editor);
    // Shift+F3 is the AE shortcut.
    let b = crate::actions::bindings();
    assert!(b.bindings.iter().any(
        |x| x.action == crate::actions::ids::GRAPH_EDITOR && x.chord.to_string().contains("F3")
    ));
    // The comp switch in the timeline header (right end of the outline).
    let sw = pos2(534.0 - 14.0, 22.0 + 17.0);
    h.click(&mut tl, sw, Modifiers::NONE);
    assert!(h.session().lock().graph_editor);
    // The button bar is shown from the next frame.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(!tl.graph_editor().last.buttons.is_empty());
}

#[test]
fn value_graph_key_drag_changes_time_and_value_in_one_undo_step() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let mut tl = h.graph();
    let k = h.key(ROT, 1.0);
    let from = key_pos(&mut tl, &k, 0);
    let va = vaxis(&mut tl, ROT);
    let to = pos2(x_of(&tl, 1.5), va.y(120.0));
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "{:?}", r.invocations);
    assert_eq!(r.invocations[0].id, "key.edit");
    h.apply(&r);
    let spec = h.spec(ROT);
    let times: Vec<i64> = spec.keyframes.keys().map(|t| t.0).collect();
    assert_eq!(times, vec![0, SEC * 3 / 2], "dragged to 1.5 s, on a frame");
    let v = spec.keyframes[&Time(SEC * 3 / 2)].value.as_f64().unwrap();
    assert!((v - 120.0).abs() < va.per_point() * 2.0, "{v}");
    // The selection follows the key.
    assert_eq!(h.session().lock().selected_keys, vec![h.key(ROT, 1.5)]);
    h.undo();
    let spec = h.spec(ROT);
    assert_eq!(spec.keyframes[&Time(SEC)].value, Value::Float(90.0));
    assert_eq!(spec.keyframes.len(), 2);
}

#[test]
fn shift_constrains_a_key_drag_to_one_axis() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let mut tl = h.graph();
    let k = h.key(ROT, 1.0);
    let from = key_pos(&mut tl, &k, 0);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    // Mostly vertical: time stays.
    let r = h.drag(&mut tl, from, from + vec2(4.0, -60.0), shift);
    h.apply(&r);
    let spec = h.spec(ROT);
    let k1 = &spec.keyframes[&Time(SEC)];
    assert!(k1.value.as_f64().unwrap() > 90.0);
}

#[test]
fn value_graph_handle_drag_sets_speed_and_influence() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let (k0, k1) = (h.key(ROT, 0.0), h.key(ROT, 1.0));
    h.run("key.ease", json!({"keys": [k0, k1]}));
    h.session().lock().selected_keys = vec![k0.clone()];
    let mut tl = h.graph();
    let from = handle_pos(&mut tl, &k0, true);
    let va = vaxis(&mut tl, ROT);
    // Out handle of the first key to (0.5 s, 45°): influence 50 %, speed 90 °/s.
    let to = pos2(x_of(&tl, 0.5), va.y(45.0));
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    assert_eq!(r.invocations[0].id, "key.edit");
    h.apply(&r);
    let k = h.spec(ROT).keyframes[&Time::ZERO].clone();
    let e = k.ease_out[0];
    assert!((e.influence - 50.0).abs() < 0.5, "{e:?}");
    assert!((e.speed - 90.0).abs() < 1.5, "{e:?}");
    assert_eq!(k.interp_out, Interpolation::Bezier);
    h.undo();
    let e = h.spec(ROT).keyframes[&Time::ZERO].ease_out[0];
    assert_eq!(e.speed, 0.0);
}

#[test]
fn alt_drag_splits_a_continuous_handle() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(0.0))],
    );
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0), h.key(ROT, 2.0)];
    h.run("key.ease", json!({ "keys": keys }));
    let mid = h.key(ROT, 1.0);
    h.session().lock().selected_keys = vec![mid.clone()];
    let mut tl = h.graph();
    let va = vaxis(&mut tl, ROT);
    // Plain drag: a continuous key keeps both sides at the same speed.
    let from = handle_pos(&mut tl, &mid, true);
    let r = h.drag(
        &mut tl,
        from,
        pos2(from.x, va.y(90.0 + 30.0)),
        Modifiers::NONE,
    );
    h.apply(&r);
    let k = h.spec(ROT).keyframes[&Time(SEC)].clone();
    assert!(k.ease_out[0].speed > 1.0);
    assert!((k.ease_in[0].speed - k.ease_out[0].speed).abs() < 1e-9);
    assert!(k.flags.temporal_continuous);
    h.undo();
    // Alt drag: only the dragged side changes and the key is no longer continuous.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let from = handle_pos(&mut tl, &mid, true);
    let r = h.drag(&mut tl, from, pos2(from.x, va.y(90.0 + 30.0)), alt());
    h.apply(&r);
    let k = h.spec(ROT).keyframes[&Time(SEC)].clone();
    assert!(k.ease_out[0].speed > 1.0);
    assert_eq!(k.ease_in[0].speed, 0.0);
    assert!(!k.flags.temporal_continuous);
}

#[test]
fn shift_keeps_a_value_graph_handle_level() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let (k0, k1) = (h.key(ROT, 0.0), h.key(ROT, 1.0));
    h.run("key.ease", json!({"keys": [k0, k1]}));
    h.session().lock().selected_keys = vec![k0.clone()];
    let mut tl = h.graph();
    let va = vaxis(&mut tl, ROT);
    let to = pos2(x_of(&tl, 0.5), va.y(60.0));
    // Plain: the handle follows the pointer (60° over 0.5 s = 120 °/s).
    let from = handle_pos(&mut tl, &k0, true);
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    let e = h.spec(ROT).keyframes[&Time::ZERO].ease_out[0];
    assert!((e.speed - 120.0).abs() < 2.0, "{e:?}");
    h.undo();
    // Shift: level (speed 0), only the influence follows.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let from = handle_pos(&mut tl, &k0, true);
    let r = h.drag(&mut tl, from, to, Modifiers::SHIFT);
    h.apply(&r);
    let e = h.spec(ROT).keyframes[&Time::ZERO].ease_out[0];
    assert_eq!(e.speed, 0.0, "{e:?}");
    assert!((e.influence - 50.0).abs() < 0.5, "{e:?}");
    h.undo();
    assert!((h.spec(ROT).keyframes[&Time::ZERO].ease_out[0].influence - 33.33).abs() < 0.1);
}

#[test]
fn speed_graph_handle_snaps_to_zero_speed() {
    let mut h = H::new();
    h.keys(POS, &[(0.0, vec3(0.0, 0.0)), (1.0, vec3(600.0, 0.0))]);
    let (k0, k1) = (h.key(POS, 0.0), h.key(POS, 1.0));
    h.run("key.ease", json!({"keys": [k0, k1]}));
    h.session().lock().selected_keys = vec![k0.clone()];
    let mut tl = h.graph();
    // A non-zero speed first.
    let va = vaxis(&mut tl, POS);
    let from = handle_pos(&mut tl, &k0, true);
    let to = pos2(x_of(&tl, 0.5), va.y(400.0));
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    // Snap on: 5 pt above the 0 line lands on 0.
    tl.graph_editor().settings.snap = true;
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let va = vaxis(&mut tl, POS);
    let from = handle_pos(&mut tl, &k0, true);
    let to = pos2(x_of(&tl, 0.5), va.y(0.0) - 5.0);
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    assert_eq!(h.spec(POS).keyframes[&Time::ZERO].ease_out[0].speed, 0.0);
    h.undo();
    // Cmd/Ctrl turns snapping off for the drag.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let from = handle_pos(&mut tl, &k0, true);
    let r = h.drag(&mut tl, from, to, cmd());
    h.apply(&r);
    assert!(h.spec(POS).keyframes[&Time::ZERO].ease_out[0].speed > 0.0);
}

#[test]
fn dragging_one_handle_moves_the_handles_of_all_selected_keys() {
    let mut h = H::new();
    const OPACITY: &str = "transform:opacity";
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    h.keys(OPACITY, &[(0.0, float(100.0)), (2.0, float(0.0))]);
    let rot = [h.key(ROT, 0.0), h.key(ROT, 1.0)];
    let op = [h.key(OPACITY, 0.0), h.key(OPACITY, 2.0)];
    h.run("key.ease", json!({"keys": [rot[0], rot[1], op[0], op[1]]}));
    h.session().lock().selected_keys = vec![rot[0].clone(), op[0].clone()];
    let mut tl = h.graph();
    let va = vaxis(&mut tl, ROT);
    let from = handle_pos(&mut tl, &rot[0], true);
    // Rotation's out handle to (0.5 s, 45°): influence 50 %, speed 90 °/s, which is
    // the segment's average speed. Opacity follows: 50 %, its own average speed.
    let to = pos2(x_of(&tl, 0.5), va.y(45.0));
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "one command, one undo step");
    h.apply(&r);
    let e = h.spec(ROT).keyframes[&Time::ZERO].ease_out[0];
    assert!((e.influence - 50.0).abs() < 0.5, "{e:?}");
    assert!((e.speed - 90.0).abs() < 1.5, "{e:?}");
    let e = h.spec(OPACITY).keyframes[&Time::ZERO].ease_out[0];
    assert!((e.influence - 50.0).abs() < 0.5, "{e:?}");
    assert!((e.speed + 50.0).abs() < 1.0, "{e:?}");
    // The unselected keys keep their eases.
    let e = h.spec(OPACITY).keyframes[&Time(2 * SEC)].ease_in[0];
    assert!((e.influence - 33.33).abs() < 0.1, "{e:?}");
    h.undo();
    for attr in [ROT, OPACITY] {
        let e = h.spec(attr).keyframes[&Time::ZERO].ease_out[0];
        assert!(
            (e.influence - 33.33).abs() < 0.1 && e.speed == 0.0,
            "{attr}: {e:?}"
        );
    }
}

#[test]
fn speed_graph_handle_drag_on_position() {
    let mut h = H::new();
    h.keys(POS, &[(0.0, vec3(0.0, 0.0)), (1.0, vec3(600.0, 0.0))]);
    let (k0, k1) = (h.key(POS, 0.0), h.key(POS, 1.0));
    h.run("key.ease", json!({"keys": [k0, k1]}));
    h.session().lock().selected_keys = vec![k0.clone()];
    let mut tl = h.graph();
    // Auto-Select Graph Type: Position is a speed graph.
    let from = handle_pos(&mut tl, &k0, true);
    let va = vaxis(&mut tl, POS);
    let to = pos2(x_of(&tl, 0.5), va.y(400.0));
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    let e = h.spec(POS).keyframes[&Time::ZERO].ease_out[0];
    assert!((e.influence - 50.0).abs() < 0.5, "{e:?}");
    assert!((e.speed - 400.0).abs() < va.per_point() * 2.0, "{e:?}");
    // Edit Value Graph: Position shows X / Y curves, no handles.
    tl.graph_editor().settings.graph_type = crate::graph::GraphType::Value;
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(tl.graph_editor().last.handles.is_empty());
    let ys: Vec<usize> = tl
        .graph_editor()
        .last
        .keys
        .iter()
        .filter(|(k, _, _)| *k == k0)
        .map(|(_, d, _)| *d)
        .collect();
    assert_eq!(ys, vec![0, 1], "X and Y of a 2D layer");
}

#[test]
fn transform_box_scales_times_about_the_opposite_edge() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(45.0))],
    );
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0), h.key(ROT, 2.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.graph();
    let b = tl
        .graph_editor()
        .last
        .selection_box
        .expect("box around 3 keys");
    // Right edge handle (drawn 4 pt outside the keys): double the width.
    let handle = pos2(b.right() + 4.0, b.center().y);
    let r = h.drag(
        &mut tl,
        handle,
        handle + vec2(b.width(), 0.0),
        Modifiers::NONE,
    );
    h.apply(&r);
    let times: Vec<i64> = h.spec(ROT).keyframes.keys().map(|t| t.0).collect();
    assert_eq!(times, vec![0, 2 * SEC, 4 * SEC]);
    h.undo();
    let times: Vec<i64> = h.spec(ROT).keyframes.keys().map(|t| t.0).collect();
    assert_eq!(times, vec![0, SEC, 2 * SEC]);
}

#[test]
fn snap_puts_a_dragged_key_on_the_cti() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (2.0, float(90.0))]);
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    h.session().lock().set_time(&info, Time::from_seconds(1.0));
    let mut tl = h.graph();
    {
        let s = &mut tl.graph_editor().settings;
        s.allow_between_frames = true;
        s.snap = true;
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let k = h.key(ROT, 2.0);
    let from = key_pos(&mut tl, &k, 0);
    let to = pos2(x_of(&tl, 1.0) + 3.0, from.y);
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    let times: Vec<i64> = h.spec(ROT).keyframes.keys().map(|t| t.0).collect();
    assert_eq!(times, vec![0, SEC], "snapped to the CTI");
    h.undo();
    // Without Snap (and between frames allowed) it lands 3 pt off.
    tl.graph_editor().settings.snap = false;
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let from = key_pos(&mut tl, &k, 0);
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    let times: Vec<i64> = h.spec(ROT).keyframes.keys().map(|t| t.0).collect();
    assert!(times[1] > SEC, "{times:?}");
}

#[test]
fn cmd_click_toggles_linear_and_auto_bezier() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(0.0))],
    );
    let mut tl = h.graph();
    let k = h.key(ROT, 1.0);
    let p = key_pos(&mut tl, &k, 0);
    let r = h.click(&mut tl, p, cmd());
    assert_eq!(r.invocations[0].id, "key.edit");
    h.apply(&r);
    let kf = h.spec(ROT).keyframes[&Time(SEC)].clone();
    assert!(kf.flags.temporal_auto_bezier);
    assert_eq!(kf.interp_out, Interpolation::Bezier);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let p = key_pos(&mut tl, &k, 0);
    let r = h.click(&mut tl, p, cmd());
    h.apply(&r);
    let kf = h.spec(ROT).keyframes[&Time(SEC)].clone();
    assert!(!kf.flags.temporal_auto_bezier);
    assert_eq!(
        (kf.interp_in, kf.interp_out),
        (Interpolation::Linear, Interpolation::Linear)
    );
}

#[test]
fn clicks_marquee_and_alt_click_select_keys() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(45.0))],
    );
    let mut tl = h.graph();
    let k1 = h.key(ROT, 1.0);
    let p1 = key_pos(&mut tl, &k1, 0);
    h.click(&mut tl, p1, Modifiers::NONE);
    assert_eq!(h.session().lock().selected_keys, vec![k1.clone()]);
    // Clicking empty graph space deselects.
    h.click(&mut tl, pos2(p1.x + 150.0, 120.0), Modifiers::NONE);
    assert!(h.session().lock().selected_keys.is_empty());
    // Marquee around the first two keys.
    let p0 = key_pos(&mut tl, &h.key(ROT, 0.0), 0);
    let a = pos2(p0.x - 10.0, p0.y.min(p1.y) - 10.0);
    let b = pos2(p1.x + 10.0, p0.y.max(p1.y) + 10.0);
    let r = h.drag(&mut tl, a, b, Modifiers::NONE);
    assert!(r.invocations.is_empty());
    assert_eq!(h.session().lock().selected_keys.len(), 2);
    // Alt-click on the curve between keys selects all keys of the property.
    let p2 = key_pos(&mut tl, &h.key(ROT, 2.0), 0);
    let mid = pos2((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
    h.click(&mut tl, mid, alt());
    assert_eq!(h.session().lock().selected_keys.len(), 3);
}

#[test]
fn bottom_buttons_convert_and_ease() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.graph();
    let interp = |h: &H| -> Vec<(Interpolation, Interpolation, bool)> {
        h.spec(ROT)
            .keyframes
            .values()
            .map(|k| (k.interp_in, k.interp_out, k.flags.temporal_auto_bezier))
            .collect()
    };
    for (label, want) in [
        (
            "Convert selected keyframes to Hold",
            (Interpolation::Hold, Interpolation::Hold, false),
        ),
        (
            "Convert selected keyframes to Auto Bezier",
            (Interpolation::Bezier, Interpolation::Bezier, true),
        ),
        (
            "Convert selected keyframes to Linear",
            (Interpolation::Linear, Interpolation::Linear, false),
        ),
    ] {
        let r = button_rect(&mut tl, label);
        let req = h.click(&mut tl, r.center(), Modifiers::NONE);
        assert_eq!(req.invocations.len(), 1, "{label}");
        h.apply(&req);
        assert!(
            interp(&h).iter().all(|x| *x == want),
            "{label}: {:?}",
            interp(&h)
        );
    }
    // Easy Ease In: the keys' incoming side.
    let r = button_rect(&mut tl, "Easy Ease In");
    let req = h.click(&mut tl, r.center(), Modifiers::NONE);
    assert_eq!(req.invocations[0].id, "key.ease");
    assert_eq!(req.invocations[0].params["mode"], "ease_in");
    h.apply(&req);
    assert!(
        h.spec(ROT)
            .keyframes
            .values()
            .all(|k| k.interp_in == Interpolation::Bezier)
    );
    // Each button is one undo step.
    h.undo();
    assert!(
        h.spec(ROT)
            .keyframes
            .values()
            .all(|k| k.interp_in == Interpolation::Linear)
    );
    // Without selected keys the key buttons are disabled.
    h.session().lock().selected_keys.clear();
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = button_rect(&mut tl, "Easy Ease");
    let req = h.click(&mut tl, r.center(), Modifiers::NONE);
    assert!(req.invocations.is_empty());
}

#[test]
fn separate_dimensions_button() {
    let mut h = H::new();
    h.keys(POS, &[(0.0, vec3(0.0, 0.0)), (1.0, vec3(600.0, 300.0))]);
    h.session().lock().selected_props = vec![(h.layer.clone(), POS.to_owned())];
    let mut tl = h.graph();
    let r = button_rect(&mut tl, "Separate Dimensions");
    let req = h.click(&mut tl, r.center(), Modifiers::NONE);
    assert_eq!(req.invocations[0].id, "transform.separate_dimensions");
    h.apply(&req);
    assert!(bae_motion::dimensions::is_separated(&h.stage, &h.layer));
    // The graph now shows X and Y Position.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let shown: Vec<String> = tl
        .graph_editor()
        .last
        .vaxes
        .iter()
        .map(|((_, a), _)| a.clone())
        .collect();
    assert!(
        shown.contains(&"transform:position_x".to_owned()),
        "{shown:?}"
    );
    assert!(
        shown.contains(&"transform:position_y".to_owned()),
        "{shown:?}"
    );
    h.undo();
    assert!(!bae_motion::dimensions::is_separated(&h.stage, &h.layer));
}

#[test]
fn clicking_a_property_name_selects_it_and_its_keys() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let mut tl = TimelinePanel::default();
    h.session()
        .lock()
        .reveal
        .insert(h.layer.clone(), crate::session::Reveal::Animated);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    // Row 2 (the revealed Rotation); the name starts after the stopwatch and
    // the Graph Editor Set switch.
    let row_y = 22.0 + 34.0 + 20.0 + 19.0 + 9.5;
    h.click(&mut tl, pos2(124.0 + 14.0 + 50.0, row_y), Modifiers::NONE);
    let s = h.session();
    let s = s.lock();
    assert_eq!(s.selected_props, vec![(h.layer.clone(), ROT.to_owned())]);
    assert_eq!(s.selected_keys.len(), 2);
    assert_eq!(s.selected_layers, vec![h.layer.clone()]);
}

#[test]
fn graph_editor_set_and_show_properties() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let mut tl = h.graph();
    assert_eq!(tl.graph_editor().last.vaxes.len(), 1);
    // Show only the Graph Editor Set: nothing until the property's switch is on.
    {
        let s = &mut tl.graph_editor().settings;
        s.show_selected = false;
        s.show_animated = false;
        s.show_set = true;
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(tl.graph_editor().last.vaxes.is_empty());
    h.session()
        .lock()
        .reveal
        .insert(h.layer.clone(), crate::session::Reveal::Animated);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let row_y = 22.0 + 34.0 + 20.0 + 19.0 + 9.5;
    h.click(&mut tl, pos2(124.0 + 14.0 - 8.0, row_y), Modifiers::NONE);
    assert_eq!(
        h.session().lock().graph_set,
        vec![(h.layer.clone(), ROT.to_owned())]
    );
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert_eq!(tl.graph_editor().last.vaxes.len(), 1);
}

// ---------------------------------------------------------------------------
// Layer-bar keyframes (AE-ANI-049, -070, -071), the keyframe menu (AE-ANI-029) and
// the keyframe dialogs (AE-ANI-047, -048).

impl H {
    /// Layer-bar mode with the solid's animated properties revealed (U).
    fn bars(&mut self) -> TimelinePanel {
        let mut tl = TimelinePanel::default();
        {
            let s = self.session();
            let mut s = s.lock();
            s.selected_layers = vec![self.layer.clone()];
            s.graph_editor = false;
            s.reveal
                .insert(self.layer.clone(), crate::session::Reveal::Animated);
        }
        self.frame(&mut tl, Vec::new(), Modifiers::NONE);
        self.frame(&mut tl, Vec::new(), Modifiers::NONE);
        tl
    }

    fn times(&self, attr: &str) -> Vec<i64> {
        self.spec(attr).keyframes.keys().map(|t| t.0).collect()
    }
}

fn bar_key(tl: &TimelinePanel, k: &KeyRef) -> Pos2 {
    tl.last_keys
        .iter()
        .find(|(x, _)| x == k)
        .map(|(_, p)| *p)
        .unwrap_or_else(|| panic!("key {k:?} not drawn"))
}

fn press(key: egui::Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn shift_snaps_a_dragged_keyframe_to_the_cti() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (2.0, float(90.0))]);
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    h.session().lock().set_time(&info, Time::from_seconds(1.0));
    let mut tl = h.bars();
    let k = h.key(ROT, 2.0);
    let from = bar_key(&tl, &k);
    // 3 pt right of the CTI: about one frame off.
    let to = pos2(x_of(&tl, 1.0) + 3.0, from.y);
    let r = h.drag(&mut tl, from, to, Modifiers::SHIFT);
    assert_eq!(r.invocations[0].id, "key.move");
    h.apply(&r);
    assert_eq!(h.times(ROT), vec![0, SEC], "snapped to the CTI");
    assert_eq!(h.session().lock().selected_keys, vec![h.key(ROT, 1.0)]);
    h.undo();
    assert_eq!(h.times(ROT), vec![0, 2 * SEC]);
    // Without Shift it moves by whole frames from the pointer.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    let t = h.times(ROT);
    let frame = SEC / 30;
    assert!(t[1] > SEC && t[1] % frame == 0, "{t:?}");
}

#[test]
fn alt_drag_scales_the_selected_keyframes_about_the_other_end() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(45.0)), (2.0, float(90.0))],
    );
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0), h.key(ROT, 2.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.bars();
    // Alt-drag the last keyframe from 2 s to 4 s: the first stays, the timing doubles.
    let from = bar_key(&tl, &keys[2]);
    let to = pos2(x_of(&tl, 4.0), from.y);
    let r = h.drag(&mut tl, from, to, alt());
    assert_eq!(r.invocations.len(), 1, "one command, one undo step");
    assert_eq!(r.invocations[0].id, "key.edit");
    h.apply(&r);
    assert_eq!(h.times(ROT), vec![0, 2 * SEC, 4 * SEC]);
    assert_eq!(
        h.spec(ROT).keyframes[&Time(2 * SEC)].value,
        Value::Float(45.0),
        "values stay"
    );
    assert_eq!(
        h.session().lock().selected_keys,
        vec![h.key(ROT, 0.0), h.key(ROT, 2.0), h.key(ROT, 4.0)]
    );
    h.undo();
    assert_eq!(h.times(ROT), vec![0, SEC, 2 * SEC]);
    // Alt-dragging the first keyframe scales about the last one.
    h.session().lock().selected_keys = keys.clone();
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let from = bar_key(&tl, &keys[0]);
    let to = pos2(x_of(&tl, 1.0), from.y);
    let r = h.drag(&mut tl, from, to, alt());
    h.apply(&r);
    assert_eq!(h.times(ROT), vec![SEC, SEC + SEC / 2, 2 * SEC]);
}

#[test]
fn cmd_click_on_a_layer_bar_keyframe_converts_its_interpolation() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let k = h.key(ROT, 0.0);
    let mut tl = h.bars();
    let p = bar_key(&tl, &k);
    // Linear → Auto Bezier.
    let r = h.click(&mut tl, p, cmd());
    h.apply(&r);
    let kf = h.spec(ROT).keyframes[&Time::ZERO].clone();
    assert!(kf.flags.temporal_auto_bezier, "{kf:?}");
    assert_eq!(kf.interp_out, Interpolation::Bezier);
    assert!(
        h.session().lock().selected_keys.is_empty(),
        "Cmd-click does not select"
    );
    // Bezier → Linear.
    let r = h.click(&mut tl, p, cmd());
    h.apply(&r);
    assert_eq!(
        h.spec(ROT).keyframes[&Time::ZERO].interp_out,
        Interpolation::Linear
    );
    // Cmd+Alt: Linear → Hold, and back.
    let r = h.click(&mut tl, p, cmd() | alt());
    h.apply(&r);
    let kf = h.spec(ROT).keyframes[&Time::ZERO].clone();
    assert_eq!(
        (kf.interp_in, kf.interp_out),
        (Interpolation::Hold, Interpolation::Hold)
    );
    h.undo();
    assert_eq!(
        h.spec(ROT).keyframes[&Time::ZERO].interp_out,
        Interpolation::Linear
    );
    // Shift-click toggles the selection.
    h.click(&mut tl, p, Modifiers::SHIFT);
    assert_eq!(h.session().lock().selected_keys, vec![k.clone()]);
    h.click(&mut tl, p, Modifiers::SHIFT);
    assert!(h.session().lock().selected_keys.is_empty());
}

#[test]
fn cmd_click_on_a_selected_keyframe_converts_the_whole_selection() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(0.0))],
    );
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0), h.key(ROT, 2.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.bars();
    let p = bar_key(&tl, &keys[1]);
    let r = h.click(&mut tl, p, cmd());
    assert_eq!(r.invocations.len(), 1, "one step");
    h.apply(&r);
    assert!(
        h.spec(ROT)
            .keyframes
            .values()
            .all(|k| k.flags.temporal_auto_bezier),
        "all three became Auto Bezier (R1, AE 26)"
    );
}

#[test]
fn alt_pressed_after_mouse_down_still_moves() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(45.0)), (2.0, float(90.0))],
    );
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0), h.key(ROT, 2.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.bars();
    let from = bar_key(&tl, &keys[2]);
    let to = pos2(x_of(&tl, 4.0), from.y);
    // Press without Alt, then hold Alt while dragging: a rigid move (AE 26).
    h.frame(&mut tl, vec![Event::PointerMoved(from)], Modifiers::NONE);
    h.frame(
        &mut tl,
        vec![button(from, true, Modifiers::NONE)],
        Modifiers::NONE,
    );
    for i in 1..=6 {
        let p = from + (to - from) * (i as f32 / 6.0);
        h.frame(&mut tl, vec![Event::PointerMoved(p)], alt());
    }
    let r = h.frame(&mut tl, vec![button(to, false, alt())], alt());
    assert_eq!(r.invocations[0].id, "key.move");
    h.apply(&r);
    assert_eq!(h.times(ROT), vec![2 * SEC, 3 * SEC, 4 * SEC]);
}

#[test]
fn keyframe_velocity_shows_linear_sides_as_ae_does() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (2.0, float(90.0))]);
    let d = crate::timeline::VelocityDialog::new(&h.stage, vec![h.key(ROT, 0.0)]).unwrap();
    // Linear: the segment's speed (45 °/s) and 16.667 % influence.
    assert!(
        (d.ease_out[0].speed - 45.0).abs() < 1e-6,
        "{:?}",
        d.ease_out
    );
    assert!((d.ease_out[0].influence - 100.0 / 6.0).abs() < 1e-6);
    // Scale: one row per dimension, Lock Dimensions on.
    h.keys(
        "transform:scale",
        &[(0.0, vec3(100.0, 100.0)), (1.0, vec3(200.0, 100.0))],
    );
    let d = crate::timeline::VelocityDialog::new(&h.stage, vec![h.key("transform:scale", 0.0)])
        .unwrap();
    assert!(d.lock_dims && d.ease_out.len() >= 2);
    assert!((d.ease_out[0].speed - 100.0).abs() < 1e-6 && d.ease_out[1].speed.abs() < 1e-6);
}

#[test]
fn keyframe_labels_and_label_groups() {
    use crate::actions::ids;
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(0.0))],
    );
    h.keys(POS, &[(0.0, vec3(0.0, 0.0)), (1.0, vec3(10.0, 0.0))]);
    let labelled = [h.key(ROT, 0.0), h.key(ROT, 2.0), h.key(POS, 1.0)];
    let edits: Vec<Json> = labelled
        .iter()
        .map(|k| json!({"key": k, "label": 3}))
        .collect();
    h.run("key.edit", json!({ "edits": edits }));
    assert_eq!(h.spec(ROT).keyframes[&Time::ZERO].label, 3);
    let session = h.session();
    crate::timeline::select_label_group(&h.stage, &session, &labelled[0]);
    let mut sel = session.lock().selected_keys.clone();
    sel.sort_by_key(|k| (k.attr.clone(), k.time));
    let mut want = labelled.to_vec();
    want.sort_by_key(|k| (k.attr.clone(), k.time));
    assert_eq!(sel, want);
    // Edit > Select Keyframe Label Group ▸ On All Layers, from one labelled key.
    let mut tl = h.bars();
    session.lock().selected_keys = vec![labelled[2].clone()];
    h.action(
        &mut tl,
        Invocation::with_params(ids::SELECT_KEY_LABEL_GROUP, json!({"scope": "all_layers"})),
    );
    assert_eq!(session.lock().selected_keys.len(), 3);
}

#[test]
fn expression_fields_use_the_expression_editor_preferences() {
    let mut resources = Resources::new();
    let (font, text, bg) = crate::common::expression_style(&resources);
    assert_eq!(font.size, 12.0, "AE default without preferences");
    assert_eq!(bg, egui::Color32::from_rgb(0x1d, 0x1d, 0x1d));
    let mut prefs = bae_prefs::Preferences::default();
    prefs.set(bae_prefs::keys::EXPR_FONT_SIZE, 20);
    prefs.set("scripting_expressions.expr_color_default", "#ff0000");
    resources.insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(prefs)));
    let (font, text2, _) = crate::common::expression_style(&resources);
    assert_eq!(font.size, 20.0);
    assert_ne!(text, text2);
    assert_eq!(text2, egui::Color32::from_rgb(0xff, 0, 0));
}

#[test]
fn keyframe_interpolation_dialog_sets_temporal_spatial_and_roving() {
    let mut h = H::new();
    h.keys(
        POS,
        &[
            (0.0, vec3(0.0, 0.0)),
            (1.0, vec3(100.0, 0.0)),
            (2.0, vec3(100.0, 100.0)),
        ],
    );
    let keys = vec![h.key(POS, 0.0), h.key(POS, 1.0), h.key(POS, 2.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.bars();
    let r = h.action(
        &mut tl,
        Invocation::new(crate::actions::ids::KEY_INTERPOLATION),
    );
    assert!(
        r.invocations.is_empty(),
        "the dialog opens; nothing changes yet"
    );
    {
        let s = h.session();
        let mut s = s.lock();
        let Some(crate::timeline::KeyDialog::Interpolation(d)) = &mut s.key_dialog else {
            panic!("Keyframe Interpolation is open");
        };
        d.temporal = 3; // Continuous Bezier
        d.spatial = 2; // Bezier
        d.roving = 1; // Rove Across Time
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.frame(&mut tl, vec![press(egui::Key::Enter)], Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "OK is one command");
    assert_eq!(r.invocations[0].id, "key.edit");
    assert!(h.session().lock().key_dialog.is_none());
    h.apply(&r);
    let spec = h.spec(POS);
    let ks: Vec<&bae_model::Keyframe> = spec.keyframes.values().collect();
    assert_eq!(ks.len(), 3);
    for k in &ks {
        assert_eq!(
            (k.interp_in, k.interp_out),
            (Interpolation::Bezier, Interpolation::Bezier)
        );
        assert!(k.flags.temporal_continuous && !k.flags.temporal_auto_bezier);
        assert!(!k.flags.spatial_auto_bezier && !k.flags.spatial_continuous);
    }
    // Bezier from Linear starts from the automatic tangents of the corner key.
    assert!(
        ks[1].spatial_out.is_some_and(|t| t != [0.0; 3]),
        "{:?}",
        ks[1]
    );
    assert!(!ks[0].flags.roving && ks[1].flags.roving && !ks[2].flags.roving);
    h.undo();
    let spec = h.spec(POS);
    assert!(
        spec.keyframes
            .values()
            .all(|k| k.interp_out == Interpolation::Linear && !k.flags.roving)
    );
}

#[test]
fn keyframe_dialogs_cancel_with_esc_and_need_selected_keys() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let mut tl = h.bars();
    // Nothing selected: the action does nothing.
    h.action(
        &mut tl,
        Invocation::new(crate::actions::ids::KEY_INTERPOLATION),
    );
    assert!(h.session().lock().key_dialog.is_none());
    h.session().lock().selected_keys = vec![h.key(ROT, 0.0)];
    h.action(&mut tl, Invocation::new(crate::actions::ids::KEY_VELOCITY));
    assert!(h.session().lock().key_dialog.is_some());
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.frame(&mut tl, vec![press(egui::Key::Escape)], Modifiers::NONE);
    assert!(r.invocations.is_empty());
    assert!(h.session().lock().key_dialog.is_none());
}

#[test]
fn keyframe_velocity_dialog_sets_every_selected_key() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(0.0))],
    );
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.bars();
    h.action(&mut tl, Invocation::new(crate::actions::ids::KEY_VELOCITY));
    {
        let s = h.session();
        let mut s = s.lock();
        let Some(crate::timeline::KeyDialog::Velocity(d)) = &mut s.key_dialog else {
            panic!("Keyframe Velocity is open");
        };
        d.ease_in[0] = bae_model::Ease {
            speed: 0.0,
            influence: 75.0,
        };
        d.ease_out[0] = bae_model::Ease {
            speed: 45.0,
            influence: 50.0,
        };
        d.continuous = false;
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.frame(&mut tl, vec![press(egui::Key::Enter)], Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1);
    h.apply(&r);
    let spec = h.spec(ROT);
    for t in [Time::ZERO, Time(SEC)] {
        let k = &spec.keyframes[&t];
        assert_eq!(k.interp_out, Interpolation::Bezier);
        assert_eq!(k.ease_out[0].speed, 45.0);
        assert_eq!(k.ease_out[0].influence, 50.0);
        assert_eq!(k.ease_in[0].influence, 75.0);
    }
    assert_eq!(
        spec.keyframes[&Time(2 * SEC)].interp_in,
        Interpolation::Linear,
        "unselected keys stay"
    );
    h.undo();
    assert!(
        h.spec(ROT)
            .keyframes
            .values()
            .all(|k| k.interp_out == Interpolation::Linear)
    );
}

#[test]
fn select_equal_previous_and_following_keyframes() {
    use crate::timeline::Related;
    let mut h = H::new();
    h.keys(
        ROT,
        &[
            (0.0, float(0.0)),
            (1.0, float(90.0)),
            (2.0, float(0.0)),
            (3.0, float(45.0)),
        ],
    );
    let session = h.session();
    let clicked = h.key(ROT, 2.0);
    crate::timeline::select_related(&h.stage, &session, &clicked, Related::Equal);
    assert_eq!(
        session.lock().selected_keys,
        vec![h.key(ROT, 0.0), h.key(ROT, 2.0)]
    );
    crate::timeline::select_related(&h.stage, &session, &clicked, Related::Previous);
    assert_eq!(
        session.lock().selected_keys,
        vec![h.key(ROT, 0.0), h.key(ROT, 1.0), h.key(ROT, 2.0)]
    );
    crate::timeline::select_related(&h.stage, &session, &clicked, Related::Following);
    assert_eq!(
        session.lock().selected_keys,
        vec![h.key(ROT, 2.0), h.key(ROT, 3.0)]
    );
}

#[test]
fn rove_across_time_needs_inner_spatial_keyframes() {
    let mut h = H::new();
    h.keys(
        POS,
        &[
            (0.0, vec3(0.0, 0.0)),
            (1.0, vec3(100.0, 0.0)),
            (2.0, vec3(100.0, 100.0)),
        ],
    );
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(9.0)), (2.0, float(0.0))],
    );
    let st = &h.stage;
    assert_eq!(
        crate::timeline::rove_state(st, &[h.key(POS, 1.0)]),
        (true, false)
    );
    assert!(
        !crate::timeline::rove_state(st, &[h.key(POS, 0.0)]).0,
        "first key"
    );
    assert!(
        !crate::timeline::rove_state(st, &[h.key(ROT, 1.0)]).0,
        "not spatial"
    );
    assert!(!crate::timeline::rove_state(st, &[]).0);
}

#[test]
fn layer_menu_entries_come_from_the_command_catalog() {
    for (top, title) in crate::timeline::LAYER_MENU {
        if top.is_empty() {
            continue;
        }
        assert!(
            crate::timeline::catalog_entry(top, title).is_some(),
            "{top} ▸ {title} is not in the command catalog"
        );
    }
    for (top, title) in [
        ("Edit", "Select Keyframe Label Group"),
        ("Animation", "Track Camera"),
        ("Animation", "Track Mask"),
        ("Layer", "Open Layer Source"),
    ] {
        assert!(
            crate::timeline::catalog_entry(top, title).is_some(),
            "{top} ▸ {title}"
        );
    }
}

#[test]
fn invert_selection_and_select_children() {
    let mut h = H::new();
    let comp = h.comp.clone();
    let a = h.layer.clone();
    let b = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    let c = path(&h.run("layer.create", json!({"comp": comp, "kind": "null"}))["path"]);
    let d = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    // c is parented to a, d to c; b is locked.
    h.run(
        "layer.set_parent",
        json!({"layers": [c], "parent": a, "time": 0}),
    );
    h.run(
        "layer.set_parent",
        json!({"layers": [d], "parent": c, "time": 0}),
    );
    h.run(
        "attr.set",
        json!({"path": b, "attr": "locked", "value": {"type": "bool", "value": true}}),
    );
    let session = h.session();
    session.lock().selected_layers = vec![a.clone()];
    crate::timeline::select_children(&h.stage, &session);
    let mut sel = session.lock().selected_layers.clone();
    sel.sort();
    let mut want = vec![a.clone(), c.clone(), d.clone()];
    want.sort();
    assert_eq!(sel, want, "children and grandchildren");
    session.lock().selected_layers = vec![c.clone()];
    crate::timeline::invert_layer_selection(&h.stage, &session, &c);
    let mut sel = session.lock().selected_layers.clone();
    sel.sort();
    let mut want = vec![a, d];
    want.sort();
    assert_eq!(sel, want, "the others, without the locked one");
}

// ---------------------------------------------------------------------------
// Markers.

fn marker_pos(tl: &TimelinePanel, m: &PrimPath) -> Pos2 {
    tl.last_markers
        .iter()
        .find(|(p, _)| p == m)
        .map(|(_, p)| *p)
        .unwrap_or_else(|| panic!("marker {m} not drawn"))
}

fn markers(h: &H, owner: &PrimPath) -> Vec<(PrimPath, i64)> {
    bae_motion::markers::markers_of(&h.stage, owner)
        .into_iter()
        .map(|(p, t)| (p, t.0))
        .collect()
}

#[test]
fn add_marker_puts_a_layer_or_comp_marker_at_the_cti() {
    use crate::actions::ids;
    let mut h = H::new();
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    // The layer starts at 1 s: its markers are in layer time.
    let layer = h.layer.clone();
    h.run("layer.shift_time", json!({"layers": [layer], "delta": SEC}));
    h.session().lock().set_time(&info, Time::from_seconds(2.0));
    let mut tl = h.bars();
    // No layer selected: a comp marker.
    h.session().lock().selected_layers.clear();
    let r = h.action(&mut tl, Invocation::new(ids::ADD_MARKER));
    assert_eq!(r.invocations.len(), 1);
    h.apply(&r);
    let comp = h.comp.clone();
    assert_eq!(markers(&h, &comp).len(), 1);
    assert_eq!(markers(&h, &comp)[0].1, 2 * SEC);
    // The layer selected: a layer marker at layer time 1 s.
    h.session().lock().selected_layers = vec![layer.clone()];
    let r = h.action(&mut tl, Invocation::new(ids::ADD_MARKER));
    h.apply(&r);
    assert_eq!(markers(&h, &layer).len(), 1);
    assert_eq!(markers(&h, &layer)[0].1, SEC);
    h.undo();
    assert!(markers(&h, &layer).is_empty());
    // Delete All Markers (of the selected layers).
    h.apply(&h.action(&mut tl, Invocation::new(ids::ADD_MARKER)));
    let r = h.action(&mut tl, Invocation::new(ids::DELETE_ALL_MARKERS));
    h.apply(&r);
    assert!(markers(&h, &layer).is_empty());
    assert_eq!(markers(&h, &comp).len(), 1, "comp markers stay");
}

#[test]
fn dragging_a_marker_moves_it_and_shift_snaps_to_the_cti() {
    let mut h = H::new();
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    let comp = h.comp.clone();
    let layer = h.layer.clone();
    h.run("layer.shift_time", json!({"layers": [layer], "delta": SEC}));
    let cm = path(&h.run("marker.add", json!({"owner": comp, "time": SEC}))["path"]);
    let lm = path(&h.run("marker.add", json!({"owner": layer, "comp_time": 2 * SEC}))["path"]);
    h.session().lock().set_time(&info, Time::from_seconds(4.0));
    let mut tl = h.bars();
    // Comp marker: Shift-drag to 3 pt right of the CTI lands on it.
    let from = marker_pos(&tl, &cm);
    let to = pos2(x_of(&tl, 4.0) + 3.0, from.y);
    let r = h.drag(&mut tl, from, to, Modifiers::SHIFT);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "marker.move");
    h.apply(&r);
    assert_eq!(markers(&h, &comp)[0].1, 4 * SEC);
    h.undo();
    // Layer marker (layer time 1 s = comp 2 s): Shift-drag onto the CTI at 4 s,
    // layer time 3 s.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let from = marker_pos(&tl, &lm);
    let to = pos2(x_of(&tl, 4.0) + 2.0, from.y);
    let r = h.drag(&mut tl, from, to, Modifiers::SHIFT);
    h.apply(&r);
    assert_eq!(markers(&h, &layer)[0].1, 3 * SEC);
}

#[test]
fn a_comp_marker_dragged_out_of_the_bin_lands_where_dropped() {
    let mut h = H::new();
    let mut tl = h.bars();
    let bin = tl.last_marker_bin.expect("marker bin drawn");
    let to = pos2(x_of(&tl, 2.0), bin.center().y);
    let r = h.drag(&mut tl, bin.center(), to, Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "marker.add");
    h.apply(&r);
    let comp = h.comp.clone();
    assert_eq!(markers(&h, &comp).len(), 1);
    let t = markers(&h, &comp)[0].1;
    assert!((t - 2 * SEC).abs() <= SEC / 30, "{t}");
}

#[test]
fn the_marker_dialog_edits_and_creates_markers() {
    use crate::actions::ids;
    let mut h = H::new();
    let comp = h.comp.clone();
    let m = path(&h.run("marker.add", json!({"owner": comp, "time": SEC}))["path"]);
    let mut tl = h.bars();
    // Double-click: the Composition Marker dialog.
    let p = marker_pos(&tl, &m);
    h.frame(&mut tl, vec![Event::PointerMoved(p)], Modifiers::NONE);
    for _ in 0..2 {
        h.frame(
            &mut tl,
            vec![button(p, true, Modifiers::NONE)],
            Modifiers::NONE,
        );
        h.frame(
            &mut tl,
            vec![button(p, false, Modifiers::NONE)],
            Modifiers::NONE,
        );
    }
    {
        let s = h.session();
        let mut s = s.lock();
        let Some(crate::timeline::KeyDialog::Marker(d)) = &mut s.key_dialog else {
            panic!("the marker dialog is open");
        };
        assert_eq!(d.time, Time(SEC));
        d.comment = "intro".into();
        d.duration = Time(SEC);
        d.label = 2;
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.frame(&mut tl, vec![press(egui::Key::Enter)], Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "marker.edit");
    h.apply(&r);
    let v = |a: &str| query::static_value(&h.stage.prim(&m).unwrap(), a);
    assert_eq!(v("comment"), Some(Value::String("intro".into())));
    assert_eq!(v("duration"), Some(Value::Time(Time(SEC))));
    assert_eq!(v("label"), Some(Value::Int(2)));
    // Add Marker with Dialog on the selected layer: OK adds it with the fields.
    let layer = h.layer.clone();
    h.session().lock().selected_layers = vec![layer.clone()];
    h.action(&mut tl, Invocation::new(ids::ADD_MARKER_DIALOG));
    if let Some(crate::timeline::KeyDialog::Marker(d)) = &mut h.session().lock().key_dialog {
        d.comment = "hit".into();
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.frame(&mut tl, vec![press(egui::Key::Enter)], Modifiers::NONE);
    assert_eq!(r.invocations[0].id, "marker.add");
    h.apply(&r);
    let lm = &markers(&h, &layer)[0].0;
    assert_eq!(
        query::static_value(&h.stage.prim(lm).unwrap(), "comment"),
        Some(Value::String("hit".into()))
    );
}

#[test]
fn timecode_fields_parse_timecode_and_frames() {
    let h = H::new();
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    let p = |s: &str| crate::timeline::parse_timecode(&info, s);
    assert_eq!(p("0:00:01:00"), Some(30));
    assert_eq!(p("0;00;02;15"), Some(75));
    assert_eq!(p("1:10"), Some(40));
    assert_eq!(p("45"), Some(45));
    assert_eq!(p("x"), None);
}

/// Frame cost with the B1-sized project (61 layers × Position, Scale, Rotation with
/// three eased keys each), every layer selected so the Graph Editor shows 183
/// properties. Run with
/// `cargo test --release -p bae-ui-motion graph_editor_frame_cost -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark"]
fn graph_editor_frame_cost() {
    let mut h = H::new();
    h.tessellate = true;
    let comp = h.comp.clone();
    let mut layers = vec![h.layer.clone()];
    for _ in 0..60 {
        layers.push(path(
            &h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"],
        ));
    }
    for l in &layers {
        for (attr, vals) in [
            (
                POS,
                vec![vec3(0.0, 0.0), vec3(300.0, 100.0), vec3(500.0, 300.0)],
            ),
            (
                "transform:scale",
                vec![vec3(100.0, 100.0), vec3(150.0, 150.0), vec3(80.0, 80.0)],
            ),
            (ROT, vec![float(0.0), float(90.0), float(180.0)]),
        ] {
            h.run(
                "attr.set_animated",
                json!({"path": l, "attr": attr, "animated": true, "time": 0}),
            );
            for (i, v) in vals.into_iter().enumerate() {
                h.run(
                    "attr.set_at",
                    json!({"path": l, "attr": attr, "time": Time::from_seconds(i as f64 * 2.0), "value": v}),
                );
            }
            let keys: Vec<KeyRef> = (0..3)
                .map(|i| KeyRef::new(l.clone(), attr, Time::from_seconds(i as f64 * 2.0)))
                .collect();
            h.run("key.ease", json!({"keys": keys}));
        }
    }
    h.session().lock().selected_layers = layers.clone();
    let mut tl = TimelinePanel::default();
    let n = 30;
    let measure = |h: &mut H, tl: &mut TimelinePanel, label: &str, graph: bool, drag: bool| {
        h.session().lock().graph_editor = graph;
        h.frame(tl, Vec::new(), Modifiers::NONE);
        let k = h.key(ROT, 2.0);
        h.session().lock().selected_keys = vec![k.clone()];
        h.frame(tl, Vec::new(), Modifiers::NONE);
        let from = if graph {
            key_pos(tl, &k, 0)
        } else {
            pos2(900.0, 400.0)
        };
        h.frame(tl, vec![Event::PointerMoved(from)], Modifiers::NONE);
        if drag {
            h.frame(
                tl,
                vec![button(from, true, Modifiers::NONE)],
                Modifiers::NONE,
            );
        }
        let t0 = std::time::Instant::now();
        for i in 1..=n {
            let to = from + vec2(i as f32 * 3.0, -(i as f32));
            h.frame(tl, vec![Event::PointerMoved(to)], Modifiers::NONE);
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
        if drag {
            let to = from + vec2(n as f32 * 3.0, -(n as f32));
            // The commit request is dropped: every pass starts from the same keys.
            h.frame(
                tl,
                vec![button(to, false, Modifiers::NONE)],
                Modifiers::NONE,
            );
        }
        eprintln!("PERF {label}: {ms:.2} ms/frame (UI + tessellation)");
    };
    measure(
        &mut h,
        &mut tl,
        "layer bar mode, pointer move",
        false,
        false,
    );
    measure(&mut h, &mut tl, "graph editor, pointer move", true, false);
    measure(&mut h, &mut tl, "graph editor, key drag", true, true);
    assert_eq!(tl.graph_editor().last.vaxes.len(), 183);
}
