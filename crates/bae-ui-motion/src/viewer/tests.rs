//! Shape tools in the Composition viewer, driven with simulated pointer and key
//! events: what each tool makes in the three selection states, modifiers, keys,
//! double-clicking a tool button, and undo.

use bae_anim::glam::DVec2;
use bae_app::Resources;
use bae_model::{AuthorId, BezierPath, CommandRegistry, PrimPath, Stage, Value};
use bae_motion::query;
use bae_ui::{Invocation, Panel, PanelContext, PanelId, PanelRequests};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};
use serde_json::json;

use super::CompositionViewer;
use crate::session::SharedSession;
use crate::tools::{FillRequest, SharedTools, Tool};

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
            ctx: egui::Context::default(),
            viewer: CompositionViewer::new(None),
            comp: PrimPath::new("/x").unwrap(),
            time: 0.0,
            mods: Modifiers::NONE,
        };
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

#[test]
fn rectangle_without_a_selection_makes_a_selected_shape_layer() {
    let mut h = H::new();
    let r = h.draw(Tool::Rectangle, (100.0, 50.0), (300.0, 150.0));
    let params = one(&r, "shape.add");
    assert_eq!(params["name"], "Shape Layer 1");
    h.apply(&r);
    let layers = query::comp_layers(&h.stage, &h.comp);
    assert_eq!(layers.len(), 1);
    let layer = layers[0].clone();
    assert_eq!(h.name(&layer), "Shape Layer 1");
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&layer)
    );
    // Rectangle 1: Rectangle Path 1, Stroke 1, Fill 1; the group sits at the drag
    // center relative to the comp center (the layer's origin).
    let group = h.contents(&layer)[0].clone();
    assert_eq!(h.name(&group), "Rectangle 1");
    let items: Vec<String> = h.contents(&group).iter().map(|p| h.name(p)).collect();
    assert_eq!(items, ["Rectangle Path 1", "Stroke 1", "Fill 1"]);
    assert!(close(
        vec2v(h.value(&group, "transform:position")),
        [-120.0, -80.0]
    ));
    let rect = h.contents(&group)[0].clone();
    assert!(close(vec2v(h.value(&rect, "size")), [200.0, 100.0]));
    // Fill and stroke from the tool options.
    let fill = h.contents(&group)[2].clone();
    assert_eq!(
        h.value(&fill, "color"),
        Some(Value::Color([1.0, 0.0, 0.0, 1.0]))
    );
    let stroke = h.contents(&group)[1].clone();
    assert_eq!(float(h.value(&stroke, "width")), 2.0);
    // One undo step.
    h.stage.undo(&h.author).unwrap();
    assert!(query::comp_layers(&h.stage, &h.comp).is_empty());
}

#[test]
fn every_shape_tool_on_a_selected_shape_layer_adds_a_group() {
    let mut h = H::new();
    let shape = h.layer("shape");
    h.select(&[&shape]);
    let tools = [
        (Tool::Rectangle, "Rectangle 1"),
        (Tool::RoundedRectangle, "Rectangle 2"),
        (Tool::Ellipse, "Ellipse 1"),
        (Tool::Polygon, "Polystar 1"),
        (Tool::Star, "Polystar 2"),
    ];
    for (tool, name) in tools {
        let r = h.draw(tool, (200.0, 100.0), (260.0, 150.0));
        assert_eq!(one(&r, "shape.add")["layer"], json!(shape), "{tool:?}");
        h.apply(&r);
        let top = h.contents(&shape)[0].clone();
        assert_eq!(h.name(&top), name, "{tool:?}: new groups go on top");
    }
    assert_eq!(
        query::comp_layers(&h.stage, &h.comp),
        std::slice::from_ref(&shape)
    );
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&shape)
    );
    let rounded = h.contents(&shape)[3].clone();
    let path = h.contents(&rounded)[0].clone();
    assert_eq!(float(h.value(&path, "roundness")), 10.0, "tool roundness");
    // Undo takes back one group.
    h.stage.undo(&h.author).unwrap();
    assert_eq!(h.contents(&shape).len(), 4);
}

#[test]
fn shape_tools_on_other_layers_or_with_tool_creates_mask_make_masks() {
    let mut h = H::new();
    let solid = h.layer("solid");
    h.select(&[&solid]);
    let r = h.draw(Tool::Rectangle, (100.0, 50.0), (300.0, 150.0));
    let params = one(&r, "mask.create");
    assert_eq!(params["layer"], json!(solid));
    assert_eq!(params["name"], "Mask 1");
    h.apply(&r);
    let masks = h.children(&solid.child("masks").unwrap());
    assert_eq!(masks.len(), 1);
    assert_eq!(h.name(&masks[0]), "Mask 1");
    assert_eq!(h.value(&masks[0], "mode"), Some(Value::Enum("add".into())));
    // Layer space of a comp-sized solid is comp space.
    let Some(Value::Bezier(b)) = h.value(&masks[0], "path") else {
        panic!("mask path");
    };
    let (lo, hi) = bounds(&b);
    assert!(
        close(lo, [100.0, 50.0]) && close(hi, [300.0, 150.0]),
        "{lo:?} {hi:?}"
    );
    // The next one is Mask 2; every shape tool draws masks.
    for tool in [Tool::Ellipse, Tool::Star] {
        let r = h.draw(tool, (320.0, 180.0), (360.0, 220.0));
        one(&r, "mask.create");
        h.apply(&r);
    }
    let names: Vec<String> = h
        .children(&solid.child("masks").unwrap())
        .iter()
        .map(|m| h.name(m))
        .collect();
    assert_eq!(names, ["Mask 1", "Mask 2", "Mask 3"]);
    h.stage.undo(&h.author).unwrap();
    assert_eq!(h.children(&solid.child("masks").unwrap()).len(), 2);

    // A shape layer with Tool Creates Mask.
    let shape = h.layer("shape");
    h.select(&[&shape]);
    h.tools().lock().options.creates_mask = true;
    let r = h.draw(Tool::Ellipse, (100.0, 50.0), (300.0, 150.0));
    assert_eq!(one(&r, "mask.create")["layer"], json!(shape));
    h.apply(&r);
    assert!(h.contents(&shape).is_empty(), "no group");

    // A null cannot take a mask: a new shape layer instead.
    h.tools().lock().options.creates_mask = false;
    let null = h.layer("null");
    h.select(&[&null]);
    let r = h.draw(Tool::Rectangle, (100.0, 50.0), (300.0, 150.0));
    let params = one(&r, "shape.add");
    assert!(params.get("layer").is_none() && params["comp"] == json!(h.comp));
}

#[test]
fn shift_constrains_and_command_draws_from_the_center() {
    let mut h = H::new();
    let solid = h.layer("solid");
    h.select(&[&solid]);
    let mask_bounds = |h: &mut H, r: &PanelRequests| {
        h.apply(r);
        let masks = h.children(&solid.child("masks").unwrap());
        let Some(Value::Bezier(b)) = h.value(masks.last().unwrap(), "path") else {
            panic!("mask path");
        };
        bounds(&b)
    };
    // Shift: a square, the larger side wins.
    h.tools().lock().set(Tool::Rectangle);
    h.drag_to((100.0, 100.0), (300.0, 160.0), Modifiers::SHIFT);
    let r = h.release((300.0, 160.0), Modifiers::SHIFT);
    let (lo, hi) = mask_bounds(&mut h, &r);
    assert!(
        close(lo, [100.0, 100.0]) && close(hi, [300.0, 300.0]),
        "{lo:?} {hi:?}"
    );
    // Cmd/Ctrl: from the center outward.
    h.drag_to((200.0, 100.0), (260.0, 140.0), Modifiers::COMMAND);
    let r = h.release((260.0, 140.0), Modifiers::COMMAND);
    let (lo, hi) = mask_bounds(&mut h, &r);
    assert!(
        close(lo, [140.0, 60.0]) && close(hi, [260.0, 140.0]),
        "{lo:?} {hi:?}"
    );
    // Polygon with Shift: no rotation.
    h.tools().lock().set(Tool::Polygon);
    h.select(&[]);
    h.drag_to((320.0, 180.0), (400.0, 200.0), Modifiers::SHIFT);
    let r = h.release((400.0, 200.0), Modifiers::SHIFT);
    let p = one(&r, "shape.add");
    assert_eq!(p["rotation"], 0.0);
}

#[test]
fn space_moves_the_shape_and_esc_cancels() {
    let mut h = H::new();
    h.tools().lock().set(Tool::Ellipse);
    h.drag_to((100.0, 50.0), (300.0, 150.0), Modifiers::NONE);
    // Space held: Space and Page Up go to the drag, not playback / frame steps.
    assert!(h.action("motion.play"));
    assert!(!h.session().lock().is_playing());
    let space = |pressed| Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let a = h.screen(300.0, 150.0);
    h.move_to(a, Modifiers::NONE, vec![space(true)]);
    for i in 1..=5 {
        let p = a + vec2(10.0, 4.0) * i as f32;
        h.move_to(p, Modifiers::NONE, Vec::new());
    }
    h.move_to(a + vec2(50.0, 20.0), Modifiers::NONE, vec![space(false)]);
    let r = h.release((350.0, 170.0), Modifiers::NONE);
    let p = one(&r, "shape.add");
    // Same size, moved by (50, 20): center (250, 120) - comp center (320, 180).
    assert!(close(
        [
            p["size"][0].as_f64().unwrap(),
            p["size"][1].as_f64().unwrap()
        ],
        [200.0, 100.0]
    ));
    assert!(close(
        [
            p["position"][0].as_f64().unwrap(),
            p["position"][1].as_f64().unwrap()
        ],
        [-70.0, -60.0]
    ));
    // Esc cancels a drag.
    h.drag_to((100.0, 50.0), (300.0, 150.0), Modifiers::NONE);
    h.key(Key::Escape);
    let r = h.release((300.0, 150.0), Modifiers::NONE);
    assert!(r.invocations.is_empty(), "{:?}", r.invocations);
    // After the drag, actions reach the viewer's normal handling again.
    assert!(h.viewer.shape.is_none());
}

#[test]
fn arrows_change_roundness_points_and_roundness_of_polystars() {
    let mut h = H::new();
    // Rounded rectangle: Up / Down step the roundness, Left = 0, Right = max.
    h.tools().lock().set(Tool::RoundedRectangle);
    h.drag_to((100.0, 50.0), (300.0, 150.0), Modifiers::NONE);
    for _ in 0..3 {
        h.key(Key::ArrowUp);
    }
    h.key(Key::ArrowDown);
    let r = h.release((300.0, 150.0), Modifiers::NONE);
    assert_eq!(one(&r, "shape.add")["roundness"], 12.0);
    // The tool remembers it.
    assert_eq!(h.tools().lock().options.roundness, 12.0);
    h.drag_to((100.0, 50.0), (300.0, 150.0), Modifiers::NONE);
    h.key(Key::ArrowRight);
    let r = h.release((300.0, 150.0), Modifiers::NONE);
    assert_eq!(
        one(&r, "shape.add")["roundness"],
        50.0,
        "half the short side"
    );
    h.drag_to((100.0, 50.0), (300.0, 150.0), Modifiers::NONE);
    h.key(Key::ArrowLeft);
    let r = h.release((300.0, 150.0), Modifiers::NONE);
    assert_eq!(one(&r, "shape.add")["roundness"], 0.0);

    // Star: drag distance = outer radius, inner = half; a point at the cursor;
    // Up adds a point, Right raises the outer roundness, Page Up the inner one.
    h.tools().lock().set(Tool::Star);
    h.drag_to((320.0, 180.0), (320.0, 80.0), Modifiers::NONE);
    h.key(Key::ArrowUp);
    h.key(Key::ArrowRight);
    h.key(Key::PageUp);
    let r = h.release((320.0, 80.0), Modifiers::NONE);
    let p = one(&r, "shape.add");
    assert_eq!(p["kind"], "star");
    assert!((p["outer_radius"].as_f64().unwrap() - 100.0).abs() < 0.01);
    assert!((p["inner_radius"].as_f64().unwrap() - 50.0).abs() < 0.01);
    assert!(p["rotation"].as_f64().unwrap().abs() < 0.01, "pointing up");
    assert_eq!(p["points"], 6.0);
    assert_eq!(p["outer_roundness"], 5.0);
    assert_eq!(p["inner_roundness"], 5.0);
    assert_eq!(h.tools().lock().options.points, 6.0);
    h.apply(&r);
    // Polygon: Down removes a point (never below 3).
    h.tools().lock().set(Tool::Polygon);
    h.drag_to((320.0, 180.0), (400.0, 180.0), Modifiers::NONE);
    for _ in 0..5 {
        h.key(Key::ArrowDown);
    }
    let r = h.release((400.0, 180.0), Modifiers::NONE);
    let p = one(&r, "shape.add");
    assert_eq!(p["kind"], "polygon");
    assert_eq!(p["points"], 3.0);
    assert!((p["rotation"].as_f64().unwrap() - 90.0).abs() < 0.01, "{p}");
}

#[test]
fn bezier_path_option_and_alt_make_paths() {
    let mut h = H::new();
    h.tools().lock().options.bezier_path = true;
    let r = h.draw(Tool::Ellipse, (100.0, 50.0), (300.0, 150.0));
    assert_eq!(one(&r, "shape.add")["bezier"], true);
    h.apply(&r);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    let group = h.contents(&layer)[0].clone();
    let path = h.contents(&group)[0].clone();
    assert_eq!(h.name(&path), "Path 1");
    assert!(matches!(h.value(&path, "path"), Some(Value::Bezier(_))));
    // Alt/Option held when the drag starts does the same.
    h.tools().lock().options.bezier_path = false;
    h.tools().lock().set(Tool::Rectangle);
    let a = h.screen(100.0, 50.0);
    h.move_to(a, Modifiers::ALT, Vec::new());
    h.pointer_button(a, true, Modifiers::ALT);
    for i in 1..=6 {
        h.move_to(a + vec2(20.0, 10.0) * i as f32, Modifiers::ALT, Vec::new());
    }
    let r = h.release((220.0, 110.0), Modifiers::NONE);
    assert_eq!(one(&r, "shape.add")["bezier"], true);
}

#[test]
fn double_clicking_a_shape_tool_fills_the_comp_or_the_layer() {
    let mut h = H::new();
    let fill = |h: &mut H, tool: Tool, shift: bool| {
        h.tools().lock().fill_request = Some(FillRequest { tool, shift });
        h.frame(Vec::new(), Modifiers::NONE)
    };
    // No selection: a comp-sized rectangle in a new shape layer.
    let r = fill(&mut h, Tool::Rectangle, false);
    let p = one(&r, "shape.add");
    assert_eq!(p["size"], json!([640.0, 360.0]));
    assert_eq!(p["position"], json!([0.0, 0.0]));
    h.apply(&r);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&layer)
    );
    // That layer selected, Shift: the largest circle that fits, added to it.
    let r = fill(&mut h, Tool::Ellipse, true);
    let p = one(&r, "shape.add");
    assert_eq!(p["layer"], json!(layer));
    assert_eq!(p["size"], json!([360.0, 360.0]));
    // A solid: a mask the size of the layer.
    let solid = h.layer("solid");
    h.select(&[&solid]);
    let r = fill(&mut h, Tool::Rectangle, false);
    let p = one(&r, "mask.create");
    assert_eq!(p["size"], json!([640.0, 360.0]));
    assert_eq!(p["position"], json!([320.0, 180.0]));
    // It also resets the tool's remembered roundness.
    h.tools().lock().options.roundness = 33.0;
    let r = fill(&mut h, Tool::RoundedRectangle, false);
    assert_eq!(one(&r, "mask.create")["roundness"], 10.0);
    assert_eq!(h.tools().lock().options.roundness, 10.0);
}

// --- Pen tools -------------------------------------------------------------

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

#[test]
fn pen_clicks_draw_an_open_path_in_a_new_shape_layer() {
    let mut h = H::new();
    h.tools().lock().set(Tool::Pen);
    // First click: a new shape layer with Shape 1 > Path 1 (one vertex).
    let r = h.click(100.0, 100.0, Modifiers::NONE);
    let p = one(&r, "shape.add");
    assert_eq!(p["kind"], "path");
    assert_eq!(p["names"]["group"], "Shape");
    h.apply(&r);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    assert_eq!(h.name(&layer), "Shape Layer 1");
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&layer)
    );
    let group = h.contents(&layer)[0].clone();
    assert_eq!(h.name(&group), "Shape 1");
    let path = h.contents(&group)[0].clone();
    assert_eq!(h.name(&path), "Path 1");
    // Layer space: the comp center is the origin.
    assert!(near(h.bezier(&path).vertices[0], [-220.0, -80.0]));
    // Each further click is one path.set_vertices.
    let r = h.click(300.0, 100.0, Modifiers::NONE);
    assert_eq!(one(&r, "path.set_vertices")["path"], json!(path));
    h.apply(&r);
    // Click-drag: a smooth vertex, handles along the drag (in = mirror).
    let r = h.click_drag((300.0, 250.0), (40.0, 0.0), Modifiers::NONE);
    one(&r, "path.set_vertices");
    h.apply(&r);
    let b = h.bezier(&path);
    assert_eq!(b.vertices.len(), 3);
    assert!(!b.closed);
    assert!(near(b.out_tangents[2], [40.0, 0.0]) && near(b.in_tangents[2], [-40.0, 0.0]));
    assert!(near(b.out_tangents[1], [0.0, 0.0]), "a click is a corner");
    // Undo takes back the last vertex; the Pen follows.
    h.stage.undo(&h.author).unwrap();
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(h.bezier(&path).vertices.len(), 2);
    let r = h.click(200.0, 300.0, Modifiers::NONE);
    h.apply(&r);
    assert_eq!(h.bezier(&path).vertices.len(), 3);
    // Switching to the Selection tool (V) leaves the path open; the next Pen
    // click starts a new path (Shape 2 in the selected layer).
    h.tools().lock().set(Tool::Selection);
    h.frame(Vec::new(), Modifiers::NONE);
    assert!(h.viewer.pen.drawing.is_none());
    h.tools().lock().set(Tool::Pen);
    let r = h.click(500.0, 300.0, Modifiers::NONE);
    assert_eq!(one(&r, "shape.add")["layer"], json!(layer));
    h.apply(&r);
    assert_eq!(h.name(&h.contents(&layer)[0]), "Shape 2");
    assert!(!h.bezier(&path).closed);
}

#[test]
fn pen_closes_a_mask_on_its_first_vertex() {
    let mut h = H::new();
    let solid = h.layer("solid");
    h.select(&[&solid]);
    h.tools().lock().set(Tool::Pen);
    let r = h.click(100.0, 100.0, Modifiers::NONE);
    let p = one(&r, "mask.create");
    assert_eq!(p["kind"], "path");
    h.apply(&r);
    for (x, y) in [(300.0, 100.0), (300.0, 250.0)] {
        let r = h.click(x, y, Modifiers::NONE);
        h.apply(&r);
    }
    let mask = h.children(&solid.child("masks").unwrap())[0].clone();
    assert_eq!(h.name(&mask), "Mask 1");
    assert!(!h.bezier(&mask).closed);
    // Back on the first vertex: closed, and the Pen starts over.
    let r = h.click(101.0, 101.0, Modifiers::NONE);
    one(&r, "path.set_vertices");
    h.apply(&r);
    let b = h.bezier(&mask);
    assert!(b.closed);
    assert_eq!(b.vertices, [[100.0, 100.0], [300.0, 100.0], [300.0, 250.0]]);
    assert!(h.viewer.pen.drawing.is_none());
    // Shift: the segment from the last vertex snaps to 45°.
    let r = h.click(400.0, 50.0, Modifiers::NONE);
    h.apply(&r);
    let r = h.click(500.0, 140.0, Modifiers::SHIFT);
    h.apply(&r);
    let masks = h.children(&solid.child("masks").unwrap());
    assert_eq!(masks.len(), 2);
    let b = h.bezier(&masks[1]);
    let d = [
        b.vertices[1][0] - b.vertices[0][0],
        b.vertices[1][1] - b.vertices[0][1],
    ];
    assert!((d[0] - d[1]).abs() < 0.01, "45°: {d:?}");
}

#[test]
fn vertex_tools_and_the_pen_over_paths() {
    let mut h = H::new();
    let solid = h.layer("solid");
    h.run(
        "mask.create",
        json!({"layer": solid, "kind": "rect", "position": [200.0, 150.0], "size": [200.0, 100.0]}),
    );
    let mask = h.children(&solid.child("masks").unwrap())[0].clone();
    h.select(&[&solid]);
    // Add Vertex: click the top edge (AE order: top right first, clockwise; the
    // top edge is the closing segment 3 → 0).
    h.tools().lock().set(Tool::AddVertex);
    let r = h.click(200.0, 100.0, Modifiers::NONE);
    assert_eq!(one(&r, "path.insert_vertex")["segment"], 3);
    h.apply(&r);
    let b = h.bezier(&mask);
    assert_eq!(b.vertices.len(), 5);
    assert!(near(b.vertices[4], [200.0, 100.0]));
    // Delete Vertex: click it again.
    h.tools().lock().set(Tool::DeleteVertex);
    let r = h.click(200.0, 100.0, Modifiers::NONE);
    assert_eq!(one(&r, "path.delete_vertex")["index"], 4);
    h.apply(&r);
    assert_eq!(h.bezier(&mask).vertices.len(), 4);
    // Convert Vertex: a click on a corner makes it smooth, again a corner; a
    // drag pulls out handles.
    h.tools().lock().set(Tool::ConvertVertex);
    let r = h.click(300.0, 100.0, Modifiers::NONE);
    one(&r, "path.set_vertices");
    h.apply(&r);
    assert!(!bae_motion::path::is_corner(&h.bezier(&mask), 0));
    let r = h.click(300.0, 100.0, Modifiers::NONE);
    h.apply(&r);
    assert!(bae_motion::path::is_corner(&h.bezier(&mask), 0));
    let r = h.click_drag((300.0, 200.0), (0.0, 30.0), Modifiers::NONE);
    h.apply(&r);
    let b = h.bezier(&mask);
    assert!(near(b.out_tangents[1], [0.0, 30.0]) && near(b.in_tangents[1], [0.0, -30.0]));
    // The Pen over a segment adds, over a vertex deletes, Alt over a vertex
    // converts.
    h.tools().lock().set(Tool::Pen);
    // (The bottom edge is curved now; the left edge is straight.)
    let r = h.click(100.0, 150.0, Modifiers::NONE);
    assert_eq!(one(&r, "path.insert_vertex")["segment"], 2);
    h.apply(&r);
    assert_eq!(h.bezier(&mask).vertices.len(), 5);
    let r = h.click(100.0, 100.0, Modifiers::ALT);
    one(&r, "path.set_vertices");
    h.apply(&r);
    let r = h.click(100.0, 100.0, Modifiers::NONE);
    one(&r, "path.delete_vertex");
    h.apply(&r);
    assert_eq!(h.bezier(&mask).vertices.len(), 4);
    assert!(h.viewer.pen.drawing.is_none(), "no new path was started");
}

#[test]
fn rotobezier_pen_paths_get_automatic_tangents() {
    let mut h = H::new();
    h.tools().lock().options.roto_bezier = true;
    h.tools().lock().set(Tool::Pen);
    for (x, y) in [(100.0, 100.0), (200.0, 50.0), (300.0, 100.0)] {
        let r = h.click(x, y, Modifiers::NONE);
        h.apply(&r);
    }
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    let group = h.contents(&layer)[0].clone();
    let path = h.contents(&group)[0].clone();
    let b = h.bezier(&path);
    assert_eq!(b.vertices.len(), 3);
    assert!(bae_motion::path::is_corner(&b, 0) && bae_motion::path::is_corner(&b, 2));
    // The middle vertex is smooth, parallel to the line through its neighbours.
    assert!(
        near(b.out_tangents[1], [200.0 / 6.0, 0.0]),
        "{:?}",
        b.out_tangents[1]
    );
}
