//! Tool frame costs and the tool preferences.

use super::*;

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
fn tools_follow_the_preferences() {
    let mut h = H::new();
    let mut p = bae_prefs::Preferences::default();
    p.set(bae_prefs::keys::CENTER_SHAPE_ANCHOR, true);
    p.set("composition.default_position_separate_dimensions", true);
    p.set(
        "composition.synthetic_layers_duration_mode",
        "fixed_duration",
    );
    p.set(
        "composition.synthetic_layers_duration_timecode",
        "0:00:00:08",
    );
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
    let prim = h.stage.prim(&layer).unwrap();
    assert_eq!(
        query::static_value(&prim, bae_motion::dimensions::COMPONENTS[0]),
        Some(Value::Float(200.0))
    );
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    assert_eq!(
        query::static_value(&prim, "out_point"),
        Some(Value::Time(info.frame_rate.frame_to_time(8)))
    );
    assert!(close3(
        h.v3(&layer, "transform:position"),
        [200.0, 100.0, 0.0]
    ));
    // #679: the Pen with Separate Dimensions on centres the anchor through
    // X / Y Position (the layer evaluates those), not the combined value.
    h.select(&[]);
    h.tools().lock().set(Tool::Pen);
    let r = h.click(100.0, 100.0, Modifiers::NONE);
    h.apply(&r);
    let pen = query::comp_layers(&h.stage, &h.comp)[0].clone();
    let r = h.click(300.0, 100.0, Modifiers::NONE);
    let edits = &one(&r, "attr.set_many")["edits"];
    let attrs: Vec<_> = edits
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["path"] == json!(pen))
        .map(|e| e["attr"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        attrs,
        [
            "transform:anchor_point",
            "transform:position_x",
            "transform:position_y"
        ]
    );
    h.tools().lock().set(Tool::Selection);
    h.frame(Vec::new(), Modifiers::NONE);
    // Appearance > Cycle Mask Colors off.
    let solid = h.layer("solid");
    h.select(&[&solid]);
    let r = h.draw(Tool::Ellipse, (100.0, 50.0), (300.0, 150.0));
    let base = bae_prefs::Preferences::default().label_color(bae_motion::shape::mask_label(0));
    let fixed = json!([
        f64::from(base[0]) / 255.0,
        f64::from(base[1]) / 255.0,
        f64::from(base[2]) / 255.0,
        1.0
    ]);
    assert_eq!(one(&r, "mask.create")["color"], fixed);
    h.apply(&r);
    let r = h.draw(Tool::Rectangle, (120.0, 70.0), (220.0, 125.0));
    assert_eq!(one(&r, "mask.create")["color"], fixed);
}

#[test]
fn contrasting_mask_uses_the_rendered_pixel_at_the_start_of_the_drag() {
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    let mut h = H::new();
    let registry = std::sync::Arc::new(bae_gpu::viewport::ViewportTextureRegistry::new());
    h.resources.insert(gpu);
    h.resources.insert(registry.clone());
    h.viewer = CompositionViewer::new(None);
    h.frame(Vec::new(), Modifiers::NONE);
    let mut prefs = bae_prefs::Preferences::default();
    for i in 1..=16 {
        prefs.set(&format!("labels.label_color_{i}"), "#ffffff");
    }
    prefs.set("labels.label_color_11", "#000000");
    h.resources
        .insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(prefs)));
    let solid = path(
        &h.run(
            "layer.create",
            json!({"comp":h.comp,"kind":"solid","color":[1,1,1,1]}),
        )["path"],
    );
    h.select(&[&solid]);
    h.tools().lock().set(Tool::Rectangle);
    let at = h.screen(180.0, 140.0);
    h.move_to(at, Modifiers::NONE, Vec::new());
    let timeout = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        for (i, entry) in registry.snapshot().into_iter().enumerate() {
            if entry.texture.is_some() && entry.display_handle.is_none() {
                registry.set_display_handle(entry.id, Some(i as u64 + 1));
            }
        }
        h.frame(Vec::new(), Modifiers::NONE);
        if h.session().lock().info.color == Some([255; 4]) {
            break;
        }
        assert!(
            std::time::Instant::now() < timeout,
            "viewer pixel {:?}",
            h.session().lock().info.color
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let r = h.draw(Tool::Rectangle, (180.0, 140.0), (250.0, 175.0));
    assert_eq!(one(&r, "mask.create")["color"], json!([0., 0., 0., 1.]));
    let store = h
        .resources
        .get::<std::sync::Arc<bae_prefs::PrefsStore>>()
        .unwrap();
    let mut prefs = (*store.current()).clone();
    prefs.set("appearance.use_contrasting_color_for_mask_path", false);
    store.set(prefs).unwrap();
    let r = h.draw(Tool::Ellipse, (180.0, 140.0), (250.0, 175.0));
    assert_eq!(one(&r, "mask.create")["color"], json!([1., 1., 1., 1.]));
}
