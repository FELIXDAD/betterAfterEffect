//! Layer-bar keyframes, the keyframe menu and dialogs, keyframe labels and selection.

use super::*;

#[test]
fn shift_snaps_a_dragged_keyframe_to_the_cti() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (2.0, float(90.0))]);
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    h.session().lock().set_time(&info, Time::from_seconds(1.0));
    let mut tl = h.bars();
    let k = h.key(ROT, 2.0);
    let from = bar_key(&tl, &k);
    // 3 pt right of the CTI: about one frame off.
    let to = pos2(x_of(&tl, 1.0) + 3.0, from.y);
    let r = h.drag(&mut tl, from, to, Modifiers::SHIFT);
    assert_eq!(r.invocations[0].id, "key.move");
    h.apply(&r);
    assert_eq!(h.times(ROT), vec![0, SEC], "snapped to the CTI");
    assert_eq!(h.session().lock().selected_keys, vec![h.key(ROT, 1.0)]);
    h.undo();
    assert_eq!(h.times(ROT), vec![0, 2 * SEC]);
    // Without Shift it moves by whole frames from the pointer.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.drag(&mut tl, from, to, Modifiers::NONE);
    h.apply(&r);
    let t = h.times(ROT);
    let frame = SEC / 30;
    assert!(t[1] > SEC && t[1] % frame == 0, "{t:?}");
}

#[test]
fn alt_drag_scales_the_selected_keyframes_about_the_other_end() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(45.0)), (2.0, float(90.0))],
    );
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0), h.key(ROT, 2.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.bars();
    // Alt-drag the last keyframe from 2 s to 4 s: the first stays, the timing doubles.
    let from = bar_key(&tl, &keys[2]);
    let to = pos2(x_of(&tl, 4.0), from.y);
    let r = h.drag(&mut tl, from, to, alt());
    assert_eq!(r.invocations.len(), 1, "one command, one undo step");
    assert_eq!(r.invocations[0].id, "key.edit");
    h.apply(&r);
    assert_eq!(h.times(ROT), vec![0, 2 * SEC, 4 * SEC]);
    assert_eq!(
        h.spec(ROT).keyframes[&Time(2 * SEC)].value,
        Value::Float(45.0),
        "values stay"
    );
    assert_eq!(
        h.session().lock().selected_keys,
        vec![h.key(ROT, 0.0), h.key(ROT, 2.0), h.key(ROT, 4.0)]
    );
    h.undo();
    assert_eq!(h.times(ROT), vec![0, SEC, 2 * SEC]);
    // Alt-dragging the first keyframe scales about the last one.
    h.session().lock().selected_keys = keys.clone();
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let from = bar_key(&tl, &keys[0]);
    let to = pos2(x_of(&tl, 1.0), from.y);
    let r = h.drag(&mut tl, from, to, alt());
    h.apply(&r);
    assert_eq!(h.times(ROT), vec![SEC, SEC + SEC / 2, 2 * SEC]);
}

#[test]
fn cmd_click_on_a_layer_bar_keyframe_converts_its_interpolation() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let k = h.key(ROT, 0.0);
    let mut tl = h.bars();
    let p = bar_key(&tl, &k);
    // Linear → Auto Bezier.
    let r = h.click(&mut tl, p, cmd());
    h.apply(&r);
    let kf = h.spec(ROT).keyframes[&Time::ZERO].clone();
    assert!(kf.flags.temporal_auto_bezier, "{kf:?}");
    assert_eq!(kf.interp_out, Interpolation::Bezier);
    assert!(
        h.session().lock().selected_keys.is_empty(),
        "Cmd-click does not select"
    );
    // Bezier → Linear.
    let r = h.click(&mut tl, p, cmd());
    h.apply(&r);
    assert_eq!(
        h.spec(ROT).keyframes[&Time::ZERO].interp_out,
        Interpolation::Linear
    );
    // Cmd+Alt: Linear → Hold, and back.
    let r = h.click(&mut tl, p, cmd() | alt());
    h.apply(&r);
    let kf = h.spec(ROT).keyframes[&Time::ZERO].clone();
    assert_eq!(
        (kf.interp_in, kf.interp_out),
        (Interpolation::Hold, Interpolation::Hold)
    );
    h.undo();
    assert_eq!(
        h.spec(ROT).keyframes[&Time::ZERO].interp_out,
        Interpolation::Linear
    );
    // Shift-click toggles the selection.
    h.click(&mut tl, p, Modifiers::SHIFT);
    assert_eq!(h.session().lock().selected_keys, vec![k.clone()]);
    h.click(&mut tl, p, Modifiers::SHIFT);
    assert!(h.session().lock().selected_keys.is_empty());
}

#[test]
fn cmd_click_on_a_selected_keyframe_converts_the_whole_selection() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(0.0))],
    );
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0), h.key(ROT, 2.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.bars();
    let p = bar_key(&tl, &keys[1]);
    let r = h.click(&mut tl, p, cmd());
    assert_eq!(r.invocations.len(), 1, "one step");
    h.apply(&r);
    assert!(
        h.spec(ROT)
            .keyframes
            .values()
            .all(|k| k.flags.temporal_auto_bezier),
        "all three became Auto Bezier (R1, AE 26)"
    );
}

#[test]
fn alt_pressed_after_mouse_down_still_moves() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(45.0)), (2.0, float(90.0))],
    );
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0), h.key(ROT, 2.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.bars();
    let from = bar_key(&tl, &keys[2]);
    let to = pos2(x_of(&tl, 4.0), from.y);
    // Press without Alt, then hold Alt while dragging: a rigid move (AE 26).
    h.frame(&mut tl, vec![Event::PointerMoved(from)], Modifiers::NONE);
    h.frame(
        &mut tl,
        vec![button(from, true, Modifiers::NONE)],
        Modifiers::NONE,
    );
    for i in 1..=6 {
        let p = from + (to - from) * (i as f32 / 6.0);
        h.frame(&mut tl, vec![Event::PointerMoved(p)], alt());
    }
    let r = h.frame(&mut tl, vec![button(to, false, alt())], alt());
    assert_eq!(r.invocations[0].id, "key.move");
    h.apply(&r);
    assert_eq!(h.times(ROT), vec![2 * SEC, 3 * SEC, 4 * SEC]);
}

#[test]
fn keyframe_velocity_shows_linear_sides_as_ae_does() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (2.0, float(90.0))]);
    let d = crate::timeline::VelocityDialog::new(&h.stage, vec![h.key(ROT, 0.0)]).unwrap();
    // Linear: the segment's speed (45 °/s) and 16.667 % influence.
    assert!(
        (d.ease_out[0].speed - 45.0).abs() < 1e-6,
        "{:?}",
        d.ease_out
    );
    assert!((d.ease_out[0].influence - 100.0 / 6.0).abs() < 1e-6);
    // Scale: one row per dimension, Lock Dimensions on.
    h.keys(
        "transform:scale",
        &[(0.0, vec3(100.0, 100.0)), (1.0, vec3(200.0, 100.0))],
    );
    let d = crate::timeline::VelocityDialog::new(&h.stage, vec![h.key("transform:scale", 0.0)])
        .unwrap();
    assert!(d.lock_dims && d.ease_out.len() >= 2);
    assert!((d.ease_out[0].speed - 100.0).abs() < 1e-6 && d.ease_out[1].speed.abs() < 1e-6);
}

#[test]
fn keyframe_labels_and_label_groups() {
    use crate::actions::ids;
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(0.0))],
    );
    h.keys(POS, &[(0.0, vec3(0.0, 0.0)), (1.0, vec3(10.0, 0.0))]);
    let labelled = [h.key(ROT, 0.0), h.key(ROT, 2.0), h.key(POS, 1.0)];
    let edits: Vec<Json> = labelled
        .iter()
        .map(|k| json!({"key": k, "label": 3}))
        .collect();
    h.run("key.edit", json!({ "edits": edits }));
    assert_eq!(h.spec(ROT).keyframes[&Time::ZERO].label, 3);
    let session = h.session();
    crate::timeline::select_label_group(&h.stage, &session, &labelled[0]);
    let mut sel = session.lock().selected_keys.clone();
    sel.sort_by_key(|k| (k.attr.clone(), k.time));
    let mut want = labelled.to_vec();
    want.sort_by_key(|k| (k.attr.clone(), k.time));
    assert_eq!(sel, want);
    // Edit > Select Keyframe Label Group ▸ On All Layers, from one labelled key.
    let mut tl = h.bars();
    session.lock().selected_keys = vec![labelled[2].clone()];
    h.action(
        &mut tl,
        Invocation::with_params(ids::SELECT_KEY_LABEL_GROUP, json!({"scope": "all_layers"})),
    );
    assert_eq!(session.lock().selected_keys.len(), 3);
}

#[test]
fn keyframe_interpolation_dialog_sets_temporal_spatial_and_roving() {
    let mut h = H::new();
    h.keys(
        POS,
        &[
            (0.0, vec3(0.0, 0.0)),
            (1.0, vec3(100.0, 0.0)),
            (2.0, vec3(100.0, 100.0)),
        ],
    );
    let keys = vec![h.key(POS, 0.0), h.key(POS, 1.0), h.key(POS, 2.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.bars();
    let r = h.action(
        &mut tl,
        Invocation::new(crate::actions::ids::KEY_INTERPOLATION),
    );
    assert!(
        r.invocations.is_empty(),
        "the dialog opens; nothing changes yet"
    );
    {
        let s = h.session();
        let mut s = s.lock();
        let Some(crate::timeline::KeyDialog::Interpolation(d)) = &mut s.key_dialog else {
            panic!("Keyframe Interpolation is open");
        };
        d.temporal = 3; // Continuous Bezier
        d.spatial = 2; // Bezier
        d.roving = 1; // Rove Across Time
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.frame(&mut tl, vec![press(egui::Key::Enter)], Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "OK is one command");
    assert_eq!(r.invocations[0].id, "key.edit");
    assert!(h.session().lock().key_dialog.is_none());
    h.apply(&r);
    let spec = h.spec(POS);
    let ks: Vec<&bae_model::Keyframe> = spec.keyframes.values().collect();
    assert_eq!(ks.len(), 3);
    for k in &ks {
        assert_eq!(
            (k.interp_in, k.interp_out),
            (Interpolation::Bezier, Interpolation::Bezier)
        );
        assert!(k.flags.temporal_continuous && !k.flags.temporal_auto_bezier);
        assert!(!k.flags.spatial_auto_bezier && !k.flags.spatial_continuous);
    }
    // Bezier from Linear starts from the automatic tangents of the corner key.
    assert!(
        ks[1].spatial_out.is_some_and(|t| t != [0.0; 3]),
        "{:?}",
        ks[1]
    );
    assert!(!ks[0].flags.roving && ks[1].flags.roving && !ks[2].flags.roving);
    h.undo();
    let spec = h.spec(POS);
    assert!(
        spec.keyframes
            .values()
            .all(|k| k.interp_out == Interpolation::Linear && !k.flags.roving)
    );
}

#[test]
fn keyframe_dialogs_cancel_with_esc_and_need_selected_keys() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let mut tl = h.bars();
    // Nothing selected: the action does nothing.
    h.action(
        &mut tl,
        Invocation::new(crate::actions::ids::KEY_INTERPOLATION),
    );
    assert!(h.session().lock().key_dialog.is_none());
    h.session().lock().selected_keys = vec![h.key(ROT, 0.0)];
    h.action(&mut tl, Invocation::new(crate::actions::ids::KEY_VELOCITY));
    assert!(h.session().lock().key_dialog.is_some());
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.frame(&mut tl, vec![press(egui::Key::Escape)], Modifiers::NONE);
    assert!(r.invocations.is_empty());
    assert!(h.session().lock().key_dialog.is_none());
}

#[test]
fn keyframe_velocity_dialog_sets_every_selected_key() {
    let mut h = H::new();
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(90.0)), (2.0, float(0.0))],
    );
    let keys = vec![h.key(ROT, 0.0), h.key(ROT, 1.0)];
    h.session().lock().selected_keys = keys.clone();
    let mut tl = h.bars();
    h.action(&mut tl, Invocation::new(crate::actions::ids::KEY_VELOCITY));
    {
        let s = h.session();
        let mut s = s.lock();
        let Some(crate::timeline::KeyDialog::Velocity(d)) = &mut s.key_dialog else {
            panic!("Keyframe Velocity is open");
        };
        d.ease_in[0] = bae_model::Ease {
            speed: 0.0,
            influence: 75.0,
        };
        d.ease_out[0] = bae_model::Ease {
            speed: 45.0,
            influence: 50.0,
        };
        d.continuous = false;
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.frame(&mut tl, vec![press(egui::Key::Enter)], Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1);
    h.apply(&r);
    let spec = h.spec(ROT);
    for t in [Time::ZERO, Time(SEC)] {
        let k = &spec.keyframes[&t];
        assert_eq!(k.interp_out, Interpolation::Bezier);
        assert_eq!(k.ease_out[0].speed, 45.0);
        assert_eq!(k.ease_out[0].influence, 50.0);
        assert_eq!(k.ease_in[0].influence, 75.0);
    }
    assert_eq!(
        spec.keyframes[&Time(2 * SEC)].interp_in,
        Interpolation::Linear,
        "unselected keys stay"
    );
    h.undo();
    assert!(
        h.spec(ROT)
            .keyframes
            .values()
            .all(|k| k.interp_out == Interpolation::Linear)
    );
}

#[test]
fn select_equal_previous_and_following_keyframes() {
    use crate::timeline::Related;
    let mut h = H::new();
    h.keys(
        ROT,
        &[
            (0.0, float(0.0)),
            (1.0, float(90.0)),
            (2.0, float(0.0)),
            (3.0, float(45.0)),
        ],
    );
    let session = h.session();
    let clicked = h.key(ROT, 2.0);
    crate::timeline::select_related(&h.stage, &session, &clicked, Related::Equal);
    assert_eq!(
        session.lock().selected_keys,
        vec![h.key(ROT, 0.0), h.key(ROT, 2.0)]
    );
    crate::timeline::select_related(&h.stage, &session, &clicked, Related::Previous);
    assert_eq!(
        session.lock().selected_keys,
        vec![h.key(ROT, 0.0), h.key(ROT, 1.0), h.key(ROT, 2.0)]
    );
    crate::timeline::select_related(&h.stage, &session, &clicked, Related::Following);
    assert_eq!(
        session.lock().selected_keys,
        vec![h.key(ROT, 2.0), h.key(ROT, 3.0)]
    );
}

#[test]
fn rove_across_time_needs_inner_spatial_keyframes() {
    let mut h = H::new();
    h.keys(
        POS,
        &[
            (0.0, vec3(0.0, 0.0)),
            (1.0, vec3(100.0, 0.0)),
            (2.0, vec3(100.0, 100.0)),
        ],
    );
    h.keys(
        ROT,
        &[(0.0, float(0.0)), (1.0, float(9.0)), (2.0, float(0.0))],
    );
    let st = &h.stage;
    assert_eq!(
        crate::timeline::rove_state(st, &[h.key(POS, 1.0)]),
        (true, false)
    );
    assert!(
        !crate::timeline::rove_state(st, &[h.key(POS, 0.0)]).0,
        "first key"
    );
    assert!(
        !crate::timeline::rove_state(st, &[h.key(ROT, 1.0)]).0,
        "not spatial"
    );
    assert!(!crate::timeline::rove_state(st, &[]).0);
}

/// #551: marquee two keys, drag one, F9: the ease applies to both moved keys
/// as its own undo step (timeline and Graph Editor), whatever panels draw
/// between the drag and the move.
#[test]
fn easy_ease_after_dragging_marquee_selected_keys() {
    for graph in [false, true] {
        let mut h = H::new();
        let (t0, t1) = (62.0 / 30.0, 242.0 / 30.0);
        h.keys(POS, &[(t0, vec3(960.0, 540.0)), (t1, vec3(1500.0, 540.0))]);
        let mut tl = if graph { h.graph() } else { h.bars() };
        let keys = vec![h.key(POS, t0), h.key(POS, t1)];
        let (a, b) = if graph {
            (key_pos(&mut tl, &keys[0], 0), key_pos(&mut tl, &keys[1], 0))
        } else {
            (bar_key(&tl, &keys[0]), bar_key(&tl, &keys[1]))
        };
        // Marquee from empty space around both keys.
        let from = pos2(a.x - 20.0, a.y.min(b.y) - 8.0);
        let to = pos2(b.x + 20.0, a.y.max(b.y) + 8.0);
        let r = h.drag(&mut tl, from, to, Modifiers::NONE);
        h.apply(&r);
        let mut sel = h.session().lock().selected_keys.clone();
        sel.sort_by_key(|k| k.time);
        assert_eq!(sel, keys, "graph {graph}: marquee selects both");
        // Drag the first key about a second to the right.
        let from = if graph {
            key_pos(&mut tl, &keys[0], 0)
        } else {
            bar_key(&tl, &keys[0])
        };
        let to = pos2(x_of(&tl, t0 + 1.0), from.y);
        let r = h.drag(&mut tl, from, to, Modifiers::NONE);
        // A panel drawn after the Timeline in the same frame syncs the session
        // before the queued move has run: the selection must survive it.
        h.session().lock().sync(&h.stage);
        assert_eq!(
            h.session().lock().selected_keys.len(),
            2,
            "graph {graph}: kept"
        );
        h.apply(&r);
        let times = h.times(POS);
        assert!(
            times[0] > (t0 * SEC as f64) as i64,
            "graph {graph}: moved {times:?}"
        );
        let sel = h.session().lock().selected_keys.clone();
        for k in &sel {
            assert!(
                h.spec(POS).keyframes.contains_key(&k.time),
                "graph {graph}: selection points at a key: {k:?} in {times:?}"
            );
        }
        assert_eq!(sel.len(), 2, "graph {graph}: both stay selected");
        h.frame(&mut tl, Vec::new(), Modifiers::NONE);
        // F9.
        let r = h.action(
            &mut tl,
            Invocation::with_params(crate::actions::ids::EASE, json!({"mode": "easy_ease"})),
        );
        assert_eq!(r.invocations.len(), 1, "graph {graph}: {:?}", r.invocations);
        assert_eq!(r.invocations[0].id, "key.ease");
        h.apply(&r);
        assert!(
            h.spec(POS)
                .keyframes
                .values()
                .all(|k| k.interp_in == Interpolation::Bezier
                    && k.interp_out == Interpolation::Bezier),
            "graph {graph}: eased"
        );
        h.undo();
        assert!(
            h.spec(POS)
                .keyframes
                .values()
                .all(|k| k.interp_out == Interpolation::Linear),
            "graph {graph}: undo takes the ease back, not the drag"
        );
        assert_eq!(h.times(POS), times, "graph {graph}: the drag stays");
    }
}
