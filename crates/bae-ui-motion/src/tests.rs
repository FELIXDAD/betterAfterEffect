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
        resources.insert(crate::SharedDialogs::default());
        Harness {
            stage: Stage::new(),
            reg,
            author: AuthorId::new("user:test").unwrap(),
            resources,
            ctx: bae_ui::testing::ctx_with_fonts(),
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

    fn dialogs(&self) -> crate::SharedDialogs {
        self.resources
            .get::<crate::SharedDialogs>()
            .unwrap()
            .clone()
    }

    /// One frame of the motion dialogs (as the workbench draws them after the
    /// dock); returns their requests.
    fn dialog_frame(&self, events: Vec<Event>) -> PanelRequests {
        use bae_ui::DialogHost as _;
        let mut requests = PanelRequests::default();
        let id = PanelId::new("dialog");
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
            events,
            ..Default::default()
        };
        let mut out = self.ctx.run_ui(raw, |ui| {
            let mut cx = PanelContext::new(
                &id,
                "",
                &self.stage,
                &self.author,
                &self.resources,
                &mut requests,
            );
            crate::MotionDialogs.ui(ui.ctx(), &mut cx);
        });
        out.textures_delta.clear();
        requests
    }

    /// Composition Settings is open: press `k` (Enter = OK, Escape = Cancel)
    /// and return what OK asked for.
    fn answer_dialog(&self, k: egui::Key) -> PanelRequests {
        assert!(self.dialogs().is_open(), "no dialog open");
        let first = self.dialog_frame(Vec::new());
        assert!(first.invocations.is_empty());
        self.dialog_frame(vec![key(k)])
    }

    /// New Composition through its dialog (Cmd+N, Enter): the requests.
    fn new_comp_requests(&self, panel: &mut dyn Panel) -> PanelRequests {
        let r = self.action(panel, Invocation::new(crate::actions::ids::NEW_COMP));
        assert!(r.invocations.is_empty(), "the dialog comes first");
        self.answer_dialog(egui::Key::Enter)
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
    // New Composition asks first (Composition Settings); each OK names the
    // next comp.
    for n in 1..=2 {
        let r = h.new_comp_requests(&mut viewer);
        assert_eq!(r.invocations[0].params["name"], format!("Comp {n}"));
        h.apply(r);
        h.frame("composition", &mut viewer, Vec::new());
    }
    assert_eq!(query::comps(&h.stage).len(), 2);
    // New Solid asks first too (Solid Settings); layers made directly still
    // reserve their names within a frame.
    let null = || Invocation::with_params(crate::actions::ids::NEW_LAYER, json!({"kind": "null"}));
    let nulls = twice(&mut h, &mut viewer, null());
    assert_eq!(nulls, ["Null 1", "Null 2"]);
    h.frame("composition", &mut viewer, Vec::new());
    // Reservations end once the items exist: the next one continues the count.
    let next = h.action(&mut viewer, null());
    assert_eq!(next.invocations[0].params["name"], "Null 3");
}

#[test]
fn new_comp_and_layers_are_selected_once_created() {
    let mut h = Harness::new();
    let mut viewer = CompositionViewer::new(None);
    // Empty viewer offers "New Composition"; the action does the same: the
    // Composition Settings dialog, then OK creates the comp.
    let r = h.new_comp_requests(&mut viewer);
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
    use bae_gpu::wgpu;
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
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

/// The menu bar the motion module registers, as the shell builds it.
fn motion_menu_tree() -> bae_ui::MenuTree {
    let mut b = AppBuilder::new();
    b.add_module(bae_ui::UiModule)
        .add_module(crate::MotionUiModule);
    let mut app = b.build();
    let tree =
        bae_ui::menu::build_menus(&app.registry::<bae_ui::MenuItemSpec>().snapshot(), "motion");
    assert!(tree.conflicts.is_empty(), "{:?}", tree.conflicts);
    tree
}

/// `Top > Sub > Title` lines of a menu tree (`---` separators, `▸` submenus).
fn menu_lines(tree: &bae_ui::MenuTree) -> Vec<String> {
    use bae_ui::menu::MenuEntry;
    fn walk(prefix: &str, entries: &[MenuEntry], out: &mut Vec<String>) {
        for e in entries {
            match e {
                MenuEntry::Item { spec, .. } => out.push(format!("{prefix} > {}", spec.title)),
                MenuEntry::Separator => out.push(format!("{prefix} > ---")),
                MenuEntry::Submenu { title, entries } => {
                    out.push(format!("{prefix} > {title} ▸"));
                    walk(&format!("{prefix} > {title}"), entries, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    for (top, entries) in &tree.menus {
        out.push(format!("{top} ▸"));
        walk(top, entries, &mut out);
    }
    out
}

/// The same lines from the catalog for this platform: run-time list
/// placeholders are not shown, so separators around them fold away; the
/// entries AE sorts by name are in English alphabetical order (the catalog
/// lists them in the order of AE's Chinese menu).
fn catalog_lines() -> Vec<String> {
    use crate::catalog::{EFFECT_CATEGORIES, MAC_APP_MENU, MENUS, Node, Status};
    fn walk(prefix: &str, nodes: &[Node], out: &mut Vec<String>) {
        let mut level: Vec<String> = Vec::new();
        // Start of the current run of sorted items in `level`.
        let mut run: Option<usize> = None;
        let sort_run = |level: &mut Vec<String>, run: &mut Option<usize>| {
            if let Some(start) = run.take() {
                level[start..].sort();
            }
        };
        for n in nodes {
            match n {
                Node::Item(it) if it.platform.current() && it.status != Status::Dynamic => {
                    if it.sorted {
                        run.get_or_insert(level.len());
                    } else {
                        sort_run(&mut level, &mut run);
                    }
                    level.push(format!(
                        "{prefix} > {}",
                        crate::catalog::shown_title(it.title)
                    ));
                }
                Node::Separator => {
                    sort_run(&mut level, &mut run);
                    level.push(format!("{prefix} > ---"));
                }
                Node::Submenu(sub) if sub.platform.current() => {
                    sort_run(&mut level, &mut run);
                    level.push(format!("{prefix} > {} ▸", sub.title));
                    let mut inner = Vec::new();
                    walk(
                        &format!("{prefix} > {}", sub.title),
                        sub.children,
                        &mut inner,
                    );
                    level.extend(inner);
                }
                Node::Effects => {
                    // Each category lists AE's effects by name (crate::effects).
                    for c in EFFECT_CATEGORIES {
                        level.push(format!("{prefix} > {c} ▸"));
                        let mut names: Vec<&str> = crate::effects::directory()
                            .iter()
                            .filter(|e| e.category == *c)
                            .map(|e| e.name)
                            .collect();
                        names.sort_unstable();
                        level.extend(names.iter().map(|n| format!("{prefix} > {c} > {n}")));
                    }
                }
                _ => {}
            }
        }
        sort_run(&mut level, &mut run);
        // Fold separators that no longer separate anything at this level.
        let sep = format!("{prefix} > ---");
        let child = format!("{prefix} > ");
        let own = |l: &String| l.starts_with(&child) && !l[child.len()..].contains(" > ");
        let mut folded: Vec<String> = Vec::new();
        for l in level {
            if l == sep {
                let last_own = folded.iter().rev().find(|l| own(l));
                if last_own.is_none_or(|l| *l == sep) {
                    continue;
                }
            }
            folded.push(l);
        }
        while let Some(i) = folded.iter().rposition(&own) {
            if folded[i] != sep {
                break;
            }
            folded.remove(i);
        }
        out.extend(folded);
    }
    let mut out = Vec::new();
    let app = cfg!(target_os = "macos").then_some(("App", MAC_APP_MENU));
    for (top, nodes) in app.into_iter().chain(MENUS.iter().copied()) {
        out.push(format!("{top} ▸"));
        walk(top, nodes, &mut out);
    }
    out
}

/// The menu bar is AE 26's, entry by entry: top-level menus, submenus, order and
/// separators as in the catalog (which a catalog test checks against AE's
/// `menus.json`). A change that drops, adds or moves an entry fails here.
#[test]
fn menu_bar_follows_the_ae_catalog() {
    let tree = motion_menu_tree();
    let tops: Vec<&str> = tree.menus.iter().map(|(t, _)| t.as_str()).collect();
    let mut expect = vec![
        "File",
        "Edit",
        "Composition",
        "Layer",
        "Effect",
        "Animation",
        "View",
        "Window",
        "Help",
    ];
    if cfg!(target_os = "macos") {
        expect.insert(0, "App");
    }
    assert_eq!(tops, expect);
    let built = menu_lines(&tree);
    let wanted = catalog_lines();
    let first_diff = built
        .iter()
        .zip(&wanted)
        .position(|(a, b)| a != b)
        .unwrap_or(built.len().min(wanted.len()));
    assert_eq!(
        built.len(),
        wanted.len(),
        "first difference at {first_diff}: {:?} vs {:?}",
        built.get(first_diff),
        wanted.get(first_diff)
    );
    assert_eq!(built, wanted, "first difference at line {first_diff}");
    // Spot checks of the AE layout.
    let has = |line: &str| built.iter().any(|l| l == line);
    assert!(has("Layer > New > Solid..."));
    assert!(has("Edit > History ▸"), "empty AE submenus stay");
    assert!(has("Effect > Blur & Sharpen ▸"));
    assert_eq!(
        cfg!(target_os = "macos"),
        has("App > Settings > Startup & Repair..."),
        "macOS: Settings in the application menu"
    );
    assert_eq!(
        !cfg!(target_os = "macos"),
        has("Edit > Preferences > Startup & Repair..."),
        "Windows: Edit > Preferences"
    );
    assert_eq!(!cfg!(target_os = "macos"), has("File > Exit"));
    assert_eq!(!cfg!(target_os = "macos"), has("Help > About BAE..."));
}

/// Menu items show the shortcut bound to the same invocation; the global
/// handler runs them with no motion panel focused.
#[test]
fn menus_and_global_handler() {
    use bae_ui::ActionHandler;
    use bae_ui::menu::MenuContext;
    let tree = motion_menu_tree();
    let item = |id: &str| {
        tree.items()
            .into_iter()
            .find(|(i, _)| *i == id)
            .unwrap_or_else(|| panic!("no menu item {id}"))
            .1
            .clone()
    };
    let mut keymap = bae_ui::presets::ae_keymap();
    keymap.bindings.extend(crate::actions::bindings().bindings);
    let shortcut = |id: &str| {
        keymap
            .chord_for_invocation(&item(id).invocation())
            .map(|c| c.to_string())
    };
    assert_eq!(shortcut("motion.composition.new").as_deref(), Some("Cmd+N"));
    assert_eq!(shortcut("motion.layer.new.solid").as_deref(), Some("Cmd+Y"));
    assert_eq!(
        shortcut("motion.layer.new.null").as_deref(),
        Some("Cmd+Alt+Shift+Y")
    );
    assert_eq!(shortcut("motion.animation.ease").as_deref(), Some("F9"));
    assert_eq!(shortcut("motion.layer.new.shape"), None);

    // Enabled state follows the session.
    let mut h = Harness::new();
    let solid_item = item("motion.layer.new.solid");
    let new_comp = item("motion.composition.new");
    {
        let cx = MenuContext::new(&h.stage, &h.resources, "motion");
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
    let ok = h.answer_dialog(egui::Key::Enter);
    h.apply(ok);
    h.frame("history", &mut NoopPanel, Vec::new());
    let comp = h.session().lock().active_comp.clone().expect("comp opened");
    {
        let cx = MenuContext::new(&h.stage, &h.resources, "motion");
        assert!(solid_item.is_enabled(&cx));
    }
    // Layer ▸ New ▸ Solid... opens Solid Settings; OK creates the solid.
    run(&mut h, solid_item.invocation());
    let ok = h.answer_dialog(egui::Key::Enter);
    h.apply(ok);
    assert_eq!(query::comp_layers(&h.stage, &comp).len(), 1);
}

/// AE's greying: items need an open comp, selected layers or selected
/// keyframes; items whose command no module handles stay greyed; Window and
/// Workspace items show check marks.
#[test]
fn menu_items_grey_and_check_like_ae() {
    use bae_ui::menu::MenuContext;
    let tree = motion_menu_tree();
    let item = |id: &str| {
        tree.items()
            .into_iter()
            .find(|(i, _)| *i == id)
            .unwrap_or_else(|| panic!("no menu item {id}"))
            .1
            .clone()
    };
    let ids = [
        "motion.composition.new",
        "motion.layer.new.solid",
        "motion.composition.trim",
        "motion.edit.duplicate",
        "motion.layer.arrange.front",
        "motion.animation.ease",
        "menu.edit.copy",
        "menu.layer.transform.fit_to_comp",
        "menu.view.show_grid",
        "shell.file.save",
    ];
    let mut h = Harness::new();
    let handled: std::collections::HashSet<String> = ["motion.layer.fit_to_comp".to_owned()].into();
    let enabled = |h: &Harness, handled: Option<&std::collections::HashSet<String>>| {
        let mut cx = MenuContext::new(&h.stage, &h.resources, "motion");
        if let Some(a) = handled {
            cx = cx.with_actions(a);
        }
        ids.iter()
            .filter(|id| item(id).is_enabled(&cx))
            .copied()
            .collect::<Vec<_>>()
    };
    // Nothing open: only what needs nothing.
    assert_eq!(
        enabled(&h, None),
        ["motion.composition.new", "shell.file.save"]
    );
    // A comp, nothing selected.
    let (_comp, solid, _shape) = h.comp_with_layers();
    h.session().lock().selected_layers.clear();
    assert_eq!(
        enabled(&h, None),
        [
            "motion.composition.new",
            "motion.layer.new.solid",
            "motion.composition.trim",
            "shell.file.save"
        ]
    );
    // A selected layer: layer commands too; Fit to Comp once handled.
    h.session().lock().selected_layers = vec![solid.clone()];
    let layer_cmds = [
        "motion.composition.new",
        "motion.layer.new.solid",
        "motion.composition.trim",
        "motion.edit.duplicate",
        "motion.layer.arrange.front",
        "menu.edit.copy",
        "shell.file.save",
    ];
    assert_eq!(enabled(&h, None), layer_cmds);
    let mut with_fit = layer_cmds.to_vec();
    with_fit.insert(6, "menu.layer.transform.fit_to_comp");
    assert_eq!(enabled(&h, Some(&handled)), with_fit);
    // Selected keyframes: the keyframe assistants.
    h.session().lock().selected_layers.clear();
    h.session().lock().selected_keys = vec![bae_motion::KeyRef {
        path: solid.clone(),
        attr: "transform:opacity".into(),
        time: Time::ZERO,
    }];
    let keys = enabled(&h, None);
    assert!(keys.contains(&"motion.animation.ease"), "{keys:?}");
    assert!(keys.contains(&"menu.edit.copy"));
    assert!(!keys.contains(&"motion.edit.duplicate"));

    // Check marks: open panels and the active workspace.
    let open = [PanelId::new("project")];
    let cx = MenuContext::new(&h.stage, &h.resources, "motion").with_open_panels(&open);
    let project = item("shell.window.project");
    assert!(project.is_checkable() && project.is_checked(&cx));
    assert!(!item("shell.window.timeline").is_checked(&cx));
    let default = item("menu.window.workspace.default");
    assert!(default.is_checked(&cx));
    assert!(!item("menu.window.workspace.review").is_checked(&cx));
    assert!(!item("motion.layer.new.solid").is_checkable());
}

struct NoopPanel;

impl Panel for NoopPanel {
    fn ui(&mut self, _ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
        // Like every motion panel: resolve pending selections once per frame.
        crate::common::session(cx.resources).lock().sync(cx.stage);
    }
}

/// Every menu title (items and submenus) has a Chinese name.
#[test]
fn every_menu_title_has_chinese() {
    bae_ui::i18n::register(crate::i18n::ZH);
    bae_ui::i18n::register(crate::catalog::MENU_ZH);
    let mut missing = std::collections::BTreeSet::new();
    let mut check = |t: &str| {
        if bae_ui::i18n::translation(t).is_none() {
            missing.insert(t.to_owned());
        }
    };
    for (path, item) in crate::catalog::menu_items() {
        for t in path.iter().skip(1) {
            check(t);
        }
        if item.status != crate::catalog::Status::Dynamic {
            check(item.title);
        }
    }
    for (top, _) in crate::catalog::MENUS {
        check(top);
    }
    for c in crate::catalog::EFFECT_CATEGORIES {
        check(c);
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
            .any(|(id, item)| id == "motion.composition.render_queue"
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
    let Some(gpu) = bae_gpu::testing::shared_context() else {
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

#[test]
fn new_composition_dialog_ok_and_cancel() {
    let mut h = Harness::new();
    let mut viewer = CompositionViewer::new(None);
    // Cancel (Esc): nothing is created.
    let r = h.action(&mut viewer, Invocation::new(crate::actions::ids::NEW_COMP));
    assert!(r.invocations.is_empty());
    let r = h.answer_dialog(egui::Key::Escape);
    assert!(r.invocations.is_empty());
    assert!(!h.dialogs().is_open());
    assert!(query::comps(&h.stage).is_empty());
    // OK with changed settings creates the comp with all of them in one step.
    h.action(&mut viewer, Invocation::new(crate::actions::ids::NEW_COMP));
    {
        let d = h.dialogs();
        let mut d = d.lock();
        let Some(crate::dialogs::Dialog::CompSettings(cs)) = d.open.as_mut() else {
            panic!("Composition Settings is not open");
        };
        assert!(cs.is_new());
        assert_eq!(cs.name, "Comp 1");
        assert_eq!((cs.values.width, cs.values.height), (1920, 1080));
        cs.name = "Title".into();
        cs.values.width = 1280;
        cs.values.height = 720;
        cs.values.fps = 25.0;
        cs.values.bg = [1.0, 1.0, 1.0, 1.0];
        cs.values.duration = Time::from_seconds(4.0);
    }
    let r = h.answer_dialog(egui::Key::Enter);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "comp.create");
    h.apply(r);
    h.frame("composition", &mut viewer, Vec::new());
    let comp = h.session().lock().active_comp.clone().expect("opened");
    let info = query::comp_info(&h.stage, &comp).unwrap();
    assert_eq!(info.name, "Title");
    assert_eq!((info.width, info.height), (1280, 720));
    assert_eq!(info.frame_rate, bae_model::FrameRate::FPS_25);
    assert_eq!(info.bg_color, [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(info.duration, Time::from_seconds(4.0));
    // The next New Composition starts from the last settings (AE), named on.
    h.action(&mut viewer, Invocation::new(crate::actions::ids::NEW_COMP));
    {
        let d = h.dialogs();
        let d = d.lock();
        let Some(crate::dialogs::Dialog::CompSettings(cs)) = d.open.as_ref() else {
            panic!("Composition Settings is not open");
        };
        assert_eq!(cs.name, "Comp 1");
        assert_eq!((cs.values.width, cs.values.fps), (1280, 25.0));
    }
    let r = h.answer_dialog(egui::Key::Escape);
    assert!(r.invocations.is_empty());
}

#[test]
fn composition_settings_edit_is_one_undo_step() {
    let mut h = Harness::new();
    let (comp, solid, shape) = h.comp_with_layers();
    let mut viewer = CompositionViewer::new(None);
    h.frame("composition", &mut viewer, Vec::new());
    let before = |h: &Harness, l: &PrimPath| {
        query::static_value(&h.stage.prim(l).unwrap(), "transform:position")
    };
    let solid_pos = before(&h, &solid);
    let shape_pos = before(&h, &shape);
    // Cmd+K with nothing changed: OK runs no command.
    h.action(
        &mut viewer,
        Invocation::new(crate::actions::ids::COMP_SETTINGS),
    );
    let r = h.answer_dialog(egui::Key::Enter);
    assert!(r.invocations.is_empty(), "{:?}", r.invocations);
    // Cmd+K, change size and name, OK: one comp.set_settings.
    h.action(
        &mut viewer,
        Invocation::new(crate::actions::ids::COMP_SETTINGS),
    );
    {
        let d = h.dialogs();
        let mut d = d.lock();
        let Some(crate::dialogs::Dialog::CompSettings(cs)) = d.open.as_mut() else {
            panic!("Composition Settings is not open");
        };
        assert!(!cs.is_new());
        assert_eq!((cs.values.width, cs.values.height), (640, 360));
        cs.values.width = 800;
        cs.values.height = 400;
        cs.name = "Renamed".into();
    }
    let r = h.answer_dialog(egui::Key::Enter);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "comp.set_settings");
    assert_eq!(r.invocations[0].params["anchor"], "center");
    h.apply(r);
    let info = query::comp_info(&h.stage, &comp).unwrap();
    assert_eq!(
        (info.width, info.height, info.name.as_str()),
        (800, 400, "Renamed")
    );
    // Layers keep their place in the frame (moved by half the change).
    let moved = |p: Option<Value>| match p {
        Some(Value::Vec3([x, y, z])) => Some(Value::Vec3([x + 80.0, y + 20.0, z])),
        other => other,
    };
    assert_eq!(before(&h, &solid), moved(solid_pos.clone()));
    assert_eq!(before(&h, &shape), moved(shape_pos.clone()));
    // One undo restores the comp and the layers.
    h.stage.undo(&h.author).unwrap();
    let info = query::comp_info(&h.stage, &comp).unwrap();
    assert_eq!((info.width, info.height), (640, 360));
    assert_eq!(before(&h, &solid), solid_pos);
    assert_eq!(before(&h, &shape), shape_pos);
}

#[test]
fn composition_settings_follows_the_project_panel_selection() {
    let mut h = Harness::new();
    let (comp, _, _) = h.comp_with_layers();
    let other = PrimPath::new(
        h.run(
            "comp.create",
            json!({"name": "Other", "width": 320, "height": 240}),
        )["path"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    h.session().lock().selected_items = vec![other.clone()];
    let target = |h: &Harness, kind: &str| {
        let mut requests = PanelRequests::default();
        let id = PanelId::new(kind);
        let cx = PanelContext::new(&id, kind, &h.stage, &h.author, &h.resources, &mut requests);
        crate::actions::settings_target(&cx, &h.session())
    };
    // The Project panel has focus: its selected comp; elsewhere: the active comp.
    assert_eq!(target(&h, "project"), Some(other));
    assert_eq!(target(&h, "timeline"), Some(comp));
}

#[test]
fn dialog_strings_have_chinese() {
    let missing: Vec<_> = crate::dialogs::strings()
        .into_iter()
        .filter(|s| {
            !bae_ui::i18n::has_translation(s) && !crate::i18n::ZH.iter().any(|(en, _)| en == s)
        })
        .collect();
    assert!(missing.is_empty(), "no Chinese for {missing:?}");
}

#[test]
fn new_solid_dialog_and_solid_settings() {
    use crate::dialogs::{Dialog, solid_settings_default_color};
    let mut h = Harness::new();
    let mut viewer = CompositionViewer::new(None);
    let r = h.new_comp_requests(&mut viewer);
    h.apply(r);
    h.frame("composition", &mut viewer, Vec::new());
    let comp = h.session().lock().active_comp.clone().expect("opened");
    let solid =
        || Invocation::with_params(crate::actions::ids::NEW_LAYER, json!({"kind": "solid"}));
    // Cancel (Esc): nothing is created.
    assert!(h.action(&mut viewer, solid()).invocations.is_empty());
    assert!(h.answer_dialog(egui::Key::Escape).invocations.is_empty());
    assert!(query::comp_layers(&h.stage, &comp).is_empty());
    // AE's defaults: comp size, 17% gray, named after the color.
    h.action(&mut viewer, solid());
    {
        let d = h.dialogs();
        let mut d = d.lock();
        let Some(Dialog::SolidSettings(s)) = d.open.as_mut() else {
            panic!("Solid Settings is not open");
        };
        assert!(s.is_new());
        assert_eq!(s.values.name, "Dark Gray Solid 1");
        assert_eq!((s.values.width, s.values.height), (1920, 1080));
        assert_eq!(s.values.color, solid_settings_default_color());
        s.values.width = 640;
        s.values.color = [1.0, 0.0, 0.0, 1.0];
    }
    let r = h.answer_dialog(egui::Key::Enter);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "layer.create");
    h.apply(r);
    h.frame("composition", &mut viewer, Vec::new());
    let layer = query::comp_layers(&h.stage, &comp)[0].clone();
    assert_eq!(
        query::display_name_of(&h.stage, &layer),
        "Dark Gray Solid 1"
    );
    let item = bae_motion::solid::layer_solid(&h.stage, &layer).unwrap();
    let info = bae_motion::solid::solid_info(&h.stage, &item).unwrap();
    assert_eq!((info.width, info.height), (640, 1080));
    assert_eq!(info.color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(h.session().lock().selected_layers, vec![layer.clone()]);
    // The next New Solid starts from the last color, named after it.
    h.action(&mut viewer, solid());
    {
        let d = h.dialogs();
        let d = d.lock();
        let Some(Dialog::SolidSettings(s)) = d.open.as_ref() else {
            panic!("Solid Settings is not open");
        };
        assert_eq!(s.values.color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(s.values.name, "Red Solid 1");
    }
    h.answer_dialog(egui::Key::Escape);
    // Layer Settings (Cmd/Ctrl+Shift+Y) on the selected solid: nothing
    // changed, no command; a change is one solid.set_settings.
    let settings = || Invocation::new(crate::actions::ids::LAYER_SETTINGS);
    h.action(&mut viewer, settings());
    assert!(h.answer_dialog(egui::Key::Enter).invocations.is_empty());
    h.action(&mut viewer, settings());
    {
        let d = h.dialogs();
        let mut d = d.lock();
        let Some(Dialog::SolidSettings(s)) = d.open.as_mut() else {
            panic!("Solid Settings is not open");
        };
        assert!(!s.is_new());
        assert_eq!(s.values.name, "Dark Gray Solid 1");
        s.values.name = "Backdrop".into();
        s.values.height = 200;
    }
    let r = h.answer_dialog(egui::Key::Enter);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "solid.set_settings");
    assert_eq!(r.invocations[0].params["affect_all"], false);
    h.apply(r);
    let info = bae_motion::solid::solid_info(&h.stage, &item).unwrap();
    assert_eq!((info.name.as_str(), info.height), ("Backdrop", 200));
    assert_eq!(query::display_name_of(&h.stage, &layer), "Backdrop");
    // One undo step.
    h.stage.undo(&h.author).unwrap();
    let info = bae_motion::solid::solid_info(&h.stage, &item).unwrap();
    assert_eq!(
        (info.name.as_str(), info.height),
        ("Dark Gray Solid 1", 1080)
    );
    // A shape layer has no Solid Settings.
    let r = h.action(
        &mut viewer,
        Invocation::with_params(
            crate::actions::ids::NEW_LAYER,
            json!({"kind": "shape", "shape": "rect"}),
        ),
    );
    h.apply(r);
    h.frame("composition", &mut viewer, Vec::new());
    h.action(&mut viewer, settings());
    assert!(!h.dialogs().is_open());
}

#[test]
fn layer_settings_target_follows_focus_and_selection() {
    let mut h = Harness::new();
    let (_, solid, shape) = h.comp_with_layers();
    let item = bae_motion::solid::layer_solid(&h.stage, &solid).unwrap();
    let target = |h: &Harness, project: bool| {
        crate::actions::layer_settings_target(&h.stage, &h.session(), project)
    };
    // The Project panel has focus with the solid item selected: the item
    // (Solid Settings then changes every layer that uses it).
    h.session().lock().selected_items = vec![item.clone()];
    h.session().lock().selected_layers = vec![shape.clone()];
    assert_eq!(target(&h, true), Some(item.clone()));
    // Elsewhere: the one selected layer, when it uses a solid.
    assert_eq!(target(&h, false), None);
    h.session().lock().selected_layers = vec![solid.clone()];
    assert_eq!(target(&h, false), Some(solid.clone()));
    // Several layers: none (AE edits one layer's settings).
    h.session().lock().selected_layers = vec![solid.clone(), shape];
    assert_eq!(target(&h, false), None);
}

#[test]
fn camera_and_light_settings_dialogs() {
    use crate::dialogs::Dialog;
    let mut h = Harness::new();
    let mut viewer = CompositionViewer::new(None);
    let r = h.new_comp_requests(&mut viewer);
    h.apply(r);
    h.frame("composition", &mut viewer, Vec::new());
    let comp = h.session().lock().active_comp.clone().expect("opened");
    let new =
        |kind: &str| Invocation::with_params(crate::actions::ids::NEW_LAYER, json!({"kind": kind}));
    // New Camera: AE's 50mm two-node camera; OK creates it in one step.
    assert!(h.action(&mut viewer, new("camera")).invocations.is_empty());
    {
        let d = h.dialogs();
        let mut d = d.lock();
        let Some(Dialog::CameraSettings(c)) = d.open.as_mut() else {
            panic!("Camera Settings is not open");
        };
        assert!(c.is_new());
        assert_eq!(c.values.name, "Camera 1");
        assert_eq!(c.values.camera_type, "two_node");
        assert!((c.values.zoom - 1920.0 * 50.0 / 36.0).abs() < 1e-3);
        c.values.camera_type = "one_node".into();
        c.values.depth_of_field = true;
    }
    let r = h.answer_dialog(egui::Key::Enter);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "layer.create");
    h.apply(r);
    h.frame("composition", &mut viewer, Vec::new());
    let cam = query::comp_layers(&h.stage, &comp)[0].clone();
    let value =
        |h: &Harness, l: &PrimPath, a: &str| query::static_value(&h.stage.prim(l).unwrap(), a);
    assert_eq!(
        value(&h, &cam, "camera:type"),
        Some(Value::Enum("one_node".into()))
    );
    assert_eq!(
        value(&h, &cam, "camera:depth_of_field"),
        Some(Value::Bool(true))
    );
    // New Light: Spot Light 1; switching the type renames it until typed.
    assert!(h.action(&mut viewer, new("light")).invocations.is_empty());
    {
        let d = h.dialogs();
        let mut d = d.lock();
        let Some(Dialog::LightSettings(l)) = d.open.as_mut() else {
            panic!("Light Settings is not open");
        };
        assert_eq!(l.values.name, "Spot Light 1");
        assert_eq!(l.values.intensity, 100.0);
        l.values.light_type = "point".into();
        l.values.name = "Key".into();
        l.values.intensity = 80.0;
    }
    let r = h.answer_dialog(egui::Key::Enter);
    h.apply(r);
    h.frame("composition", &mut viewer, Vec::new());
    let light = query::comp_layers(&h.stage, &comp)[0].clone();
    assert_eq!(query::display_name_of(&h.stage, &light), "Key");
    assert_eq!(
        value(&h, &light, "light:type"),
        Some(Value::Enum("point".into()))
    );
    assert_eq!(
        value(&h, &light, "light:intensity"),
        Some(Value::Float(80.0))
    );
    // Layer Settings on the light: Light Settings; one light.set_settings.
    h.session().lock().selected_layers = vec![light.clone()];
    let settings = || Invocation::new(crate::actions::ids::LAYER_SETTINGS);
    h.action(&mut viewer, settings());
    {
        let d = h.dialogs();
        let mut d = d.lock();
        let Some(Dialog::LightSettings(l)) = d.open.as_mut() else {
            panic!("Light Settings is not open");
        };
        assert!(!l.is_new());
        assert_eq!(l.values.intensity, 80.0);
        l.values.intensity = 120.0;
    }
    let r = h.answer_dialog(egui::Key::Enter);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "light.set_settings");
    assert_eq!(r.invocations[0].params["intensity"], 120.0);
    assert!(
        r.invocations[0].params.get("light_type").is_none(),
        "only changes"
    );
    h.apply(r);
    assert_eq!(
        value(&h, &light, "light:intensity"),
        Some(Value::Float(120.0))
    );
    // Layer Settings on the camera: Camera Settings.
    h.session().lock().selected_layers = vec![cam.clone()];
    h.action(&mut viewer, settings());
    {
        let d = h.dialogs();
        let mut d = d.lock();
        let Some(Dialog::CameraSettings(c)) = d.open.as_mut() else {
            panic!("Camera Settings is not open");
        };
        assert!(!c.is_new());
        c.values.zoom = 1000.0;
    }
    let r = h.answer_dialog(egui::Key::Enter);
    assert_eq!(r.invocations[0].id, "camera.set_settings");
    h.apply(r);
    assert_eq!(value(&h, &cam, "camera:zoom"), Some(Value::Float(1000.0)));
}

#[test]
fn precompose_dialog_options() {
    use crate::dialogs::Dialog;
    let mut h = Harness::new();
    let (comp, solid, shape) = h.comp_with_layers();
    let mut viewer = CompositionViewer::new(None);
    h.frame("composition", &mut viewer, Vec::new());
    let pre = || Invocation::new(crate::actions::ids::PRE_COMPOSE);
    // Nothing selected: no dialog.
    h.session().lock().selected_layers.clear();
    h.action(&mut viewer, pre());
    assert!(!h.dialogs().is_open());
    // One solid: Leave all attributes is offered; AE's factory choice is
    // Move, and the last choice is remembered (#199).
    h.session().lock().selected_layers = vec![solid.clone()];
    h.action(&mut viewer, pre());
    {
        let d = h.dialogs();
        let mut d = d.lock();
        let Some(Dialog::Precompose(p)) = d.open.as_mut() else {
            panic!("Pre-compose is not open");
        };
        assert!(!p.leave);
        assert!(p.name.ends_with(" Comp 1"), "{}", p.name);
        p.leave = true;
        d.precompose.leave = true;
    }
    h.answer_dialog(egui::Key::Escape);
    h.action(&mut viewer, pre());
    {
        let d = h.dialogs();
        let d = d.lock();
        let Some(Dialog::Precompose(p)) = d.open.as_ref() else {
            panic!("Pre-compose is not open");
        };
        assert!(p.leave, "the last choice is remembered");
    }
    h.answer_dialog(egui::Key::Escape);
    // Two layers: Move all attributes only; OK is one layer.precompose.
    h.session().lock().selected_layers = vec![solid.clone(), shape.clone()];
    h.action(&mut viewer, pre());
    {
        let d = h.dialogs();
        let mut d = d.lock();
        let Some(Dialog::Precompose(p)) = d.open.as_mut() else {
            panic!("Pre-compose is not open");
        };
        assert!(!p.leave);
        assert_eq!(p.name, "Pre-comp 1");
        p.open_new = true;
    }
    let r = h.answer_dialog(egui::Key::Enter);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "layer.precompose");
    assert_eq!(r.invocations[0].params["mode"], "move");
    h.apply(r);
    h.frame("composition", &mut viewer, Vec::new());
    assert_eq!(query::comp_layers(&h.stage, &comp).len(), 1);
    // Open New Composition: the new comp is the active one.
    let active = h.session().lock().active_comp.clone().unwrap();
    assert_ne!(active, comp);
    assert_eq!(query::display_name_of(&h.stage, &active), "Pre-comp 1");
    assert_eq!(query::comp_layers(&h.stage, &active).len(), 2);
}

/// A valid 1×1 RGBA PNG.
const PNG_1X1: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 218, 99, 100, 96, 248, 95, 15, 0, 2,
    135, 1, 128, 235, 71, 186, 146, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// A folder with a still, a 3-frame sequence in `plates/` and a text file.
fn import_fixture(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("bae-import-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("plates")).unwrap();
    std::fs::write(dir.join("logo.png"), PNG_1X1).unwrap();
    for n in 1..=3 {
        std::fs::write(dir.join("plates").join(format!("shot_{n:04}.png")), PNG_1X1).unwrap();
    }
    std::fs::write(dir.join("readme.txt"), b"notes").unwrap();
    dir
}

#[test]
fn double_l_reveals_the_imported_audio_layer_waveform() {
    let mut h = Harness::new();
    let (comp, _, _) = h.comp_with_layers();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tone.wav");
    // One mono 16-bit PCM sample at 8 kHz.
    std::fs::write(&file,b"RIFF\x26\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x80\x3e\0\0\x02\0\x10\0data\x02\0\0\0\xff\x3f").unwrap();
    let probed = crate::import::probe(std::slice::from_ref(&file), &Default::default());
    h.run("item.import", json!({"files":probed.files}));
    let asset = h
        .stage
        .children(&PrimPath::new(query::ASSETS_FOLDER).unwrap())
        .into_iter()
        .find(|p| query::display_name_of(&h.stage, p) == "tone.wav")
        .unwrap();
    let layer = PrimPath::new(
        h.run(
            "layer.create",
            json!({"comp":comp,"kind":"footage","source":asset}),
        )["path"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        crate::audio::layer_file(&h.stage, &layer)
            .unwrap()
            .canonicalize()
            .unwrap(),
        file.canonicalize().unwrap()
    );
    h.session().lock().selected_layers = vec![layer.clone()];
    let mut panel = TimelinePanel::default();
    for expected in ["audio:levels", "audio:waveform"] {
        h.action(
            &mut panel,
            Invocation::with_params(
                crate::actions::ids::REVEAL,
                json!({"what":"audio:levels","add":false}),
            ),
        );
        assert_eq!(
            h.session().lock().reveal.get(&layer),
            Some(&crate::session::Reveal::Attrs(vec![expected.into()]))
        );
    }
}

#[test]
fn import_probes_files_sequences_and_folders() {
    let dir = import_fixture("probe");
    let defaults = crate::import::ImportDefaults::default();
    let probed = crate::import::probe(
        &[
            dir.join("logo.png"),
            dir.join("plates").join("shot_0002.png"),
            dir.join("plates").join("shot_0003.png"),
            dir.join("readme.txt"),
        ],
        &defaults,
    );
    let names: Vec<_> = probed
        .files
        .iter()
        .map(|f| f.name.clone().unwrap())
        .collect();
    // One numbered frame imports its whole sequence, once, named like AE.
    assert_eq!(names, ["logo.png", "shot_[0001-0003].png"]);
    assert_eq!(probed.files[0].attrs.get("width"), Some(&Value::Int(1)));
    assert_eq!(probed.failed, ["readme.txt"]);
    // A dropped folder keeps its name as a project folder.
    let probed = crate::import::probe(&[dir.join("plates")], &defaults);
    assert_eq!(probed.files.len(), 1);
    assert_eq!(probed.files[0].folder.as_deref(), Some("plates"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[derive(Debug)]
struct Dropped(std::path::PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

struct Picker(std::sync::Mutex<Vec<Vec<std::path::PathBuf>>>);

impl bae_ui::files::FilePicker for Picker {
    fn open(&self, _: &bae_ui::files::OpenRequest) -> Vec<std::path::PathBuf> {
        let mut answers = self.0.lock().unwrap();
        if answers.is_empty() {
            Vec::new()
        } else {
            answers.remove(0)
        }
    }
}

#[test]
fn import_file_and_files_dropped_on_the_project_panel() {
    let dir = import_fixture("ui");
    let mut h = Harness::new();
    // File > Import > File: the picker's files become one item.import.
    h.resources
        .insert(bae_ui::files::SharedFilePicker::new(Picker(
            std::sync::Mutex::new(vec![vec![dir.join("logo.png")]]),
        )));
    let mut project = ProjectPanel::default();
    let r = h.action(
        &mut project,
        Invocation::new(crate::actions::ids::IMPORT_FILE),
    );
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "item.import");
    h.apply(r);
    let assets = PrimPath::new(query::ASSETS_FOLDER).unwrap();
    let names = |h: &Harness| -> Vec<String> {
        h.stage
            .children(&assets)
            .iter()
            .map(|c| query::display_name_of(&h.stage, c))
            .collect()
    };
    assert_eq!(names(&h), ["logo.png"]);
    // Cancelled: nothing.
    let r = h.action(
        &mut project,
        Invocation::new(crate::actions::ids::IMPORT_FILE),
    );
    assert!(r.invocations.is_empty());
    // A folder dropped from Finder / Explorer onto the panel.
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
        dropped_files: vec![std::sync::Arc::new(Dropped(dir.join("plates")))],
        ..Default::default()
    };
    let mut requests = PanelRequests::default();
    let id = PanelId::new("project");
    let mut out = h.ctx.run_ui(raw, |ui| {
        let mut cx = PanelContext::new(
            &id,
            "project",
            &h.stage,
            &h.author,
            &h.resources,
            &mut requests,
        );
        project.ui(ui, &mut cx);
    });
    out.textures_delta.clear();
    assert_eq!(requests.invocations.len(), 1, "{:?}", requests.invocations);
    h.apply(requests);
    assert_eq!(names(&h), ["logo.png", "plates"]);
    // An unreadable file: an alert, no import.
    let r = h.action(
        &mut project,
        Invocation::new(crate::actions::ids::IMPORT_FILE),
    );
    assert!(r.invocations.is_empty());
    h.resources
        .insert(bae_ui::files::SharedFilePicker::new(Picker(
            std::sync::Mutex::new(vec![vec![dir.join("readme.txt")]]),
        )));
    let r = h.action(
        &mut project,
        Invocation::new(crate::actions::ids::IMPORT_FILE),
    );
    assert!(r.invocations.is_empty());
    assert!(matches!(
        h.dialogs().lock().open,
        Some(crate::dialogs::Dialog::Alert { .. })
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn preferences_reach_new_layers_comps_and_imports() {
    let mut prefs = bae_prefs::Preferences::default();
    assert!(prefs.set("labels.label_default_null", 9));
    assert!(prefs.set("labels.label_default_comp", 11));
    assert!(prefs.set("composition.create_layers_at_comp_start", false));
    assert!(prefs.set("import.still_footage_duration_mode", "fixed_duration"));
    assert!(prefs.set("import.still_footage_duration_timecode", "0:00:05:00"));
    assert!(prefs.set("labels.label_default_still", 12));
    let mut h = Harness::new();
    h.resources
        .insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(
            prefs.clone(),
        )));
    let mut viewer = CompositionViewer::new(None);
    // New Composition: the comp gets the Composition label.
    let r = h.new_comp_requests(&mut viewer);
    h.apply(r);
    h.frame("composition", &mut viewer, Vec::new());
    let comp = h.session().lock().active_comp.clone().unwrap();
    let value =
        |h: &Harness, p: &PrimPath, a: &str| query::static_value(&h.stage.prim(p).unwrap(), a);
    assert_eq!(value(&h, &comp, "label"), Some(Value::Int(11)));
    // A new null at 2 s: Create Layers at Composition Start Time is off.
    let info = query::comp_info(&h.stage, &comp).unwrap();
    h.session().lock().set_time(&info, Time::from_seconds(2.0));
    let r = h.action(
        &mut viewer,
        Invocation::with_params(crate::actions::ids::NEW_LAYER, json!({"kind": "null"})),
    );
    h.apply(r);
    let null = query::comp_layers(&h.stage, &comp)[0].clone();
    assert_eq!(value(&h, &null, "label"), Some(Value::Int(9)));
    assert_eq!(
        query::layer_timing(&h.stage, &null).start_time,
        Time::from_seconds(2.0)
    );
    // Import: a still gets the fixed duration and the Still label.
    let dir = import_fixture("prefs");
    let plain = crate::import::probe(&[dir.join("logo.png")], &Default::default());
    assert!(!plain.files[0].attrs.contains_key("duration"));
    let defaults = crate::import::ImportDefaults::of(&prefs);
    let still = crate::import::probe(&[dir.join("logo.png")], &defaults);
    let attrs = &still.files[0].attrs;
    assert_eq!(
        attrs.get("duration"),
        Some(&Value::Time(Time::from_seconds(5.0)))
    );
    assert_eq!(attrs.get("label"), Some(&Value::Int(12)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every AE default shortcut of a command that runs is in the AE keymap (the
/// shell's own `ae` keymap or the motion workspace's bindings), with AE's chord
/// for this platform and AE's panel scope; no chord is bound twice.
#[test]
fn ae_default_shortcuts_of_working_commands_are_bound() {
    use crate::catalog::{BOUND_ELSEWHERE, NATIVE, SHADOWED, keys};
    let base = bae_ui::presets::ae_keymap();
    let motion = crate::actions::bindings();
    let mut all = base.bindings.clone();
    all.extend(motion.bindings.iter().cloned());
    let keymap = bae_ui::Keymap::new("test", all);
    assert!(keymap.conflicts().is_empty(), "{:?}", keymap.conflicts());
    // The shell binds the third AE workspace slot by position (AE's Learn
    // workspace is not in BAE).
    const SLOT_KEYS: &[&str] = &["Shift+F12"];
    let mac = cfg!(target_os = "macos");
    let mut missing = Vec::new();
    let mut active = 0;
    for k in keys() {
        if !k.status.works()
            || k.flags & (NATIVE | BOUND_ELSEWHERE | SHADOWED) != 0
            || !crate::catalog::in_own_panel(k)
        {
            continue;
        }
        let params: serde_json::Value = if k.params.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(k.params).unwrap()
        };
        let mut any = false;
        for chord in k.chords(mac) {
            let Ok(parsed) = bae_ui::Chord::parse(&chord) else {
                continue;
            };
            any = true;
            let bound = keymap
                .bindings
                .iter()
                .find(|b| b.chord == parsed && b.when.as_deref() == k.scope());
            match bound {
                Some(b)
                    if b.action == k.action
                        && (b.params == params || SLOT_KEYS.contains(&chord.as_str())) => {}
                other => missing.push(format!(
                    "{}/{} {chord}: {:?}",
                    k.context,
                    k.command,
                    other.map(|b| (&b.action, &b.params))
                )),
            }
        }
        active += usize::from(any);
    }
    assert!(missing.is_empty(), "{missing:#?}");
    assert!(active > 100, "only {active} AE shortcuts active");
}

/// A command another module implements gets AE's default shortcut when the app
/// starts, without a catalog change; further variants of commands the motion
/// workspace already runs do not.
#[test]
fn ae_shortcuts_follow_other_modules_handlers() {
    struct Center;
    impl bae_ui::ActionHandler for Center {
        fn actions(&self) -> Vec<String> {
            vec!["motion.layer.center_in_view".into()]
        }
        fn handle(&self, _: &Invocation, _: &mut PanelContext<'_>) -> bool {
            true
        }
    }
    #[derive(Debug)]
    struct CenterModule;
    impl bae_app::AppModule for CenterModule {
        fn name(&self) -> &str {
            "center"
        }
        fn build(&self, app: &mut AppBuilder) {
            let h: std::sync::Arc<dyn bae_ui::ActionHandler> = std::sync::Arc::new(Center);
            let _ = app.register_arc::<dyn bae_ui::ActionHandler>("center", h);
        }
    }
    let bound = |with_module: bool| {
        let mut b = AppBuilder::new();
        b.add_module(bae_ui::UiModule)
            .add_module(crate::MotionUiModule);
        if with_module {
            b.add_module(CenterModule);
        }
        let mut app = b.build();
        app.registry::<bae_ui::KeymapBindings>()
            .snapshot()
            .iter()
            .flat_map(|e| e.item().bindings.clone())
            .map(|b| (b.chord.to_string(), b.action, b.params))
            .collect::<Vec<_>>()
    };
    let center = |v: &[(String, String, serde_json::Value)]| {
        v.iter()
            .filter(|(_, a, _)| a == "motion.layer.center_in_view")
            .map(|(c, ..)| c.clone())
            .collect::<Vec<_>>()
    };
    assert!(center(&bound(false)).is_empty());
    let with = bound(true);
    assert_eq!(
        center(&with),
        [bae_ui::Chord::parse("Cmd+Home").unwrap().to_string()]
    );
    // motion.reveal runs ("animated"); its unimplemented variants stay unbound.
    assert!(
        !with
            .iter()
            .any(|(_, a, p)| a == "motion.reveal" && p["what"] == "modified"),
        "variants of a working command wait for the catalog"
    );
}

/// Layer ▸ Switches, 3D Layer, Quality, Edit ▸ Label: each is one undo step on
/// the selected layers, AE's toggle rule (on for all unless all are on), and
/// the menu shows check marks for the selection's state.
#[test]
fn layer_switch_menu_commands_and_undo() {
    use crate::actions::{ids, label_of, quality_of, switch_on};
    let mut h = Harness::new();
    let (_comp, solid, shape) = h.comp_with_layers();
    let mut tl = TimelinePanel::default();
    let select = |h: &Harness, layers: &[&PrimPath]| {
        let s = h.session();
        let mut s = s.lock();
        s.selected_layers = layers.iter().map(|l| (*l).clone()).collect();
        s.selected_keys.clear();
    };
    let run = |h: &mut Harness, tl: &mut TimelinePanel, id: &str, params: serde_json::Value| {
        let r = h.action(tl, Invocation::with_params(id, params));
        h.apply(r);
    };
    // Shy: on for both, then off (all on), undo back to on.
    select(&h, &[&solid, &shape]);
    run(&mut h, &mut tl, ids::LAYER_SWITCH, json!({"switch": "shy"}));
    assert!(switch_on(&h.stage, &solid, "shy") && switch_on(&h.stage, &shape, "shy"));
    run(&mut h, &mut tl, ids::LAYER_SWITCH, json!({"switch": "shy"}));
    assert!(!switch_on(&h.stage, &solid, "shy") && !switch_on(&h.stage, &shape, "shy"));
    h.stage.undo(&h.author).unwrap();
    assert!(switch_on(&h.stage, &solid, "shy") && switch_on(&h.stage, &shape, "shy"));
    // Check marks follow the selection.
    let tree = motion_menu_tree();
    let item = |id: &str| {
        tree.items()
            .into_iter()
            .find(|(i, _)| *i == id)
            .unwrap_or_else(|| panic!("no menu item {id}"))
            .1
            .clone()
    };
    let cx = bae_ui::menu::MenuContext::new(&h.stage, &h.resources, "motion");
    let shy = item("menu.layer.switches.shy");
    assert!(shy.is_enabled(&cx) && shy.is_checked(&cx));
    assert!(!item("menu.layer.switches.solo").is_checked(&cx));
    assert!(item("menu.layer.quality.best").is_checked(&cx));
    // Lock, then Unlock All Layers.
    select(&h, &[&solid]);
    run(
        &mut h,
        &mut tl,
        ids::LAYER_SWITCH,
        json!({"switch": "locked"}),
    );
    assert!(switch_on(&h.stage, &solid, "locked"));
    run(&mut h, &mut tl, ids::UNLOCK_ALL_LAYERS, json!(null));
    assert!(!switch_on(&h.stage, &solid, "locked"));
    // Hide Other Video, Show All Video.
    select(&h, &[&shape]);
    run(&mut h, &mut tl, ids::HIDE_OTHER_VIDEO, json!(null));
    assert!(!switch_on(&h.stage, &solid, "enabled") && switch_on(&h.stage, &shape, "enabled"));
    run(&mut h, &mut tl, ids::SHOW_ALL_VIDEO, json!(null));
    assert!(switch_on(&h.stage, &solid, "enabled"));
    h.stage.undo(&h.author).unwrap();
    assert!(
        !switch_on(&h.stage, &solid, "enabled"),
        "Show All Video undoes"
    );
    // 3D Layer and Quality.
    select(&h, &[&solid, &shape]);
    run(
        &mut h,
        &mut tl,
        ids::LAYER_SWITCH,
        json!({"switch": "three_d"}),
    );
    assert!(switch_on(&h.stage, &solid, "three_d") && switch_on(&h.stage, &shape, "three_d"));
    run(&mut h, &mut tl, ids::QUALITY, json!({"quality": "draft"}));
    assert_eq!(quality_of(&h.stage, &solid), "draft");
    assert_eq!(quality_of(&h.stage, &shape), "draft");
    let cx = bae_ui::menu::MenuContext::new(&h.stage, &h.resources, "motion");
    assert!(item("menu.layer.quality.draft").is_checked(&cx));
    assert!(!item("menu.layer.quality.best").is_checked(&cx));
    h.stage.undo(&h.author).unwrap();
    assert_eq!(quality_of(&h.stage, &solid), "best", "one undo step");
    assert_eq!(quality_of(&h.stage, &shape), "best");
    // Label, then Select Label Group.
    select(&h, &[&solid]);
    run(&mut h, &mut tl, ids::LABEL, json!({"label": 9}));
    assert_eq!(label_of(&h.stage, &solid), 9);
    select(&h, &[&shape]);
    run(&mut h, &mut tl, ids::LABEL, json!({"label": 9}));
    select(&h, &[&solid]);
    run(&mut h, &mut tl, ids::SELECT_LABEL_GROUP, json!(null));
    assert_eq!(h.session().lock().selected_layers.len(), 2);
    h.stage.undo(&h.author).unwrap();
    assert_ne!(label_of(&h.stage, &shape), 9);
    // Reveal All Modified Properties.
    run(
        &mut h,
        &mut tl,
        "motion.reveal",
        json!({"what": "modified", "add": false}),
    );
    assert!(matches!(
        h.session().lock().reveal.get(&solid),
        Some(crate::session::Reveal::Modified)
    ));
}

/// Edit ▸ Split Layer follows Preferences ▸ Composition ▸ Create Split Layers
/// Above Original Layer: the new layer goes above the original (default) or
/// below it.
#[test]
fn split_layer_follows_the_preference() {
    for above in [true, false] {
        let mut h = Harness::new();
        let mut prefs = bae_prefs::Preferences::default();
        prefs.set(bae_prefs::keys::SPLIT_LAYERS_ABOVE, above);
        h.resources
            .insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(prefs)));
        let (comp, solid, _shape) = h.comp_with_layers();
        {
            let s = h.session();
            let mut s = s.lock();
            s.selected_layers = vec![solid.clone()];
            let info = query::comp_info(&h.stage, &comp).unwrap();
            s.set_time(&info, Time(Time::FLICKS_PER_SECOND));
        }
        let mut tl = TimelinePanel::default();
        let r = h.action(&mut tl, Invocation::new(crate::actions::ids::SPLIT));
        h.apply(r);
        let layers = query::comp_layers(&h.stage, &comp);
        assert_eq!(layers.len(), 3);
        let i_old = layers.iter().position(|l| *l == solid).unwrap();
        let new = if above {
            &layers[i_old - 1]
        } else {
            &layers[i_old + 1]
        };
        assert_eq!(
            query::layer_timing(&h.stage, new).in_point,
            Time(Time::FLICKS_PER_SECOND),
            "above = {above}: {layers:?}"
        );
        h.stage.undo(&h.author).unwrap();
        assert_eq!(
            query::comp_layers(&h.stage, &comp).len(),
            2,
            "one undo step"
        );
    }
}

/// #204: Edit ▸ Copy / Paste / Cut with layers selected. Paste puts copies
/// above the topmost selected layer (named like Duplicate, selected), one undo
/// step; Cut removes the layers and they still paste, also into another comp.
/// Paste is greyed while the clipboard is empty.
#[test]
fn layer_copy_cut_and_paste() {
    use crate::actions::ids;
    let mut h = Harness::new();
    let (comp, solid, shape) = h.comp_with_layers();
    let mut tl = TimelinePanel::default();
    let run = |h: &mut Harness, tl: &mut TimelinePanel, id: &str| {
        let r = h.action(tl, Invocation::new(id));
        h.apply(r);
        h.frame("timeline", tl, Vec::new());
    };
    let tree = motion_menu_tree();
    let paste_item = tree
        .items()
        .into_iter()
        .find(|(_, s)| s.action == ids::PASTE)
        .unwrap()
        .1
        .clone();
    let paste_enabled = |h: &Harness| {
        paste_item.is_enabled(&bae_ui::menu::MenuContext::new(
            &h.stage,
            &h.resources,
            "motion",
        ))
    };
    assert!(!paste_enabled(&h), "nothing to paste yet");
    // Copy the solid, select the shape (on top), paste: the copy goes above it.
    h.session().lock().selected_layers = vec![solid.clone()];
    run(&mut h, &mut tl, ids::COPY);
    assert!(paste_enabled(&h));
    h.session().lock().selected_layers = vec![shape.clone()];
    run(&mut h, &mut tl, ids::PASTE);
    let layers = query::comp_layers(&h.stage, &comp);
    assert_eq!(layers.len(), 3);
    let pasted = layers[0].clone();
    assert_eq!(layers[1], shape);
    assert_eq!(query::display_name_of(&h.stage, &pasted), "Solid 2");
    assert_eq!(h.session().lock().selected_layers, vec![pasted.clone()]);
    h.stage.undo(&h.author).unwrap();
    assert_eq!(
        query::comp_layers(&h.stage, &comp).len(),
        2,
        "one undo step"
    );
    // Cut the solid, paste into a new comp.
    h.session().lock().selected_layers = vec![solid.clone()];
    run(&mut h, &mut tl, ids::CUT);
    assert!(!h.stage.has_prim(&solid), "Cut removes the layer");
    let other = PrimPath::new(
        h.run("comp.create", json!({"width": 320, "height": 240}))["path"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    h.session().lock().open_comp(&other);
    run(&mut h, &mut tl, ids::PASTE);
    let in_other = query::comp_layers(&h.stage, &other);
    assert_eq!(in_other.len(), 1);
    assert_eq!(
        query::layer_kind(&h.stage, &in_other[0]).as_deref(),
        Some("solid")
    );
}

#[test]
fn expression_errors_reach_the_session_banner_and_timeline() {
    let mut h = Harness::new();
    let (comp, _solid, shape) = h.comp_with_layers();
    h.run(
        "attr.set_expression",
        json!({"path": shape, "attr": "transform:rotation", "source": "nope * 2"}),
    );
    let mut viewer = CompositionViewer::new(None);
    let mut tl = TimelinePanel::default();
    h.frame("composition", &mut viewer, Vec::new());
    let errors = h.session().lock().expr_errors.clone();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].attr, "transform:rotation");
    let index = query::comp_layers(&h.stage, &comp)
        .iter()
        .position(|l| *l == shape)
        .unwrap()
        + 1;
    let text = crate::viewer::errors::describe(&h.stage, &errors[0]);
    let layer = query::display_name_of(&h.stage, &shape);
    let comp_name = query::display_name_of(&h.stage, &comp);
    assert!(
        text.starts_with(&format!(
            "Error in property 'Rotation' of layer {index} ('{layer}') in comp '{comp_name}'.\n"
        )),
        "{text}"
    );
    assert!(text.contains("nope"), "{text}");
    // The timeline draws the row with its warning (no invocations).
    h.session()
        .lock()
        .reveal
        .insert(shape.clone(), crate::session::Reveal::Animated);
    assert!(
        h.frame("timeline", &mut tl, Vec::new())
            .invocations
            .is_empty()
    );
    // Fixed: the errors go away.
    h.run(
        "attr.set_expression",
        json!({"path": shape, "attr": "transform:rotation", "source": "time * 2"}),
    );
    h.frame("composition", &mut viewer, Vec::new());
    assert!(h.session().lock().expr_errors.is_empty());
}
