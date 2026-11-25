//! 3D views and cameras.

use super::*;

#[test]
fn three_d_views_switch_and_3d_layers_are_seen_through_their_camera() {
    use crate::viewer::view3d::{View3d, ids};
    use bae_render::camera::OrthoView;
    let mut h = H::new();
    // A 3D solid one default zoom behind z = 0 (640 × 50 / 36 px): the default
    // camera shows it at half size, halfway to the comp centre.
    let far = h.solid([100.0, 100.0], [480.0, 180.0]);
    h.run(
        "attr.set",
        json!({"path": far, "attr": "three_d", "value": {"type": "bool", "value": true}}),
    );
    let z = 640.0 * 50.0 / 36.0;
    h.run(
        "attr.set",
        json!({"path": far, "attr": "transform:position", "value": {"type": "vec3", "value": [480.0, 180.0, z]}}),
    );
    let view = |h: &H| {
        h.session()
            .lock()
            .views3d
            .get(&h.comp)
            .map_or(View3d::Active, |v| v.current)
    };
    let pick = |h: &mut H, x: f64| {
        h.session().lock().selected_layers.clear();
        h.click(x, 180.0, Modifiers::NONE);
        !h.session().lock().selected_layers.is_empty()
    };
    h.frame(Vec::new(), Modifiers::NONE);
    assert!(pick(&mut h, 400.0), "Active Camera: at x 375–425");
    assert!(!pick(&mut h, 470.0));
    // Front is orthographic: full size where the 2D position puts it.
    tool_action(&h, ids::SWITCH, json!({"view": "front"}));
    assert_eq!(view(&h), View3d::Ortho(OrthoView::Front));
    h.frame(Vec::new(), Modifiers::NONE);
    assert!(pick(&mut h, 470.0), "Front: at x 430–530");
    // F10, F11, F12: Front, Custom View 1, Active Camera (AE's defaults).
    for (slot, v) in [
        ("b", View3d::Custom(1)),
        ("c", View3d::Active),
        ("a", View3d::Ortho(OrthoView::Front)),
    ] {
        tool_action(&h, ids::SHORTCUT, json!({"slot": slot}));
        assert_eq!(view(&h), v);
    }
    // Esc in the Composition panel: the previous view; an open menu takes it
    // first.
    assert!(
        crate::tools::bindings()
            .iter()
            .any(|b| b.action == ids::LAST && b.when.as_deref() == Some("composition"))
    );
    h.right_click(100.0, 100.0);
    assert!(h.labeled("Composition Settings...").is_some());
    assert!(h.action(ids::LAST));
    assert_eq!(
        view(&h),
        View3d::Ortho(OrthoView::Front),
        "the menu took Esc"
    );
    h.frame(Vec::new(), Modifiers::NONE);
    h.frame(Vec::new(), Modifiers::NONE);
    assert!(
        h.labeled("Composition Settings...").is_none(),
        "menu closed"
    );
    assert!(h.action(ids::LAST));
    assert_eq!(view(&h), View3d::Active);
    // Assign 3D View Shortcut ▸ A: F10 goes to the current view from now on.
    tool_action(&h, ids::SWITCH, json!({"view": "custom_2"}));
    tool_action(&h, ids::ASSIGN, json!({"slot": "a"}));
    tool_action(&h, ids::SWITCH, json!({"view": "top"}));
    tool_action(&h, ids::SHORTCUT, json!({"slot": "a"}));
    assert_eq!(view(&h), View3d::Custom(2));
}

#[test]
fn camera_tools_move_the_default_view_or_the_camera_layer() {
    use crate::viewer::view3d::{View3d, ViewCamera};
    let mut h = H::new();
    let current = |h: &H| {
        let s = h.session();
        let s = s.lock();
        let v = s.views3d.get(&h.comp).cloned().unwrap_or_default();
        (
            v.current,
            v.view_camera(v.current, bae_anim::glam::DVec2::new(640.0, 360.0)),
        )
    };
    // No camera layer: Pan Under Cursor switches to Default and pans it (z = 0
    // follows the pointer 1:1).
    h.tools().lock().set(Tool::PanCursor);
    h.frame(Vec::new(), Modifiers::NONE);
    h.drag((300.0, 180.0), (350.0, 180.0), Modifiers::NONE);
    let (view, cam) = current(&h);
    assert_eq!(view, View3d::Default);
    let ViewCamera::Perspective { position, .. } = cam else {
        panic!("{cam:?}")
    };
    assert!((position.x - 270.0).abs() < 0.5, "{position:?}");
    // A two-node camera layer in Active Camera: Orbit Around Camera POI moves
    // Position around the point of interest, one attr.set_many.
    let camera = path(&h.run("layer.create", json!({"comp": h.comp, "kind": "camera"}))["path"]);
    let set = |h: &mut H, attr: &str, v: serde_json::Value| {
        h.run(
            "attr.set",
            json!({"path": camera, "attr": attr, "value": v}),
        );
    };
    set(
        &mut h,
        "auto_orient",
        json!({"type": "enum", "value": "towards_point_of_interest"}),
    );
    set(
        &mut h,
        "transform:position",
        json!({"type": "vec3", "value": [320.0, 180.0, -500.0]}),
    );
    set(
        &mut h,
        "transform:point_of_interest",
        json!({"type": "vec3", "value": [320.0, 180.0, 0.0]}),
    );
    h.session()
        .lock()
        .views3d
        .get_mut(&h.comp)
        .unwrap()
        .switch(View3d::Active);
    h.tools().lock().set(Tool::OrbitPoi);
    h.frame(Vec::new(), Modifiers::NONE);
    let r = h.drag((320.0, 180.0), (420.0, 180.0), Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "{:?}", r.invocations);
    assert_eq!(r.invocations[0].id, "attr.set_many");
    h.apply(&r);
    let p = h.v3(&camera, "transform:position");
    let d = (p[0] - 320.0).hypot(p[2]);
    assert!(
        p[0] != 320.0 && (d - 500.0).abs() < 1e-6 && p[1] == 180.0,
        "{p:?}"
    );
}
