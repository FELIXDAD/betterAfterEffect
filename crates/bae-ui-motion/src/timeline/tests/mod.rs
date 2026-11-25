//! Timeline and Graph Editor interaction tests: the panel runs headless with
//! simulated pointer and keyboard input, its requests run through the command
//! registry the way the shell runs them, and every edit is checked in the project
//! and undone.

use crate::TimelinePanel;
use crate::session::SharedSession;
use bae_app::Resources;
use bae_model::{AuthorId, CommandRegistry, Interpolation, PrimPath, Stage, Time, Value};
use bae_motion::{KeyRef, query};
use bae_ui::{Invocation, Panel, PanelContext, PanelId, PanelRequests};
use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use serde_json::{Value as Json, json};
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
            ctx: bae_ui::testing::ctx_with_fonts(),
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

fn bar_rect(tl: &TimelinePanel, l: &PrimPath) -> Rect {
    tl.last_bars
        .iter()
        .find(|(p, _)| p == l)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("bar of {l} not drawn"))
}

/// F4: the Modes column instead of Switches (AE starts with Switches).
fn show_modes(h: &mut H, tl: &mut TimelinePanel) {
    h.action(
        tl,
        Invocation::new(crate::actions::ids::TOGGLE_SWITCHES_MODES),
    );
    h.frame(tl, Vec::new(), Modifiers::NONE);
}

fn modes_cell(tl: &TimelinePanel, l: &PrimPath) -> (Rect, Pos2, Pos2) {
    tl.last_modes
        .iter()
        .find(|(p, ..)| p == l)
        .map(|(_, t, w, a)| (*t, *w, *a))
        .unwrap_or_else(|| panic!("modes of {l} not drawn"))
}

impl H {
    /// A value edit of `attr` of `path` at time 0, the way the timeline and
    /// Effect Controls make it.
    fn edit(&self, path: &PrimPath, attr: &str, edit: crate::common::Edit) -> PanelRequests {
        let mut requests = PanelRequests::default();
        let id = PanelId::new("timeline");
        let session = self.session();
        let mut cx = PanelContext::new(
            &id,
            "timeline",
            &self.stage,
            &self.author,
            &self.resources,
            &mut requests,
        );
        crate::common::apply_edit(&mut cx, &session, path, attr, Time::ZERO, edit);
        requests
    }
}

mod expressions;
mod graph;
mod keyframes;
mod layer_bars;
mod layers;
mod markers;
mod multi_edit;
mod outline;
mod switches;
