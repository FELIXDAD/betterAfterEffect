//! Free Transform Points on a mask.

use super::*;

#[test]
fn free_transform_points_moves_and_scales_a_mask() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.run(
        "mask.create",
        json!({"layer": a, "kind": "rect", "position": [100.0, 50.0], "size": [100.0, 50.0]}),
    );
    let mask = h.children(&a.child("masks").unwrap())[0].clone();
    h.select(&[&a]);
    h.tools().lock().set(Tool::Selection);
    // Double-click the top edge of the mask (comp y 155).
    h.click(320.0, 155.0, Modifiers::NONE);
    h.click(320.0, 155.0, Modifiers::NONE);
    assert!(h.viewer.select.free.is_some(), "free transform box");
    // Drag inside the box: every vertex moves; the layer does not.
    let r = h.drag((320.0, 180.0), (340.0, 190.0), Modifiers::NONE);
    one(&r, "path.set_vertices");
    h.apply(&r);
    let b = h.bezier(&mask);
    assert!(
        near(b.vertices[0], [170.0, 35.0]) && near(b.vertices[2], [70.0, 85.0]),
        "{:?}",
        b.vertices
    );
    assert!(close3(h.v3(&a, "transform:position"), [320.0, 180.0, 0.0]));
    h.stage.undo(&h.author).unwrap();
    h.frame(Vec::new(), Modifiers::NONE);
    // The bottom-right handle (comp (370, 205)) out by (50, 25): twice the size
    // around the box's center.
    let r = h.drag((370.0, 205.0), (420.0, 230.0), Modifiers::NONE);
    h.apply(&r);
    let b = h.bezier(&mask);
    assert!(
        near(b.vertices[0], [200.0, 0.0]) && near(b.vertices[2], [0.0, 100.0]),
        "{:?}",
        b.vertices
    );
    // Esc ends it; the menu command starts it again on the mask selected in
    // the Timeline.
    h.key(Key::Escape);
    assert!(h.viewer.select.free.is_none());
    h.session().lock().selected_groups = vec![format!("{a}#{mask}")];
    let mut requests = PanelRequests::default();
    {
        let pid = PanelId::new("timeline");
        let mut cx = PanelContext::new(
            &pid,
            "Timeline",
            &h.stage,
            &h.author,
            &h.resources,
            &mut requests,
        );
        use bae_ui::ActionHandler as _;
        let inv = Invocation::new(crate::tools::ids::FREE_TRANSFORM);
        assert!(crate::tools::ToolActions.handle(&inv, &mut cx));
    }
    h.frame(Vec::new(), Modifiers::NONE);
    assert!(h.viewer.select.free.is_some());
}
