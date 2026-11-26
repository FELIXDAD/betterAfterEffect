//! The effects UI with the real built-in effects: the Effect menu's entries,
//! the apply actions, the Effect Controls panel and drag payloads.

use bae_app::{AppBuilder, Resources};
use bae_model::{AuthorId, CommandRegistry, PrimPath, Stage, Value};
use bae_ui::menu::MenuContext;
use bae_ui::{ActionHandler, Invocation, Panel, PanelContext, PanelId, PanelRequests};
use egui::{Pos2, RawInput, Rect, vec2};
use serde_json::json;

use super::*;
use crate::session::SharedSession;

struct Fx {
    stage: Stage,
    reg: CommandRegistry,
    author: AuthorId,
    resources: Resources,
    ctx: egui::Context,
}

impl Fx {
    fn new() -> Self {
        let mut app = AppBuilder::new();
        app.add_module(bae_effects::EffectsModule);
        let mut app = app.build();
        let registries = EffectRegistries {
            defs: app.registry::<bae_model::EffectDef>(),
            infos: app.registry::<EffectInfo>(),
        };
        set_global(registries.clone());
        let mut reg = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        bae_motion::effect::register_effect_commands(&mut reg, registries.defs.clone());
        let mut resources = Resources::new();
        resources.insert(SharedSession::default());
        resources.insert(registries);
        Fx {
            stage: Stage::new(),
            reg,
            author: AuthorId::new("user:test").unwrap(),
            resources,
            ctx: bae_ui::testing::ctx_with_fonts(),
        }
    }

    fn run(&mut self, id: &str, params: serde_json::Value) -> serde_json::Value {
        self.reg
            .execute(&mut self.stage, &self.author, id, params)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .output
    }

    fn exec_all(&mut self, r: PanelRequests) {
        for inv in r.invocations {
            self.run(&inv.id, inv.params);
        }
    }

    /// A comp with one selected solid layer, active in the session.
    fn comp_with_layer(&mut self) -> PrimPath {
        let comp = self.run("comp.create", json!({"width": 64, "height": 32, "fps": 30}));
        let comp = PrimPath::new(comp["path"].as_str().unwrap()).unwrap();
        let out = self.run("layer.create", json!({"comp": comp, "kind": "solid"}));
        let layer = PrimPath::new(out["path"].as_str().unwrap()).unwrap();
        let session = self.resources.get::<SharedSession>().unwrap().clone();
        let mut s = session.lock();
        s.active_comp = Some(comp);
        s.selected_layers = vec![layer.clone()];
        layer
    }

    fn handle(&self, inv: Invocation) -> PanelRequests {
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
        assert!(EffectActions.handle(&inv, &mut cx));
        requests
    }

    fn frame(&self, kind: &str, panel: &mut dyn Panel) -> PanelRequests {
        let mut requests = PanelRequests::default();
        let id = PanelId::new(kind);
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 700.0))),
            ..Default::default()
        };
        let mut out = self.ctx.run_ui(raw, |ui| {
            let mut cx = PanelContext::new(
                &id,
                kind,
                &self.stage,
                &self.author,
                &self.resources,
                &mut requests,
            );
            panel.ui(ui, &mut cx);
        });
        out.textures_delta.clear();
        requests
    }
}

#[test]
fn menu_follows_the_effect_registry() {
    let mut fx = Fx::new();
    let category = "Blur & Sharpen";
    let path = ["Effect", category];
    let items = menu_items(&path, &[], category, "motion");
    assert_eq!(items.len(), 16, "AE 26 lists 16 Blur & Sharpen effects");
    let reg = registries(&fx.resources).unwrap();
    let enabled_titles = |fx: &Fx| -> Vec<&'static str> {
        let cx = MenuContext::new(&fx.stage, &fx.resources, "motion");
        items
            .iter()
            .filter(|(_, spec)| spec.is_enabled(&cx))
            .map(|(t, _)| *t)
            .collect()
    };
    // No layer selected: nothing to apply to.
    assert!(enabled_titles(&fx).is_empty());
    fx.comp_with_layer();
    // With a layer: exactly the effects the registry implements (whatever
    // other workstreams have added by now), the rest greyed like AE.
    let implemented: Vec<&'static str> = directory()
        .iter()
        .filter(|e| e.category == category && reg.def(e.match_name).is_some())
        .map(|e| e.name)
        .collect();
    assert!(implemented.contains(&"Gaussian Blur"));
    assert_eq!(enabled_titles(&fx), implemented);
    // Unregistering an effect greys its entry at once (plugins unload). On
    // registries of their own: other tests share the process-wide ones.
    let mut app = AppBuilder::new();
    app.add_module(bae_effects::EffectsModule);
    let mut app = app.build();
    let own = EffectRegistries {
        defs: app.registry::<bae_model::EffectDef>(),
        infos: app.registry::<EffectInfo>(),
    };
    own.defs
        .unregister(&bae_render::effect::registry_id("ADBE Gaussian Blur 2"))
        .expect("registered");
    let mut resources = Resources::new();
    resources.insert(fx.resources.get::<SharedSession>().unwrap().clone());
    resources.insert(own);
    let cx = MenuContext::new(&fx.stage, &resources, "motion");
    let greyed = items.iter().find(|(t, _)| *t == "Gaussian Blur").unwrap();
    assert!(!greyed.1.is_enabled(&cx));
}

#[test]
fn apply_last_effect_and_remove_all() {
    let mut fx = Fx::new();
    let layer = fx.comp_with_layer();
    let r = fx.handle(Invocation::with_params(
        APPLY,
        json!({"match_name": "ADBE Tint"}),
    ));
    assert_eq!(r.invocations.len(), 1, "one undo step");
    fx.exec_all(r);
    let r = fx.handle(Invocation::new(LAST_EFFECT));
    fx.exec_all(r);
    let effects = bae_motion::effect::effects_of(&fx.stage, &layer);
    assert_eq!(effects.len(), 2);
    let names: Vec<String> = effects
        .iter()
        .map(|e| bae_motion::query::display_name_of(&fx.stage, e))
        .collect();
    assert_eq!(names, ["Tint", "Tint 2"]);
    let r = fx.handle(Invocation::new(REMOVE_ALL));
    fx.exec_all(r);
    assert!(bae_motion::effect::effects_of(&fx.stage, &layer).is_empty());
}

#[test]
fn effect_controls_show_definition_controls() {
    let mut fx = Fx::new();
    let layer = fx.comp_with_layer();
    for m in ["ADBE Gaussian Blur 2", "ADBE Easy Levels2", "ADBE Tint"] {
        fx.run("effect.apply", json!({"layers": [layer], "match_name": m}));
    }
    let mut panel = crate::EffectControlsPanel::default();
    // Draw twice (layout settles); drawing alone changes nothing.
    assert!(fx.frame("effect_controls", &mut panel).is_empty());
    assert!(fx.frame("effect_controls", &mut panel).is_empty());
    // Labels and options come from the definitions.
    let reg = registries(&fx.resources).unwrap();
    let blur = reg.def("ADBE Gaussian Blur 2").unwrap();
    let dims = blur.param(2).unwrap();
    // Other tests switch the global UI language: accept either.
    let label = param_label("ADBE Gaussian Blur 2", dims);
    assert!(
        matches!(label.as_str(), "Blur Dimensions" | "模糊方向"),
        "{label}"
    );
    let options = option_labels("ADBE Gaussian Blur 2", dims);
    assert!(
        options == ["Horizontal and Vertical", "Horizontal", "Vertical"]
            || options == ["水平和垂直", "水平", "垂直"],
        "{options:?}"
    );
    let info = reg.info("ADBE Gaussian Blur 2").unwrap();
    assert_eq!(
        info.param_options_zh,
        vec![(
            2,
            vec!["水平和垂直".to_owned(), "水平".into(), "垂直".into()]
        )]
    );
    let levels = reg.info("ADBE Easy Levels2").unwrap();
    assert!(
        levels
            .param_options_zh
            .iter()
            .any(|(id, o)| *id == 8 && o.len() == 3)
    );
    // The timeline's property names use the definitions too.
    let blurriness = param_label_of("ADBE Gaussian Blur 2", 1);
    assert!(
        matches!(blurriness.as_deref(), Some("Blurriness" | "模糊度")),
        "{blurriness:?}"
    );
}

#[test]
fn effects_panel_lists_ae_categories() {
    let mut fx = Fx::new();
    fx.comp_with_layer();
    let mut panel = browser::EffectsPanel::default();
    assert!(fx.frame("effects", &mut panel).is_empty());
    assert!(same_in_chinese("CC Radial Blur"));
    assert!(!same_in_chinese("Gaussian Blur"));
    let name = effect_name("ADBE Gaussian Blur 2", None);
    assert!(
        matches!(name.as_str(), "Gaussian Blur" | "高斯模糊"),
        "{name}"
    );
}

/// Every text the panel drew.
fn drawn_texts(fx: &Fx, panel: &mut dyn Panel) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.galley.text().to_owned()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 700.0))),
        ..Default::default()
    };
    let mut requests = PanelRequests::default();
    let id = PanelId::new("effect_controls");
    let mut out = fx.ctx.run_ui(raw, |ui| {
        let mut cx = PanelContext::new(
            &id,
            "effect_controls",
            &fx.stage,
            &fx.author,
            &fx.resources,
            &mut requests,
        );
        panel.ui(ui, &mut cx);
    });
    out.textures_delta.clear();
    let mut texts = Vec::new();
    for s in &out.shapes {
        collect(&s.shape, &mut texts);
    }
    texts
}

#[test]
fn mask_params_pick_a_mask_of_the_layer() {
    let mut fx = Fx::new();
    let layer = fx.comp_with_layer();
    let mask = |fx: &mut Fx| {
        let out = fx.run(
            "mask.create",
            json!({"layer": layer, "kind": "rect", "position": [32.0, 16.0], "size": [20.0, 10.0]}),
        );
        PrimPath::new(out["mask"].as_str().unwrap()).unwrap()
    };
    let (m1, m2) = (mask(&mut fx), mask(&mut fx));
    let out = fx.run(
        "effect.apply",
        json!({"layers": [layer], "match_name": "ADBE Fill"}),
    );
    let effect = PrimPath::new(out["effects"][0].as_str().unwrap()).unwrap();
    assert_eq!(layer_masks(&fx.stage, &effect), [m1.clone(), m2.clone()]);

    // Fill's Fill Mask as it is once the renderer feeds it (on registries of
    // this test's own: other tests share the process-wide ones).
    let mut app = AppBuilder::new();
    app.add_module(bae_effects::EffectsModule);
    let mut app = app.build();
    let own = EffectRegistries {
        defs: app.registry::<bae_model::EffectDef>(),
        infos: app.registry::<EffectInfo>(),
    };
    let id = bae_render::effect::registry_id("ADBE Fill");
    let mut fill = (*own.defs.get(&id).unwrap()).clone();
    fill.params[0].flags.disabled = false;
    own.defs
        .replace(bae_app::Owner::Core, id, std::sync::Arc::new(fill))
        .unwrap();
    fx.resources.insert(own);

    let mut panel = crate::EffectControlsPanel::default();
    let name = bae_motion::query::display_name_of(&fx.stage, &m2);
    let none = |t: &[String]| t.iter().any(|t| t == "None" || t == "无") && !t.contains(&name);
    let texts = drawn_texts(&fx, &mut panel);
    assert!(none(&texts), "{texts:?}");
    // Choosing a mask writes the parameter: one undo step.
    let attr = bae_model::effect::param_attr(1);
    fx.run(
        "attr.set_at",
        json!({"path": effect, "attr": attr, "time": 0, "value": Value::PrimRef(m2.clone())}),
    );
    let texts = drawn_texts(&fx, &mut panel);
    assert!(texts.contains(&name), "{name} in {texts:?}");
    fx.stage.undo(&fx.author).unwrap();
    assert!(none(&drawn_texts(&fx, &mut panel)));
    // "None" writes an empty reference; a deleted mask also shows "None".
    fx.run(
        "attr.set_at",
        json!({"path": effect, "attr": attr, "time": 0, "value": Value::String(String::new())}),
    );
    fx.run(
        "attr.set_at",
        json!({"path": effect, "attr": attr, "time": 0, "value": Value::PrimRef(m1.clone())}),
    );
    fx.run("prim.delete", json!({"path": m1}));
    assert!(none(&drawn_texts(&fx, &mut panel)));
}

/// With Liquify shown in the Layer panel, a drag warps its Distortion Mesh:
/// previewed live, one `attr.set_at` on release.
#[test]
fn liquify_brush_in_the_layer_panel_warps_the_mesh() {
    let mut fx = Fx::new();
    let layer = fx.comp_with_layer();
    fx.run(
        "effect.apply",
        json!({"layers": [layer], "match_name": "ADBE Liquify"}),
    );
    let session = fx.resources.get::<SharedSession>().unwrap().clone();
    session.lock().layer_panel = Some(layer.clone());
    let mut panel = crate::layer_panel::LayerPanel::new(None);
    panel.show = crate::layer_panel::Show::Effect(0);
    let frame = |fx: &Fx, panel: &mut crate::layer_panel::LayerPanel, events: Vec<egui::Event>| {
        let mut requests = PanelRequests::default();
        let id = PanelId::new("layer");
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 700.0))),
            events,
            ..Default::default()
        };
        let mut out = fx.ctx.run_ui(raw, |ui| {
            let mut cx = PanelContext::new(
                &id,
                "layer",
                &fx.stage,
                &fx.author,
                &fx.resources,
                &mut requests,
            );
            panel.ui(ui, &mut cx);
        });
        out.textures_delta.clear();
        requests
    };
    frame(&fx, &mut panel, Vec::new());
    // The 64 × 32 solid at 100 % in the middle of the 900 × 650 picture.
    let at = |x: f32| egui::pos2(418.0 + x, 325.0);
    let button = |x: f32, pressed: bool| egui::Event::PointerButton {
        pos: at(x),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let mut last = PanelRequests::default();
    for events in [
        vec![egui::Event::PointerMoved(at(20.0))],
        vec![button(20.0, true)],
        vec![egui::Event::PointerMoved(at(30.0))],
        vec![egui::Event::PointerMoved(at(40.0))],
        vec![button(40.0, false)],
    ] {
        last = frame(&fx, &mut panel, events);
    }
    let set = last
        .invocations
        .iter()
        .find(|i| i.id == "attr.set_at")
        .expect("the mesh is set");
    assert_eq!(set.params["attr"], "param:0010");
    fx.exec_all(last);
    let effect = bae_motion::effect::effects_of(&fx.stage, &layer)[0].clone();
    let v =
        bae_motion::query::static_value(&fx.stage.prim(&effect).unwrap(), "param:0010").unwrap();
    let mesh = bae_effects::effects::liquify::Mesh::from_value(&v).expect("a mesh");
    assert!(mesh.reach() > 1.0, "warped");
}

#[test]
fn animation_presets_save_and_apply_effects_with_their_values() {
    let mut fx = Fx::new();
    let layer = fx.comp_with_layer();
    let out = fx.run(
        "effect.apply",
        json!({"layers": [layer], "match_name": "ADBE Gaussian Blur 2"}),
    );
    let blur = PrimPath::new(out["effects"][0].as_str().unwrap()).unwrap();
    fx.run(
        "attr.set",
        json!({"path": blur, "attr": "param:1", "value": {"type": "float", "value": 25.0}}),
    );
    // Save: the layer's effects and their values.
    let preset = crate::presets::preset_of(&fx.stage, &[blur], bae_model::Time::ZERO);
    assert_eq!(preset.effects[0].match_name, "ADBE Gaussian Blur 2");
    assert_eq!(
        preset.effects[0].values.get("param:1"),
        Some(&Value::Float(25.0))
    );
    // Apply to another layer: the effect with the stored value, one step.
    let comp = bae_motion::query::owning_comp(&fx.stage, &layer).unwrap();
    let other = fx.run("layer.create", json!({"comp": comp, "kind": "solid"}));
    let other = PrimPath::new(other["path"].as_str().unwrap()).unwrap();
    let out = fx.run(
        "effect.apply_preset",
        json!({"layers": [other], "effects": preset.effects}),
    );
    let applied = PrimPath::new(out["effects"][0].as_str().unwrap()).unwrap();
    let v = fx.stage.prim(&applied).unwrap();
    assert_eq!(
        bae_motion::query::static_value(&v, "param:1"),
        Some(Value::Float(25.0))
    );
}

/// The text and enabled state users get from the actual panel menu.
fn menu_states(fx: &Fx, panel: &mut dyn Panel) -> Vec<(String, bool, bool)> {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let id = PanelId::new("menu-test");
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, 900.0))),
        ..Default::default()
    };
    let mut output = ctx.run_ui(raw, |ui| {
        let mut requests = PanelRequests::default();
        let mut cx = PanelContext::new(
            &id,
            "Panel",
            &fx.stage,
            &fx.author,
            &fx.resources,
            &mut requests,
        );
        panel.menu(ui, &mut cx);
    });
    output.textures_delta.clear();
    let mut nodes = output.platform_output.accesskit_update.unwrap().nodes;
    nodes.sort_by(|(_, a), (_, b)| {
        let y = |node: &egui::accesskit::Node| node.bounds().map_or(0.0, |r| r.y0);
        y(a).total_cmp(&y(b))
    });
    nodes
        .into_iter()
        .filter_map(|(_, n)| {
            let label = n.label()?;
            let zh = if label == "Save Animation Preset..." {
                format!(
                    "{}...",
                    bae_ui::i18n::translation("Save Animation Preset").unwrap()
                )
            } else {
                bae_ui::i18n::translation(label).unwrap_or(label).to_owned()
            };
            Some((
                zh,
                !n.is_disabled(),
                n.toggled() == Some(egui::accesskit::Toggled::True),
            ))
        })
        .collect()
}

#[test]
fn panel_menus_match_chinese_and_selection() {
    let _app = {
        let mut builder = AppBuilder::new();
        builder.add_module(crate::MotionUiModule);
        builder.build()
    };
    let mut fx = Fx::new();
    let layer = fx.comp_with_layer();
    let mut browser = browser::EffectsPanel::default();
    fx.run("project.set_settings", json!({"bit_depth": "8"}));
    fx.run(
        "effect.apply",
        json!({"layers":[layer],"match_name":"ADBE Gaussian Blur 2"}),
    );
    let before = menu_states(&fx, &mut browser);
    let expected = [
        ("保存动画预设...", true, false),
        ("浏览预设...", true, false),
        ("类别", true, true),
        (
            if cfg!(windows) {
                "资源管理器文件夹"
            } else {
                "查找程序文件夹"
            },
            true,
            false,
        ),
        ("按字母顺序", true, false),
        ("显示所有颜色深度的效果", false, true),
        ("仅显示 GPU 能够实现的效果", true, false),
        ("显示效果", true, true),
        ("显示动画预设", true, true),
        (
            if cfg!(windows) {
                "在资源管理器中显示"
            } else {
                "在 Finder 中显示"
            },
            false,
            false,
        ),
        ("刷新列表", true, false),
        ("管理效果...", true, false),
    ];
    let observed: Vec<_> = before
        .iter()
        .map(|(label, enabled, checked)| (label.as_str(), *enabled, *checked))
        .collect();
    assert_eq!(observed, expected);
    fx.run("project.set_settings", json!({"bit_depth": "32"}));
    assert!(menu_states(&fx, &mut browser).contains(&(
        "显示所有颜色深度的效果".into(),
        true,
        true
    )));
    let tools = crate::tools::SharedTools::default();
    fx.resources.insert(tools.clone());
    let mut brushes = crate::paint_panels::BrushesPanel::default();
    let brush = menu_states(&fx, &mut brushes);
    assert_eq!(brush.len(), 9);
    assert!(
        brush
            .iter()
            .all(|(name, enabled, checked)| !enabled && *checked == (name == "小缩览图"))
    );
    tools.lock().set(crate::tools::Tool::Brush);
    let brush = menu_states(&fx, &mut brushes);
    for (label, enabled) in [
        ("新建画笔...", true),
        ("重命名画笔...", false),
        ("删除画笔", false),
        ("大列表", true),
        ("重置画笔笔尖...", true),
    ] {
        assert!(brush.contains(&(label.into(), enabled, false)), "{label}");
    }
    assert!(menu_states(&fx, &mut crate::paint_panels::PaintPanel).is_empty());
    assert!(
        menu_states(&fx, &mut crate::EffectControlsPanel::default())
            .iter()
            .any(|(label, enabled, _)| label == "显示动画预设" && *enabled)
    );
}

#[test]
fn effect_controls_flyout_uses_only_the_measured_panel_commands() {
    let fx = Fx::new();
    let mut panel = crate::panels::EffectControlsPanel::default();
    let id = PanelId::new("effect_controls");
    let mut requests = PanelRequests::default();
    let mut cx = PanelContext::new(
        &id,
        "Effect Controls",
        &fx.stage,
        &fx.author,
        &fx.resources,
        &mut requests,
    );
    let mut app = AppBuilder::new();
    app.add_module(crate::MotionUiModule);
    let _app = app.build();
    fx.ctx.enable_accesskit();
    let mut out = fx
        .ctx
        .run_ui(Default::default(), |ui| panel.menu(ui, &mut cx));
    out.textures_delta.clear();
    let tree = out.platform_output.accesskit_update.unwrap();
    let rows: Vec<_> = tree
        .nodes
        .iter()
        .filter_map(|(_, n)| {
            let label = n.label()?;
            Some((label, !n.is_disabled()))
        })
        .collect();
    assert_eq!(
        rows,
        [
            (bae_ui::tr("Show Animation Presets"), true),
            (bae_ui::tr("Manage Effects..."), true)
        ]
    );
    assert_eq!(
        bae_ui::i18n::translation("Show Animation Presets"),
        Some("显示动画预设")
    );
    assert_eq!(
        bae_ui::i18n::translation("Manage Effects..."),
        Some("管理效果...")
    );
}
