use super::*;
use egui_kittest::kittest::Queryable as _;

#[test]
fn roto_brush_layer_strokes_and_undo() {
    let mut h = gui_with(Gui::with_app(crate::app_builder().build()));
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let layer = h.state().layers()[0].clone();
    key(&mut h, Modifiers::ALT, Key::W);
    h.state_mut().invoke(Invocation::with_params(
        "window.panel",
        serde_json::json!({"id":"layer"}),
    ));
    settle(&mut h);
    let center = h.root().get_by_label("Roto Brush Canvas").rect().center();
    drag(
        &mut h,
        center,
        center + egui::vec2(30., 0.),
        Modifiers::NONE,
    );
    let read = |h: &Ui| {
        let effect = bae_motion::effect::effects_of(h.state().stage(), &layer)
            .into_iter()
            .next()?;
        let prim = h.state().stage().prim(&effect)?;
        match prim.value("param:0000")? {
            bae_model::Value::FloatList(v) => Some(v.clone()),
            _ => None,
        }
    };
    let fg = read(&h).expect("foreground stroke on a new Roto Brush effect");
    drag(
        &mut h,
        center + egui::vec2(0., 22.),
        center + egui::vec2(30., 22.),
        Modifiers::ALT,
    );
    let both = read(&h).unwrap();
    assert!(both.len() > fg.len());
    let (background, tail) = both[fg.len()..].as_chunks::<6>();
    assert!(tail.is_empty() && background.iter().all(|s| s[1] < 0.));
    let (modifiers, key_z) = cmd(Key::Z);
    key(&mut h, modifiers, key_z);
    assert_eq!(read(&h), Some(fg));
    key(&mut h, modifiers, key_z);
    assert_eq!(read(&h), None);
}

#[test]
fn freeze_bakes_frames_and_unfreeze_keeps_the_seed_stroke() {
    let Some(gpu) = gpu() else { return };
    let mut app = crate::app_builder().build();
    let directory = tempfile::tempdir().unwrap();
    let project = bae_ui::files::ProjectFile::default();
    project.set(Some(directory.path().join("freeze.bae")));
    app.insert_resource(project);
    app.get_registry::<dyn bae_gpu::WarmUp>()
        .unwrap()
        .get("vision.sequence")
        .unwrap()
        .run(&gpu)
        .unwrap();
    let mut h = gui_with(Gui::with_app(app));
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let layer = h.state().layers()[0].clone();
    let comp = h.state().active_comp().unwrap();
    let dt = bae_motion::query::comp_info(h.state().stage(), &comp)
        .unwrap()
        .frame_duration()
        .0;
    h.state_mut().dispatch(Invocation::with_params("roto.stroke",serde_json::json!({
        "layer":layer,"time":0,"radius":8.0,"background":false,"points":[[100.,100.],[160.,100.]]
    })));
    let fx = bae_motion::effect::effects_of(h.state().stage(), &layer)[0].clone();
    h.state_mut().dispatch(Invocation::with_params("attr.set_at",serde_json::json!({
        "path":fx,"attr":"param:50001","time":0,"value":bae_model::Value::Vec2([0.,(2*dt) as f64])
    })));
    key(&mut h, Modifiers::ALT, Key::W);
    h.state_mut().invoke(Invocation::with_params(
        "window.panel",
        serde_json::json!({"id":"layer"}),
    ));
    settle(&mut h);
    h.get_by_label("Freeze").click();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(40);
    loop {
        settle(&mut h);
        if h.query_by_label("Unfreeze").is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "freeze: {:?}",
            painted_texts(&h)
        );
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    let matte = h
        .state()
        .stage()
        .prim(&fx)
        .unwrap()
        .value("param:50002")
        .and_then(bae_model::Value::as_prim_ref)
        .unwrap()
        .clone();
    assert_eq!(
        h.state().stage().prim(&matte).unwrap().value("enabled"),
        Some(&bae_model::Value::Bool(false))
    );
    h.get_by_label("Unfreeze").click();
    settle(&mut h);
    assert!(!h.state().stage().has_prim(&matte));
    assert!(
        h.state()
            .stage()
            .prim(&fx)
            .unwrap()
            .value("param:0000")
            .is_some()
    );
}
