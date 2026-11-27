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
    key(&mut h, Modifiers::ALT, Key::W);
    let edge = h.get_by_label("Refine Edge Canvas").rect().center();
    drag(&mut h, edge, edge + egui::vec2(15., 20.), Modifiers::NONE);
    let effect = bae_motion::effect::effects_of(h.state().stage(), &layer)[0].clone();
    assert!(
        h.state()
            .stage()
            .prim(&effect)
            .unwrap()
            .value("param:50005")
            .is_some()
    );
    key(&mut h, modifiers, key_z);
    assert!(
        h.state()
            .stage()
            .prim(&effect)
            .unwrap()
            .value("param:50005")
            .is_none()
    );
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

#[test]
fn content_aware_fill_generates_a_masked_sequence_and_undo_removes_it() {
    use bae_model::Value;
    use serde_json::json;
    let Some(gpu) = gpu() else { return };
    let mut app = crate::app_builder().build();
    let dir = tempfile::tempdir().unwrap();
    let project = bae_ui::files::ProjectFile::default();
    project.set(Some(dir.path().join("fill.bae")));
    app.insert_resource(project);
    app.get_registry::<dyn bae_gpu::WarmUp>()
        .unwrap()
        .get("vision.sequence")
        .unwrap()
        .run(&gpu)
        .unwrap();
    let mut h = gui_with(Gui::with_app(app));
    let run =
        |h: &mut Ui, id: &str, args| h.state_mut().dispatch(Invocation::with_params(id, args));
    run(
        &mut h,
        "comp.create",
        json!({"name":"Fill","width":64,"height":32,"fps":25.,"duration":56448000}),
    );
    let comp = bae_motion::query::comps(h.state().stage())[0].clone();
    h.state().session().lock().open_comp(&comp);
    run(
        &mut h,
        "layer.create",
        json!({"comp":comp,"kind":"solid","color":[1.,0.,0.,1.]}),
    );
    let layer = h.state().layers()[0].clone();
    run(
        &mut h,
        "mask.create",
        json!({"layer":layer,"kind":"rect","mode":"subtract","position":[32,16],"size":[14,10]}),
    );
    h.state().session().lock().selected_layers = vec![layer.clone()];
    h.state_mut().invoke(Invocation::with_params(
        "window.panel",
        json!({"id":"content_aware_fill"}),
    ));
    settle(&mut h);
    let number = h
        .query_all_by_label("Alpha Expansion")
        .map(|node| node.rect())
        .max_by(|a, b| a.right().total_cmp(&b.right()))
        .expect("alpha expansion control")
        .center();
    let end = number + egui::vec2(24., 0.);
    send(&mut h, Event::PointerMoved(number));
    send(
        &mut h,
        Event::PointerButton {
            pos: number,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
    );
    send(&mut h, Event::PointerMoved(end));
    let live = h
        .state()
        .session()
        .lock()
        .scrub
        .as_ref()
        .filter(|edit| edit.path == layer && edit.attr == "vision:fill_expansion")
        .and_then(|edit| edit.value.as_f64())
        .expect("live expansion during drag");
    assert!(live > 0.);
    send(
        &mut h,
        Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    );
    settle(&mut h);
    let expansion = |h: &Ui| {
        h.state()
            .stage()
            .prim(&layer)
            .and_then(|p| p.value("vision:fill_expansion").and_then(Value::as_f64))
            .unwrap_or(0.)
    };
    assert_eq!(expansion(&h), live);
    let (modifiers, z) = cmd(Key::Z);
    key(&mut h, modifiers, z);
    assert_eq!(expansion(&h), 0.);
    h.get_by_label("Generate Fill Layer").click();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(40);
    loop {
        settle(&mut h);
        if h.state().layers().len() == 2 {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "fill: {:?}",
            painted_texts(&h)
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let output = h.state().layers()[0].clone();
    let prim = h.state().stage().prim(&output).unwrap();
    assert_eq!(
        prim.value("parent").and_then(Value::as_prim_ref),
        Some(&layer)
    );
    let asset = prim
        .value("source")
        .and_then(Value::as_prim_ref)
        .unwrap()
        .clone();
    let footage = h.state().stage().prim(&asset).unwrap();
    // Like AE: "Fill 1" footage inside a "Fills" project folder.
    let stage = h.state().stage();
    assert_eq!(bae_motion::query::display_name_of(stage, &asset), "Fill 1");
    assert_eq!(
        bae_motion::query::display_name_of(stage, &asset.parent().unwrap()),
        "Fills"
    );
    let file = std::path::Path::new(
        footage
            .value("file:original")
            .and_then(Value::as_str)
            .unwrap(),
    );
    for name in ["frame_00000000.png", "frame_00000001.png"] {
        let mut png = png::Decoder::new(std::io::BufReader::new(
            std::fs::File::open(file.with_file_name(name)).unwrap(),
        ))
        .read_info()
        .unwrap();
        let mut buf = vec![0; png.output_buffer_size().unwrap()];
        let f = png.next_frame(&mut buf).unwrap();
        assert_eq!((f.width, f.height), (64, 32));
        let i = (16 * 64 + 32) * 4;
        assert!(
            buf[i] > 245 && buf[i + 3] > 245,
            "hole filled from red layer, got {:?}",
            &buf[i..i + 4]
        );
        assert_eq!(buf[3], 0, "original outside the mask stays clear");
    }
    let (modifiers, z) = cmd(Key::Z);
    key(&mut h, modifiers, z);
    assert_eq!(h.state().layers(), vec![layer]);
    assert!(!h.state().stage().has_prim(&asset));
}
