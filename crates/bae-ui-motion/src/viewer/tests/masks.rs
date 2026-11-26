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
