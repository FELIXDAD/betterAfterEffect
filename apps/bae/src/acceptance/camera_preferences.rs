use super::*;
use bae_motion::query;
use egui_kittest::kittest::Queryable as _;

#[test]
fn number_keys_switch_between_camera_tools_and_composition_markers() {
    let mut h = gui();
    new_comp(&mut h);
    let comp = h.state().active_comp().unwrap();
    let info = query::comp_info(h.state().stage(), &comp).unwrap();
    let tools = h
        .state()
        .app
        .resource::<bae_ui_motion::SharedTools>()
        .unwrap()
        .clone();
    h.state_mut().workspace.focus_panel("composition");
    key(&mut h, Modifiers::NONE, Key::Num1);
    assert_eq!(tools.current(), bae_ui_motion::Tool::OrbitCursor);

    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("3d.override_123456_for_gizmos", false);
    h.state().prefs.set(prefs.clone()).unwrap();
    settle(&mut h);
    let at = info.frame_rate.frame_to_time(7);
    h.state().session().lock().set_time(&info, at);
    key(&mut h, Modifiers::SHIFT, Key::Num6);
    h.state()
        .session()
        .lock()
        .set_time(&info, bae_model::Time::ZERO);
    key(&mut h, Modifiers::NONE, Key::Num6);
    assert_eq!(h.state().now(), at);
    assert_eq!(tools.current(), bae_ui_motion::Tool::OrbitCursor);
    prefs.set("3d.override_123456_for_gizmos", true);
    h.state().prefs.set(prefs).unwrap();
    settle(&mut h);
    key(&mut h, Modifiers::NONE, Key::Num2);
    assert_eq!(tools.current(), bae_ui_motion::Tool::PanCursor);
}

#[test]
fn wheel_and_drag_preferences_move_the_camera_in_the_chosen_direction() {
    let mut h = gui();
    new_comp(&mut h);
    let comp = h.state().active_comp().unwrap();
    h.state_mut().dispatch(Invocation::with_params(
        "layer.create",
        serde_json::json!({"comp":comp, "kind":"camera"}),
    ));
    settle(&mut h);
    let path = h.state().layers().pop().unwrap();
    let position = |h: &Ui| {
        let camera = h.state().stage().prim(&path).unwrap();
        match query::static_value(&camera, "transform:position").unwrap() {
            bae_model::Value::Vec3(p) => p[2],
            v => panic!("camera position: {v:?}"),
        }
    };
    let center = h
        .get_by_role_and_label(egui::accesskit::Role::Image, "Composition")
        .rect()
        .center();
    let pref = |h: &mut Ui, key: &str, value: &str| {
        let mut prefs = (*h.state().prefs.current()).clone();
        prefs.set(key, value);
        h.state().prefs.set(prefs).unwrap();
        settle(h);
    };
    for (mode, expected) in [
        ("zoom_comp", 0.0),
        ("dolly_camera", 1.0),
        ("reverse_dolly", -1.0),
    ] {
        pref(&mut h, "3d.mouse_wheel_behavior", mode);
        send(&mut h, Event::PointerMoved(center));
        let before = position(&h);
        send(
            &mut h,
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        );
        settle(&mut h);
        let delta = position(&h) - before;
        assert!(
            if expected == 0.0 {
                delta == 0.0
            } else {
                delta * expected > 0.0
            },
            "{mode}: {delta}"
        );
    }
    h.state_mut()
        .dispatch(Invocation::new(bae_ui_motion::tools::ids::CAMERA_DOLLY));
    for (mode, expected) in [("normal", 1.0), ("inverted", -1.0)] {
        pref(&mut h, "3d.drag_direction", mode);
        let before = position(&h);
        drag(
            &mut h,
            center,
            center + egui::vec2(0.0, 45.0),
            Modifiers::NONE,
        );
        assert!((position(&h) - before) * expected > 0.0, "{mode}");
    }
}
