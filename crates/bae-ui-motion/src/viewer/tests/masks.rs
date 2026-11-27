//! Masks: the Mask Feather tool.

use super::*;

#[test]
fn mask_feather_tool_adds_drags_and_deletes_feather_points() {
    let mut h = H::new();
    let solid = h.layer("solid");
    h.run(
        "mask.create",
        json!({"layer": solid, "kind": "rect", "position": [200.0, 150.0], "size": [200.0, 100.0]}),
    );
    let mask = h.children(&solid.child("masks").unwrap())[0].clone();
    h.select(&[&solid]);
    h.tools().lock().set(Tool::MaskFeather);
    let points = |h: &H| h.bezier(&mask).feather;
    // Press on the top edge (segment 3 → 0) and drag 20 px out.
    let r = h.click_drag((200.0, 100.0), (0.0, -20.0), Modifiers::NONE);
    h.apply(&r);
    let p = points(&h);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].segment, 3);
    assert!(
        (p[0].t - 0.5).abs() < 0.05 && (p[0].radius - 20.0).abs() < 1.0 && !p[0].inner,
        "{p:?}"
    );
    // Drag the point 10 px further.
    let r = h.click_drag((200.0, 80.0), (0.0, -10.0), Modifiers::NONE);
    h.apply(&r);
    assert!(
        (points(&h)[0].radius - 30.0).abs() < 1.0,
        "{:?}",
        points(&h)
    );
    // Alt+click deletes it; undo brings it back.
    let r = h.click(200.0, 70.0, Modifiers::ALT);
    h.apply(&r);
    assert!(points(&h).is_empty());
    h.stage.undo(&h.author).unwrap();
    assert_eq!(points(&h).len(), 1);
}

/// Layer ▸ Mask and Shape Path on the selected mask: Set First Vertex on the
/// selected vertex, RotoBezier on and off, Closed (greyed while closed).
#[test]
fn mask_and_shape_path_menu_edits_the_selected_mask() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.run(
        "mask.create",
        json!({"layer": a, "kind": "rect", "position": [100.0, 50.0], "size": [100.0, 50.0]}),
    );
    let mask = h.children(&a.child("masks").unwrap())[0].clone();
    h.select(&[&a]);
    h.tools().lock().set(Tool::Selection);
    let before = h.bezier(&mask);
    // The layer's top-left corner is at comp (220, 130).
    let v = before.vertices[2];
    h.click(v[0] + 220.0, v[1] + 130.0, Modifiers::NONE);
    assert_eq!(h.viewer.select.vertices.indices, [2]);
    let r = tool_action(&h, crate::path_menu::SET_FIRST_VERTEX, json!({}));
    h.apply(&r);
    let after = h.bezier(&mask);
    assert_eq!(after.vertices[0], before.vertices[2]);
    assert_eq!(after.vertices[2], before.vertices[0]);
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(h.viewer.select.vertices.indices, [0], "still that vertex");
    // RotoBezier: the tangents follow the vertices; off again, they stay.
    let r = tool_action(&h, crate::path_menu::ROTOBEZIER, json!({}));
    h.apply(&r);
    assert!(bae_motion::path::is_roto_bezier(&h.stage, &mask));
    let smooth = h.bezier(&mask);
    assert!(smooth.out_tangents.iter().all(|t| *t != [0.0, 0.0]));
    let r = tool_action(&h, crate::path_menu::ROTOBEZIER, json!({}));
    h.apply(&r);
    assert!(!bae_motion::path::is_roto_bezier(&h.stage, &mask));
    assert_eq!(h.bezier(&mask), smooth);
    // Closed does nothing to a closed mask; an open one closes.
    let r = tool_action(&h, crate::path_menu::CLOSED, json!({}));
    assert!(r.invocations.is_empty());
    let mut open = h.bezier(&mask);
    open.closed = false;
    let mut params = serde_json::to_value(&open).unwrap();
    params["path"] = json!(mask);
    h.run("path.set_vertices", params);
    let r = tool_action(&h, crate::path_menu::CLOSED, json!({}));
    h.apply(&r);
    assert!(h.bezier(&mask).closed);
}
