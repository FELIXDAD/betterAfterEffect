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
fn three_d_axis_arrows_move_along_one_axis_and_rings_rotate() {
    let mut h = H::new();
    let l = h.solid([100.0, 100.0], [320.0, 180.0]);
    h.run(
        "attr.set",
        json!({"path": l, "attr": "three_d", "value": {"type": "bool", "value": true}}),
    );
    h.select(&[&l]);
    h.frame(Vec::new(), Modifiers::NONE);
    // Arrows and rings are 70 points long at the anchor (z = 0: 1:1).
    let r = 70.0 / f64::from(h.viewer.last_view.unwrap().scale);
    // The X arrow: only X changes, wherever the pointer goes.
    let x = 320.0 + r / 2.0;
    let done = h.drag((x, 180.0), (x + 50.0, 210.0), Modifiers::NONE);
    h.apply(&done);
    let p = h.v3(&l, "transform:position");
    assert!(
        (p[0] - 370.0).abs() < 0.5 && p[1] == 180.0 && p[2] == 0.0,
        "{p:?}"
    );
    // The Rotation tool's Z ring: a clockwise drag of 30° turns Orientation Z
    // (the tool's default Set option).
    h.tools().lock().set(Tool::Rotation);
    h.frame(Vec::new(), Modifiers::NONE);
    let at = |deg: f64| {
        let a = deg.to_radians();
        (370.0 + r * a.cos(), 180.0 + r * a.sin())
    };
    let done = h.drag(at(45.0), at(75.0), Modifiers::NONE);
    h.apply(&done);
    let o = h.v3(&l, "transform:orientation");
    assert!((o[2] - 30.0).abs() < 1.0 && o[0].abs() < 1e-6, "{o:?}");
}
