//! Layer bars: move, snap, trim, slip edit; double-click opens the Layer panel.

use super::*;

#[test]
fn layer_bar_drags_move_snap_and_trim() {
    let mut h = H::new();
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    let layer = h.layer.clone();
    h.run(
        "layer.trim",
        json!({"layers": [layer], "in_point": 2 * SEC, "out_point": 6 * SEC}),
    );
    h.session().lock().set_time(&info, Time::from_seconds(3.0));
    let mut tl = h.bars();
    let b = bar_rect(&tl, &layer);
    // Shift-drag the body to 3 pt short of the CTI: In snaps onto it (R1, AE 26).
    let from = b.center();
    let to = from + vec2(x_of(&tl, 3.0) - x_of(&tl, 2.0) - 3.0, 0.0);
    let r = h.drag(&mut tl, from, to, Modifiers::SHIFT);
    assert_eq!(r.invocations[0].id, "layer.shift_time");
    h.apply(&r);
    let t = query::layer_timing(&h.stage, &layer);
    assert_eq!((t.in_point, t.out_point), (Time(3 * SEC), Time(7 * SEC)));
    h.undo();
    // Alt-dragging the body moves it too (AE 26: no slip edit).
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let to = from + vec2(x_of(&tl, 3.0) - x_of(&tl, 2.0), 0.0);
    let r = h.drag(&mut tl, from, to, alt());
    assert_eq!(r.invocations[0].id, "layer.shift_time");
    // Dragging the left edge trims In only.
    let edge = pos2(b.left() + 1.0, b.center().y);
    let to = pos2(x_of(&tl, 3.0) + 1.0, edge.y);
    let r = h.drag(&mut tl, edge, to, Modifiers::NONE);
    assert_eq!(r.invocations[0].id, "layer.trim");
    h.apply(&r);
    let t = query::layer_timing(&h.stage, &layer);
    assert_eq!(t.start_time, Time::ZERO);
    assert_eq!(t.out_point, Time(6 * SEC));
    assert!(
        (t.in_point - Time(3 * SEC)).0.abs() <= SEC / 30,
        "{:?}",
        t.in_point
    );
}

#[test]
fn dragging_the_pale_source_bar_slip_edits() {
    let mut h = H::new();
    let comp = h.comp.clone();
    let inner = path(&h.run("comp.create", json!({"width": 64, "height": 64, "fps": 30}))["path"]);
    h.session().lock().open_comp(&comp);
    let pre = path(
        &h.run(
            "layer.create",
            json!({"comp": comp, "kind": "pre_comp", "source": inner}),
        )["path"],
    );
    h.run(
        "layer.trim",
        json!({"layers": [pre], "in_point": 2 * SEC, "out_point": 5 * SEC}),
    );
    let mut tl = TimelinePanel::default();
    h.session().lock().selected_layers = vec![pre.clone()];
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let b = bar_rect(&tl, &pre);
    // The source (0 .. 10 s) shows pale beyond the bar (2 .. 5 s): drag it 1 s left.
    let from = pos2(x_of(&tl, 7.0), b.center().y);
    let to = pos2(x_of(&tl, 6.0), b.center().y);
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    assert_eq!(r.invocations[0].id, "layer.slip");
    h.apply(&r);
    let t = query::layer_timing(&h.stage, &pre);
    assert_eq!((t.in_point, t.out_point), (Time(2 * SEC), Time(5 * SEC)));
    assert_eq!(t.start_time, Time(-SEC));
    h.undo();
    assert_eq!(query::layer_timing(&h.stage, &pre).start_time, Time::ZERO);
}

#[test]
fn double_clicking_a_layer_bar_opens_it_in_the_layer_panel() {
    let mut h = H::new();
    let layer = h.layer.clone();
    let mut tl = h.bars();
    let at = bar_rect(&tl, &layer).center();
    h.click(&mut tl, at, Modifiers::NONE);
    let r = h.click(&mut tl, at, Modifiers::NONE);
    assert!(r.open.iter().any(|p| p.kind() == "layer"), "{:?}", r.open);
    assert_eq!(h.session().lock().layer_panel.as_ref(), Some(&layer));
    // The panel shows the layer alone, at its own size, from the origin.
    let comp = bae_anim::evaluate_comp(&h.stage, &h.comp, Time::ZERO).unwrap();
    let (one, min) = crate::layer_panel::alone(&comp, &layer, 0).unwrap();
    let l = &one.layers[0];
    assert_eq!(one.layers.len(), 1);
    let (a, b) = crate::render::layer_bounds(l).unwrap();
    assert_eq!(
        l.world_matrix.transform_point3(a.extend(0.0)).truncate(),
        a - min
    );
    assert_eq!(
        (f64::from(one.width), f64::from(one.height)),
        ((b - a).x.ceil(), (b - a).y.ceil())
    );
}
