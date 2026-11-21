//! Headless tests: panels run in an egui context without a window; the requests they
//! make are executed through the command registry the way the shell does.

use bae_app::{AppBuilder, Resources};
use bae_model::{AuthorId, CommandRegistry, DynCommand, PrimPath, Stage, Time, Value};
use bae_motion::query;
use bae_ui::{Invocation, Panel, PanelContext, PanelId, PanelKind, PanelRequests};
use egui::{Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use serde_json::json;

use crate::session::SharedSession;
use crate::{CompositionViewer, ProjectPanel, TimelinePanel};

struct Harness {
    stage: Stage,
    reg: CommandRegistry,
    author: AuthorId,
    resources: Resources,
    ctx: egui::Context,
}

impl Harness {
    fn new() -> Self {
        let mut reg = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        let mut resources = Resources::new();
        resources.insert(SharedSession::default());
        Harness {
            stage: Stage::new(),
            reg,
            author: AuthorId::new("user:test").unwrap(),
            resources,
            ctx: egui::Context::default(),
        }
    }

    fn session(&self) -> SharedSession {
        self.resources.get::<SharedSession>().unwrap().clone()
    }

    fn exec(&mut self, inv: &Invocation) {
        let params = if inv.params.is_null() {
            json!({})
        } else {
            inv.params.clone()
        };
        self.reg
            .execute(&mut self.stage, &self.author, &inv.id, params)
            .unwrap_or_else(|e| panic!("{}: {e}", inv.id));
    }

    fn run(&mut self, id: &str, params: serde_json::Value) -> serde_json::Value {
        self.reg
            .execute(&mut self.stage, &self.author, id, params)
            .unwrap()
            .output
    }

    /// One frame of `panel`; returns its requests.
    fn frame(&self, kind: &str, panel: &mut dyn Panel, events: Vec<Event>) -> PanelRequests {
        let mut requests = PanelRequests::default();
        let id = PanelId::new(kind);
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
            events,
            ..Default::default()
        };
        let stage = &self.stage;
        let author = &self.author;
        let resources = &self.resources;
        let mut out = self.ctx.run_ui(raw, |ui| {
            let mut cx = PanelContext::new(&id, kind, stage, author, resources, &mut requests);
            cx.focused = true;
            panel.ui(ui, &mut cx);
        });
        out.textures_delta.clear();
        requests
    }

    fn action(&self, panel: &mut dyn Panel, inv: Invocation) -> PanelRequests {
        let mut requests = PanelRequests::default();
        let id = PanelId::new("timeline");
        let mut cx = PanelContext::new(
            &id,
            "Timeline",
            &self.stage,
            &self.author,
            &self.resources,
            &mut requests,
        );
        cx.focused = true;
        assert!(panel.on_action(&inv, &mut cx), "{} not handled", inv.id);
        requests
    }

    fn apply(&mut self, requests: PanelRequests) {
        for inv in &requests.invocations {
            self.exec(inv);
        }
    }

    fn comp_with_layers(&mut self) -> (PrimPath, PrimPath, PrimPath) {
        let comp = PrimPath::new(
            self.run("comp.create", json!({"width": 640, "height": 360}))["path"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let solid = PrimPath::new(
            self.run(
                "layer.create",
                json!({"comp": comp, "kind": "solid", "color": [1.0, 0.0, 0.0, 1.0]}),
            )["path"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let shape = PrimPath::new(
            self.run(
                "layer.create",
                json!({"comp": comp, "kind": "shape", "shape": "star", "size": [100, 100]}),
            )["path"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        self.session().lock().open_comp(&comp);
        (comp, solid, shape)
    }
}

fn pointer(events: &mut Vec<Event>, pos: Pos2, pressed: Option<bool>) {
    events.push(Event::PointerMoved(pos));
    if let Some(p) = pressed {
        events.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: p,
            modifiers: Default::default(),
        });
    }
}

#[test]
fn module_registers_panels_shortcuts_and_commands() {
    let mut b = AppBuilder::new();
    b.add_module(bae_ui::UiModule)
        .add_module(crate::MotionUiModule);
    let mut app = b.build();
    let kinds = app.registry::<PanelKind>();
    for id in [
        "project",
        "composition",
        "timeline",
        "properties",
        "effect_controls",
        "preview",
    ] {
        let entry = kinds.snapshot();
        let e = entry.entry(id).unwrap_or_else(|| panic!("{id}"));
        assert_eq!(e.owner(), &bae_app::Owner::module("bae-ui-motion"), "{id}");
    }
    assert!(app.registry::<bae_ui::KeymapBindings>().contains("motion"));
    assert!(app.registry::<dyn DynCommand>().contains("layer.create"));
    assert!(app.resource::<SharedSession>().is_some());
}

#[test]
fn new_comp_and_layers_are_selected_once_created() {
    let mut h = Harness::new();
    let mut viewer = CompositionViewer::new(None);
    // Empty viewer offers "New Composition"; the action does the same.
    let r = h.action(&mut viewer, Invocation::new(crate::actions::ids::NEW_COMP));
    assert_eq!(r.invocations[0].id, "comp.create");
    h.apply(r);
    h.frame("composition", &mut viewer, Vec::new());
    let comp = h.session().lock().active_comp.clone().expect("opened");
    assert_eq!(query::comp_info(&h.stage, &comp).unwrap().name, "Comp 1");

    let r = h.action(
        &mut viewer,
        Invocation::with_params(
            crate::actions::ids::NEW_LAYER,
            json!({"kind": "shape", "shape": "rect"}),
        ),
    );
    h.apply(r);
    h.frame("composition", &mut viewer, Vec::new());
    let sel = h.session().lock().selected_layers.clone();
    assert_eq!(sel.len(), 1);
    assert_eq!(query::display_name_of(&h.stage, &sel[0]), "Shape Layer 1");
}

#[test]
fn viewer_drag_moves_the_layer_in_one_step() {
    let mut h = Harness::new();
    let (comp, solid, _shape) = h.comp_with_layers();
    let mut viewer = CompositionViewer::new(None);
    h.frame("composition", &mut viewer, Vec::new());
    // The solid covers the comp; the comp is centered in the viewer area.
    let center = pos2(640.0, 380.0);
    let mut evs = Vec::new();
    pointer(&mut evs, center + vec2(-120.0, -60.0), None);
    h.frame("composition", &mut viewer, evs);
    let mut evs = Vec::new();
    pointer(&mut evs, center + vec2(-120.0, -60.0), Some(true));
    h.frame("composition", &mut viewer, evs);
    for i in 1..=5 {
        let mut evs = Vec::new();
        pointer(
            &mut evs,
            center + vec2(-120.0 + 10.0 * i as f32, -60.0),
            None,
        );
        let r = h.frame("composition", &mut viewer, evs);
        assert!(r.invocations.is_empty(), "no command while dragging");
    }
    let mut evs = Vec::new();
    pointer(&mut evs, center + vec2(-70.0, -60.0), Some(false));
    let r = h.frame("composition", &mut viewer, evs);
    assert_eq!(h.session().lock().selected_layers.len(), 1);
    let moved = h.session().lock().selected_layers[0].clone();
    assert_eq!(r.invocations.len(), 1, "{:?}", r.invocations);
    assert_eq!(r.invocations[0].id, "attr.set_many");
    let before = query::static_value(&h.stage.prim(&moved).unwrap(), "transform:position");
    h.apply(r);
    let after = query::static_value(&h.stage.prim(&moved).unwrap(), "transform:position");
    let (Some(Value::Vec3(b)), Some(Value::Vec3(a))) = (before, after) else {
        panic!("positions");
    };
    assert!(
        a[0] > b[0] + 1.0 && (a[1] - b[1]).abs() < 1e-6,
        "{b:?} → {a:?}"
    );
    let _ = (comp, solid);
}

#[test]
fn shortcuts_turn_selection_into_commands() {
    let mut h = Harness::new();
    let (comp, solid, shape) = h.comp_with_layers();
    let mut tl = TimelinePanel::default();
    let session = h.session();
    // Stopwatch on position at 0, then a value at 1 s → two keys.
    let info = query::comp_info(&h.stage, &comp).unwrap();
    h.run(
        "attr.set_animated",
        json!({"path": shape, "attr": "transform:position", "animated": true, "time": 0}),
    );
    session.lock().set_time(&info, Time::ONE_SECOND);
    h.run("attr.set_at", json!({"path": shape, "attr": "transform:position", "time": Time::ONE_SECOND, "value": {"type": "vec3", "value": [10.0, 10.0, 0.0]}}));
    // J / K jump between keyframes.
    session.lock().selected_layers = vec![shape.clone()];
    h.action(&mut tl, Invocation::new(crate::actions::ids::PREV_KEY));
    assert_eq!(session.lock().time(&comp), Time::ZERO);
    h.action(&mut tl, Invocation::new(crate::actions::ids::NEXT_KEY));
    assert_eq!(session.lock().time(&comp), Time::ONE_SECOND);
    // F9 on the selected keys.
    session.lock().selected_keys = vec![
        bae_motion::KeyRef::new(shape.clone(), "transform:position", Time::ZERO),
        bae_motion::KeyRef::new(shape.clone(), "transform:position", Time::ONE_SECOND),
    ];
    let r = h.action(
        &mut tl,
        Invocation::with_params(crate::actions::ids::EASE, json!({"mode": "easy_ease"})),
    );
    assert_eq!(r.invocations[0].id, "key.ease");
    h.apply(r);
    let spec = query::attr_spec(&h.stage, &shape, "transform:position").unwrap();
    assert!(
        spec.keyframes
            .values()
            .all(|k| k.interp_out == bae_model::Interpolation::Bezier)
    );
    // Copy / paste keys onto the solid at the current time.
    h.action(&mut tl, Invocation::new("edit.copy"));
    session.lock().selected_layers = vec![solid.clone()];
    session.lock().selected_keys.clear();
    session.lock().set_time(&info, Time::from_seconds(2.0));
    let r = h.action(&mut tl, Invocation::new("edit.paste"));
    h.apply(r);
    let pasted = query::attr_spec(&h.stage, &solid, "transform:position").unwrap();
    assert_eq!(pasted.keyframes.len(), 2);
    // Delete with keys selected deletes keys; with layers selected, layers.
    session.lock().selected_keys = vec![bae_motion::KeyRef::new(
        shape.clone(),
        "transform:position",
        Time::ZERO,
    )];
    let r = h.action(&mut tl, Invocation::new(crate::actions::ids::DELETE));
    assert_eq!(r.invocations[0].id, "key.delete");
    h.apply(r);
    session.lock().selected_layers = vec![solid.clone()];
    let r = h.action(&mut tl, Invocation::new(crate::actions::ids::DELETE));
    assert_eq!(r.invocations[0].id, "layer.delete");
    h.apply(r);
    assert!(!h.stage.has_prim(&solid));
    // Space plays, Space stops.
    h.action(&mut tl, Invocation::new(crate::actions::ids::PLAY));
    assert!(session.lock().is_playing());
    h.action(&mut tl, Invocation::new(crate::actions::ids::PLAY));
    assert!(!session.lock().is_playing());
    // P reveals position; P again hides it.
    session.lock().selected_layers = vec![shape.clone()];
    h.action(
        &mut tl,
        Invocation::with_params(
            crate::actions::ids::REVEAL,
            json!({"what": "transform:position"}),
        ),
    );
    assert!(session.lock().reveal.contains_key(&shape));
    h.action(
        &mut tl,
        Invocation::with_params(
            crate::actions::ids::REVEAL,
            json!({"what": "transform:position"}),
        ),
    );
    assert!(!session.lock().reveal.contains_key(&shape));
}

#[test]
fn panels_render_a_project() {
    let mut h = Harness::new();
    let (_comp, _solid, shape) = h.comp_with_layers();
    h.run(
        "attr.set_animated",
        json!({"path": shape, "attr": "transform:scale", "animated": true, "time": 0}),
    );
    h.run(
        "attr.set_expression",
        json!({"path": shape, "attr": "transform:rotation", "source": "time * 10"}),
    );
    h.session()
        .lock()
        .reveal
        .insert(shape.clone(), crate::session::Reveal::Animated);
    let mut tl = TimelinePanel::default();
    let mut project = ProjectPanel::default();
    let mut viewer = CompositionViewer::new(None);
    let mut props = crate::PropertiesPanel::default();
    let mut fx = crate::EffectControlsPanel::default();
    let mut preview = crate::PreviewPanel;
    h.session().lock().selected_layers = vec![shape.clone()];
    for _ in 0..2 {
        assert!(
            h.frame("timeline", &mut tl, Vec::new())
                .invocations
                .is_empty()
        );
        assert!(
            h.frame("project", &mut project, Vec::new())
                .invocations
                .is_empty()
        );
        assert!(
            h.frame("composition", &mut viewer, Vec::new())
                .invocations
                .is_empty()
        );
        assert!(
            h.frame("properties", &mut props, Vec::new())
                .invocations
                .is_empty()
        );
        assert!(
            h.frame("effect_controls", &mut fx, Vec::new())
                .invocations
                .is_empty()
        );
        assert!(
            h.frame("preview", &mut preview, Vec::new())
                .invocations
                .is_empty()
        );
    }
}

#[test]
fn every_english_ui_string_has_chinese() {
    // The language is process-global, so check the table instead of switching it.
    let zh = crate::i18n::ZH;
    for (_, en) in crate::actions::ACTIONS {
        assert!(zh.iter().any(|(e, _)| e == en), "missing zh for {en}");
    }
    assert!(zh.contains(&("Easy Ease", "缓动")));
    let mut seen = std::collections::HashSet::new();
    for (en, _) in zh {
        assert!(seen.insert(en), "duplicate {en}");
    }
}

#[test]
fn demo_project_builds_and_renders() {
    let mut h = Harness::new();
    for inv in crate::demo::steps() {
        h.exec(&inv);
    }
    crate::demo::open(&h.resources, &h.stage);
    let comp = h.session().lock().active_comp.clone().unwrap();
    assert_eq!(query::comp_layers(&h.stage, &comp).len(), 4);
    let e = bae_anim::evaluate_comp(&h.stage, &comp, Time::from_seconds(1.5)).unwrap();
    assert!(e.errors.is_empty(), "{:?}", e.errors);
    let mut tl = TimelinePanel::default();
    let mut viewer = CompositionViewer::new(None);
    h.frame("timeline", &mut tl, Vec::new());
    h.frame("composition", &mut viewer, Vec::new());
}
