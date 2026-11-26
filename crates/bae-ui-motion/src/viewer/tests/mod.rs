//! Shape tools in the Composition viewer, driven with simulated pointer and key
//! events: what each tool makes in the three selection states, modifiers, keys,
//! double-clicking a tool button, and undo.

use super::CompositionViewer;
use crate::session::SharedSession;
use crate::tools::{FillRequest, SharedTools, Tool};
use bae_anim::glam::DVec2;
use bae_app::Resources;
use bae_model::{AuthorId, BezierPath, CommandRegistry, PrimPath, Stage, Value};
use bae_motion::query;
use bae_ui::{Invocation, Panel, PanelContext, PanelId, PanelRequests};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};
use serde_json::json;

struct H {
    stage: Stage,
    reg: CommandRegistry,
    author: AuthorId,
    resources: Resources,
    ctx: egui::Context,
    viewer: CompositionViewer,
    comp: PrimPath,
    time: f64,
    /// Modifiers egui last heard of.
    mods: Modifiers,
    shapes: Vec<egui::epaint::ClippedShape>,
    /// The accessibility tree of the last frame (menus are found by label).
    access: Option<egui::accesskit::TreeUpdate>,
}

fn path(v: &serde_json::Value) -> PrimPath {
    PrimPath::new(v.as_str().expect("path")).unwrap()
}

impl H {
    /// A 640×360 comp, open in the viewer; nothing selected.
    fn new() -> Self {
        let mut reg = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        let mut resources = Resources::new();
        resources.insert(SharedSession::default());
        resources.insert(SharedTools::default());
        let mut h = H {
            stage: Stage::new(),
            reg,
            author: AuthorId::new("user:test").unwrap(),
            resources,
            ctx: bae_ui::testing::ctx_with_fonts(),
            viewer: CompositionViewer::new(None),
            comp: PrimPath::new("/x").unwrap(),
            time: 0.0,
            mods: Modifiers::NONE,
            access: None,
            shapes: Vec::new(),
        };
        h.ctx.enable_accesskit();
        h.comp = path(&h.run("comp.create", json!({"width": 640, "height": 360}))["path"]);
        h.session().lock().open_comp(&h.comp);
        h.frame(Vec::new(), Modifiers::NONE);
        h
    }

    fn session(&self) -> SharedSession {
        self.resources.get::<SharedSession>().unwrap().clone()
    }

    fn tools(&self) -> SharedTools {
        self.resources.get::<SharedTools>().unwrap().clone()
    }

    fn run(&mut self, id: &str, params: serde_json::Value) -> serde_json::Value {
        self.reg
            .execute(&mut self.stage, &self.author, id, params)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .output
    }

    fn layer(&mut self, kind: &str) -> PrimPath {
        path(&self.run("layer.create", json!({"comp": self.comp, "kind": kind}))["path"])
    }

    fn select(&self, layers: &[&PrimPath]) {
        self.session().lock().selected_layers = layers.iter().map(|l| (*l).clone()).collect();
    }

    /// One frame (1/60 s) of the viewer with `events`.
    fn frame(&mut self, mut events: Vec<Event>, modifiers: Modifiers) -> PanelRequests {
        self.time += 1.0 / 60.0;
        if modifiers != self.mods {
            self.mods = modifiers;
            events.insert(0, Event::ModifiersChanged(modifiers));
        }
        let mut requests = PanelRequests::default();
        let id = PanelId::new("composition");
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
            time: Some(self.time),
            predicted_dt: 1.0 / 60.0,
            events,
            ..Default::default()
        };
        let (stage, author, resources) = (&self.stage, &self.author, &self.resources);
        let viewer = &mut self.viewer;
        let mut out = self.ctx.run_ui(raw, |ui| {
            let mut cx =
                PanelContext::new(&id, "Composition", stage, author, resources, &mut requests);
            cx.focused = true;
            viewer.ui(ui, &mut cx);
        });
        out.textures_delta.clear();
        self.access = out.platform_output.accesskit_update.take();
        self.shapes = out.shapes;
        requests
    }

    /// Run the commands a frame asked for (the shell does this between frames).
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
        // Pending selections resolve on the next frame.
        self.frame(Vec::new(), Modifiers::NONE);
    }

    fn action(&mut self, id: &str) -> bool {
        let mut requests = PanelRequests::default();
        let pid = PanelId::new("composition");
        let mut cx = PanelContext::new(
            &pid,
            "Composition",
            &self.stage,
            &self.author,
            &self.resources,
            &mut requests,
        );
        cx.focused = true;
        self.viewer.on_action(&Invocation::new(id), &mut cx)
    }

    /// Screen position of a comp pixel.
    fn screen(&self, x: f64, y: f64) -> Pos2 {
        self.viewer
            .last_view
            .expect("a frame ran")
            .to_screen(DVec2::new(x, y))
    }

    fn pointer_button(&mut self, at: Pos2, pressed: bool, m: Modifiers) -> PanelRequests {
        let events = vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: m,
            },
        ];
        self.frame(events, m)
    }

    fn move_to(&mut self, at: Pos2, m: Modifiers, mut extra: Vec<Event>) -> PanelRequests {
        let mut events = vec![Event::PointerMoved(at)];
        events.append(&mut extra);
        self.frame(events, m)
    }

    /// Press at comp `(x0, y0)` and drag to `(x1, y1)` in steps (no release).
    fn drag_to(&mut self, from: (f64, f64), to: (f64, f64), m: Modifiers) {
        let a = self.screen(from.0, from.1);
        let b = self.screen(to.0, to.1);
        self.move_to(a, Modifiers::NONE, Vec::new());
        self.pointer_button(a, true, Modifiers::NONE);
        for i in 1..=6 {
            let t = i as f32 / 6.0;
            let r = self.move_to(a + (b - a) * t, m, Vec::new());
            assert!(r.invocations.is_empty(), "no command while dragging");
        }
    }

    fn release(&mut self, at: (f64, f64), m: Modifiers) -> PanelRequests {
        let p = self.screen(at.0, at.1);
        self.pointer_button(p, false, m)
    }

    /// A whole drag with `tool`; returns the requests of the release frame.
    fn draw(&mut self, tool: Tool, from: (f64, f64), to: (f64, f64)) -> PanelRequests {
        self.tools().lock().set(tool);
        self.drag_to(from, to, Modifiers::NONE);
        self.release(to, Modifiers::NONE)
    }

    fn key(&mut self, key: Key) {
        let at = self
            .ctx
            .input(|i| i.pointer.latest_pos())
            .unwrap_or_default();
        let ev = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        self.move_to(at, Modifiers::NONE, vec![ev(true)]);
        self.move_to(at, Modifiers::NONE, vec![ev(false)]);
    }

    fn value(&self, p: &PrimPath, attr: &str) -> Option<Value> {
        query::static_value(&self.stage.prim(p).unwrap(), attr)
    }

    fn name(&self, p: &PrimPath) -> String {
        query::display_name_of(&self.stage, p)
    }

    fn children(&self, p: &PrimPath) -> Vec<PrimPath> {
        self.stage.children(p)
    }

    fn contents(&self, p: &PrimPath) -> Vec<PrimPath> {
        self.children(&p.child("contents").unwrap())
    }
}

fn one(r: &PanelRequests, id: &str) -> serde_json::Value {
    assert_eq!(r.invocations.len(), 1, "{:?}", r.invocations);
    assert_eq!(r.invocations[0].id, id);
    r.invocations[0].params.clone()
}

fn vec2v(v: Option<Value>) -> [f64; 2] {
    match v {
        Some(Value::Vec2(a)) => a,
        Some(Value::Vec3(a)) => [a[0], a[1]],
        other => panic!("not a vector: {other:?}"),
    }
}

fn float(v: Option<Value>) -> f64 {
    v.and_then(|v| v.as_f64()).expect("number")
}

fn close(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01
}

fn bounds(p: &BezierPath) -> ([f64; 2], [f64; 2]) {
    let xs = p.vertices.iter().map(|v| v[0]);
    let ys = p.vertices.iter().map(|v| v[1]);
    (
        [
            xs.clone().fold(f64::MAX, f64::min),
            ys.clone().fold(f64::MAX, f64::min),
        ],
        [xs.fold(f64::MIN, f64::max), ys.fold(f64::MIN, f64::max)],
    )
}

impl H {
    /// A Pen click (press and release without moving) at comp `(x, y)`.
    fn click(&mut self, x: f64, y: f64, m: Modifiers) -> PanelRequests {
        let p = self.screen(x, y);
        self.move_to(p, m, Vec::new());
        let a = self.pointer_button(p, true, m);
        let b = self.pointer_button(p, false, m);
        let mut r = a;
        r.invocations.extend(b.invocations);
        r
    }

    /// Press at `(x, y)`, drag to `(x + dx, y + dy)`, release.
    fn click_drag(&mut self, at: (f64, f64), d: (f64, f64), m: Modifiers) -> PanelRequests {
        let a = self.screen(at.0, at.1);
        let b = self.screen(at.0 + d.0, at.1 + d.1);
        self.move_to(a, m, Vec::new());
        self.pointer_button(a, true, m);
        for i in 1..=4 {
            self.move_to(a + (b - a) * (i as f32 / 4.0), m, Vec::new());
        }
        self.pointer_button(b, false, m)
    }

    fn bezier(&self, prim: &PrimPath) -> BezierPath {
        match self.value(prim, "path") {
            Some(Value::Bezier(b)) => b,
            other => panic!("no path on {prim}: {other:?}"),
        }
    }
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01
}

impl H {
    /// A solid of `size` centered at comp `(x, y)` (anchor at its center).
    fn solid(&mut self, size: [f64; 2], at: [f64; 2]) -> PrimPath {
        let size = [size[0] as u32, size[1] as u32];
        path(
            &self.run(
                "layer.create",
                json!({"comp": self.comp, "kind": "solid", "size": size, "position": at}),
            )["path"],
        )
    }

    fn action_with(&mut self, id: &str, params: serde_json::Value) -> PanelRequests {
        let mut requests = PanelRequests::default();
        let pid = PanelId::new("composition");
        let mut cx = PanelContext::new(
            &pid,
            "Composition",
            &self.stage,
            &self.author,
            &self.resources,
            &mut requests,
        );
        cx.focused = true;
        assert!(
            self.viewer
                .on_action(&Invocation::with_params(id, params), &mut cx),
            "{id}"
        );
        requests
    }

    fn v3(&self, p: &PrimPath, attr: &str) -> [f64; 3] {
        match self.value(p, attr) {
            Some(Value::Vec3(v)) => v,
            other => panic!("{attr}: {other:?}"),
        }
    }

    /// Drag from comp `a` to comp `b` in steps and release; returns the
    /// release frame's requests.
    fn drag(&mut self, a: (f64, f64), b: (f64, f64), m: Modifiers) -> PanelRequests {
        self.drag_to(a, b, m);
        self.release(b, m)
    }
}

fn close3(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 0.01)
}

impl H {
    fn type_text(&mut self, events: Vec<Event>) -> PanelRequests {
        let at = self
            .ctx
            .input(|i| i.pointer.latest_pos())
            .unwrap_or_default();
        let mut evs = vec![Event::PointerMoved(at)];
        evs.extend(events);
        self.frame(evs, Modifiers::NONE)
    }

    fn escape(&mut self) -> PanelRequests {
        let ev = |pressed| Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let r = self.type_text(vec![ev(true)]);
        let mut r2 = self.type_text(vec![ev(false)]);
        r2.invocations.splice(0..0, r.invocations);
        r2
    }

    /// Cmd/Ctrl+Enter: commit the text being typed (AE).
    fn commit_text(&mut self) -> PanelRequests {
        let ev = |pressed| Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        let r = self.frame(vec![ev(true)], Modifiers::COMMAND);
        let mut r2 = self.frame(vec![ev(false)], Modifiers::COMMAND);
        self.frame(Vec::new(), Modifiers::NONE);
        r2.invocations.splice(0..0, r.invocations);
        r2
    }

    fn doc(&self, layer: &PrimPath) -> bae_model::TextDocument {
        match self.value(layer, "text:source") {
            Some(Value::TextDocument(d)) => *d,
            other => panic!("{other:?}"),
        }
    }
}

impl H {
    /// Center of the widget labeled `label` in the last frame.
    fn labeled(&self, label: &str) -> Option<Pos2> {
        let update = self.access.as_ref()?;
        update.nodes.iter().find_map(|(_, n)| {
            (n.label() == Some(label))
                .then(|| n.bounds())
                .flatten()
                .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
        })
    }

    /// Right-click at comp `(x, y)`; the menu shows from the next frame.
    fn right_click(&mut self, x: f64, y: f64) {
        let p = self.screen(x, y);
        self.move_to(p, Modifiers::NONE, Vec::new());
        for pressed in [true, false] {
            let events = vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Secondary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ];
            self.frame(events, Modifiers::NONE);
        }
        self.frame(Vec::new(), Modifiers::NONE);
    }

    /// Click the menu entry labeled `label`.
    fn choose(&mut self, label: &str) -> PanelRequests {
        let p = self
            .labeled(label)
            .unwrap_or_else(|| panic!("no menu entry {label}"));
        self.move_to(p, Modifiers::NONE, Vec::new());
        let a = self.pointer_button(p, true, Modifiers::NONE);
        let mut b = self.pointer_button(p, false, Modifiers::NONE);
        b.invocations.splice(0..0, a.invocations);
        b
    }
}

fn tool_action(h: &H, id: &str, params: serde_json::Value) -> PanelRequests {
    use bae_ui::ActionHandler as _;
    let mut requests = PanelRequests::default();
    let pid = PanelId::new("timeline");
    {
        let mut cx = PanelContext::new(
            &pid,
            "Timeline",
            &h.stage,
            &h.author,
            &h.resources,
            &mut requests,
        );
        assert!(crate::tools::ToolActions.handle(&Invocation::with_params(id, params), &mut cx));
    }
    requests
}

mod context_menu;
mod display;
mod effect_points;
mod free_transform;
mod guides;
mod masks;
mod motion_sketch;
mod navigation;
mod pen;
mod resolution;
mod selection;
mod shape_tools;
mod tools;
mod type_tool;
mod view3d;
mod view_options;
