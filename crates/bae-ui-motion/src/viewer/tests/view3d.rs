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
fn view_layouts_show_more_views_and_a_click_makes_one_active() {
    use crate::viewer::view3d::{View3d, ids};
    use bae_render::camera::OrthoView::*;
    let mut h = H::new();
    tool_action(&h, ids::LAYOUT, json!({"views": 4}));
    let panes = |h: &H| {
        let s = h.session();
        let s = s.lock();
        let v = s.views3d.get(&h.comp).unwrap();
        (v.panes(), v.active())
    };
    assert_eq!(
        panes(&h),
        (
            vec![
                View3d::Ortho(Top),
                View3d::Ortho(Front),
                View3d::Ortho(Right),
                View3d::Active
            ],
            3
        )
    );
    h.frame(Vec::new(), Modifiers::NONE);
    // The top left view (Top) becomes the active one; the order stays.
    let at = Pos2::new(100.0, 100.0);
    h.move_to(at, Modifiers::NONE, Vec::new());
    h.pointer_button(at, true, Modifiers::NONE);
    h.pointer_button(at, false, Modifiers::NONE);
    let (p, active) = panes(&h);
    assert_eq!((p[0], active), (View3d::Ortho(Top), 0));
    assert_eq!(p[3], View3d::Active);
    // The 3D view popup and the menu act on the active view.
    tool_action(&h, ids::SWITCH, json!({"view": "custom_2"}));
    assert_eq!(panes(&h).0[0], View3d::Custom(2));
    tool_action(&h, ids::LAYOUT, json!({"views": 1}));
    assert_eq!(panes(&h), (vec![View3d::Custom(2)], 0));
}

/// Reset 3D View in Active Camera resets the camera layer ("Reset Camera 1").
#[test]
fn reset_3d_view_in_active_camera_resets_the_camera_layer() {
    let mut h = H::new();
    let camera = h.layer("camera");
    h.run(
        "attr.set",
        json!({"path": camera, "attr": "transform:position", "value": {"type": "vec3", "value": [10.0, 20.0, -300.0]}}),
    );
    let r = tool_action(&h, crate::viewer::view3d::ids::RESET, json!({}));
    assert_eq!(r.invocations.len(), 1, "{:?}", r.invocations);
    h.apply(&r);
    let zoom = match h.value(&camera, "camera:zoom") {
        Some(Value::Float(z)) => z,
        other => panic!("{other:?}"),
    };
    assert!(close3(
        h.v3(&camera, "transform:position"),
        [320.0, 180.0, -zoom]
    ));
}
