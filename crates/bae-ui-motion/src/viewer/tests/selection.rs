//! Selection, Rotation and Pan Behind tools: moving, snapping, handles, marquee, nudging, mask vertices, picking under other layers.

use super::*;

#[test]
fn moving_layers_snaps_to_other_layers_and_the_comp() {
    let mut h = H::new();
    // B's right edge midpoint is at (150, 180).
    let b = h.solid([100.0, 100.0], [100.0, 180.0]);
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.tools().lock().set(Tool::Selection);
    // Grabbed near its left edge midpoint (220, 180) but away from the scale
    // handle there, A moves by −67.
    let grab = (228.0, 180.0);
    let to = (161.0, 180.0);
    // Snapping off: the layer follows the pointer.
    let r = h.drag(grab, to, Modifiers::NONE);
    one(&r, "attr.set_many");
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [253.0, 180.0, 0.0]));
    assert_eq!(h.session().lock().selected_layers, std::slice::from_ref(&a));
    h.stage.undo(&h.author).unwrap();
    // On: the grabbed feature (left midpoint, 153) snaps to B's right midpoint.
    h.tools().lock().options.snapping = true;
    let r = h.drag(grab, to, Modifiers::NONE);
    h.apply(&r);
    assert!(
        close3(h.v3(&a, "transform:position"), [250.0, 180.0, 0.0]),
        "{:?}",
        h.v3(&a, "transform:position")
    );
    h.stage.undo(&h.author).unwrap();
    // Cmd/Ctrl inverts the switch while held.
    let r = h.drag(grab, to, Modifiers::COMMAND);
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [253.0, 180.0, 0.0]));
    h.stage.undo(&h.author).unwrap();
    // The comp frame is not a target (AE 26): 5 px from the left edge stays.
    let r = h.drag(grab, (13.0, 180.0), Modifiers::NONE);
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [105.0, 180.0, 0.0]));
    let _ = b;
}

#[test]
fn bounding_box_handles_scale_around_the_anchor() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.select(&[&a]);
    h.tools().lock().set(Tool::Selection);
    // Bottom-right corner (420, 230) out by (100, 50): twice the size.
    let r = h.drag((420.0, 230.0), (520.0, 280.0), Modifiers::NONE);
    one(&r, "attr.set_many");
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:scale"), [200.0, 200.0, 100.0]));
    assert!(
        close3(h.v3(&a, "transform:position"), [320.0, 180.0, 0.0]),
        "not moved"
    );
    h.stage.undo(&h.author).unwrap();
    // Shift keeps the proportions with the larger ratio (AE 26: 200 % × 120 %
    // becomes 200 % × 200 %).
    let r = h.drag((420.0, 230.0), (520.0, 240.0), Modifiers::SHIFT);
    h.apply(&r);
    let s = h.v3(&a, "transform:scale");
    assert!(
        (s[0] - 200.0).abs() < 0.01 && (s[0] - s[1]).abs() < 1e-9,
        "{s:?}"
    );
    h.stage.undo(&h.author).unwrap();
    // The right edge handle scales X only.
    let r = h.drag((420.0, 180.0), (470.0, 180.0), Modifiers::NONE);
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:scale"), [150.0, 100.0, 100.0]));
    // One undo step.
    h.stage.undo(&h.author).unwrap();
    assert!(close3(h.v3(&a, "transform:scale"), [100.0, 100.0, 100.0]));
}

#[test]
fn marquee_selects_the_layers_it_touches() {
    let mut h = H::new();
    let b = h.solid([100.0, 100.0], [100.0, 100.0]);
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.tools().lock().set(Tool::Selection);
    h.drag_to((10.0, 10.0), (160.0, 160.0), Modifiers::NONE);
    assert_eq!(h.session().lock().selected_layers, std::slice::from_ref(&b));
    h.move_to(h.screen(240.0, 140.0), Modifiers::NONE, Vec::new());
    assert_eq!(h.session().lock().selected_layers, [a.clone(), b.clone()]);
    let r = h.release((240.0, 140.0), Modifiers::NONE);
    assert!(r.invocations.is_empty());
    assert_eq!(h.session().lock().selected_layers.len(), 2);
    // Shift adds to the selection; a plain marquee in empty space replaces it.
    h.select(&[&a]);
    h.drag((10.0, 10.0), (60.0, 60.0), Modifiers::SHIFT);
    assert_eq!(h.session().lock().selected_layers, [a.clone(), b.clone()]);
    h.drag((600.0, 10.0), (630.0, 20.0), Modifiers::NONE);
    assert!(h.session().lock().selected_layers.is_empty());
}

#[test]
fn rotation_tool_rotates_around_the_anchor_shift_in_45_degree_steps() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.tools().lock().set(Tool::Rotation);
    // A quarter turn clockwise around the anchor (320, 180).
    let r = h.drag((400.0, 180.0), (320.0, 260.0), Modifiers::NONE);
    one(&r, "attr.set_many");
    h.apply(&r);
    assert_eq!(h.session().lock().selected_layers, std::slice::from_ref(&a));
    assert!((float(h.value(&a, "transform:rotation")) - 90.0).abs() < 0.01);
    h.stage.undo(&h.author).unwrap();
    let r = h.drag((400.0, 180.0), (330.0, 260.0), Modifiers::SHIFT);
    h.apply(&r);
    assert!((float(h.value(&a, "transform:rotation")) - 90.0).abs() < 1e-9);
    h.stage.undo(&h.author).unwrap();
    let r = h.drag((400.0, 180.0), (390.0, 230.0), Modifiers::SHIFT);
    h.apply(&r);
    assert!((float(h.value(&a, "transform:rotation")) - 45.0).abs() < 1e-9);
}

#[test]
fn pan_behind_moves_the_anchor_or_pans_the_content() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.select(&[&a]);
    h.tools().lock().set(Tool::PanBehind);
    // Dragging the anchor point: it moves, the layer stays where it is.
    let r = h.drag((320.0, 180.0), (370.0, 200.0), Modifiers::NONE);
    one(&r, "attr.set_many");
    h.apply(&r);
    assert!(close3(
        h.v3(&a, "transform:anchor_point"),
        [150.0, 70.0, 0.0]
    ));
    assert!(close3(h.v3(&a, "transform:position"), [370.0, 200.0, 0.0]));
    h.stage.undo(&h.author).unwrap();
    // Dragging elsewhere on the layer: the content pans behind the anchor.
    let r = h.drag((250.0, 150.0), (300.0, 170.0), Modifiers::NONE);
    h.apply(&r);
    assert!(close3(
        h.v3(&a, "transform:anchor_point"),
        [50.0, 30.0, 0.0]
    ));
    assert!(close3(h.v3(&a, "transform:position"), [320.0, 180.0, 0.0]));
    // Cmd/Ctrl double-click on the tool button: anchor back to the center.
    h.tools().lock().center_anchor_request = true;
    let r = h.frame(Vec::new(), Modifiers::NONE);
    h.apply(&r);
    assert!(close3(
        h.v3(&a, "transform:anchor_point"),
        [100.0, 50.0, 0.0]
    ));
    // The content moved by (50, 20): its center is at (370, 200) in the comp.
    assert!(close3(h.v3(&a, "transform:position"), [370.0, 200.0, 0.0]));
}

#[test]
fn arrow_keys_nudge_by_screen_pixels() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.select(&[&a]);
    let r = h.action_with(crate::tools::ids::NUDGE, json!({"dx": 1, "dy": 0}));
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [321.0, 180.0, 0.0]));
    let r = h.action_with(crate::tools::ids::NUDGE, json!({"dx": 0, "dy": -10}));
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [321.0, 170.0, 0.0]));
    // At 200 % one screen pixel is half a comp pixel.
    h.viewer.zoom = Some(2.0);
    h.frame(Vec::new(), Modifiers::NONE);
    let r = h.action_with(crate::tools::ids::NUDGE, json!({"dx": -1, "dy": 0}));
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [320.5, 170.0, 0.0]));
    // From another panel the tool handler does the same at the viewer's
    // magnification.
    let mut requests = PanelRequests::default();
    let pid = PanelId::new("timeline");
    {
        let mut cx = PanelContext::new(
            &pid,
            "Timeline",
            &h.stage,
            &h.author,
            &h.resources,
            &mut requests,
        );
        use bae_ui::ActionHandler as _;
        let inv = Invocation::with_params(crate::tools::ids::NUDGE, json!({"dx": 0, "dy": 1}));
        assert!(crate::tools::ToolActions.handle(&inv, &mut cx));
    }
    h.apply(&requests);
    assert!(close3(h.v3(&a, "transform:position"), [320.5, 170.5, 0.0]));
}

#[test]
fn selection_tool_edits_mask_vertices() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.run(
        "mask.create",
        json!({"layer": a, "kind": "rect", "position": [100.0, 50.0], "size": [100.0, 50.0]}),
    );
    let mask = h.children(&a.child("masks").unwrap())[0].clone();
    h.select(&[&a]);
    h.tools().lock().set(Tool::Selection);
    // Vertex 0 (top right, layer (150, 25)) is at comp (370, 155).
    let r = h.click(370.0, 155.0, Modifiers::NONE);
    assert!(r.invocations.is_empty());
    assert_eq!(h.viewer.select.vertices.indices, [0]);
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&a),
        "layer stays selected"
    );
    let r = h.drag((370.0, 155.0), (390.0, 165.0), Modifiers::NONE);
    one(&r, "path.set_vertices");
    h.apply(&r);
    let b = h.bezier(&mask);
    assert!(near(b.vertices[0], [170.0, 35.0]), "{:?}", b.vertices);
    assert!(
        close3(h.v3(&a, "transform:position"), [320.0, 180.0, 0.0]),
        "layer not moved"
    );
    // Arrow keys move the selected vertex, Delete removes it.
    let r = h.action_with(crate::tools::ids::NUDGE, json!({"dx": 0, "dy": 1}));
    h.apply(&r);
    assert!(near(h.bezier(&mask).vertices[0], [170.0, 36.0]));
    let r = h.action_with(crate::actions::ids::DELETE, json!({}));
    one(&r, "path.set_vertices");
    h.apply(&r);
    assert_eq!(h.bezier(&mask).vertices.len(), 3);
    assert!(h.viewer.select.vertices.is_empty());
    // Clicking empty space deselects the layer (and its vertices).
    h.click(600.0, 20.0, Modifiers::NONE);
    assert!(h.session().lock().selected_layers.is_empty());
}

/// #400: with a layer selected, the viewer edits it even where layers above it
/// cover it; a click on empty space deselects, and then a click picks the
/// topmost layer.
#[test]
fn the_viewer_edits_the_selected_layer_under_others() {
    let mut h = H::new();
    let bottom = h.solid([200.0, 200.0], [320.0, 180.0]);
    let top = h.solid([100.0, 100.0], [320.0, 180.0]);
    h.select(&[&bottom]);
    h.frame(Vec::new(), Modifiers::NONE);
    let r = h.drag((320.0, 180.0), (340.0, 180.0), Modifiers::NONE);
    h.apply(&r);
    assert!(close3(
        h.v3(&bottom, "transform:position"),
        [340.0, 180.0, 0.0]
    ));
    assert!(close3(
        h.v3(&top, "transform:position"),
        [320.0, 180.0, 0.0]
    ));
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&bottom)
    );
    // Outside every layer: nothing selected; then the topmost layer.
    h.click(10.0, 10.0, Modifiers::NONE);
    assert!(h.session().lock().selected_layers.is_empty());
    h.click(320.0, 180.0, Modifiers::NONE);
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&top)
    );
}

/// The bar's Toggle Mask and Shape Path Visibility off: mask paths are not
/// shown, so their vertices cannot be picked.
#[test]
fn hidden_mask_paths_have_no_vertices_to_pick() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.run(
        "mask.create",
        json!({"layer": a, "kind": "rect", "position": [100.0, 50.0], "size": [100.0, 50.0]}),
    );
    h.select(&[&a]);
    h.tools().lock().set(Tool::Selection);
    h.session().lock().view_options.paths = false;
    h.click(370.0, 155.0, Modifiers::NONE);
    assert!(h.viewer.select.vertices.indices.is_empty());
    h.session().lock().view_options.paths = true;
    h.click(370.0, 155.0, Modifiers::NONE);
    assert_eq!(h.viewer.select.vertices.indices, [0]);
}
