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
        // BAE ships no bundled fonts (#149): without the system UI fonts text has no
        // glyphs and text fields lose their selection.
        let ctx = egui::Context::default();
        bae_ui::theme::install_fonts(&ctx);
        Harness {
            stage: Stage::new(),
            reg,
            author: AuthorId::new("user:test").unwrap(),
            resources,
            ctx,
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

/// Issue #114: two New Composition (or New Solid) in one frame, before either
/// command ran, get Comp 1 / Comp 2 and Solid 1 / Solid 2, and both are created.
#[test]
fn default_names_stay_unique_within_a_frame() {
    let mut h = Harness::new();
    let mut viewer = CompositionViewer::new(None);
    // Two actions in one frame (nothing applied in between), then applied.
    fn twice(h: &mut Harness, viewer: &mut CompositionViewer, inv: Invocation) -> Vec<String> {
        let a = h.action(viewer, inv.clone());
        let b = h.action(viewer, inv);
        let out = [&a, &b]
            .iter()
            .map(|r| r.invocations[0].params["name"].as_str().unwrap().to_owned())
            .collect();
        h.apply(a);
        h.apply(b);
        out
    }
    let comps = twice(
        &mut h,
        &mut viewer,
        Invocation::new(crate::actions::ids::NEW_COMP),
    );
    assert_eq!(comps, ["Comp 1", "Comp 2"]);
    h.frame("composition", &mut viewer, Vec::new());
    assert_eq!(query::comps(&h.stage).len(), 2);
    let solid =
        || Invocation::with_params(crate::actions::ids::NEW_LAYER, json!({"kind": "solid"}));
    let solids = twice(&mut h, &mut viewer, solid());
    assert_eq!(solids, ["Solid 1", "Solid 2"]);
    h.frame("composition", &mut viewer, Vec::new());
    // Reservations end once the items exist: the next one continues the count.
    let next = h.action(&mut viewer, solid());
    assert_eq!(next.invocations[0].params["name"], "Solid 3");
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

/// The viewer's GPU path: 04's renderer draws the comp, the 8-bit texture is
/// published, and the viewer gets the egui handle the shell set.
#[test]
fn gpu_preview_publishes_the_rendered_comp() {
    use bae_gpu::viewport::ViewportTextureRegistry;
    use bae_gpu::{GpuContext, GpuContextDescriptor, wgpu};
    let gpu = match GpuContext::new_headless(GpuContextDescriptor::default()) {
        Ok(gpu) => gpu,
        Err(e) => {
            eprintln!("no GPU, skipping: {e}");
            return;
        }
    };
    eprintln!(
        "adapter: {} ({:?})",
        gpu.adapter_info().name,
        gpu.adapter_info().device_type
    );
    let mut h = Harness::new();
    let (comp, _solid, _shape) = h.comp_with_layers();
    let registry = std::sync::Arc::new(ViewportTextureRegistry::new());
    let mut preview = crate::gpu_preview::GpuPreview::with(gpu, registry.clone());
    let evaluated = bae_anim::evaluate_comp(&h.stage, &comp, Time::ZERO).unwrap();
    // No display handle until the shell registers the texture.
    assert!(
        preview
            .frame(&h.resources, &h.stage, &evaluated, 1)
            .is_none()
    );
    let published = registry.get(preview.viewport()).expect("published");
    assert_eq!(published.size, [640, 360]);
    assert_eq!(published.format, wgpu::TextureFormat::Rgba8Unorm);
    registry.set_display_handle(preview.viewport(), Some(7));
    let img = preview
        .frame(&h.resources, &h.stage, &evaluated, 1)
        .expect("image");
    assert_eq!(img.handle, 7);
    assert_eq!(
        (img.rect.x1 - img.rect.x0, img.rect.y1 - img.rect.y0),
        (640, 360)
    );
    // Half resolution publishes a half-size texture.
    preview.frame(&h.resources, &h.stage, &evaluated, 2);
    assert_eq!(registry.get(preview.viewport()).unwrap().size, [320, 180]);
    // The pixels: the full-comp red solid shows where the star is not.
    let mut anim = bae_render::AnimEvaluator::new(None);
    let rc = anim.convert(&h.stage, &evaluated, 0).unwrap();
    let frame = preview
        .renderer()
        .render_frame(&rc, &bae_render::RenderRequest::full())
        .unwrap();
    let pixels = preview
        .renderer()
        .read_rgba8(&frame, bae_render::OutputAlpha::Premultiplied)
        .unwrap();
    let px = pixels.pixel(10, 10);
    assert!(
        px[0] >= 250 && px[1] <= 3 && px[2] <= 3 && px[3] >= 250,
        "{px:?}"
    );
    let id = preview.viewport();
    drop(preview);
    assert!(registry.get(id).is_none(), "removed on drop");
}

/// Menus: AE structure, enabled only when they can act, same invocation as the
/// shortcut; the global handler runs them with no motion panel focused.
#[test]
fn menus_and_global_handler() {
    use bae_ui::menu::{MenuContext, MenuEntry, build_menus};
    use bae_ui::{ActionHandler, MenuItemSpec};
    let mut b = AppBuilder::new();
    b.add_module(bae_ui::UiModule)
        .add_module(crate::MotionUiModule);
    let mut app = b.build();
    let tree = build_menus(&app.registry::<MenuItemSpec>().snapshot(), "motion");
    assert!(tree.conflicts.is_empty(), "{:?}", tree.conflicts);
    let tops: Vec<&str> = tree.menus.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(
        tops,
        ["Edit", "Composition", "Layer", "Animation", "Window"]
    );
    let layer = &tree.menus[2].1;
    let MenuEntry::Submenu { title, entries } = &layer[0] else {
        panic!("Layer ▸ New");
    };
    assert_eq!(title, "New");
    let names: Vec<String> = entries
        .iter()
        .map(|e| match e {
            MenuEntry::Item { spec, .. } => spec.title.clone(),
            MenuEntry::Separator => "---".into(),
            MenuEntry::Submenu { title, .. } => title.clone(),
        })
        .collect();
    assert_eq!(
        names,
        [
            "Text",
            "Solid",
            "Light",
            "Camera",
            "Null Object",
            "Shape Layer",
            "Adjustment Layer",
            "---",
            "Rectangle",
            "Ellipse"
        ]
    );
    // Menu items show the shortcut bound to the same invocation.
    let mut keymap = bae_ui::presets::ae_keymap();
    keymap.bindings.extend(crate::actions::bindings().bindings);
    let shortcut = |id: &str| {
        let (_, spec) = tree.items().into_iter().find(|(i, _)| *i == id).unwrap();
        keymap
            .chord_for_invocation(&spec.invocation())
            .map(|c| c.to_string())
    };
    assert_eq!(shortcut("motion.composition.new").as_deref(), Some("Cmd+N"));
    assert_eq!(shortcut("motion.layer.new.solid").as_deref(), Some("Cmd+Y"));
    assert_eq!(
        shortcut("motion.layer.new.null").as_deref(),
        Some("Cmd+Alt+Shift+Y")
    );
    assert_eq!(shortcut("motion.animation.ease").as_deref(), Some("F9"));
    assert_eq!(shortcut("motion.layer.new.rectangle"), None);

    // Enabled state follows the session.
    let mut h = Harness::new();
    let solid_item = tree
        .items()
        .into_iter()
        .find(|(i, _)| *i == "motion.layer.new.solid")
        .unwrap()
        .1
        .clone();
    let new_comp = tree
        .items()
        .into_iter()
        .find(|(i, _)| *i == "motion.composition.new")
        .unwrap()
        .1
        .clone();
    {
        let cx = MenuContext {
            stage: &h.stage,
            resources: &h.resources,
            workspace: "motion",
        };
        assert!(!solid_item.is_enabled(&cx), "no comp yet");
        assert!(new_comp.is_enabled(&cx));
    }
    // The handler runs a menu item with a non-motion panel focused.
    let handler = crate::menus::MotionActions;
    assert!(handler.actions().iter().any(|a| a == "motion.new_layer"));
    let run = |h: &mut Harness, inv: Invocation| {
        let mut requests = PanelRequests::default();
        let id = PanelId::new("history");
        let mut cx = PanelContext::new(
            &id,
            "History",
            &h.stage,
            &h.author,
            &h.resources,
            &mut requests,
        );
        assert!(handler.handle(&inv, &mut cx));
        h.apply(requests);
        h.frame("history", &mut NoopPanel, Vec::new());
    };
    run(&mut h, new_comp.invocation());
    let comp = h.session().lock().active_comp.clone().expect("comp opened");
    {
        let cx = MenuContext {
            stage: &h.stage,
            resources: &h.resources,
            workspace: "motion",
        };
        assert!(solid_item.is_enabled(&cx));
    }
    run(&mut h, solid_item.invocation());
    assert_eq!(query::comp_layers(&h.stage, &comp).len(), 1);
    let _ = &mut app;
}

struct NoopPanel;

impl Panel for NoopPanel {
    fn ui(&mut self, _ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
        // Like every motion panel: resolve pending selections once per frame.
        crate::common::session(cx.resources).lock().sync(cx.stage);
    }
}

#[test]
fn every_menu_title_has_chinese() {
    let zh = crate::i18n::ZH;
    let mut missing = Vec::new();
    for (id, spec) in crate::menus::items() {
        for t in spec.path.iter().chain(std::iter::once(&spec.title)) {
            if !zh.iter().any(|(en, _)| en == t) {
                missing.push(format!("{id}: {t}"));
            }
        }
    }
    assert!(missing.is_empty(), "missing Chinese for {missing:?}");
}

fn key(k: egui::Key) -> Event {
    Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }
}

fn renamed_to(r: &PanelRequests) -> Option<String> {
    r.invocations
        .iter()
        .find(|i| i.id == "attr.set" && i.params["attr"] == "name")
        .and_then(|i| i.params["value"]["value"].as_str().map(str::to_owned))
}

/// AE: Enter commits a rename, clicking elsewhere commits, Esc cancels.
#[test]
fn timeline_and_project_rename_commit_on_enter_and_click_away() {
    let mut h = Harness::new();
    let (comp, solid, _shape) = h.comp_with_layers();
    let mut tl = TimelinePanel::default();
    // Enter commits; the field opens with the whole name selected (AE), so
    // typing replaces it.
    tl.start_rename(solid.clone(), "Bg");
    h.frame("timeline", &mut tl, Vec::new());
    h.frame("timeline", &mut tl, vec![Event::Text("X".into())]);
    let r = h.frame("timeline", &mut tl, vec![key(egui::Key::Enter)]);
    assert_eq!(renamed_to(&r).as_deref(), Some("X"), "{:?}", r.invocations);
    assert!(
        h.frame("timeline", &mut tl, Vec::new())
            .invocations
            .is_empty(),
        "rename ended"
    );
    // Clicking elsewhere commits.
    tl.start_rename(solid.clone(), "Away");
    h.frame("timeline", &mut tl, Vec::new());
    h.frame("timeline", &mut tl, Vec::new());
    let far = pos2(1200.0, 780.0);
    let mut evs = Vec::new();
    pointer(&mut evs, far, Some(true));
    h.frame("timeline", &mut tl, evs);
    let mut evs = Vec::new();
    pointer(&mut evs, far, Some(false));
    let r = h.frame("timeline", &mut tl, evs);
    let r2 = h.frame("timeline", &mut tl, Vec::new());
    let name = renamed_to(&r).or_else(|| renamed_to(&r2));
    assert_eq!(name.as_deref(), Some("Away"));
    // Escape cancels.
    tl.start_rename(solid.clone(), "Nope");
    h.frame("timeline", &mut tl, Vec::new());
    h.frame("timeline", &mut tl, Vec::new());
    let r = h.frame("timeline", &mut tl, vec![key(egui::Key::Escape)]);
    assert_eq!(renamed_to(&r), None);
    assert!(
        h.frame("timeline", &mut tl, vec![key(egui::Key::Enter)])
            .invocations
            .is_empty()
    );

    // Project panel: Enter commits once and does not reopen the field.
    let mut project = ProjectPanel::default();
    h.session().lock().selected_items = vec![comp.clone()];
    project.start_rename(comp.clone(), "Main Comp");
    h.frame("project", &mut project, Vec::new());
    let r = h.frame("project", &mut project, vec![key(egui::Key::Enter)]);
    assert_eq!(renamed_to(&r).as_deref(), Some("Main Comp"));
    assert!(
        h.frame("project", &mut project, vec![Event::Text("z".into())])
            .invocations
            .is_empty()
    );
    let r = h.frame("project", &mut project, vec![key(egui::Key::Enter)]);
    assert_eq!(
        renamed_to(&r),
        None,
        "Enter after a commit starts a new rename, it does not commit one"
    );
}

/// Composition ▸ Add to Render Queue (Cmd/Ctrl+M) queues the active comp with
/// its work area and opens the Render Queue panel; the panel draws it.
#[test]
fn add_to_render_queue_action() {
    let mut h = Harness::new();
    h.resources
        .insert(crate::render_queue::SharedRenderQueue::default());
    let (comp, _, _) = h.comp_with_layers();
    let mut tl = TimelinePanel::default();
    let r = h.action(
        &mut tl,
        Invocation::new(crate::actions::ids::ADD_TO_RENDER_QUEUE),
    );
    assert_eq!(r.open, vec![PanelId::new("render_queue")]);
    let queue = h
        .resources
        .get::<crate::render_queue::SharedRenderQueue>()
        .unwrap()
        .clone();
    {
        let q = queue.lock();
        assert_eq!(q.items().len(), 1);
        assert_eq!(q.items()[0].comp, comp);
        assert_eq!(q.items()[0].status, crate::render_queue::ItemStatus::Queued);
    }
    let mut panel = crate::RenderQueuePanel::default();
    assert!(h.frame("render_queue", &mut panel, Vec::new()).is_empty());
    // The menu item and its AE shortcut.
    assert!(
        crate::menus::items()
            .iter()
            .any(|(id, item)| *id == "motion.composition.render_queue"
                && item.action == crate::actions::ids::ADD_TO_RENDER_QUEUE)
    );
    assert!(
        crate::actions::bindings()
            .bindings
            .iter()
            .any(|b| b.chord == bae_ui::Chord::parse("Cmd+M").unwrap()
                && b.action == crate::actions::ids::ADD_TO_RENDER_QUEUE)
    );
}

/// The Render Queue's Render button works from the keyboard: Tab focuses it,
/// then Space or Enter starts the render (the workbench leaves those keys to a
/// focused widget; see bae-ui's `focused_widgets_get_their_keys_before_shortcuts`).
#[test]
fn render_button_activates_with_space_and_enter() {
    let Ok(gpu) = bae_gpu::GpuContext::new_headless(bae_gpu::GpuContextDescriptor::default())
    else {
        eprintln!("no GPU, skipping");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    for (n, activate) in [egui::Key::Space, egui::Key::Enter].into_iter().enumerate() {
        let mut h = Harness::new();
        h.resources.insert(gpu.clone());
        let queue = crate::render_queue::SharedRenderQueue::default();
        h.resources.insert(queue.clone());
        let (comp, _, _) = h.comp_with_layers();
        let info = query::comp_info(&h.stage, &comp).unwrap();
        {
            let mut q = queue.lock();
            let id = q.add(&info, dir.path());
            let item = q.item_mut(id).unwrap();
            item.output = dir
                .path()
                .join(format!("k{n}/f_####.png"))
                .to_string_lossy()
                .into_owned();
            (item.start, item.end) = (0, 1);
        }
        let mut panel = crate::RenderQueuePanel::default();
        let key = |key: egui::Key| Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        h.frame("render_queue", &mut panel, Vec::new());
        h.frame("render_queue", &mut panel, vec![key(egui::Key::Tab)]);
        h.frame("render_queue", &mut panel, Vec::new());
        assert!(
            h.ctx.memory(|m| m.focused().is_some()),
            "Tab focuses Render"
        );
        h.frame("render_queue", &mut panel, vec![key(activate)]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let status = queue.lock().items()[0].status.clone();
            match status {
                crate::render_queue::ItemStatus::Done { files, .. } => {
                    assert_eq!(files, 2, "{activate:?}");
                    break;
                }
                crate::render_queue::ItemStatus::Failed(e) => panic!("{activate:?}: {e}"),
                crate::render_queue::ItemStatus::Queued if !queue.lock().is_running() => {
                    panic!("{activate:?} did not press Render")
                }
                _ => {}
            }
            assert!(std::time::Instant::now() < deadline, "render timed out");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

/// The 5090 keyboard run: Enter on a focused button (Render Queue ▸ Stop) is
/// that button's; the Project panel holding the dock focus must not also start
/// renaming its selected item. Without a focused widget, Enter renames (AE).
#[test]
fn enter_on_a_focused_button_does_not_rename_in_the_project_panel() {
    let mut h = Harness::new();
    let (comp, _, _) = h.comp_with_layers();
    h.session().lock().selected_items = vec![comp.clone()];
    let enter = || Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    // One frame: a button (focused on request) next to the Project panel.
    let frame = |h: &Harness, project: &mut ProjectPanel, focus: bool, events: Vec<Event>| {
        let mut presses = 0;
        let mut requests = PanelRequests::default();
        let id = PanelId::new("project");
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
            events,
            ..Default::default()
        };
        let mut out = h.ctx.run_ui(raw, |ui| {
            let stop = ui.button("Stop");
            if focus {
                stop.request_focus();
            }
            if stop.clicked() {
                presses += 1;
            }
            let mut cx = PanelContext::new(
                &id,
                "Project",
                &h.stage,
                &h.author,
                &h.resources,
                &mut requests,
            );
            cx.focused = true;
            project.ui(ui, &mut cx);
        });
        out.textures_delta.clear();
        presses
    };

    let mut project = ProjectPanel::default();
    frame(&h, &mut project, true, Vec::new());
    frame(&h, &mut project, false, Vec::new());
    assert!(h.ctx.memory(|m| m.focused().is_some()));
    assert_eq!(
        frame(&h, &mut project, false, vec![enter()]),
        1,
        "Stop pressed"
    );
    assert!(!project.is_renaming(), "the Enter was the button's");

    // Nothing focused: Enter renames the selected item.
    h.ctx.memory_mut(|m| m.stop_text_input());
    frame(&h, &mut project, false, Vec::new());
    assert!(h.ctx.memory(|m| m.focused().is_none()));
    assert_eq!(frame(&h, &mut project, false, vec![enter()]), 0);
    assert!(project.is_renaming());
}

/// Issue #124: a locked layer cannot be picked in the composition viewer (AE:
/// clicks go through it) nor dragged.
#[test]
fn viewer_does_not_pick_locked_layers() {
    let mut h = Harness::new();
    let (_comp, solid, _shape) = h.comp_with_layers();
    h.run(
        "attr.set",
        json!({"path": solid, "attr": "locked", "value": {"type": "bool", "value": true}}),
    );
    h.session().lock().selected_layers.clear();
    let mut viewer = CompositionViewer::new(None);
    h.frame("composition", &mut viewer, Vec::new());
    // The solid covers the comp; away from the star nothing else is there.
    let at = pos2(640.0, 380.0) + vec2(-120.0, -60.0);
    let mut evs = Vec::new();
    pointer(&mut evs, at, None);
    h.frame("composition", &mut viewer, evs);
    let mut evs = Vec::new();
    pointer(&mut evs, at, Some(true));
    h.frame("composition", &mut viewer, evs);
    let mut evs = Vec::new();
    pointer(&mut evs, at, Some(false));
    let r = h.frame("composition", &mut viewer, evs);
    assert!(
        !h.session().lock().selected_layers.contains(&solid),
        "clicking selected the locked layer"
    );
    // Dragging over it moves nothing.
    let mut evs = Vec::new();
    pointer(&mut evs, at, Some(true));
    h.frame("composition", &mut viewer, evs);
    for i in 1..=4 {
        let mut evs = Vec::new();
        pointer(&mut evs, at + vec2(10.0 * i as f32, 0.0), None);
        h.frame("composition", &mut viewer, evs);
    }
    let mut evs = Vec::new();
    pointer(&mut evs, at + vec2(50.0, 0.0), Some(false));
    let drag = h.frame("composition", &mut viewer, evs);
    assert!(
        r.invocations.is_empty() && drag.invocations.is_empty(),
        "{:?} {:?}",
        r.invocations,
        drag.invocations
    );
}

/// Accessibility labels the motion panels add (issues #116, #128) have Chinese.
#[test]
fn accessibility_labels_have_chinese() {
    let labels = [
        "Video",
        "Audio",
        "Solo",
        "Lock",
        "Shy",
        "Collapse Transformations",
        "Quality and Sampling",
        "Frame Blending",
        "Motion Blur",
        "Adjustment Layer",
        "3D Layer",
        "Switch",
        "Time Graph",
        "Current Time",
        "Work Area",
        "Time Ruler",
        "Parent Pick Whip",
        "Parent & Link",
        "Duration Bar",
        "Keyframe",
        "Enable Expression",
        "Revolutions",
        "Degrees",
        "Project Items",
    ];
    let zh = crate::i18n::ZH;
    let missing: Vec<_> = labels
        .iter()
        .filter(|l| !zh.iter().any(|(en, _)| en == *l) && !bae_ui::i18n::has_translation(l))
        .collect();
    assert!(missing.is_empty(), "no Chinese for {missing:?}");
}
