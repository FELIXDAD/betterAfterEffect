use super::*;

#[test]
fn media_panels_edit_metadata_and_trim_footage_from_the_window_menu() {
    use bae_model::{FrameRate, PrimPath, Time, Value};
    use egui_kittest::kittest::Queryable as _;
    use serde_json::json;

    let mut h = gui();
    let file = tempfile::NamedTempFile::new().unwrap();
    let rate = FrameRate::FPS_30;
    let duration = rate.frame_to_time(90);
    h.state_mut().dispatch(Invocation::with_params(
        "item.import",
        json!({
            "files":[{"path":file.path(), "name":"Clip", "attrs":{"duration":Value::Time(duration),
                "frame_rate":Value::FrameRate(rate),"dc:title":Value::String("Original".into())}}]
        }),
    ));
    let item = h
        .state()
        .stage()
        .children(&PrimPath::new("/assets").unwrap())[0]
        .clone();
    let session = h.state().session();
    session.lock().selected_items = vec![item.clone()];
    session.lock().footage_view = Some(item.clone());
    assert!(h.state_mut().menu("menu.window.metadata"));
    settle(&mut h);
    let field = h
        .query_by(|n| {
            n.label().as_deref() == Some(bae_ui::tr("Document Title"))
                && n.role() == egui::accesskit::Role::TextInput
        })
        .expect("metadata title field")
        .rect()
        .center();
    click_at(&mut h, field);
    type_text(&mut h, "123");
    let v = bae_motion::query::static_value(&h.state().stage().prim(&item).unwrap(), "dc:title");
    assert!(matches!(v, Some(Value::String(s)) if s.contains("123")));
    assert!(h.state_mut().menu("menu.window.footage"));
    settle(&mut h);
    let button = h
        .query_by(|n| n.label().as_deref() == Some(bae_ui::tr("Set Out Point to Current Time")))
        .expect("mark out button")
        .rect()
        .center();
    click_at(&mut h, button);
    settle(&mut h);
    assert_eq!(
        bae_motion::query::footage_range(h.state().stage(), &item, duration),
        (Time::ZERO, rate.frame_to_time(1))
    );
    h.state_mut().dispatch(Invocation::with_params(
        "comp.create",
        json!({"name":"Trimmed"}),
    ));
    let comp = bae_motion::query::comps(h.state().stage())[0].clone();
    h.state_mut().dispatch(Invocation::with_params(
        "layer.create",
        json!({"comp":comp,"kind":"footage","source":item}),
    ));
    let layer = bae_motion::query::comp_layers(h.state().stage(), &comp)[0].clone();
    assert_eq!(
        bae_motion::query::layer_timing(h.state().stage(), &layer).out_point,
        rate.frame_to_time(1)
    );
    for id in ["menu.window.media_browser", "menu.window.progress"] {
        assert!(h.state_mut().menu(id));
        settle(&mut h);
        assert!(h.state_mut().menu(id));
        settle(&mut h);
    }
}
