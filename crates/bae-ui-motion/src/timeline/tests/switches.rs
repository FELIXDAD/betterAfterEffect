//! Switches and Modes columns, F4, Track Matte, comp switches.

use super::*;

#[test]
fn f4_alternates_switches_and_modes() {
    let mut h = H::new();
    let mut tl = h.bars();
    let p = |tl: &TimelinePanel| {
        (
            tl.column_visible(crate::timeline::Column::Switches),
            tl.column_visible(crate::timeline::Column::Modes),
        )
    };
    assert_eq!(p(&tl), (true, false), "AE starts with Switches");
    let f4 = || Invocation::new(crate::actions::ids::TOGGLE_SWITCHES_MODES);
    h.action(&mut tl, f4());
    assert_eq!(p(&tl), (false, true));
    h.action(&mut tl, f4());
    assert_eq!(p(&tl), (true, false));
    h.action(
        &mut tl,
        Invocation::new(crate::actions::ids::TOGGLE_PARENT_COLUMN),
    );
    assert!(!tl.column_visible(crate::timeline::Column::Parent));
    // With Modes hidden no Modes cells are drawn.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert!(tl.last_modes.is_empty());
}

#[test]
fn hide_shy_layers_switch_hides_shy_layers_from_the_timeline() {
    let mut h = H::new();
    let comp = h.comp.clone();
    let shy = h.layer.clone();
    let other = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    h.run(
        "attr.set",
        json!({"path": shy, "attr": "shy", "value": {"type": "bool", "value": true}}),
    );
    let mut tl = h.bars();
    show_modes(&mut h, &mut tl);
    let drawn = |tl: &TimelinePanel| tl.last_modes.len();
    assert_eq!(drawn(&tl), 2);
    // The comp switch three left of the Graph Editor switch (AE order).
    let sw = pos2(tl.last_axis.unwrap().left - 2.0 - 14.0 - 78.0, 22.0 + 17.0);
    let r = h.click(&mut tl, sw, Modifiers::NONE);
    h.apply(&r);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert_eq!(tl.last_modes.len(), 1);
    assert_eq!(tl.last_modes[0].0, other);
    h.undo();
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert_eq!(drawn(&tl), 2);
}

#[test]
fn modes_column_t_switch_and_track_matte_pick_whip() {
    let mut h = H::new();
    let comp = h.comp.clone();
    let fill = h.layer.clone();
    let matte = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    let mut tl = h.bars();
    show_modes(&mut h, &mut tl);
    let attr = |h: &H, l: &PrimPath, a: &str| query::static_value(&h.stage.prim(l).unwrap(), a);
    // T: Preserve Underlying Transparency.
    let (t, ..) = modes_cell(&tl, &fill);
    let r = h.click(&mut tl, t.center(), Modifiers::NONE);
    h.apply(&r);
    assert_eq!(
        attr(&h, &fill, "preserve_transparency"),
        Some(Value::Bool(true))
    );
    // Pick whip from the fill layer onto the other layer: an Alpha matte, and the
    // matte layer's Video goes off.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let (_, whip, _) = modes_cell(&tl, &fill);
    let (mt, ..) = modes_cell(&tl, &matte);
    let target = pos2(500.0, mt.center().y);
    let r = h.drag(&mut tl, whip, target, Modifiers::NONE);
    assert_eq!(r.invocations[0].id, "layer.set_track_matte");
    h.apply(&r);
    assert_eq!(
        attr(&h, &fill, "track_matte"),
        Some(Value::Enum("alpha".into()))
    );
    assert_eq!(
        attr(&h, &fill, "track_matte_layer"),
        Some(Value::PrimRef(matte.clone()))
    );
    assert_eq!(attr(&h, &matte, "enabled"), Some(Value::Bool(false)));
    h.undo();
    assert_eq!(
        attr(&h, &fill, "track_matte"),
        Some(Value::Enum("none".into()))
    );
    // Layer > Track Matte > Matte with Layer Above > Luma Matte.
    h.session().lock().selected_layers = vec![fill.clone()];
    let r = h.action(
        &mut tl,
        Invocation::with_params(
            crate::actions::ids::TRACK_MATTE,
            json!({"matte": "luma", "source": "above"}),
        ),
    );
    h.apply(&r);
    assert_eq!(
        attr(&h, &fill, "track_matte"),
        Some(Value::Enum("luma".into()))
    );
    assert_eq!(
        attr(&h, &fill, "track_matte_layer"),
        Some(Value::PrimRef(matte.clone()))
    );
    // The Invert, then the Alpha / Luma toggle in the row.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let (.., alpha_luma) = modes_cell(&tl, &fill);
    let r = h.click(&mut tl, alpha_luma + vec2(17.0, 0.0), Modifiers::NONE);
    h.apply(&r);
    assert_eq!(
        attr(&h, &fill, "track_matte"),
        Some(Value::Enum("luma_inverted".into()))
    );
    let r = h.click(&mut tl, alpha_luma, Modifiers::NONE);
    h.apply(&r);
    assert_eq!(
        attr(&h, &fill, "track_matte"),
        Some(Value::Enum("alpha_inverted".into()))
    );
    h.undo();
    assert_eq!(
        attr(&h, &fill, "track_matte"),
        Some(Value::Enum("luma_inverted".into()))
    );
    assert_eq!(
        crate::timeline::flip_matte("luma", false, true),
        "luma_inverted"
    );
    assert_eq!(
        crate::timeline::flip_matte("alpha_inverted", true, false),
        "luma_inverted"
    );
}

#[test]
fn comp_switches_and_layer_switches_reach_the_render_request() {
    use crate::actions::ids;
    let mut h = H::new();
    h.keys(POS, &[(0.0, vec3(0.0, 0.0)), (1.0, vec3(600.0, 0.0))]);
    let comp = h.comp.clone();
    let mut tl = h.bars();
    let request = |h: &H| {
        let ec = bae_anim::evaluate_comp(&h.stage, &comp, Time::from_seconds(0.5)).unwrap();
        bae_render::AnimEvaluator::new(None)
            .convert(&h.stage, &ec, 0)
            .unwrap()
    };
    // The layer's Motion Blur switch alone: no samples (AE needs both).
    let r = h.action(
        &mut tl,
        Invocation::with_params(ids::LAYER_SWITCH, json!({"switch": "motion_blur"})),
    );
    h.apply(&r);
    assert!(request(&h).motion_blur.is_none());
    // Enable Motion Blur, the comp switch second from the Graph Editor switch.
    let ge = pos2(tl.last_axis.unwrap().left - 2.0 - 14.0, 22.0 + 17.0);
    let r = h.click(&mut tl, ge - vec2(26.0, 0.0), Modifiers::NONE);
    h.apply(&r);
    let rc = request(&h);
    assert!(rc.motion_blur.is_some());
    assert!(rc.layers.iter().any(|l| !l.motion_samples.is_empty()));
    // Hide Shy Layers and Draft 3D are comp settings; Frame Blending is disabled.
    for (dx, attr, on) in [
        (52.0, "frame_blending", false),
        (78.0, "hide_shy_layers", true),
        (104.0, "draft_3d", true),
    ] {
        let r = h.click(&mut tl, ge - vec2(dx, 0.0), Modifiers::NONE);
        h.apply(&r);
        let v = query::static_value(&h.stage.prim(&comp).unwrap(), attr);
        assert_eq!(v.and_then(|v| v.as_bool()).unwrap_or(false), on, "{attr}");
    }
    // 3D Layer: the layer gets its 3D matrix.
    assert!(request(&h).layers.iter().all(|l| l.matrix_3d.is_none()));
    let r = h.action(
        &mut tl,
        Invocation::with_params(ids::LAYER_SWITCH, json!({"switch": "three_d"})),
    );
    h.apply(&r);
    assert!(request(&h).layers.iter().any(|l| l.matrix_3d.is_some()));
    // Undo the comp switches one step at a time.
    h.undo();
    h.undo();
    h.undo();
    assert!(request(&h).motion_blur.is_some());
    h.undo();
    assert!(request(&h).motion_blur.is_none());
}
