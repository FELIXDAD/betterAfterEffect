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
            ctx: bae_ui::testing::ctx_with_fonts(),
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
    assert!(params.get("name").is_none(), "the command names it");
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

    // Several layers selected: a new shape layer above the topmost (AE 26).
    h.select(&[&solid, &shape]);
    let r = h.draw(Tool::Rectangle, (100.0, 50.0), (300.0, 150.0));
    let params = one(&r, "shape.add");
    assert!(params.get("layer").is_none(), "{params}");
    let order = query::comp_layers(&h.stage, &h.comp);
    let top = order
        .iter()
        .position(|l| *l == solid || *l == shape)
        .unwrap();
    assert_eq!(params["index"], top);
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
    assert_eq!(p["outer_roundness"], 10.0);
    assert_eq!(p["inner_roundness"], 10.0);
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
    // An open mask is mode None until it is closed (AE 26).
    assert_eq!(h.value(&mask, "mode"), Some(Value::Enum("none".into())));
    // Back on the first vertex: closed, and the Pen starts over.
    let r = h.click(101.0, 101.0, Modifiers::NONE);
    one(&r, "path.set_vertices");
    h.apply(&r);
    let b = h.bezier(&mask);
    assert!(b.closed);
    assert_eq!(b.vertices, [[100.0, 100.0], [300.0, 100.0], [300.0, 250.0]]);
    assert_eq!(h.value(&mask, "mode"), Some(Value::Enum("add".into())));
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
    // Convert Vertex: a click on a corner does nothing; a drag pulls out
    // symmetric handles; a click on a smooth vertex makes it a corner (AE 26).
    h.tools().lock().set(Tool::ConvertVertex);
    let r = h.click(300.0, 100.0, Modifiers::NONE);
    assert!(r.invocations.is_empty(), "{:?}", r.invocations);
    let r = h.click_drag((300.0, 100.0), (30.0, 0.0), Modifiers::NONE);
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
    // Alt over the smooth vertex (300, 200): Convert Vertex makes it a corner.
    let r = h.click(300.0, 200.0, Modifiers::ALT);
    one(&r, "path.set_vertices");
    h.apply(&r);
    assert!(bae_motion::path::is_corner(&h.bezier(&mask), 1));
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
    // The middle vertex is smooth, along the line through its neighbours, each
    // handle a third of its own segment (AE 26).
    let third = 100f64.hypot(50.0) / 3.0;
    assert!(
        near(b.out_tangents[1], [third, 0.0]),
        "{:?}",
        b.out_tangents[1]
    );
    assert!(
        near(b.in_tangents[1], [-third, 0.0]),
        "{:?}",
        b.in_tangents[1]
    );
}

#[test]
fn command_makes_the_pen_the_selection_tool() {
    let mut h = H::new();
    let solid = h.layer("solid");
    h.tools().lock().set(Tool::Pen);
    // Cmd/Ctrl-click selects the layer instead of starting a path.
    let r = h.click(320.0, 180.0, Modifiers::COMMAND);
    assert!(r.invocations.is_empty(), "{:?}", r.invocations);
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&solid)
    );
    assert!(h.viewer.pen.drawing.is_none());
    // An open mask path ends when Cmd-clicking away deselects the layer.
    let r = h.click(100.0, 100.0, Modifiers::NONE);
    h.apply(&r);
    let r = h.click(200.0, 100.0, Modifiers::NONE);
    h.apply(&r);
    assert!(h.viewer.pen.drawing.is_some());
    h.session().lock().selected_layers.clear();
    h.frame(Vec::new(), Modifiers::NONE);
    assert!(h.viewer.pen.drawing.is_none());
    assert_eq!(
        h.tools().lock().current(),
        Tool::Pen,
        "the tool stays the Pen"
    );
}

// --- Selection, Rotation and Pan Behind tools --------------------------------

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

#[test]
fn moving_layers_snaps_to_other_layers_and_the_comp() {
    let mut h = H::new();
    // B's right edge midpoint is at (150, 180).
    let b = h.solid([100.0, 100.0], [100.0, 180.0]);
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.tools().lock().set(Tool::Selection);
    // Grabbed near its left edge midpoint (220, 180) but away from the scale
    // handle there, A moves by −67.
    let grab = (228.0, 180.0);
    let to = (161.0, 180.0);
    // Snapping off: the layer follows the pointer.
    let r = h.drag(grab, to, Modifiers::NONE);
    one(&r, "attr.set_many");
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [253.0, 180.0, 0.0]));
    assert_eq!(h.session().lock().selected_layers, std::slice::from_ref(&a));
    h.stage.undo(&h.author).unwrap();
    // On: the grabbed feature (left midpoint, 153) snaps to B's right midpoint.
    h.tools().lock().options.snapping = true;
    let r = h.drag(grab, to, Modifiers::NONE);
    h.apply(&r);
    assert!(
        close3(h.v3(&a, "transform:position"), [250.0, 180.0, 0.0]),
        "{:?}",
        h.v3(&a, "transform:position")
    );
    h.stage.undo(&h.author).unwrap();
    // Cmd/Ctrl inverts the switch while held.
    let r = h.drag(grab, to, Modifiers::COMMAND);
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [253.0, 180.0, 0.0]));
    h.stage.undo(&h.author).unwrap();
    // The comp frame is not a target (AE 26): 5 px from the left edge stays.
    let r = h.drag(grab, (13.0, 180.0), Modifiers::NONE);
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [105.0, 180.0, 0.0]));
    let _ = b;
}

#[test]
fn bounding_box_handles_scale_around_the_anchor() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.select(&[&a]);
    h.tools().lock().set(Tool::Selection);
    // Bottom-right corner (420, 230) out by (100, 50): twice the size.
    let r = h.drag((420.0, 230.0), (520.0, 280.0), Modifiers::NONE);
    one(&r, "attr.set_many");
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:scale"), [200.0, 200.0, 100.0]));
    assert!(
        close3(h.v3(&a, "transform:position"), [320.0, 180.0, 0.0]),
        "not moved"
    );
    h.stage.undo(&h.author).unwrap();
    // Shift keeps the proportions.
    let r = h.drag((420.0, 230.0), (520.0, 240.0), Modifiers::SHIFT);
    h.apply(&r);
    let s = h.v3(&a, "transform:scale");
    assert!(
        (s[0] - 184.0).abs() < 0.01 && (s[0] - s[1]).abs() < 1e-9,
        "{s:?}"
    );
    h.stage.undo(&h.author).unwrap();
    // The right edge handle scales X only.
    let r = h.drag((420.0, 180.0), (470.0, 180.0), Modifiers::NONE);
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:scale"), [150.0, 100.0, 100.0]));
    // One undo step.
    h.stage.undo(&h.author).unwrap();
    assert!(close3(h.v3(&a, "transform:scale"), [100.0, 100.0, 100.0]));
}

#[test]
fn marquee_selects_the_layers_it_touches() {
    let mut h = H::new();
    let b = h.solid([100.0, 100.0], [100.0, 100.0]);
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.tools().lock().set(Tool::Selection);
    h.drag_to((10.0, 10.0), (160.0, 160.0), Modifiers::NONE);
    assert_eq!(h.session().lock().selected_layers, std::slice::from_ref(&b));
    h.move_to(h.screen(240.0, 140.0), Modifiers::NONE, Vec::new());
    assert_eq!(h.session().lock().selected_layers, [a.clone(), b.clone()]);
    let r = h.release((240.0, 140.0), Modifiers::NONE);
    assert!(r.invocations.is_empty());
    assert_eq!(h.session().lock().selected_layers.len(), 2);
    // Shift adds to the selection; a plain marquee in empty space replaces it.
    h.select(&[&a]);
    h.drag((10.0, 10.0), (60.0, 60.0), Modifiers::SHIFT);
    assert_eq!(h.session().lock().selected_layers, [a.clone(), b.clone()]);
    h.drag((600.0, 10.0), (630.0, 20.0), Modifiers::NONE);
    assert!(h.session().lock().selected_layers.is_empty());
}

#[test]
fn rotation_tool_rotates_around_the_anchor_shift_in_45_degree_steps() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.tools().lock().set(Tool::Rotation);
    // A quarter turn clockwise around the anchor (320, 180).
    let r = h.drag((400.0, 180.0), (320.0, 260.0), Modifiers::NONE);
    one(&r, "attr.set_many");
    h.apply(&r);
    assert_eq!(h.session().lock().selected_layers, std::slice::from_ref(&a));
    assert!((float(h.value(&a, "transform:rotation")) - 90.0).abs() < 0.01);
    h.stage.undo(&h.author).unwrap();
    let r = h.drag((400.0, 180.0), (330.0, 260.0), Modifiers::SHIFT);
    h.apply(&r);
    assert!((float(h.value(&a, "transform:rotation")) - 90.0).abs() < 1e-9);
    h.stage.undo(&h.author).unwrap();
    let r = h.drag((400.0, 180.0), (390.0, 230.0), Modifiers::SHIFT);
    h.apply(&r);
    assert!((float(h.value(&a, "transform:rotation")) - 45.0).abs() < 1e-9);
}

#[test]
fn pan_behind_moves_the_anchor_or_pans_the_content() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.select(&[&a]);
    h.tools().lock().set(Tool::PanBehind);
    // Dragging the anchor point: it moves, the layer stays where it is.
    let r = h.drag((320.0, 180.0), (370.0, 200.0), Modifiers::NONE);
    one(&r, "attr.set_many");
    h.apply(&r);
    assert!(close3(
        h.v3(&a, "transform:anchor_point"),
        [150.0, 70.0, 0.0]
    ));
    assert!(close3(h.v3(&a, "transform:position"), [370.0, 200.0, 0.0]));
    h.stage.undo(&h.author).unwrap();
    // Dragging elsewhere on the layer: the content pans behind the anchor.
    let r = h.drag((250.0, 150.0), (300.0, 170.0), Modifiers::NONE);
    h.apply(&r);
    assert!(close3(
        h.v3(&a, "transform:anchor_point"),
        [50.0, 30.0, 0.0]
    ));
    assert!(close3(h.v3(&a, "transform:position"), [320.0, 180.0, 0.0]));
    // Cmd/Ctrl double-click on the tool button: anchor back to the center.
    h.tools().lock().center_anchor_request = true;
    let r = h.frame(Vec::new(), Modifiers::NONE);
    h.apply(&r);
    assert!(close3(
        h.v3(&a, "transform:anchor_point"),
        [100.0, 50.0, 0.0]
    ));
    // The content moved by (50, 20): its center is at (370, 200) in the comp.
    assert!(close3(h.v3(&a, "transform:position"), [370.0, 200.0, 0.0]));
}

#[test]
fn arrow_keys_nudge_by_screen_pixels() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.select(&[&a]);
    let r = h.action_with(crate::tools::ids::NUDGE, json!({"dx": 1, "dy": 0}));
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [321.0, 180.0, 0.0]));
    let r = h.action_with(crate::tools::ids::NUDGE, json!({"dx": 0, "dy": -10}));
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [321.0, 170.0, 0.0]));
    // At 200 % one screen pixel is half a comp pixel.
    h.viewer.zoom = Some(2.0);
    h.frame(Vec::new(), Modifiers::NONE);
    let r = h.action_with(crate::tools::ids::NUDGE, json!({"dx": -1, "dy": 0}));
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [320.5, 170.0, 0.0]));
    // From another panel the tool handler does the same at the viewer's
    // magnification.
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
        use bae_ui::ActionHandler as _;
        let inv = Invocation::with_params(crate::tools::ids::NUDGE, json!({"dx": 0, "dy": 1}));
        assert!(crate::tools::ToolActions.handle(&inv, &mut cx));
    }
    h.apply(&requests);
    assert!(close3(h.v3(&a, "transform:position"), [320.5, 170.5, 0.0]));
}

#[test]
fn selection_tool_edits_mask_vertices() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.run(
        "mask.create",
        json!({"layer": a, "kind": "rect", "position": [100.0, 50.0], "size": [100.0, 50.0]}),
    );
    let mask = h.children(&a.child("masks").unwrap())[0].clone();
    h.select(&[&a]);
    h.tools().lock().set(Tool::Selection);
    // Vertex 0 (top right, layer (150, 25)) is at comp (370, 155).
    let r = h.click(370.0, 155.0, Modifiers::NONE);
    assert!(r.invocations.is_empty());
    assert_eq!(h.viewer.select.vertices.indices, [0]);
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&a),
        "layer stays selected"
    );
    let r = h.drag((370.0, 155.0), (390.0, 165.0), Modifiers::NONE);
    one(&r, "path.set_vertices");
    h.apply(&r);
    let b = h.bezier(&mask);
    assert!(near(b.vertices[0], [170.0, 35.0]), "{:?}", b.vertices);
    assert!(
        close3(h.v3(&a, "transform:position"), [320.0, 180.0, 0.0]),
        "layer not moved"
    );
    // Arrow keys move the selected vertex, Delete removes it.
    let r = h.action_with(crate::tools::ids::NUDGE, json!({"dx": 0, "dy": 1}));
    h.apply(&r);
    assert!(near(h.bezier(&mask).vertices[0], [170.0, 36.0]));
    let r = h.action_with(crate::actions::ids::DELETE, json!({}));
    one(&r, "path.set_vertices");
    h.apply(&r);
    assert_eq!(h.bezier(&mask).vertices.len(), 3);
    assert!(h.viewer.select.vertices.is_empty());
    // Clicking empty space deselects the layer (and its vertices).
    h.click(600.0, 20.0, Modifiers::NONE);
    assert!(h.session().lock().selected_layers.is_empty());
}

// --- Type tool ------------------------------------------------------------------

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

    fn doc(&self, layer: &PrimPath) -> bae_model::TextDocument {
        match self.value(layer, "text:source") {
            Some(Value::TextDocument(d)) => *d,
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn type_tool_click_and_type_makes_point_text() {
    let mut h = H::new();
    h.tools().lock().set(Tool::TypeHorizontal);
    let r = h.click(100.0, 200.0, Modifiers::NONE);
    assert!(r.invocations.is_empty());
    assert!(h.viewer.type_tool.editing.is_some());
    let r = h.type_text(vec![Event::Text("Hi".into())]);
    assert!(r.invocations.is_empty(), "nothing until committed");
    // Typing keys go to the field, not to tool shortcuts (V, H …).
    h.type_text(vec![Event::Text("V".into())]);
    assert_eq!(h.tools().lock().current(), Tool::TypeHorizontal);
    let r = h.escape();
    let p = one(&r, "text.create");
    assert_eq!(p["text"], "HiV");
    assert_eq!(p["position"], json!([100.0, 200.0]));
    assert!(p.get("box_size").is_none());
    h.apply(&r);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    assert_eq!(h.name(&layer), "HiV");
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&layer)
    );
    assert_eq!(h.doc(&layer).text, "HiV");
    assert!(close3(
        h.v3(&layer, "transform:position"),
        [100.0, 200.0, 0.0]
    ));
    assert!(close3(
        h.v3(&layer, "transform:anchor_point"),
        [0.0, 0.0, 0.0]
    ));
    // One undo step.
    h.stage.undo(&h.author).unwrap();
    assert!(query::comp_layers(&h.stage, &h.comp).is_empty());
    // Committing without typing still makes an (empty) text layer (AE 26).
    h.click(300.0, 200.0, Modifiers::NONE);
    let r = h.escape();
    let p = one(&r, "text.create");
    assert_eq!(p["text"], "");
    assert_eq!(p["name"], "<empty text layer>");
    // While editing, clicking elsewhere in the viewer keeps editing; Cmd/Ctrl+
    // Enter commits.
    h.click(100.0, 300.0, Modifiers::NONE);
    h.type_text(vec![Event::Text("A".into())]);
    let r = h.click(500.0, 100.0, Modifiers::NONE);
    assert!(r.invocations.is_empty(), "{:?}", r.invocations);
    assert!(h.viewer.type_tool.editing.is_some());
    h.type_text(vec![Event::Text("B".into())]);
    let enter = |pressed| Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    };
    let r = h.frame(vec![enter(true)], Modifiers::COMMAND);
    h.frame(vec![enter(false)], Modifiers::COMMAND);
    assert_eq!(one(&r, "text.create")["text"], "AB");
}

#[test]
fn type_tool_takes_input_method_composition() {
    let mut h = H::new();
    h.tools().lock().set(Tool::TypeHorizontal);
    h.click(100.0, 200.0, Modifiers::NONE);
    use egui::ImeEvent;
    let preedit = |t: &str| {
        Event::Ime(ImeEvent::Preedit {
            text: t.into(),
            active_range_chars: None,
        })
    };
    h.type_text(vec![preedit("ni")]);
    h.type_text(vec![preedit("ni'hao")]);
    assert_eq!(h.tools().lock().current(), Tool::TypeHorizontal);
    h.type_text(vec![
        preedit(""),
        Event::Ime(ImeEvent::Commit("你好".into())),
    ]);
    let r = h.escape();
    assert_eq!(one(&r, "text.create")["text"], "你好");
}

#[test]
fn type_tool_drag_makes_paragraph_text() {
    let mut h = H::new();
    h.tools().lock().set(Tool::TypeHorizontal);
    let r = h.drag((100.0, 50.0), (300.0, 150.0), Modifiers::NONE);
    assert!(r.invocations.is_empty());
    h.type_text(vec![Event::Text("Box".into())]);
    // Switching tools commits too.
    h.tools().lock().set(Tool::Selection);
    let r = h.frame(Vec::new(), Modifiers::NONE);
    let p = one(&r, "text.create");
    assert_eq!(p["box_size"], json!([200.0, 100.0]));
    assert_eq!(p["position"], json!([100.0, 50.0]));
    h.apply(&r);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    let d = h.doc(&layer);
    assert_eq!(d.box_size, Some([200.0, 100.0]));
    // AE 26: the layer sits at the box's center, the box offset by half.
    assert_eq!(d.box_position, Some([-100.0, -50.0]));
    assert!(close3(
        h.v3(&layer, "transform:position"),
        [200.0, 100.0, 0.0]
    ));
}

#[test]
fn type_tool_edits_an_existing_text_layer() {
    let mut h = H::new();
    let layer = path(
        &h.run(
            "text.create",
            json!({"comp": h.comp, "text": "Hello", "position": [100.0, 200.0]}),
        )["path"],
    );
    h.tools().lock().set(Tool::TypeHorizontal);
    // A click at the start of the text (its first line sits above the
    // baseline): the caret goes there (AE).
    h.click(101.0, 180.0, Modifiers::NONE);
    assert_eq!(
        h.viewer.type_tool.edited_layer(),
        Some(&layer),
        "editing the layer"
    );
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&layer)
    );
    h.type_text(vec![Event::Text("!".into())]);
    let r = h.escape();
    let p = one(&r, "text.set_text");
    assert_eq!(p["layer"], json!(layer));
    let text = p["text"].as_str().unwrap().to_owned();
    assert_eq!(text, "!Hello");
    h.apply(&r);
    assert_eq!(h.doc(&layer).text, text);
    assert_eq!(h.doc(&layer).font_size, 72.0, "style kept");
    h.stage.undo(&h.author).unwrap();
    assert_eq!(h.doc(&layer).text, "Hello");
    // Past the end of the text: the caret goes to the end.
    h.click(295.0, 180.0, Modifiers::NONE);
    h.type_text(vec![Event::Text("?".into())]);
    let r = h.escape();
    assert_eq!(one(&r, "text.set_text")["text"], "Hello?");
    // Layer > New > Text (E) asks the viewer to edit a layer.
    h.tools().lock().set(Tool::Selection);
    h.tools().lock().edit_text_request = Some(layer.clone());
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(h.tools().lock().current(), Tool::TypeHorizontal);
    assert_eq!(h.viewer.type_tool.edited_layer(), Some(&layer));
}

#[test]
fn free_transform_points_moves_and_scales_a_mask() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.run(
        "mask.create",
        json!({"layer": a, "kind": "rect", "position": [100.0, 50.0], "size": [100.0, 50.0]}),
    );
    let mask = h.children(&a.child("masks").unwrap())[0].clone();
    h.select(&[&a]);
    h.tools().lock().set(Tool::Selection);
    // Double-click the top edge of the mask (comp y 155).
    h.click(320.0, 155.0, Modifiers::NONE);
    h.click(320.0, 155.0, Modifiers::NONE);
    assert!(h.viewer.select.free.is_some(), "free transform box");
    // Drag inside the box: every vertex moves; the layer does not.
    let r = h.drag((320.0, 180.0), (340.0, 190.0), Modifiers::NONE);
    one(&r, "path.set_vertices");
    h.apply(&r);
    let b = h.bezier(&mask);
    assert!(
        near(b.vertices[0], [170.0, 35.0]) && near(b.vertices[2], [70.0, 85.0]),
        "{:?}",
        b.vertices
    );
    assert!(close3(h.v3(&a, "transform:position"), [320.0, 180.0, 0.0]));
    h.stage.undo(&h.author).unwrap();
    h.frame(Vec::new(), Modifiers::NONE);
    // The bottom-right handle (comp (370, 205)) out by (50, 25): twice the size
    // around the box's center.
    let r = h.drag((370.0, 205.0), (420.0, 230.0), Modifiers::NONE);
    h.apply(&r);
    let b = h.bezier(&mask);
    assert!(
        near(b.vertices[0], [200.0, 0.0]) && near(b.vertices[2], [0.0, 100.0]),
        "{:?}",
        b.vertices
    );
    // Esc ends it; the menu command starts it again on the selected layer's mask.
    h.key(Key::Escape);
    assert!(h.viewer.select.free.is_none());
    let mut requests = PanelRequests::default();
    {
        let pid = PanelId::new("timeline");
        let mut cx = PanelContext::new(
            &pid,
            "Timeline",
            &h.stage,
            &h.author,
            &h.resources,
            &mut requests,
        );
        use bae_ui::ActionHandler as _;
        let inv = Invocation::new(crate::tools::ids::FREE_TRANSFORM);
        assert!(crate::tools::ToolActions.handle(&inv, &mut cx));
    }
    h.frame(Vec::new(), Modifiers::NONE);
    assert!(h.viewer.select.free.is_some());
}

/// Frame cost of the tool code on benchmark scene B1 (61 layers): idle, hover
/// (hit testing and the hover outline) and a move drag (live preview). Run with
/// `cargo test -p bae-ui-motion b1_tool_frames -- --ignored --nocapture`.
#[test]
#[ignore = "timing report, not a check"]
fn b1_tool_frames() {
    let dir = tempfile::tempdir().unwrap();
    let (b, comp) = bae_render::fixture::b1(dir.path()).unwrap();
    let mut h = H::new();
    h.stage = b.into_stage();
    h.comp = comp.clone();
    h.session().lock().open_comp(&comp);
    h.tools().lock().set(Tool::Selection);
    h.frame(Vec::new(), Modifiers::NONE);
    let layers = query::comp_layers(&h.stage, &comp);
    println!("B1: {} layers", layers.len());
    let time = |h: &mut H, f: &mut dyn FnMut(&mut H, usize)| {
        let t0 = std::time::Instant::now();
        for i in 0..60 {
            f(h, i);
        }
        t0.elapsed().as_secs_f64() * 1000.0 / 60.0
    };
    let idle = time(&mut h, &mut |h, _| {
        h.frame(Vec::new(), Modifiers::NONE);
    });
    let hover = time(&mut h, &mut |h, i| {
        let p = h.screen(200.0 + 20.0 * i as f64, 540.0);
        h.move_to(p, Modifiers::NONE, Vec::new());
    });
    h.select(&[&layers[0]]);
    let a = h.screen(960.0, 540.0);
    h.move_to(a, Modifiers::NONE, Vec::new());
    h.pointer_button(a, true, Modifiers::NONE);
    let drag = time(&mut h, &mut |h, i| {
        let p = a + vec2(3.0 * i as f32, 1.0 * i as f32);
        h.move_to(p, Modifiers::NONE, Vec::new());
    });
    println!("ms/frame (debug build): idle {idle:.2}, hover {hover:.2}, drag {drag:.2}");
}

#[test]
fn mouse_wheel_zooms_around_the_pointer_and_trackpads_pan() {
    let mut h = H::new();
    let at = h.screen(100.0, 100.0);
    let wheel = |unit, d: egui::Vec2, m: Modifiers| Event::MouseWheel {
        unit,
        delta: d,
        phase: egui::TouchPhase::Move,
        modifiers: m,
    };
    let before = h.viewer.last_view.unwrap().scale;
    h.move_to(
        at,
        Modifiers::NONE,
        vec![wheel(
            egui::MouseWheelUnit::Line,
            vec2(0.0, 1.0),
            Modifiers::NONE,
        )],
    );
    h.frame(Vec::new(), Modifiers::NONE);
    let v = h.viewer.last_view.unwrap();
    assert!(
        (v.scale / before - 1.125).abs() < 1e-3,
        "{} → {}",
        before,
        v.scale
    );
    // The comp point under the pointer stays there.
    let p = v.to_screen(DVec2::new(100.0, 100.0));
    assert!(p.distance(at) < 0.5, "{p:?} {at:?}");
    // A trackpad scroll pans, the magnification stays.
    let o = v.origin;
    h.move_to(
        at,
        Modifiers::NONE,
        vec![wheel(
            egui::MouseWheelUnit::Point,
            vec2(10.0, -20.0),
            Modifiers::NONE,
        )],
    );
    h.frame(Vec::new(), Modifiers::NONE);
    let v2 = h.viewer.last_view.unwrap();
    assert!((v2.scale - v.scale).abs() < 1e-6);
    assert!(
        (v2.origin - o - vec2(10.0, -20.0)).length() < 0.5,
        "{:?}",
        v2.origin - o
    );
}

#[test]
fn tools_follow_the_preferences() {
    let mut h = H::new();
    let mut p = bae_prefs::Preferences::default();
    p.set(bae_prefs::keys::CENTER_SHAPE_ANCHOR, true);
    p.set("appearance.cycle_mask_colors", false);
    p.set(bae_prefs::keys::PATH_POINT_SIZE, 9);
    h.resources
        .insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(p)));
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(
        h.viewer.last_view.unwrap().point,
        9.0,
        "point and handle size"
    );
    // Composition > Center Anchor Point in New Shape Layers.
    let r = h.draw(Tool::Rectangle, (100.0, 50.0), (300.0, 150.0));
    assert_eq!(one(&r, "shape.add")["center_anchor"], true);
    h.apply(&r);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    assert!(close3(
        h.v3(&layer, "transform:position"),
        [200.0, 100.0, 0.0]
    ));
    // Appearance > Cycle Mask Colors off.
    let solid = h.layer("solid");
    h.select(&[&solid]);
    let r = h.draw(Tool::Ellipse, (100.0, 50.0), (300.0, 150.0));
    assert_eq!(one(&r, "mask.create")["cycle_colors"], false);
}

// --- Resolution ------------------------------------------------------------------

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

#[test]
fn the_viewer_renders_at_the_comps_resolution_or_auto() {
    let mut h = H::new();
    assert_eq!(h.viewer.render_factor, 1);
    // View > Resolution > Half (Cmd/Ctrl+Shift+J): the comp's resolution, which
    // Composition Settings shows too.
    let r = tool_action(&h, crate::tools::ids::RESOLUTION, json!({"factor": 2}));
    let p = one(&r, "comp.set_settings");
    assert_eq!(p["resolution"], json!([2, 2]));
    h.apply(&r);
    assert_eq!(
        h.value(&h.comp, "resolution"),
        Some(Value::Vec2([2.0, 2.0]))
    );
    assert_eq!(h.viewer.render_factor, 2);
    // Composition Settings changing it: the viewer follows.
    h.run(
        "comp.set_settings",
        json!({"comp": h.comp, "resolution": [3, 3]}),
    );
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(h.viewer.render_factor, 3);
    // One undo step.
    h.stage.undo(&h.author).unwrap();
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(h.viewer.render_factor, 2);
    // Auto: the step the magnification hides.
    h.session().lock().auto_resolution = true;
    for (zoom, factor) in [(1.0, 1), (0.5, 2), (0.333, 3), (0.25, 4), (2.0, 1)] {
        h.viewer.zoom = Some(zoom);
        h.frame(Vec::new(), Modifiers::NONE);
        assert_eq!(h.viewer.render_factor, factor, "{zoom}");
    }
    // Choosing a resolution turns Auto off.
    let r = tool_action(&h, crate::tools::ids::RESOLUTION, json!({"factor": 1}));
    h.apply(&r);
    assert!(!h.session().lock().auto_resolution);
    assert_eq!(h.viewer.render_factor, 1);
}

#[test]
fn resolution_recurse_sets_nested_comps_and_ae_shortcuts_are_bound() {
    let mut h = H::new();
    let inner = path(&h.run("comp.create", json!({"width": 100, "height": 100}))["path"]);
    h.run(
        "layer.create",
        json!({"comp": h.comp, "kind": "pre_comp", "source": inner}),
    );
    let r = tool_action(
        &h,
        crate::tools::ids::RESOLUTION,
        json!({"factor": 4, "nested": true}),
    );
    assert_eq!(r.invocations.len(), 2, "{:?}", r.invocations);
    h.apply(&r);
    assert_eq!(h.value(&inner, "resolution"), Some(Value::Vec2([4.0, 4.0])));
    // AE's shortcuts bind once the action has a handler (Cmd/Ctrl+J, +Shift, …).
    use bae_ui::ActionHandler as _;
    let handled: std::collections::HashSet<String> =
        crate::tools::ToolActions.actions().into_iter().collect();
    let late = crate::actions::late_bindings(&handled, &[]);
    let half = late
        .iter()
        .find(|b| b.action == crate::tools::ids::RESOLUTION && b.params == json!({"factor": 2}))
        .expect("Half bound");
    assert_eq!(half.chord.to_string(), "Cmd+Shift+J");
}
