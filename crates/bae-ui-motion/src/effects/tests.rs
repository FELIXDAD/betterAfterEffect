//! The effects UI with the real built-in effects: the Effect menu's entries,
//! the apply actions, the Effect Controls panel and drag payloads.

use bae_app::{AppBuilder, Resources};
use bae_model::{AuthorId, CommandRegistry, PrimPath, Stage};
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
