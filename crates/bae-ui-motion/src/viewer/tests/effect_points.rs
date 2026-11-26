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
}

#[test]
fn particle_world_guides_follow_the_selected_layer_and_guide_controls() {
    let mut h = H::new();
    let mut app = bae_app::AppBuilder::new();
    app.add_module(bae_effects::EffectsModule);
    bae_motion::effect::register_effect_commands(
        &mut h.reg,
        app.build().registry::<bae_model::EffectDef>(),
    );
    let layer = h.solid([640., 360.], [320., 180.]);
    let fx = path(
        &h.run(
            "effect.apply",
            json!({"layers": [layer], "match_name": "CC Particle World"}),
        )["effects"][0],
    );
    h.select(&[&layer]);
    let set = |h: &mut H, id: u32, value: Value| {
        h.run(
            "attr.set_at",
            json!({"path": fx, "attr": format!("param:{id:04}"), "time": 0, "value": value}),
        );
    };
    let lines = |h: &mut H| {
        h.frame(vec![], Modifiers::NONE);
        h.shapes
            .iter()
            .filter_map(|s| match s.shape {
                egui::Shape::LineSegment { points, .. } => Some(points),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for id in [51, 52, 53, 55, 60, 61] {
        set(&mut h, id, Value::Bool(false));
    }
    let plain = lines(&mut h);
    for id in [51, 52, 55, 60, 61] {
        set(&mut h, id, Value::Bool(true));
        let shown = lines(&mut h);
        assert!(shown.len() > plain.len(), "guide {id}");
        set(&mut h, id, Value::Bool(false));
    }
    // A quarter layer-width on X becomes a quarter-width on Y after a 90° roll.
    set(&mut h, 51, Value::Bool(true));
    set(&mut h, 7, Value::Float(0.25));
    set(&mut h, 36, Value::Float(90.));
    let center = h.screen(320., 340.);
    let diagonal = vec2(4., 4.);
    assert!(lines(&mut h).contains(&[center - diagonal, center + diagonal]));
    tool_action(
        &h,
        crate::viewer::view3d::ids::SWITCH,
        json!({"view": "front"}),
    );
    let front = h.screen(480., 180.);
    assert!(lines(&mut h).contains(&[front - diagonal, front + diagonal]));
    tool_action(
        &h,
        crate::viewer::view3d::ids::SWITCH,
        json!({"view": "active_camera"}),
    );
    set(&mut h, 51, Value::Bool(false));
    set(&mut h, 7, Value::Float(0.));
    set(&mut h, 36, Value::Float(0.));
    set(&mut h, 55, Value::Bool(true));
    set(&mut h, 58, Value::Float(3.));
    let floor = lines(&mut h);
    assert_eq!(floor.len(), plain.len() + 8);
    set(&mut h, 57, Value::Int(3));
    assert_eq!(lines(&mut h), floor);
    set(&mut h, 56, Value::Int(2));
    assert_ne!(lines(&mut h), floor);
    h.select(&[]);
    let deselected = lines(&mut h);
    set(&mut h, 55, Value::Bool(false));
    assert_eq!(lines(&mut h), deselected);

    h.select(&[&layer]);
    set(&mut h, 53, Value::Bool(true));
    assert_eq!(lines(&mut h), plain);
    h.run(
        "attr.set_animated",
        json!({"path":fx,"attr":"param:0007","time":0,"animated":true}),
    );
    h.run("attr.set_at", json!({"path":fx,"attr":"param:0007","time":bae_model::Time::from_seconds(1.).0,"value":Value::Float(0.3)}));
    set(&mut h, 54, Value::Float(3.));
    assert_eq!(lines(&mut h).len(), plain.len() + 3);
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    h.session()
        .lock()
        .set_time(&info, bae_model::Time::from_seconds(0.5));
    assert_eq!(lines(&mut h).len(), plain.len() + 6);
}

#[test]
fn cryptomatte_click_commits_one_stable_object_id() {
    use bae_effects::exr_controls::{self, CryptoSelection};
    use bae_gpu::channels::{ChannelInfo, CryptoLayer};
    use std::sync::Arc;
    let mut h = H::new();
    let mut builder = bae_app::AppBuilder::new();
    builder.add_module(bae_effects::EffectsModule);
    let mut app = builder.build();
    let regs = crate::effects::EffectRegistries {
        defs: app.registry::<bae_model::EffectDef>(),
        infos: app.registry::<bae_effects::EffectInfo>(),
    };
    bae_motion::effect::register_effect_commands(&mut h.reg, regs.defs.clone());
    h.resources.insert(regs);
    let layer = h.layer("solid");
    h.select(&[&layer]);
    let applied = h.run(
        "effect.apply",
        json!({"layers":[layer],"match_name":"Cryptomatte"}),
    );
    let effect = path(&applied["effects"][0]);
    h.run("attr.set",json!({"path":effect,"attr":bae_model::effect::param_attr(3),"value":{"type":"bool","value":true}}));
    h.frame(Vec::new(), Modifiers::NONE);
    let dominant = (0..640 * 360)
        .map(|i| if i % 640 < 320 { 98 } else { 99 })
        .collect();
    let info = ChannelInfo {
        channels: vec![],
        size: [640, 360],
        crypto: vec![CryptoLayer {
            name: "Object".into(),
            ranks: vec![],
            manifest: Default::default(),
            dominant,
        }],
    };
    h.session().lock().channel_frame = Some(Arc::new(crate::gpu_preview::ChannelFrame {
        comp: h.comp.clone(),
        time: bae_model::Time::ZERO,
        layers: [(layer, Arc::new(info))].into(),
    }));
    h.tools().lock().set(Tool::Selection);
    let result = h.click(400., 125., Modifiers::NONE);
    assert_eq!(result.invocations.len(), 1);
    h.apply(&result);
    let value = h.value(&effect, &bae_model::effect::param_attr(1));
    let sel: CryptoSelection = exr_controls::read(value.as_ref());
    assert_eq!(sel.object, Some(99));
    assert_eq!(sel.layer, "Object");
}
