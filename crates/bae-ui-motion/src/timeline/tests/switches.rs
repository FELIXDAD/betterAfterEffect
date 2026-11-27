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
    // F4 / Shift+F4 pressed while another panel is in front reach the
    // Timeline through the session (#459).
    {
        let session = h.session();
        let mut s = session.lock();
        s.timeline_toggles
            .push(crate::actions::ids::TOGGLE_SWITCHES_MODES);
        s.timeline_toggles
            .push(crate::actions::ids::TOGGLE_PARENT_COLUMN);
    }
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    assert_eq!(p(&tl), (false, true));
    assert!(tl.column_visible(crate::timeline::Column::Parent));
    assert!(h.session().lock().timeline_toggles.is_empty());
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
    let mut prefs = bae_prefs::Preferences::default();
    prefs.set("previews.auto_enable_frame_blending_motion_blur", false);
    h.resources
        .insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(prefs)));
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
    // Draft 3D has no renderer. Frame Mix is available with its sampling backend.
    for (dx, attr, on) in [
        (52.0, "frame_blending", true),
        (78.0, "hide_shy_layers", true),
        (104.0, "draft_3d", false),
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
    // Undo the 3D layer, Hide Shy and Frame Mix.
    h.undo();
    h.undo();
    h.undo();
    assert!(request(&h).motion_blur.is_some());
    h.undo();
    assert!(request(&h).motion_blur.is_none());
}

#[test]
fn sweeping_a_switch_crosses_skipped_rows_and_backtracks_in_one_undo() {
    let mut h = H::new();
    for _ in 0..3 {
        h.run("layer.create", json!({"comp": h.comp, "kind": "solid"}));
    }
    let mut tl = h.bars();
    let mut rows = tl.last_bars.clone();
    rows.sort_by(|a, b| a.1.center().y.total_cmp(&b.1.center().y));
    assert_eq!(rows.len(), 4);
    let point = |i: usize| pos2(12.0, rows[i].1.center().y);
    let read = |h: &H| {
        rows.iter()
            .map(|(p, _)| {
                query::static_value(&h.stage.prim(p).unwrap(), "enabled") == Some(Value::Bool(true))
            })
            .collect::<Vec<_>>()
    };
    h.run(
        "layer.set_attr",
        json!({"layers":[rows[1].0],"attr":"locked","value":{"type":"bool","value":true}}),
    );
    // The eye of a locked layer can be switched too (AE, #485).
    for (initial, expected) in [(true, vec![false; 4]), (false, vec![true; 4])] {
        for events in [
            vec![Event::PointerMoved(point(0))],
            vec![button(point(0), true, Modifiers::NONE)],
            vec![Event::PointerMoved(point(3))],
            vec![Event::PointerMoved(point(0))],
            vec![Event::PointerMoved(point(0) + vec2(30.0, 0.0))],
        ] {
            assert!(
                h.frame(&mut tl, events, Modifiers::NONE)
                    .invocations
                    .is_empty()
            );
        }
        let r = h.frame(
            &mut tl,
            vec![button(point(0) + vec2(30.0, 0.0), false, Modifiers::NONE)],
            Modifiers::NONE,
        );
        assert_eq!(r.invocations.len(), 1);
        assert_eq!(
            r.invocations[0].params["layers"].as_array().unwrap().len(),
            4
        );
        h.apply(&r);
        assert_eq!(read(&h), expected);
        h.undo();
        assert_eq!(read(&h)[0], initial);
        h.apply(&r);
        h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    }
}

#[test]
fn locked_switches_preserve_selection_and_video_solo_follows_the_eye() {
    let mut h = H::new();
    let file = tempfile::NamedTempFile::new().unwrap();
    let asset = path(
        &h.run(
            "item.import",
            json!({"files":[{"path":file.path(), "attrs":{
                "kind": Value::Enum("footage".into()), "has_audio": Value::Bool(true),
                "duration": Value::Time(Time::from_seconds(2.0))
            }}]}),
        )["items"][0],
    );
    let mut make_layer = |name| {
        path(
            &h.run(
                "layer.create",
                json!({
                    "kind":"footage", "comp":h.comp, "source":asset, "name":name
                }),
            )["path"],
        )
    };
    let other = make_layer("Other");
    let locked = make_layer("Locked");
    h.run(
        "attr.set",
        json!({"path":locked,"attr":"locked","value":Value::Bool(true)}),
    );
    let mut tl = h.bars();
    let y = |path: &PrimPath| {
        tl.last_bars
            .iter()
            .find(|(p, _)| p == path)
            .unwrap()
            .1
            .center()
            .y
    };
    let (y1, y2) = (y(&locked), y(&other));
    let switches = tl.last_axis.unwrap().left - 2.0 - 120.0 - 134.0;
    let point = |col: usize, y| pos2(5.0 + 15.0 * (col as f32 + 0.5), y);
    let click = |h: &mut H, tl: &mut TimelinePanel, pos| {
        let request = h.click(tl, pos, Modifiers::NONE);
        h.apply(&request);
        assert!(h.stage.is_layer_locked(&locked));
        assert!(!h.session().lock().selected_layers.contains(&locked));
    };
    let check = |h: &H, shown: [bool; 2], audio_enabled: bool| {
        let comp = bae_anim::evaluate_comp(&h.stage, &h.comp, Time::ZERO).unwrap();
        assert_eq!(
            [
                comp.layer(&locked).unwrap().visible,
                comp.layer(&other).unwrap().visible
            ],
            shown
        );
        let audio = h
            .stage
            .prim(&locked)
            .unwrap()
            .value("audio_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        assert_eq!(audio, audio_enabled);
    };
    click(&mut h, &mut tl, pos2(switches - 60.0, y1)); // name stays unselected
    check(&h, [true, true], true);
    click(&mut h, &mut tl, point(2, y1)); // solo locked layer
    check(&h, [true, false], true);
    click(&mut h, &mut tl, point(0, y1)); // close its eye
    check(&h, [false, true], true);
    click(&mut h, &mut tl, point(1, y1)); // mute its audio
    check(&h, [false, true], false);
    click(&mut h, &mut tl, point(0, y1));
    check(&h, [true, false], false);
    click(&mut h, &mut tl, point(1, y1));
    click(&mut h, &mut tl, point(2, y1));
    click(&mut h, &mut tl, point(2, y2)); // now solo the unlocked layer
    check(&h, [false, true], true);
    for (col, attr) in [(0, "shy"), (5, "motion_blur")] {
        for expected in [true, false] {
            click(
                &mut h,
                &mut tl,
                pos2(switches + 15.0 * (col as f32 + 0.5), y1),
            );
            assert_eq!(
                h.stage
                    .prim(&locked)
                    .unwrap()
                    .value(attr)
                    .and_then(Value::as_bool),
                Some(expected)
            );
        }
    }
}

#[test]
fn footage_frame_mix_retains_the_layer_setting_when_the_comp_switch_is_off() {
    let mut h = H::new();
    let comp = h.comp.clone();
    let file = tempfile::NamedTempFile::new().unwrap();
    let imported = h.run(
        "item.import",
        json!({"files": [{
            "path": file.path(),
            "attrs": {
                "kind": Value::Enum("sequence".into()),
                "frame_rate": Value::FrameRate(bae_model::FrameRate::FPS_30),
                "duration": Value::Time(Time::from_seconds(2.0))
            }
        }]}),
    );
    let asset = path(&imported["items"][0]);
    let layer = path(
        &h.run(
            "layer.create",
            json!({"comp": comp,"kind":"footage","source":asset}),
        )["path"],
    );
    let mut tl = h.bars();
    let y = |p: &PrimPath| {
        tl.last_bars
            .iter()
            .find(|(path, _)| path == p)
            .unwrap()
            .1
            .center()
            .y
    };
    let right = tl.last_axis.unwrap().left - 2.0;
    let x = right - 120.0 - 134.0 + 4.5 * 15.0;
    let row = pos2(x, y(&layer));
    let solid = pos2(x, y(&h.layer));
    let master = pos2(right - 14.0 - 52.0, 39.0);
    assert!(
        h.click(&mut tl, solid, Modifiers::NONE)
            .invocations
            .is_empty()
    );
    let r = h.click(&mut tl, row, Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1);
    h.apply(&r);
    assert_eq!(
        query::static_value(&h.stage.prim(&comp).unwrap(), "frame_blending"),
        Some(Value::Bool(true))
    );
    for expected in [false, true, false] {
        let r = h.click(&mut tl, master, Modifiers::NONE);
        h.apply(&r);
        assert_eq!(
            query::static_value(&h.stage.prim(&comp).unwrap(), "frame_blending"),
            Some(Value::Bool(expected))
        );
        assert_eq!(
            query::static_value(&h.stage.prim(&layer).unwrap(), "frame_blending"),
            Some(Value::Enum("frame_mix".into()))
        );
    }
    // Without an interpolator, a second click goes straight back to Off.
    let r = h.click(&mut tl, row, Modifiers::NONE);
    h.apply(&r);
    assert_eq!(
        query::static_value(&h.stage.prim(&layer).unwrap(), "frame_blending"),
        Some(Value::Enum("off".into()))
    );

    let mut app = bae_app::AppBuilder::new();
    app.add_module(bae_effects::EffectsModule);
    let effects = app
        .build()
        .get_registry::<dyn bae_render::Effect>()
        .unwrap();
    let pixel_motion = effects.contains(super::super::FRAME_INTERPOLATOR_ID);
    h.resources
        .insert(crate::gpu_preview::PreviewEffects(effects));
    let mut prefs = bae_prefs::Preferences::default();
    prefs.set("previews.auto_enable_frame_blending_motion_blur", false);
    h.resources
        .insert(std::sync::Arc::new(bae_prefs::PrefsStore::new(prefs)));
    let modes = ["frame_mix", "pixel_motion", "off"];
    for expected in modes
        .into_iter()
        .filter(|mode| *mode != "pixel_motion" || pixel_motion)
    {
        let r = h.click(&mut tl, row, Modifiers::NONE);
        assert_eq!(r.invocations.len(), 1, "one edit per click");
        h.apply(&r);
        assert_eq!(
            query::static_value(&h.stage.prim(&comp).unwrap(), "frame_blending"),
            Some(Value::Bool(false))
        );
        assert_eq!(
            query::static_value(&h.stage.prim(&layer).unwrap(), "frame_blending"),
            Some(Value::Enum(expected.into()))
        );
    }
}

/// AE: a camera or light row has Video, Solo, Lock and Shy only, a fixed axis
/// icon in the 3D column and no Mode or Track Matte; double-clicking it opens
/// Camera Settings or Light Settings instead of the Layer panel.
#[test]
fn camera_and_light_rows_show_only_their_switches_and_open_their_settings() {
    use crate::dialogs::Dialog;
    let mut h = H::new();
    let comp = h.comp.clone();
    let solid = h.layer.clone();
    let camera = path(&h.run("layer.create", json!({"comp": comp, "kind": "camera"}))["path"]);
    let light = path(&h.run("layer.create", json!({"comp": comp, "kind": "light"}))["path"]);
    let mut tl = h.bars();
    let y = |tl: &TimelinePanel, l: &PrimPath| bar_rect(tl, l).center().y;
    let switches = tl.last_axis.unwrap().left - 2.0 - 120.0 - 134.0;
    let cell = |col: usize, y| pos2(switches + 15.0 * (col as f32 + 0.5), y);
    let attr = |h: &H, a: &str| {
        h.stage
            .prim(&camera)
            .unwrap()
            .value(a)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let three_d = attr(&h, "three_d");
    // Collapse … 3D: no switch to click.
    for col in 1..8 {
        let at = cell(col, y(&tl, &camera));
        let r = h.click(&mut tl, at, Modifiers::NONE);
        assert!(
            r.invocations.is_empty(),
            "column {col}: {:?}",
            r.invocations
        );
        h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    }
    assert_eq!(attr(&h, "three_d"), three_d);
    assert!(!attr(&h, "motion_blur"));
    // Shy is there.
    let at = cell(0, y(&tl, &camera));
    let r = h.click(&mut tl, at, Modifiers::NONE);
    h.apply(&r);
    assert!(attr(&h, "shy"));
    // Modes: only the solid has Mode and Track Matte.
    show_modes(&mut h, &mut tl);
    let drawn: Vec<_> = tl.last_modes.iter().map(|(p, ..)| p.clone()).collect();
    assert_eq!(drawn, [solid]);
    show_modes(&mut h, &mut tl);
    // Double-click the camera's name, the light's bar: Camera Settings, then
    // Light Settings.
    for (layer, camera_dialog) in [(&camera, true), (&light, false)] {
        let at = if camera_dialog {
            pos2(switches - 60.0, y(&tl, layer))
        } else {
            bar_rect(&tl, layer).center()
        };
        // Not part of an earlier click run (egui counts clicks within 0.3 s).
        h.time += 1.0;
        h.click(&mut tl, at, Modifiers::NONE);
        let r = h.click(&mut tl, at, Modifiers::NONE);
        assert!(r.open.is_empty(), "{:?}", r.open);
        let dialogs = crate::dialogs::shared(&h.resources);
        let mut d = dialogs.lock();
        match d.open.take() {
            Some(Dialog::CameraSettings(_)) if camera_dialog => {}
            Some(Dialog::LightSettings(_)) if !camera_dialog => {}
            _ => panic!("no settings dialog for {layer}"),
        }
    }
}
