use bae_model::{Time, Value};
use bae_render::{LayerContent, RenderComp, RenderLayer};
use std::sync::Arc;
#[path = "ae/categories/mod.rs"]
mod categories;
#[path = "ae/harness.rs"]
mod harness;

#[path = "common/time_scene.rs"]
mod time_scene;
use time_scene::*;

#[test]
fn posterize_freezes_earlier_effects_in_layer_time() {
    let Some(mut r) = renderer() else { return };
    let result = pixels(
        &mut r,
        |time| {
            let mut comp = RenderComp::new(path("/comps/main"), 16, 8);
            comp.time = time;
            let mut layer = solid(CLIP, 16, [1.; 4]);
            let local = time.as_seconds() / 2.;
            layer.time = Time::from_seconds(local);
            layer.time_scale = 2.;
            layer.effects = vec![
                effect(
                    "fill",
                    "ADBE Fill",
                    vec![(2, Value::Color([local, 0., 0., 1.]))],
                ),
                effect("time", "ADBE Posterize Time", vec![(1, Value::Float(2.))]),
            ];
            comp.layers.push(layer);
            comp
        },
        1.5,
    );
    assert_eq!(&result[..4], &[128, 0, 0, 255]);
}

#[test]
fn echo_and_time_displacement_sample_separate_frames() {
    let Some(mut r) = renderer() else { return };
    let color = |t: Time| {
        if t.as_seconds() < 0.99 {
            [1., 0., 0., 1.]
        } else if t.as_seconds() > 1.01 {
            [0., 0., 1., 1.]
        } else {
            [0., 1., 0., 1.]
        }
    };
    let echo = pixels(
        &mut r,
        |time| {
            let mut comp = RenderComp::new(path("/comps/main"), 16, 8);
            comp.time = time;
            let mut layer = solid(CLIP, 16, color(time));
            layer.time = time;
            layer
                .effects
                .push(effect("echo", "ADBE Echo", vec![(1, Value::Float(-0.25))]));
            comp.layers.push(layer);
            comp
        },
        1.,
    );
    assert_eq!(&echo[..4], &[255, 255, 0, 255]);

    let displacement = pixels(
        &mut r,
        |time| {
            let mut comp = RenderComp::new(path("/comps/main"), 16, 8);
            comp.time = time;
            let mut layer = solid(CLIP, 16, color(time));
            layer.time = time;
            layer.effects.push(effect(
                "displace",
                "ADBE Time Displacement",
                vec![
                    (1, Value::PrimRef(path("/comps/main/layers/map"))),
                    (2, Value::Float(0.25)),
                    (3, Value::Float(8.)),
                ],
            ));
            let mut map = RenderComp::new(path("/comps/control"), 16, 8);
            map.layers = vec![
                solid("/comps/control/layers/black", 8, [0., 0., 0., 1.]),
                solid("/comps/control/layers/white", 16, [1.; 4]),
            ];
            let mut map = RenderLayer::new(
                path("/comps/main/layers/map"),
                LayerContent::Comp(Arc::new(map)),
            );
            map.visible = false;
            comp.layers = vec![layer, map];
            comp
        },
        1.,
    );
    assert_eq!(&displacement[0..4], &[255, 0, 0, 255]);
    assert_eq!(&displacement[48..52], &[0, 0, 255, 255]);
}

#[test]
fn scene_detection_stops_at_a_cut_in_both_directions() {
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    for (name, smooth, detect) in [
        ("ADBE AutoLevels", 1, 2),
        ("ADBE AutoContrast", 1, 2),
        ("ADBE AutoColor", 1, 2),
        ("ADBE ShadowHighlight", 4, 5),
    ] {
        let text = serde_json::json!({"cases":[{
            "name":"scene", "effect":name, "input":"frame-mid", "time":2. / 30.,
            "frames":["frame-cut","frame-cut","frame-mid","frame-mid","frame-cut"],
            "params":{}
        }]});
        let mut case = harness::parse_cases(&text.to_string()).remove(0);
        let single = harness::render(&gpu, &case);
        case.params.push((smooth, Value::Float(0.16)));
        let crossed = harness::render(&gpu, &case);
        case.params.push((detect, Value::Bool(true)));
        let gated = harness::render(&gpu, &case);
        let delta = |image: &[[u8; 4]]| {
            image
                .iter()
                .zip(&single)
                .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)))
                .max()
                .unwrap()
        };
        assert!(delta(&crossed) > 2, "{name} includes the other scene");
        assert!(delta(&gated) <= 1, "{name} excludes the other scene");
    }
}

#[test]
fn wide_time_averages_frames_and_time_difference_compares_them() {
    let Some(mut r) = renderer() else { return };
    // Red before 1 s, blue from then on.
    let scene = |match_name: &'static str, params: Vec<(u32, Value)>| {
        move |time: Time| {
            let mut comp = RenderComp::new(path("/comps/main"), 16, 8);
            comp.time = time;
            let color = if time.as_seconds() < 0.99 {
                [1., 0., 0., 1.]
            } else {
                [0., 0., 1., 1.]
            };
            let mut layer = solid(CLIP, 16, color);
            layer.time = time;
            layer
                .effects
                .push(effect("time", match_name, params.clone()));
            comp.layers.push(layer);
            comp
        }
    };
    // One frame back (red), now and one frame on (blue), a third each.
    let wide = pixels(&mut r, scene("CC Wide Time", Vec::new()), 1.);
    let near = |a: &[u8], b: [u8; 4]| a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2);
    assert!(near(&wide[..4], [85, 0, 170, 255]), "{:?}", &wide[..4]);
    // Blue now against red one frame earlier: 50 % grey ± the difference.
    let difference = pixels(
        &mut r,
        scene("ADBE Difference", vec![(2, Value::Float(-1. / 30.))]),
        1.,
    );
    assert!(
        near(&difference[..4], [0, 128, 255, 255]),
        "{:?}",
        &difference[..4]
    );
}
