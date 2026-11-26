//! Graph Editor: value and speed graphs, key and handle drags, the transform box, snapping, its bottom buttons and frame cost.

use super::*;

#[test]
fn shift_f3_and_the_switch_toggle_the_graph_editor() {
    let mut h = H::new();
    let mut tl = TimelinePanel::default();
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    h.action(&mut tl, Invocation::new(crate::actions::ids::GRAPH_EDITOR));
    assert!(h.session().lock().graph_editor);
    h.action(&mut tl, Invocation::new(crate::actions::ids::GRAPH_EDITOR));
    assert!(!h.session().lock().graph_editor);
    // Shift+F3 is the AE shortcut.
    let b = crate::actions::bindings();
    assert!(b.bindings.iter().any(
        |x| x.action == crate::actions::ids::GRAPH_EDITOR && x.chord.to_string().contains("F3")
    ));
    // The comp switch in the timeline header (right end of the outline).
    let sw = pos2(tl.last_axis.unwrap().left - 2.0 - 14.0, 22.0 + 17.0);
    h.click(&mut tl, sw, Modifiers::NONE);
    assert!(h.session().lock().graph_editor);
    // The graph and its button bar are visible on the release frame.
    assert!(!tl.graph_editor().last.buttons.is_empty());
}

#[test]
fn value_graph_key_drag_changes_time_and_value_in_one_undo_step() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let mut tl = h.graph();
    let k = h.key(ROT, 1.0);
    let from = key_pos(&mut tl, &k, 0);
    let va = vaxis(&mut tl, ROT);
    let to = pos2(x_of(&tl, 1.5), va.y(120.0));
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "{:?}", r.invocations);
    assert_eq!(r.invocations[0].id, "key.edit");
    h.apply(&r);
    let spec = h.spec(ROT);
    let times: Vec<i64> = spec.keyframes.keys().map(|t| t.0).collect();
    assert_eq!(times, vec![0, SEC * 3 / 2], "dragged to 1.5 s, on a frame");
    let v = spec.keyframes[&Time(SEC * 3 / 2)].value.as_f64().unwrap();
    assert!((v - 120.0).abs() < va.per_point() * 2.0, "{v}");
    // The selection follows the key.
    assert_eq!(h.session().lock().selected_keys, vec![h.key(ROT, 1.5)]);
    h.undo();
    let spec = h.spec(ROT);
    assert_eq!(spec.keyframes[&Time(SEC)].value, Value::Float(90.0));
    assert_eq!(spec.keyframes.len(), 2);
}

#[test]
fn shift_constrains_a_key_drag_to_one_axis() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let mut tl = h.graph();
    let k = h.key(ROT, 1.0);
    let from = key_pos(&mut tl, &k, 0);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    // Mostly vertical: time stays.
    let r = h.drag(&mut tl, from, from + vec2(4.0, -60.0), shift);
    h.apply(&r);
    let spec = h.spec(ROT);
    let k1 = &spec.keyframes[&Time(SEC)];
    assert!(k1.value.as_f64().unwrap() > 90.0);
}

#[test]
fn value_graph_handle_drag_sets_speed_and_influence() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let (k0, k1) = (h.key(ROT, 0.0), h.key(ROT, 1.0));
    h.run("key.ease", json!({"keys": [k0, k1]}));
    h.session().lock().selected_keys = vec![k0.clone()];
    let mut tl = h.graph();
    let from = handle_pos(&mut tl, &k0, true);
    let va = vaxis(&mut tl, ROT);
    // Out handle of the first key to (0.5 s, 45°): influence 50 %, speed 90 °/s.
    let to = pos2(x_of(&tl, 0.5), va.y(45.0));
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    assert_eq!(r.invocations[0].id, "key.edit");
    h.apply(&r);
    let k = h.spec(ROT).keyframes[&Time::ZERO].clone();
    let e = k.ease_out[0];
    assert!((e.influence - 50.0).abs() < 0.5, "{e:?}");
    assert!((e.speed - 90.0).abs() < 1.5, "{e:?}");
    assert_eq!(k.interp_out, Interpolation::Bezier);
    h.undo();
    let e = h.spec(ROT).keyframes[&Time::ZERO].ease_out[0];
    assert_eq!(e.speed, 0.0);
}

#[test]
fn alt_drag_splits_a_continuous_handle() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(0.0))],
    );
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0), h.key(ROT, 2.0)];
    h.run("key.ease", json!({ "keys": keys }));
    let mid = h.key(ROT, 1.0);
    h.session().lock().selected_keys = vec![mid.clone()];
    let mut tl = h.graph();
    let va = vaxis(&mut tl, ROT);
    // Plain drag: a continuous key keeps both sides at the same speed.
    let from = handle_pos(&mut tl, &mid, true);
    let r = h.drag(
        &mut tl,
        from,
        pos2(from.x, va.y(90.0 + 30.0)),
        Modifiers::NONE,
    );
    h.apply(&r);
    let k = h.spec(ROT).keyframes[&Time(SEC)].clone();
    assert!(k.ease_out[0].speed > 1.0);
    assert!((k.ease_in[0].speed - k.ease_out[0].speed).abs() < 1e-9);
    assert!(k.flags.temporal_continuous);
    h.undo();
    // Alt drag: only the dragged side changes and the key is no longer continuous.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let from = handle_pos(&mut tl, &mid, true);
    let r = h.drag(&mut tl, from, pos2(from.x, va.y(90.0 + 30.0)), alt());
    h.apply(&r);
    let k = h.spec(ROT).keyframes[&Time(SEC)].clone();
    assert!(k.ease_out[0].speed > 1.0);
    assert_eq!(k.ease_in[0].speed, 0.0);
    assert!(!k.flags.temporal_continuous);
}

#[test]
fn shift_keeps_a_value_graph_handle_level() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let (k0, k1) = (h.key(ROT, 0.0), h.key(ROT, 1.0));
    h.run("key.ease", json!({"keys": [k0, k1]}));
    h.session().lock().selected_keys = vec![k0.clone()];
    let mut tl = h.graph();
    let va = vaxis(&mut tl, ROT);
    let to = pos2(x_of(&tl, 0.5), va.y(60.0));
    // Plain: the handle follows the pointer (60° over 0.5 s = 120 °/s).
    let from = handle_pos(&mut tl, &k0, true);
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    let e = h.spec(ROT).keyframes[&Time::ZERO].ease_out[0];
    assert!((e.speed - 120.0).abs() < 2.0, "{e:?}");
    h.undo();
    // Shift: level (speed 0), only the influence follows.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let from = handle_pos(&mut tl, &k0, true);
    let r = h.drag(&mut tl, from, to, Modifiers::SHIFT);
    h.apply(&r);
    let e = h.spec(ROT).keyframes[&Time::ZERO].ease_out[0];
    assert_eq!(e.speed, 0.0, "{e:?}");
    assert!((e.influence - 50.0).abs() < 0.5, "{e:?}");
    h.undo();
    assert!((h.spec(ROT).keyframes[&Time::ZERO].ease_out[0].influence - 33.33).abs() < 0.1);
}

#[test]
fn speed_graph_handle_snaps_to_zero_speed() {
    let mut h = H::new();
    h.keys(POS, &[(0.0, vec3(0.0, 0.0)), (1.0, vec3(600.0, 0.0))]);
    let (k0, k1) = (h.key(POS, 0.0), h.key(POS, 1.0));
    h.run("key.ease", json!({"keys": [k0, k1]}));
    h.session().lock().selected_keys = vec![k0.clone()];
    let mut tl = h.graph();
    // A non-zero speed first.
    let va = vaxis(&mut tl, POS);
    let from = handle_pos(&mut tl, &k0, true);
    let to = pos2(x_of(&tl, 0.5), va.y(400.0));
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    // Snap on: 5 pt above the 0 line lands on 0.
    tl.graph_editor().settings.snap = true;
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let va = vaxis(&mut tl, POS);
    let from = handle_pos(&mut tl, &k0, true);
    let to = pos2(x_of(&tl, 0.5), va.y(0.0) - 5.0);
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    assert_eq!(h.spec(POS).keyframes[&Time::ZERO].ease_out[0].speed, 0.0);
    h.undo();
    // Cmd/Ctrl turns snapping off for the drag.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let from = handle_pos(&mut tl, &k0, true);
    let r = h.drag(&mut tl, from, to, cmd());
    h.apply(&r);
    assert!(h.spec(POS).keyframes[&Time::ZERO].ease_out[0].speed > 0.0);
}

#[test]
fn dragging_one_handle_moves_the_handles_of_all_selected_keys() {
    let mut h = H::new();
    const OPACITY: &str = "transform:opacity";
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    h.keys(OPACITY, &[(0.0, float(100.0)), (2.0, float(0.0))]);
    let rot = [h.key(ROT, 0.0), h.key(ROT, 1.0)];
    let op = [h.key(OPACITY, 0.0), h.key(OPACITY, 2.0)];
    h.run("key.ease", json!({"keys": [rot[0], rot[1], op[0], op[1]]}));
    h.session().lock().selected_keys = vec![rot[0].clone(), op[0].clone()];
    let mut tl = h.graph();
    let va = vaxis(&mut tl, ROT);
    let from = handle_pos(&mut tl, &rot[0], true);
    // Rotation's out handle to (0.5 s, 45°): influence 50 %, speed 90 °/s, which is
    // the segment's average speed. Opacity follows: 50 %, its own average speed.
    let to = pos2(x_of(&tl, 0.5), va.y(45.0));
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "one command, one undo step");
    h.apply(&r);
    let e = h.spec(ROT).keyframes[&Time::ZERO].ease_out[0];
    assert!((e.influence - 50.0).abs() < 0.5, "{e:?}");
    assert!((e.speed - 90.0).abs() < 1.5, "{e:?}");
    let e = h.spec(OPACITY).keyframes[&Time::ZERO].ease_out[0];
    assert!((e.influence - 50.0).abs() < 0.5, "{e:?}");
    assert!((e.speed + 50.0).abs() < 1.0, "{e:?}");
    // The unselected keys keep their eases.
    let e = h.spec(OPACITY).keyframes[&Time(2 * SEC)].ease_in[0];
    assert!((e.influence - 33.33).abs() < 0.1, "{e:?}");
    h.undo();
    for attr in [ROT, OPACITY] {
        let e = h.spec(attr).keyframes[&Time::ZERO].ease_out[0];
        assert!(
            (e.influence - 33.33).abs() < 0.1 && e.speed == 0.0,
            "{attr}: {e:?}"
        );
    }
}

#[test]
fn speed_graph_handle_drag_on_position() {
    let mut h = H::new();
    h.keys(POS, &[(0.0, vec3(0.0, 0.0)), (1.0, vec3(600.0, 0.0))]);
    let (k0, k1) = (h.key(POS, 0.0), h.key(POS, 1.0));
    h.run("key.ease", json!({"keys": [k0, k1]}));
    h.session().lock().selected_keys = vec![k0.clone()];
    let mut tl = h.graph();
    // Auto-Select Graph Type: Position is a speed graph.
    let from = handle_pos(&mut tl, &k0, true);
    let va = vaxis(&mut tl, POS);
    let to = pos2(x_of(&tl, 0.5), va.y(400.0));
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    let e = h.spec(POS).keyframes[&Time::ZERO].ease_out[0];
    assert!((e.influence - 50.0).abs() < 0.5, "{e:?}");
    assert!((e.speed - 400.0).abs() < va.per_point() * 2.0, "{e:?}");
    // Edit Value Graph: Position shows X / Y curves, no handles.
    tl.graph_editor().settings.graph_type = crate::graph::GraphType::Value;
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(tl.graph_editor().last.handles.is_empty());
    let ys: Vec<usize> = tl
        .graph_editor()
        .last
        .keys
        .iter()
        .filter(|(k, _, _)| *k == k0)
        .map(|(_, d, _)| *d)
        .collect();
    assert_eq!(ys, vec![0, 1], "X and Y of a 2D layer");
}

#[test]
fn transform_box_scales_times_about_the_opposite_edge() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(45.0))],
    );
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0), h.key(ROT, 2.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.graph();
    let b = tl
        .graph_editor()
        .last
        .selection_box
        .expect("box around 3 keys");
    // Right edge handle (drawn 4 pt outside the keys): double the width.
    let handle = pos2(b.right() + 4.0, b.center().y);
    let r = h.drag(
        &mut tl,
        handle,
        handle + vec2(b.width(), 0.0),
        Modifiers::NONE,
    );
    h.apply(&r);
    let times: Vec<i64> = h.spec(ROT).keyframes.keys().map(|t| t.0).collect();
    assert_eq!(times, vec![0, 2 * SEC, 4 * SEC]);
    h.undo();
    let times: Vec<i64> = h.spec(ROT).keyframes.keys().map(|t| t.0).collect();
    assert_eq!(times, vec![0, SEC, 2 * SEC]);
}

#[test]
fn snap_puts_a_dragged_key_on_the_cti() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (2.0, float(90.0))]);
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    h.session().lock().set_time(&info, Time::from_seconds(1.0));
    let mut tl = h.graph();
    {
        let s = &mut tl.graph_editor().settings;
        s.allow_between_frames = true;
        s.snap = true;
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let k = h.key(ROT, 2.0);
    let from = key_pos(&mut tl, &k, 0);
    let to = pos2(x_of(&tl, 1.0) + 3.0, from.y);
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    let times: Vec<i64> = h.spec(ROT).keyframes.keys().map(|t| t.0).collect();
    assert_eq!(times, vec![0, SEC], "snapped to the CTI");
    h.undo();
    // Without Snap (and between frames allowed) it lands 3 pt off.
    tl.graph_editor().settings.snap = false;
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let from = key_pos(&mut tl, &k, 0);
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    let times: Vec<i64> = h.spec(ROT).keyframes.keys().map(|t| t.0).collect();
    assert!(times[1] > SEC, "{times:?}");
}

#[test]
fn cmd_click_toggles_linear_and_auto_bezier() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(0.0))],
    );
    let mut tl = h.graph();
    let k = h.key(ROT, 1.0);
    let p = key_pos(&mut tl, &k, 0);
    let r = h.click(&mut tl, p, cmd());
    assert_eq!(r.invocations[0].id, "key.edit");
    h.apply(&r);
    let kf = h.spec(ROT).keyframes[&Time(SEC)].clone();
    assert!(kf.flags.temporal_auto_bezier);
    assert_eq!(kf.interp_out, Interpolation::Bezier);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let p = key_pos(&mut tl, &k, 0);
    let r = h.click(&mut tl, p, cmd());
    h.apply(&r);
    let kf = h.spec(ROT).keyframes[&Time(SEC)].clone();
    assert!(!kf.flags.temporal_auto_bezier);
    assert_eq!(
        (kf.interp_in, kf.interp_out),
        (Interpolation::Linear, Interpolation::Linear)
    );
}

#[test]
fn clicks_marquee_and_alt_click_select_keys() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(45.0))],
    );
    let mut tl = h.graph();
    let k1 = h.key(ROT, 1.0);
    let p1 = key_pos(&mut tl, &k1, 0);
    h.click(&mut tl, p1, Modifiers::NONE);
    assert_eq!(h.session().lock().selected_keys, vec![k1.clone()]);
    // Clicking empty graph space deselects.
    h.click(&mut tl, pos2(p1.x + 150.0, 120.0), Modifiers::NONE);
    assert!(h.session().lock().selected_keys.is_empty());
    // Marquee around the first two keys.
    let p0 = key_pos(&mut tl, &h.key(ROT, 0.0), 0);
    let a = pos2(p0.x - 4.0, p0.y.min(p1.y) - 10.0);
    let b = pos2(p1.x + 10.0, p0.y.max(p1.y) + 10.0);
    let r = h.drag(&mut tl, a, b, Modifiers::NONE);
    assert!(r.invocations.is_empty());
    assert_eq!(h.session().lock().selected_keys.len(), 2);
    // Alt-click on the curve between keys selects all keys of the property.
    let p2 = key_pos(&mut tl, &h.key(ROT, 2.0), 0);
    let mid = pos2((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
    h.click(&mut tl, mid, alt());
    assert_eq!(h.session().lock().selected_keys.len(), 3);
}

#[test]
fn bottom_buttons_convert_and_ease() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.graph();
    let interp = |h: &H| -> Vec<(Interpolation, Interpolation, bool)> {
        h.spec(ROT)
            .keyframes
            .values()
            .map(|k| (k.interp_in, k.interp_out, k.flags.temporal_auto_bezier))
            .collect()
    };
    for (label, want) in [
        (
            "Convert selected keyframes to Hold",
            (Interpolation::Hold, Interpolation::Hold, false),
        ),
        (
            "Convert selected keyframes to Auto Bezier",
            (Interpolation::Bezier, Interpolation::Bezier, true),
        ),
        (
            "Convert selected keyframes to Linear",
            (Interpolation::Linear, Interpolation::Linear, false),
        ),
    ] {
        let r = button_rect(&mut tl, label);
        let req = h.click(&mut tl, r.center(), Modifiers::NONE);
        assert_eq!(req.invocations.len(), 1, "{label}");
        h.apply(&req);
        assert!(
            interp(&h).iter().all(|x| *x == want),
            "{label}: {:?}",
            interp(&h)
        );
    }
    // Easy Ease In: the keys' incoming side.
    let r = button_rect(&mut tl, "Easy Ease In");
    let req = h.click(&mut tl, r.center(), Modifiers::NONE);
    assert_eq!(req.invocations[0].id, "key.ease");
    assert_eq!(req.invocations[0].params["mode"], "ease_in");
    h.apply(&req);
    assert!(
        h.spec(ROT)
            .keyframes
            .values()
            .all(|k| k.interp_in == Interpolation::Bezier)
    );
    // Each button is one undo step.
    h.undo();
    assert!(
        h.spec(ROT)
            .keyframes
            .values()
            .all(|k| k.interp_in == Interpolation::Linear)
    );
    // Without selected keys the key buttons are disabled.
    h.session().lock().selected_keys.clear();
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = button_rect(&mut tl, "Easy Ease");
    let req = h.click(&mut tl, r.center(), Modifiers::NONE);
    assert!(req.invocations.is_empty());
}

#[test]
fn separate_dimensions_button() {
    let mut h = H::new();
    h.keys(POS, &[(0.0, vec3(0.0, 0.0)), (1.0, vec3(600.0, 300.0))]);
    h.session().lock().selected_props = vec![(h.layer.clone(), POS.to_owned())];
    let mut tl = h.graph();
    let r = button_rect(&mut tl, "Separate Dimensions");
    let req = h.click(&mut tl, r.center(), Modifiers::NONE);
    assert_eq!(req.invocations[0].id, "transform.separate_dimensions");
    h.apply(&req);
    assert!(bae_motion::dimensions::is_separated(&h.stage, &h.layer));
    // The graph now shows X and Y Position.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let shown: Vec<String> = tl
        .graph_editor()
        .last
        .vaxes
        .iter()
        .map(|((_, a), _)| a.clone())
        .collect();
    assert!(
        shown.contains(&"transform:position_x".to_owned()),
        "{shown:?}"
    );
    assert!(
        shown.contains(&"transform:position_y".to_owned()),
        "{shown:?}"
    );
    h.undo();
    assert!(!bae_motion::dimensions::is_separated(&h.stage, &h.layer));
}

#[test]
fn clicking_a_property_name_selects_it_and_its_keys() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let mut tl = TimelinePanel::default();
    h.session()
        .lock()
        .reveal
        .insert(h.layer.clone(), crate::session::Reveal::Animated);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    // Row 2 (the revealed Rotation); the name starts after the stopwatch and
    // the Graph Editor Set switch.
    let row_y = 22.0 + 34.0 + 20.0 + 19.0 + 9.5;
    h.click(&mut tl, pos2(124.0 + 14.0 + 50.0, row_y), Modifiers::NONE);
    let s = h.session();
    let s = s.lock();
    assert_eq!(s.selected_props, vec![(h.layer.clone(), ROT.to_owned())]);
    assert_eq!(s.selected_keys.len(), 2);
    assert_eq!(s.selected_layers, vec![h.layer.clone()]);
}

#[test]
fn graph_editor_set_and_show_properties() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let mut tl = h.graph();
    assert_eq!(tl.graph_editor().last.vaxes.len(), 1);
    // Show only the Graph Editor Set: nothing until the property's switch is on.
    {
        let s = &mut tl.graph_editor().settings;
        s.show_selected = false;
        s.show_animated = false;
        s.show_set = true;
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(tl.graph_editor().last.vaxes.is_empty());
    h.session()
        .lock()
        .reveal
        .insert(h.layer.clone(), crate::session::Reveal::Animated);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let row_y = 22.0 + 34.0 + 20.0 + 19.0 + 9.5;
    h.click(&mut tl, pos2(124.0 + 14.0 - 8.0, row_y), Modifiers::NONE);
    assert_eq!(
        h.session().lock().graph_set,
        vec![(h.layer.clone(), ROT.to_owned())]
    );
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert_eq!(tl.graph_editor().last.vaxes.len(), 1);
}

/// Frame cost with the B1-sized project (61 layers × Position, Scale, Rotation with
/// three eased keys each), every layer selected so the Graph Editor shows 183
/// properties. Run with
/// `cargo test --release -p bae-ui-motion graph_editor_frame_cost -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark"]
fn graph_editor_frame_cost() {
    let mut h = H::new();
    h.tessellate = true;
    let comp = h.comp.clone();
    let mut layers = vec![h.layer.clone()];
    for _ in 0..60 {
        layers.push(path(
            &h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"],
        ));
    }
    for l in &layers {
        for (attr, vals) in [
            (
                POS,
                vec![vec3(0.0, 0.0), vec3(300.0, 100.0), vec3(500.0, 300.0)],
            ),
            (
                "transform:scale",
                vec![vec3(100.0, 100.0), vec3(150.0, 150.0), vec3(80.0, 80.0)],
            ),
            (ROT, vec![float(0.0), float(90.0), float(180.0)]),
        ] {
            h.run(
                "attr.set_animated",
                json!({"path": l, "attr": attr, "animated": true, "time": 0}),
            );
            for (i, v) in vals.into_iter().enumerate() {
                h.run(
                    "attr.set_at",
                    json!({"path": l, "attr": attr, "time": Time::from_seconds(i as f64 * 2.0), "value": v}),
                );
            }
            let keys: Vec<KeyRef> = (0..3)
                .map(|i| KeyRef::new(l.clone(), attr, Time::from_seconds(i as f64 * 2.0)))
                .collect();
            h.run("key.ease", json!({"keys": keys}));
        }
    }
    h.session().lock().selected_layers = layers.clone();
    let mut tl = TimelinePanel::default();
    let n = 30;
    let measure = |h: &mut H, tl: &mut TimelinePanel, label: &str, graph: bool, drag: bool| {
        h.session().lock().graph_editor = graph;
        h.frame(tl, Vec::new(), Modifiers::NONE);
        let k = h.key(ROT, 2.0);
        h.session().lock().selected_keys = vec![k.clone()];
        h.frame(tl, Vec::new(), Modifiers::NONE);
        let from = if graph {
            key_pos(tl, &k, 0)
        } else {
            pos2(900.0, 400.0)
        };
        h.frame(tl, vec![Event::PointerMoved(from)], Modifiers::NONE);
        if drag {
            h.frame(
                tl,
                vec![button(from, true, Modifiers::NONE)],
                Modifiers::NONE,
            );
        }
        let t0 = std::time::Instant::now();
        for i in 1..=n {
            let to = from + vec2(i as f32 * 3.0, -(i as f32));
            h.frame(tl, vec![Event::PointerMoved(to)], Modifiers::NONE);
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
        if drag {
            let to = from + vec2(n as f32 * 3.0, -(n as f32));
            // The commit request is dropped: every pass starts from the same keys.
            h.frame(
                tl,
                vec![button(to, false, Modifiers::NONE)],
                Modifiers::NONE,
            );
        }
        eprintln!("PERF {label}: {ms:.2} ms/frame (UI + tessellation)");
    };
    measure(
        &mut h,
        &mut tl,
        "layer bar mode, pointer move",
        false,
        false,
    );
    measure(&mut h, &mut tl, "graph editor, pointer move", true, false);
    measure(&mut h, &mut tl, "graph editor, key drag", true, true);
    assert_eq!(tl.graph_editor().last.vaxes.len(), 183);
}

/// #603: Show Layer Markers draws the shown layers' markers in the Graph
/// Editor; turning it off hides them (both menu items work, as in AE).
#[test]
fn graph_editor_shows_layer_markers() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (2.0, float(90.0))]);
    let layer = h.layer.clone();
    h.run("marker.add", json!({"owner": layer, "comp_time": SEC}));
    let mut tl = h.graph();
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let xs = tl.graph_editor().last.markers.clone();
    assert_eq!(xs.len(), 1, "{xs:?}");
    assert!((xs[0] - x_of(&tl, 1.0)).abs() < 1.0, "{xs:?}");
    tl.graph_editor().settings.show_markers = false;
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(tl.graph_editor().last.markers.is_empty());
    assert!(crate::graph::GraphSettings::default().show_waveforms);
}

/// #593: a keyframe drag in the Graph Editor, or of layer-bar keyframes, is
/// evaluated live everywhere (the viewer's evaluator sees the value at the
/// CTI change during the drag), and the preview stays until the edit lands.
#[test]
fn keyframe_drags_are_evaluated_live() {
    use crate::common::EvalCache;
    let mut h = H::new();
    let comp = h.comp.clone();
    let layer = h.layer.clone();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let mut eval = EvalCache::default();
    // Rotation as the viewer evaluates it at `secs`.
    let rot_at = |h: &H, eval: &mut EvalCache, secs: f64| {
        let mut requests = PanelRequests::default();
        let id = PanelId::new("composition");
        let cx = PanelContext::new(
            &id,
            "composition",
            &h.stage,
            &h.author,
            &h.resources,
            &mut requests,
        );
        crate::viewer::live_evaluator(eval, &cx, &h.session())
            .evaluate_attribute(&h.stage, &layer, ROT, Time::from_seconds(secs))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap()
    };
    assert_eq!(rot_at(&h, &mut eval, 1.0), 90.0);
    // Graph Editor: drag the 1 s key up to about 150°, check mid-drag.
    let mut tl = h.graph();
    let k = h.key(ROT, 1.0);
    let from = key_pos(&mut tl, &k, 0);
    let va = vaxis(&mut tl, ROT);
    let to = pos2(from.x, va.y(150.0));
    h.frame(&mut tl, vec![Event::PointerMoved(from)], Modifiers::NONE);
    h.frame(
        &mut tl,
        vec![button(from, true, Modifiers::NONE)],
        Modifiers::NONE,
    );
    for i in 1..=4 {
        let p = from + (to - from) * (i as f32 / 4.0);
        h.frame(&mut tl, vec![Event::PointerMoved(p)], Modifiers::NONE);
    }
    let live = rot_at(&h, &mut eval, 1.0);
    assert!((live - 150.0).abs() < 5.0, "live {live}");
    let r = h.frame(
        &mut tl,
        vec![button(to, false, Modifiers::NONE)],
        Modifiers::NONE,
    );
    assert_eq!(r.invocations.len(), 1);
    // Sent, not run yet: still shown.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!((rot_at(&h, &mut eval, 1.0) - live).abs() < 1e-6);
    h.apply(&r);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(h.session().lock().key_preview.is_none());
    assert!((rot_at(&h, &mut eval, 1.0) - live).abs() < 1e-6);

    // Layer bars: drag the 1 s key to 2 s; at 1 s the value is then halfway.
    h.session().lock().graph_editor = false;
    let mut tl = h.bars();
    let k = h.key(ROT, 1.0);
    h.session().lock().selected_keys = vec![k.clone()];
    let from = bar_key(&tl, &k);
    let to = pos2(x_of(&tl, 2.0), from.y);
    h.frame(&mut tl, vec![Event::PointerMoved(from)], Modifiers::NONE);
    h.frame(
        &mut tl,
        vec![button(from, true, Modifiers::NONE)],
        Modifiers::NONE,
    );
    for i in 1..=4 {
        let p = from + (to - from) * (i as f32 / 4.0);
        h.frame(&mut tl, vec![Event::PointerMoved(p)], Modifiers::NONE);
    }
    let mid = rot_at(&h, &mut eval, 1.0);
    assert!(
        (mid - live / 2.0).abs() < live * 0.1,
        "{mid} vs {}",
        live / 2.0
    );
    let _ = comp;
}

/// #466: the Timeline's empty-space menu runs Reveal Composition in Project
/// and Composition Flowchart (AE enables both).
#[test]
fn reveal_comp_in_project_and_flowchart() {
    use crate::actions::ids;
    let mut h = H::new();
    let comp = h.comp.clone();
    let mut tl = TimelinePanel::default();
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.action(&mut tl, Invocation::new(ids::REVEAL_COMP_IN_PROJECT));
    assert!(r.open.iter().any(|p| p.kind() == "project"), "{:?}", r.open);
    assert_eq!(h.session().lock().selected_items, vec![comp]);
    let r = h.action(&mut tl, Invocation::new(ids::COMP_FLOWCHART));
    assert!(
        r.open.iter().any(|p| p.kind() == "flowchart"),
        "{:?}",
        r.open
    );
}
