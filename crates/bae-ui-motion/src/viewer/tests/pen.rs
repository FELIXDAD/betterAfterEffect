//! Pen tools: open and closed paths, masks, vertex tools, RotoBezier.

use super::*;

#[test]
fn pen_clicks_draw_an_open_path_in_a_new_shape_layer() {
    let mut h = H::new();
    h.tools().lock().set(Tool::Pen);
    // First click: a new shape layer with Shape 1 > Path 1 (one vertex).
    let r = h.click(100.0, 100.0, Modifiers::NONE);
    let p = one(&r, "shape.add");
    assert_eq!(p["kind"], "path");
    h.apply(&r);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    assert_eq!(h.name(&layer), "Shape Layer 1");
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&layer)
    );
    let group = h.contents(&layer)[0].clone();
    assert_eq!(h.name(&group), "Shape 1");
    let path = h.contents(&group)[0].clone();
    assert_eq!(h.name(&path), "Path 1");
    // Layer space: the comp center is the origin.
    assert!(near(h.bezier(&path).vertices[0], [-220.0, -80.0]));
    // Each further click is one path.set_vertices.
    let r = h.click(300.0, 100.0, Modifiers::NONE);
    assert_eq!(one(&r, "path.set_vertices")["path"], json!(path));
    h.apply(&r);
    // Click-drag: a smooth vertex, handles along the drag (in = mirror).
    let r = h.click_drag((300.0, 250.0), (40.0, 0.0), Modifiers::NONE);
    one(&r, "path.set_vertices");
    h.apply(&r);
    let b = h.bezier(&path);
    assert_eq!(b.vertices.len(), 3);
    assert!(!b.closed);
    assert!(near(b.out_tangents[2], [40.0, 0.0]) && near(b.in_tangents[2], [-40.0, 0.0]));
    assert!(near(b.out_tangents[1], [0.0, 0.0]), "a click is a corner");
    // Undo takes back the last vertex; the Pen follows.
    h.stage.undo(&h.author).unwrap();
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(h.bezier(&path).vertices.len(), 2);
    let r = h.click(200.0, 300.0, Modifiers::NONE);
    h.apply(&r);
    assert_eq!(h.bezier(&path).vertices.len(), 3);
    // Switching to the Selection tool (V) leaves the path open; the next Pen
    // click starts a new path (Shape 2 in the selected layer).
    h.tools().lock().set(Tool::Selection);
    h.frame(Vec::new(), Modifiers::NONE);
    assert!(h.viewer.pen.drawing.is_none());
    h.tools().lock().set(Tool::Pen);
    let r = h.click(500.0, 300.0, Modifiers::NONE);
    assert_eq!(one(&r, "shape.add")["layer"], json!(layer));
    h.apply(&r);
    assert_eq!(h.name(&h.contents(&layer)[0]), "Shape 2");
    assert!(!h.bezier(&path).closed);
}

#[test]
fn pen_closes_a_mask_on_its_first_vertex() {
    let mut h = H::new();
    let solid = h.layer("solid");
    h.select(&[&solid]);
    h.tools().lock().set(Tool::Pen);
    let r = h.click(100.0, 100.0, Modifiers::NONE);
    let p = one(&r, "mask.create");
    assert_eq!(p["kind"], "path");
    h.apply(&r);
    for (x, y) in [(300.0, 100.0), (300.0, 250.0)] {
        let r = h.click(x, y, Modifiers::NONE);
        h.apply(&r);
    }
    let mask = h.children(&solid.child("masks").unwrap())[0].clone();
    assert_eq!(h.name(&mask), "Mask 1");
    assert!(!h.bezier(&mask).closed);
    // An open mask is mode None until it is closed (AE 26).
    assert_eq!(h.value(&mask, "mode"), Some(Value::Enum("none".into())));
    // Back on the first vertex: closed, and the Pen starts over.
    let r = h.click(101.0, 101.0, Modifiers::NONE);
    one(&r, "path.set_vertices");
    h.apply(&r);
    let b = h.bezier(&mask);
    assert!(b.closed);
    assert_eq!(b.vertices, [[100.0, 100.0], [300.0, 100.0], [300.0, 250.0]]);
    assert_eq!(h.value(&mask, "mode"), Some(Value::Enum("add".into())));
    assert!(h.viewer.pen.drawing.is_none());
    // Shift: the segment from the last vertex snaps to 45°.
    let r = h.click(400.0, 50.0, Modifiers::NONE);
    h.apply(&r);
    let r = h.click(500.0, 140.0, Modifiers::SHIFT);
    h.apply(&r);
    let masks = h.children(&solid.child("masks").unwrap());
    assert_eq!(masks.len(), 2);
    let b = h.bezier(&masks[1]);
    let d = [
        b.vertices[1][0] - b.vertices[0][0],
        b.vertices[1][1] - b.vertices[0][1],
    ];
    assert!((d[0] - d[1]).abs() < 0.01, "45°: {d:?}");
}

#[test]
fn vertex_tools_and_the_pen_over_paths() {
    let mut h = H::new();
    let solid = h.layer("solid");
    h.run(
        "mask.create",
        json!({"layer": solid, "kind": "rect", "position": [200.0, 150.0], "size": [200.0, 100.0]}),
    );
    let mask = h.children(&solid.child("masks").unwrap())[0].clone();
    h.select(&[&solid]);
    // Add Vertex: click the top edge (AE order: top right first, clockwise; the
    // top edge is the closing segment 3 → 0).
    h.tools().lock().set(Tool::AddVertex);
    let r = h.click(200.0, 100.0, Modifiers::NONE);
    assert_eq!(one(&r, "path.insert_vertex")["segment"], 3);
    h.apply(&r);
    let b = h.bezier(&mask);
    assert_eq!(b.vertices.len(), 5);
    assert!(near(b.vertices[4], [200.0, 100.0]));
    // Delete Vertex: click it again.
    h.tools().lock().set(Tool::DeleteVertex);
    let r = h.click(200.0, 100.0, Modifiers::NONE);
    assert_eq!(one(&r, "path.delete_vertex")["index"], 4);
    h.apply(&r);
    assert_eq!(h.bezier(&mask).vertices.len(), 4);
    // Convert Vertex: a click on a corner does nothing; a drag pulls out
    // symmetric handles; a click on a smooth vertex makes it a corner (AE 26).
    h.tools().lock().set(Tool::ConvertVertex);
    let r = h.click(300.0, 100.0, Modifiers::NONE);
    assert!(r.invocations.is_empty(), "{:?}", r.invocations);
    let r = h.click_drag((300.0, 100.0), (30.0, 0.0), Modifiers::NONE);
    one(&r, "path.set_vertices");
    h.apply(&r);
    assert!(!bae_motion::path::is_corner(&h.bezier(&mask), 0));
    let r = h.click(300.0, 100.0, Modifiers::NONE);
    h.apply(&r);
    assert!(bae_motion::path::is_corner(&h.bezier(&mask), 0));
    let r = h.click_drag((300.0, 200.0), (0.0, 30.0), Modifiers::NONE);
    h.apply(&r);
    let b = h.bezier(&mask);
    assert!(near(b.out_tangents[1], [0.0, 30.0]) && near(b.in_tangents[1], [0.0, -30.0]));
    // The Pen over a segment adds, over a vertex deletes, Alt over a vertex
    // converts.
    h.tools().lock().set(Tool::Pen);
    // (The bottom edge is curved now; the left edge is straight.)
    let r = h.click(100.0, 150.0, Modifiers::NONE);
    assert_eq!(one(&r, "path.insert_vertex")["segment"], 2);
    h.apply(&r);
    assert_eq!(h.bezier(&mask).vertices.len(), 5);
    // Alt over the smooth vertex (300, 200): Convert Vertex makes it a corner.
    let r = h.click(300.0, 200.0, Modifiers::ALT);
    one(&r, "path.set_vertices");
    h.apply(&r);
    assert!(bae_motion::path::is_corner(&h.bezier(&mask), 1));
    let r = h.click(100.0, 100.0, Modifiers::NONE);
    one(&r, "path.delete_vertex");
    h.apply(&r);
    assert_eq!(h.bezier(&mask).vertices.len(), 4);
    assert!(h.viewer.pen.drawing.is_none(), "no new path was started");
}

#[test]
fn rotobezier_pen_paths_get_automatic_tangents() {
    let mut h = H::new();
    h.tools().lock().options.roto_bezier = true;
    h.tools().lock().set(Tool::Pen);
    for (x, y) in [(100.0, 100.0), (200.0, 50.0), (300.0, 100.0)] {
        let r = h.click(x, y, Modifiers::NONE);
        h.apply(&r);
    }
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    let group = h.contents(&layer)[0].clone();
    let path = h.contents(&group)[0].clone();
    let b = h.bezier(&path);
    assert_eq!(b.vertices.len(), 3);
    assert!(bae_motion::path::is_corner(&b, 0) && bae_motion::path::is_corner(&b, 2));
    // The middle vertex is smooth, along the line through its neighbours, each
    // handle a third of its own segment (AE 26).
    let third = 100f64.hypot(50.0) / 3.0;
    assert!(
        near(b.out_tangents[1], [third, 0.0]),
        "{:?}",
        b.out_tangents[1]
    );
    assert!(
        near(b.in_tangents[1], [-third, 0.0]),
        "{:?}",
        b.in_tangents[1]
    );
}

#[test]
fn command_makes_the_pen_the_selection_tool() {
    let mut h = H::new();
    let solid = h.layer("solid");
    h.tools().lock().set(Tool::Pen);
    // Cmd/Ctrl-click selects the layer instead of starting a path.
    let r = h.click(320.0, 180.0, Modifiers::COMMAND);
    assert!(r.invocations.is_empty(), "{:?}", r.invocations);
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&solid)
    );
    assert!(h.viewer.pen.drawing.is_none());
    // An open mask path ends when Cmd-clicking away deselects the layer.
    let r = h.click(100.0, 100.0, Modifiers::NONE);
    h.apply(&r);
    let r = h.click(200.0, 100.0, Modifiers::NONE);
    h.apply(&r);
    assert!(h.viewer.pen.drawing.is_some());
    h.session().lock().selected_layers.clear();
    h.frame(Vec::new(), Modifiers::NONE);
    assert!(h.viewer.pen.drawing.is_none());
    assert_eq!(
        h.tools().lock().current(),
        Tool::Pen,
        "the tool stays the Pen"
    );
}
