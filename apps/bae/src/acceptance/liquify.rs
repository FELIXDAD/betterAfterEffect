use super::*;
use bae_effects::effects::liquify::{MATCH_NAME, MESH, Mesh};
use egui_kittest::kittest::Queryable as _;

#[test]
fn liquify_tool_opens_the_viewer_and_a_stroke_undoes_once() {
    let mut app = Gui::with_app(crate::app_builder().build());
    if !app.with_gpu() {
        return;
    }
    let gpu = app
        .app
        .resources()
        .get::<std::sync::Arc<bae_gpu::GpuContext>>()
        .unwrap()
        .clone();
    let views = std::sync::Arc::new(bae_gpu::viewport::ViewportTextureRegistry::new());
    app.app.insert_resource(views.clone());
    let mut h = gui_with(app);
    new_comp(&mut h);
    new_solid(&mut h);
    h.state_mut().invoke(Invocation::with_params(
        "motion.effect.apply",
        serde_json::json!({"match_name": MATCH_NAME}),
    ));
    settle(&mut h);
    let layer = h.state().layers()[0].clone();
    let effect = bae_motion::effect::effects_of(h.state().stage(), &layer)[0].clone();
    h.state_mut().invoke(Invocation::with_params(
        "attr.set_at",
        serde_json::json!({
            "path": effect, "attr":bae_model::effect::param_attr(10), "time":bae_model::Time::ZERO,
            "value":bae_model::Value::Bool(true)
        }),
    ));
    settle(&mut h);
    let tool = h.get_by_label("Bloat").rect().center();
    click_at(&mut h, tool);
    settle(&mut h);
    let area = h
        .root()
        .query_all_by_label("Liquify")
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap();
    assert!(area.height() > 100.);
    let center = area.center();
    let to = center + egui::vec2(24., 4.);
    // Register the same display handles the window renderer supplies. Read the
    // texture in the image shape that actually covers the layer viewer.
    for entry in views.snapshot() {
        views.set_display_handle(entry.id, Some(5000 + entry.id.raw()));
    }
    frame(&mut h);
    let handle = h
        .output()
        .shapes
        .iter()
        .find_map(|s| {
            if let egui::Shape::Mesh(m) = &s.shape
                && s.clip_rect.contains(center)
                && m.calc_bounds().contains(center)
                && let egui::TextureId::User(handle) = m.texture_id
            {
                Some(handle)
            } else {
                None
            }
        })
        .expect("the layer viewer displays its GPU image");
    let held = views
        .snapshot()
        .into_iter()
        .find(|v| v.display_handle == Some(handle))
        .unwrap();
    let pixels = |frame: &bae_gpu::viewport::ViewportTexture| {
        bae_gpu::transfer::read_texture(&gpu, frame.view.texture())
            .unwrap()
            .data
    };
    let baseline = pixels(held.texture.as_ref().unwrap());
    let revision = h.state().stage().revision();
    send(&mut h, Event::PointerMoved(center));
    send(
        &mut h,
        Event::PointerButton {
            pos: center,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
    );
    send(&mut h, Event::PointerMoved(to));
    settle(&mut h);
    let start = std::time::Instant::now();
    let mut changed_pixels = 0;
    while start.elapsed().as_secs() < 10 {
        frame(&mut h);
        let live = views.get(held.id).unwrap();
        // The viewport normally paints into the existing texture.
        let current = pixels(&live);
        changed_pixels = baseline
            .as_chunks::<4>()
            .0
            .iter()
            .zip(current.as_chunks::<4>().0)
            .filter(|(a, b)| a != b)
            .count();
        if changed_pixels != 0 {
            break;
        }
    }
    assert!(
        changed_pixels > 0,
        "the displayed GPU grid must move before pointer-up"
    );
    eprintln!("Liquify viewer: {changed_pixels} pixels changed during the drag");
    assert_eq!(
        h.state().stage().revision(),
        revision,
        "an active brush edits GPU data only"
    );
    send(
        &mut h,
        Event::PointerButton {
            pos: to,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    );
    let mesh = |h: &Ui| {
        let prim = h.state().stage().prim(&effect).unwrap();
        bae_motion::query::static_value(&prim, &bae_model::effect::param_attr(MESH))
            .and_then(|v| Mesh::from_value(&v))
    };
    let start = std::time::Instant::now();
    while mesh(&h).is_none() && start.elapsed().as_secs() < 10 {
        frame(&mut h);
    }
    assert!(mesh(&h).unwrap().reach() > 0.);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert!(mesh(&h).is_none());
}
