//! 3D views and cameras.

use super::*;

#[test]
fn alt_camera_navigation_draws_the_chosen_pivot_indicator() {
    use bae_ui::theme::color;
    for (on, mode) in [
        (false, "indicator"),
        (true, "none"),
        (true, "indicator"),
        (true, "directional_indicator"),
    ] {
        let mut h = H::new();
        h.layer("camera");
        let mut prefs = bae_prefs::Preferences::default();
        prefs.set("3d.override_alt_for_camera_navigation", on);
        prefs.set("3d.camera_point_of_interest", mode);
        h.resources
            .insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(prefs)));
        h.tools().lock().set(Tool::Selection);
        h.frame(Vec::new(), Modifiers::NONE);
        h.drag_to((300.0, 180.0), (330.0, 190.0), Modifiers::ALT);
        assert_eq!(h.viewer.camera_drag.is_some(), on);
        let ring = h.shapes.iter().any(|s| {
            matches!(&s.shape,
                egui::Shape::Circle(c) if c.radius == 5.0 && c.stroke.color == color::ACCENT
            )
        });
        assert_eq!(ring, on && mode != "none", "{mode}");
        let arrow = h.shapes.iter().any(|s| {
            matches!(&s.shape,
                egui::Shape::LineSegment { points, stroke } if stroke.color == color::ACCENT
                    && (points[0].distance(points[1]) - 17.0).abs() < 0.01
            )
        });
        assert_eq!(arrow, on && mode == "directional_indicator", "{mode}");
        if on {
            let request = h.release((330.0, 190.0), Modifiers::ALT);
            assert_eq!(request.invocations[0].id, "attr.set_many");
            assert_eq!(h.tools().current(), Tool::Selection);
        }
    }
}

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
    // Esc, whichever panel has focus (AE): the previous view; an open menu
    // takes it first.
    assert!(
        crate::tools::bindings()
            .iter()
            .any(|b| b.action == ids::LAST && b.when.is_none())
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

/// Cameras and lights show in the 3D views other than their own camera's.
#[test]
fn cameras_and_lights_draw_in_the_3d_views() {
    let mut h = H::new();
    let camera = h.layer("camera");
    let light = path(&h.run("layer.create", json!({"comp": h.comp, "kind": "light"}))["path"]);
    h.select(&[&camera, &light]);
    for view in ["active_camera", "top", "custom_1"] {
        tool_action(
            &h,
            crate::viewer::view3d::ids::SWITCH,
            json!({"view": view}),
        );
        h.frame(Vec::new(), Modifiers::NONE);
    }
    // The camera looked through in Active Camera is the one skipped.
    let comp = bae_anim::evaluate_comp(&h.stage, &h.comp, bae_model::Time::ZERO).unwrap();
    assert_eq!(comp.camera.map(|i| &comp.layers[i].path), Some(&camera));
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

#[test]
fn three_d_scale_gizmo_scales_along_one_axis() {
    let mut h = H::new();
    let l = h.solid([100.0, 100.0], [320.0, 180.0]);
    h.run(
        "attr.set",
        json!({"path": l, "attr": "three_d", "value": {"type": "bool", "value": true}}),
    );
    h.select(&[&l]);
    // 5: the 3D Gizmo's Scale mode.
    tool_action(&h, "motion.tool.gizmo_scale", json!({}));
    assert_eq!(
        h.tools().lock().options.gizmo,
        crate::tools::GizmoMode::Scale
    );
    h.frame(Vec::new(), Modifiers::NONE);
    let r = 70.0 / f64::from(h.viewer.last_view.unwrap().scale);
    // From the X square to twice as far from the anchor: Scale X doubles.
    let done = h.drag(
        (320.0 + r, 180.0),
        (320.0 + 2.0 * r, 200.0),
        Modifiers::NONE,
    );
    h.apply(&done);
    let s = h.v3(&l, "transform:scale");
    assert!((s[0] - 200.0).abs() < 1.0 && s[1] == 100.0, "{s:?}");
}

/// In a 3D view, a click on a camera selects it and a drag moves it in the
/// view's plane (Top: x and z, not y).
#[test]
fn cameras_are_picked_and_dragged_in_3d_views() {
    let mut h = H::new();
    let camera = h.layer("camera");
    h.run(
        "attr.set",
        json!({"path": camera, "attr": "transform:position", "value": {"type": "vec3", "value": [200.0, 180.0, -100.0]}}),
    );
    tool_action(
        &h,
        crate::viewer::view3d::ids::SWITCH,
        json!({"view": "top"}),
    );
    h.session().lock().selected_layers.clear();
    h.frame(Vec::new(), Modifiers::NONE);
    // Where Top shows the camera.
    let at = {
        let v = h.viewer.last_view.unwrap();
        let p = v
            .project(bae_anim::glam::DVec3::new(200.0, 180.0, -100.0), true)
            .unwrap();
        v.to_comp(p)
    };
    h.click(at.x, at.y, Modifiers::NONE);
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&camera)
    );
    let r = h.drag((at.x, at.y), (at.x + 50.0, at.y), Modifiers::NONE);
    h.apply(&r);
    let p = h.v3(&camera, "transform:position");
    assert!(
        (p[0] - 250.0).abs() < 1.0 && (p[1] - 180.0).abs() < 1e-6,
        "{p:?}"
    );
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
