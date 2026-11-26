//! AE 26 parity of the viewer tools: snapping to anchors and path vertices,
//! paint tools opening the Layer panel, dragging while previewing.

use super::*;

/// AE snaps to other layers' anchor points and mask vertices too.
#[test]
fn moving_snaps_to_anchor_points_and_mask_vertices() {
    let mut h = H::new();
    // B: 100 × 100, anchor 25 px right of its left edge, at (100, 180).
    let b = h.solid([100.0, 100.0], [100.0, 180.0]);
    h.run(
        "attr.set",
        json!({"path": b, "attr": "transform:anchor_point",
               "value": {"type": "vec3", "value": [25.0, 50.0, 0.0]}}),
    );
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.tools().lock().set(Tool::Selection);
    h.tools().lock().options.snapping = true;
    // A's left midpoint (220, 180) dragged to 103: snaps to B's anchor at 100
    // (B's left edge is now at 75, its centre at 125).
    let r = h.drag((228.0, 180.0), (111.0, 180.0), Modifiers::NONE);
    h.apply(&r);
    let p = h.v3(&a, "transform:position");
    assert!(close3(p, [200.0, 180.0, 0.0]), "{p:?}");
    h.stage.undo(&h.author).unwrap();
    // A mask vertex of B at its layer point (60, 10) = comp (135, 140).
    h.run(
        "mask.create",
        json!({"layer": b, "kind": "rect", "position": [65.0, 15.0], "size": [10.0, 10.0]}),
    );
    // A's top-left corner (220, 130) grabbed nearby and dropped 3 px off.
    let r = h.drag((226.0, 136.0), (144.0, 149.0), Modifiers::NONE);
    h.apply(&r);
    let p = h.v3(&a, "transform:position");
    assert!(close3(p, [235.0, 190.0, 0.0]), "{p:?}");
}

/// With a paint tool, double-clicking a layer opens it in the Layer panel
/// (paint goes there in AE); so does the Selection tool on a solid.
#[test]
fn paint_tools_and_the_selection_tool_open_the_layer_panel() {
    let mut h = H::new();
    let solid = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.tools().lock().set(Tool::Brush);
    h.frame(Vec::new(), Modifiers::NONE);
    h.click(320.0, 180.0, Modifiers::NONE);
    h.click(320.0, 180.0, Modifiers::NONE);
    assert_eq!(h.session().lock().layer_panel, Some(solid.clone()));
    h.session().lock().layer_panel = None;
    for _ in 0..40 {
        h.frame(Vec::new(), Modifiers::NONE);
    }
    h.tools().lock().set(Tool::Selection);
    h.click(320.0, 180.0, Modifiers::NONE);
    h.click(320.0, 180.0, Modifiers::NONE);
    assert_eq!(h.session().lock().layer_panel, Some(solid));
}

/// Dragging a layer while previewing stops the preview and moves the layer
/// (AE).
#[test]
fn dragging_while_previewing_stops_and_moves() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.tools().lock().set(Tool::Selection);
    h.frame(Vec::new(), Modifiers::NONE);
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    h.session().lock().play(&info);
    assert!(h.session().lock().is_playing());
    let r = h.drag((320.0, 180.0), (360.0, 200.0), Modifiers::NONE);
    assert!(!h.session().lock().is_playing(), "stopped");
    h.apply(&r);
    let p = h.v3(&a, "transform:position");
    assert!(close3(p, [360.0, 200.0, 0.0]), "{p:?}");
}

/// Holding an arrow key: the nudges add up, previewed, and become one undo
/// step when the key comes up.
#[test]
fn held_arrow_keys_nudge_in_one_step() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.select(&[&a]);
    let key = |pressed| Event::Key {
        key: Key::ArrowRight,
        physical_key: None,
        pressed,
        repeat: !pressed,
        modifiers: Modifiers::NONE,
    };
    h.frame(vec![key(true)], Modifiers::NONE);
    for _ in 0..3 {
        let r = h.action_with(crate::tools::ids::NUDGE, json!({"dx": 1, "dy": 0}));
        assert!(r.invocations.is_empty());
        let r = h.frame(Vec::new(), Modifiers::NONE);
        assert!(r.invocations.is_empty(), "still held");
    }
    assert!(h.viewer.nudge.is_some(), "previewed");
    let r = h.frame(vec![key(false)], Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "one undo step");
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [323.0, 180.0, 0.0]));
}

/// With the Pan Behind tool the arrow keys move the anchor point; the layer
/// stays where it is.
#[test]
fn pan_behind_nudges_the_anchor_point() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.select(&[&a]);
    h.tools().lock().set(Tool::PanBehind);
    h.frame(Vec::new(), Modifiers::NONE);
    h.nudge(10.0, 0.0);
    assert!(close3(
        h.v3(&a, "transform:anchor_point"),
        [110.0, 50.0, 0.0]
    ));
    assert!(close3(h.v3(&a, "transform:position"), [330.0, 180.0, 0.0]));
}

/// Ctrl+T with mask vertices selected: Free Transform Points, not the Type
/// tool (AE).
#[test]
fn ctrl_t_on_a_selected_path_free_transforms_it() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.run(
        "mask.create",
        json!({"layer": a, "kind": "rect", "position": [100.0, 50.0], "size": [100.0, 50.0]}),
    );
    h.select(&[&a]);
    h.tools().lock().set(Tool::Selection);
    // Vertex 0 at comp (370, 155).
    h.click(370.0, 155.0, Modifiers::NONE);
    assert_eq!(h.viewer.select.vertices.indices, [0]);
    tool_action(&h, crate::tools::ids::TYPE, json!({}));
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(h.tools().lock().current(), Tool::Selection);
    assert!(h.viewer.select.free.is_some(), "Free Transform Points");
    // Without a path selected, Ctrl+T is the Type tool.
    h.viewer.select.free = None;
    h.viewer.select.vertices.clear();
    h.frame(Vec::new(), Modifiers::NONE);
    tool_action(&h, crate::tools::ids::TYPE, json!({}));
    assert_eq!(h.tools().lock().current(), Tool::TypeHorizontal);
}
