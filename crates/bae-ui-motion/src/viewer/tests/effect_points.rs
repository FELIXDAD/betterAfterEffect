//! Effect point crosshairs.

use super::*;

#[test]
fn effect_point_crosshairs_drag_with_the_selection_tool() {
    let mut h = H::new();
    let mut app = bae_app::AppBuilder::new();
    app.add_module(bae_effects::EffectsModule);
    let mut app = app.build();
    let registries = crate::effects::EffectRegistries {
        defs: app.registry::<bae_model::EffectDef>(),
        infos: app.registry::<bae_effects::EffectInfo>(),
    };
    bae_motion::effect::register_effect_commands(&mut h.reg, registries.defs.clone());
    h.resources.insert(registries);
    let solid = h.layer("solid");
    h.select(&[&solid]);
    let out = h.run(
        "effect.apply",
        json!({"layers": [solid], "match_name": "ADBE Ramp"}),
    );
    let fx = path(&out["effects"][0]);
    let start = bae_model::effect::param_attr(1);
    h.run(
        "attr.set",
        json!({"path": fx, "attr": start, "value": {"type": "vec2", "value": [100.0, 50.0]}}),
    );
    let position = h.value(&solid, "transform:position");
    // Only selected effects show (and drag) their points.
    h.session().lock().selected_effects = vec![fx.clone()];
    h.frame(Vec::new(), Modifiers::NONE);
    h.tools().lock().set(Tool::Selection);
    h.drag_to((100.0, 50.0), (200.0, 80.0), Modifiers::NONE);
    let r = h.release((200.0, 80.0), Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "one undo step");
    h.apply(&r);
    assert!(close(vec2v(h.value(&fx, &start)), [200.0, 80.0]));
    assert_eq!(
        h.value(&solid, "transform:position"),
        position,
        "the layer stays"
    );
    h.stage.undo(&h.author).unwrap();
    assert!(close(vec2v(h.value(&fx, &start)), [100.0, 50.0]));
    // View Options ▸ Effect Controls off: no crosshair to grab.
    h.session().lock().layer_controls.effect_controls = false;
    h.frame(Vec::new(), Modifiers::NONE);
    h.drag_to((100.0, 50.0), (200.0, 80.0), Modifiers::NONE);
    let r = h.release((200.0, 80.0), Modifiers::NONE);
    h.apply(&r);
    assert!(close(vec2v(h.value(&fx, &start)), [100.0, 50.0]));
}
