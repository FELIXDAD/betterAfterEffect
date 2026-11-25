//! Markers: add, numbered comp markers, drag and snap, the marker dialog, timecode fields.

use super::*;

#[test]
fn add_marker_puts_a_layer_or_comp_marker_at_the_cti() {
    use crate::actions::ids;
    let mut h = H::new();
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    // The layer starts at 1 s: its markers are in layer time.
    let layer = h.layer.clone();
    h.run("layer.shift_time", json!({"layers": [layer], "delta": SEC}));
    h.session().lock().set_time(&info, Time::from_seconds(2.0));
    let mut tl = h.bars();
    // No layer selected: a comp marker.
    h.session().lock().selected_layers.clear();
    let r = h.action(&mut tl, Invocation::new(ids::ADD_MARKER));
    assert_eq!(r.invocations.len(), 1);
    h.apply(&r);
    let comp = h.comp.clone();
    assert_eq!(markers(&h, &comp).len(), 1);
    assert_eq!(markers(&h, &comp)[0].1, 2 * SEC);
    // The layer selected: a layer marker at layer time 1 s.
    h.session().lock().selected_layers = vec![layer.clone()];
    let r = h.action(&mut tl, Invocation::new(ids::ADD_MARKER));
    h.apply(&r);
    assert_eq!(markers(&h, &layer).len(), 1);
    assert_eq!(markers(&h, &layer)[0].1, SEC);
    h.undo();
    assert!(markers(&h, &layer).is_empty());
    // Delete All Markers (of the selected layers).
    h.apply(&h.action(&mut tl, Invocation::new(ids::ADD_MARKER)));
    let r = h.action(&mut tl, Invocation::new(ids::DELETE_ALL_MARKERS));
    h.apply(&r);
    assert!(markers(&h, &layer).is_empty());
    assert_eq!(markers(&h, &comp).len(), 1, "comp markers stay");
}

#[test]
fn numbered_comp_markers_set_move_and_go_to() {
    use crate::actions::ids;
    let mut h = H::new();
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    let comp = h.comp.clone();
    let mut tl = h.bars();
    let num = |n: i64| json!({ "number": n });
    // Shift+1 at 1 s: comp marker "1" (even with a layer selected).
    h.session().lock().set_time(&info, Time::from_seconds(1.0));
    let r = h.action(
        &mut tl,
        Invocation::with_params(ids::SET_COMP_MARKER, num(1)),
    );
    h.apply(&r);
    let m = markers(&h, &comp);
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].1, SEC);
    let comment = |h: &H, p: &PrimPath| query::static_value(&h.stage.prim(p).unwrap(), "comment");
    assert_eq!(comment(&h, &m[0].0), Some(Value::String("1".into())));
    // Shift+1 again at 3 s moves it (one marker "1"); Shift+2 adds "2".
    h.session().lock().set_time(&info, Time::from_seconds(3.0));
    h.apply(&h.action(
        &mut tl,
        Invocation::with_params(ids::SET_COMP_MARKER, num(1)),
    ));
    h.session().lock().set_time(&info, Time::from_seconds(5.0));
    h.apply(&h.action(
        &mut tl,
        Invocation::with_params(ids::SET_COMP_MARKER, num(2)),
    ));
    let m = markers(&h, &comp);
    assert_eq!(
        m.iter().map(|(_, t)| *t).collect::<Vec<_>>(),
        vec![3 * SEC, 5 * SEC]
    );
    // 1 goes to marker "1"; a missing number does nothing.
    h.action(
        &mut tl,
        Invocation::with_params(ids::GO_TO_COMP_MARKER, num(1)),
    );
    assert_eq!(h.session().lock().time(&comp), Time(3 * SEC));
    h.action(
        &mut tl,
        Invocation::with_params(ids::GO_TO_COMP_MARKER, num(7)),
    );
    assert_eq!(h.session().lock().time(&comp), Time(3 * SEC));
    // Each set is one undo step.
    h.undo();
    assert_eq!(markers(&h, &comp).len(), 1);
    // The keys are bound: Shift+0–9 and 0–9 from the catalog.
    let works = |action: &str| {
        crate::catalog::KEYS
            .iter()
            .filter(|k| k.action == action)
            .all(|k| k.status.works())
    };
    assert!(works(ids::SET_COMP_MARKER) && works(ids::GO_TO_COMP_MARKER));
}

#[test]
fn dragging_a_marker_moves_it_and_shift_snaps_to_the_cti() {
    let mut h = H::new();
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    let comp = h.comp.clone();
    let layer = h.layer.clone();
    h.run("layer.shift_time", json!({"layers": [layer], "delta": SEC}));
    let cm = path(&h.run("marker.add", json!({"owner": comp, "time": SEC}))["path"]);
    let lm = path(&h.run("marker.add", json!({"owner": layer, "comp_time": 2 * SEC}))["path"]);
    h.session().lock().set_time(&info, Time::from_seconds(4.0));
    let mut tl = h.bars();
    // Comp marker: Shift-drag to 3 pt right of the CTI lands on it.
    let from = marker_pos(&tl, &cm);
    let to = pos2(x_of(&tl, 4.0) + 3.0, from.y);
    let r = h.drag(&mut tl, from, to, Modifiers::SHIFT);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "marker.move");
    h.apply(&r);
    assert_eq!(markers(&h, &comp)[0].1, 4 * SEC);
    h.undo();
    // Layer marker (layer time 1 s = comp 2 s): Shift-drag onto the CTI at 4 s,
    // layer time 3 s.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let from = marker_pos(&tl, &lm);
    let to = pos2(x_of(&tl, 4.0) + 2.0, from.y);
    let r = h.drag(&mut tl, from, to, Modifiers::SHIFT);
    h.apply(&r);
    assert_eq!(markers(&h, &layer)[0].1, 3 * SEC);
}

#[test]
fn shift_snaps_a_dragged_keyframe_to_a_marker() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (3.0, float(90.0))]);
    let comp = h.comp.clone();
    h.run("marker.add", json!({"owner": comp, "time": 2 * SEC}));
    let mut tl = h.bars();
    let k = h.key(ROT, 3.0);
    let from = bar_key(&tl, &k);
    let to = pos2(x_of(&tl, 2.0) + 3.0, from.y);
    let r = h.drag(&mut tl, from, to, Modifiers::SHIFT);
    h.apply(&r);
    assert_eq!(h.times(ROT), vec![0, 2 * SEC], "snapped to the comp marker");
}

#[test]
fn a_comp_marker_dragged_out_of_the_bin_lands_where_dropped() {
    let mut h = H::new();
    let mut tl = h.bars();
    let bin = tl.last_marker_bin.expect("marker bin drawn");
    let to = pos2(x_of(&tl, 2.0), bin.center().y);
    let r = h.drag(&mut tl, bin.center(), to, Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "marker.add");
    h.apply(&r);
    let comp = h.comp.clone();
    assert_eq!(markers(&h, &comp).len(), 1);
    let t = markers(&h, &comp)[0].1;
    assert!((t - 2 * SEC).abs() <= SEC / 30, "{t}");
}

#[test]
fn the_marker_dialog_edits_and_creates_markers() {
    use crate::actions::ids;
    let mut h = H::new();
    let comp = h.comp.clone();
    let m = path(&h.run("marker.add", json!({"owner": comp, "time": SEC}))["path"]);
    let mut tl = h.bars();
    // Double-click: the Composition Marker dialog.
    let p = marker_pos(&tl, &m);
    h.frame(&mut tl, vec![Event::PointerMoved(p)], Modifiers::NONE);
    for _ in 0..2 {
        h.frame(
            &mut tl,
            vec![button(p, true, Modifiers::NONE)],
            Modifiers::NONE,
        );
        h.frame(
            &mut tl,
            vec![button(p, false, Modifiers::NONE)],
            Modifiers::NONE,
        );
    }
    {
        let s = h.session();
        let mut s = s.lock();
        let Some(crate::timeline::KeyDialog::Marker(d)) = &mut s.key_dialog else {
            panic!("the marker dialog is open");
        };
        assert_eq!(d.time, Time(SEC));
        d.comment = "intro".into();
        d.duration = Time(SEC);
        d.label = 2;
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.frame(&mut tl, vec![press(egui::Key::Enter)], Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "marker.edit");
    h.apply(&r);
    let v = |a: &str| query::static_value(&h.stage.prim(&m).unwrap(), a);
    assert_eq!(v("comment"), Some(Value::String("intro".into())));
    assert_eq!(v("duration"), Some(Value::Time(Time(SEC))));
    assert_eq!(v("label"), Some(Value::Int(2)));
    // Add Marker with Dialog on the selected layer: OK adds it with the fields.
    let layer = h.layer.clone();
    h.session().lock().selected_layers = vec![layer.clone()];
    h.action(&mut tl, Invocation::new(ids::ADD_MARKER_DIALOG));
    if let Some(crate::timeline::KeyDialog::Marker(d)) = &mut h.session().lock().key_dialog {
        d.comment = "hit".into();
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.frame(&mut tl, vec![press(egui::Key::Enter)], Modifiers::NONE);
    assert_eq!(r.invocations[0].id, "marker.add");
    h.apply(&r);
    let lm = &markers(&h, &layer)[0].0;
    assert_eq!(
        query::static_value(&h.stage.prim(lm).unwrap(), "comment"),
        Some(Value::String("hit".into()))
    );
}

#[test]
fn timecode_fields_parse_timecode_and_frames() {
    let h = H::new();
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    let p = |s: &str| crate::timeline::parse_timecode(&info, s);
    assert_eq!(p("0:00:01:00"), Some(30));
    assert_eq!(p("0;00;02;15"), Some(75));
    assert_eq!(p("1:10"), Some(40));
    assert_eq!(p("45"), Some(45));
    assert_eq!(p("x"), None);
}
