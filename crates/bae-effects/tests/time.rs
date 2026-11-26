use bae_model::{Time, Value};
use bae_render::{
    LayerContent, OutputAlpha, RenderComp, RenderEffect, RenderLayer, RenderRequest, Renderer,
};
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

/// A white 8 × 8 square on black moving 8 px right per frame (x = 4 at
/// frame 0), inside a pre-comp layer carrying `effect`.
fn moving_square(effect: RenderEffect) -> impl Fn(Time) -> RenderComp {
    move |time: Time| {
        let frame = (time.as_seconds() * 30.).round();
        let mut inner = RenderComp::new(path("/comps/inner"), 48, 8);
        inner.time = time;
        let mut square = solid("/comps/inner/layers/square", 8, [1.; 4]);
        square.transform = glam::DAffine2::from_translation(glam::DVec2::new(4. + 8. * frame, 0.));
        inner.layers = vec![
            square,
            solid("/comps/inner/layers/bg", 48, [0., 0., 0., 1.]),
        ];
        let mut comp = RenderComp::new(path("/comps/main"), 48, 8);
        comp.time = time;
        let mut layer = RenderLayer::new(path(CLIP), LayerContent::Comp(Arc::new(inner)));
        layer.time = time;
        layer.effects.push(effect.clone());
        comp.layers.push(layer);
        comp
    }
}

#[test]
fn cc_time_effects_respect_nested_motion_blur_on_the_current_sample() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    for (name, popup, settings) in [
        (
            "CC Wide Time",
            3,
            vec![(1, Value::Int(0)), (2, Value::Int(0))],
        ),
        ("CC Force Motion Blur", 4, vec![(3, Value::Float(0.))]),
    ] {
        let mut draw = |mode| {
            let mut params = settings.clone();
            params.push((popup, Value::Int(mode)));
            let scene = moving_square(effect("time", name, params));
            pixels(
                &mut renderer,
                |time| {
                    let mut comp = scene(time);
                    let LayerContent::Comp(inner) = &mut comp.layers[0].content else {
                        unreachable!()
                    };
                    let square = &mut Arc::make_mut(inner).layers[0];
                    square.motion_samples = [0., 8.]
                        .map(|offset| bae_render::LayerSample {
                            transform: square.transform
                                * glam::DAffine2::from_translation(glam::DVec2::new(offset, 0.)),
                            matrix_3d: None,
                        })
                        .into();
                    comp
                },
                0.,
            )
        };
        let off = draw(2);
        let on = draw(1);
        let edge = (4 * 48 + 13) * 4;
        assert_eq!(off[edge], 0, "{name}");
        assert!(on[edge] > 20, "{name}");
    }
}

#[test]
fn force_motion_blur_samples_the_backdrop_on_an_adjustment_layer() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let image = pixels(
        &mut renderer,
        |time| {
            let mut comp = RenderComp::new(path("/comps/main"), 16, 8);
            comp.time = time;
            let mut adjustment = RenderLayer::new(path(CLIP), LayerContent::None);
            adjustment.adjustment = true;
            adjustment.time = time;
            adjustment.effects.push(effect(
                "blur",
                "CC Force Motion Blur",
                vec![
                    (1, Value::Int(2)),
                    (3, Value::Float(360.)),
                    (5, Value::Float(-180.)),
                ],
            ));
            let color = if time.as_seconds() < 1. {
                [1., 0., 0., 1.]
            } else {
                [0., 0., 1., 1.]
            };
            comp.layers = vec![adjustment, solid("/comps/main/layers/bg", 16, color)];
            comp
        },
        1.,
    );
    assert!(
        image[0].abs_diff(128) <= 1 && image[2].abs_diff(128) <= 1,
        "{:?}",
        &image[..4]
    );
}

#[test]
fn pixel_motion_tweens_and_blurs_along_the_optical_flow() {
    let Some(mut r) = renderer() else { return };
    let red = |image: &[u8], x: usize| image[(4 * 48 + x) * 4];
    // Timewarp at 50 % shows source frame 0.5 at comp frame 1: Pixel Motion
    // puts the square half way ([8, 16)); Frame Mix leaves two half images.
    let tween = |method: i64| {
        effect(
            "warp",
            "ADBE Timewarp",
            vec![(1, Value::Int(method)), (6, Value::Float(100.))],
        )
    };
    let motion = pixels(&mut r, moving_square(tween(3)), 1. / 30.);
    assert!(red(&motion, 14) > 200 && red(&motion, 6) < 55, "{motion:?}");
    let mix = pixels(&mut r, moving_square(tween(2)), 1. / 30.);
    assert!(red(&mix, 14).abs_diff(128) < 10, "{mix:?}");
    // Pixel Motion Blur at 180°: ±2 px of the 8 px motion, smeared outwards
    // past the square's edges; its middle and the far background stay.
    let blur = pixels(
        &mut r,
        moving_square(effect(
            "blur",
            "ADBE OFMotionBlur",
            vec![(4, Value::Float(100.))],
        )),
        1. / 30.,
    );
    assert!((20..235).contains(&red(&blur, 11)), "{blur:?}");
    assert!(red(&blur, 16) > 240 && red(&blur, 2) < 15, "{blur:?}");
}

#[test]
fn footage_pixel_motion_uses_the_app_interpolator() {
    use bae_model::FrameRate;
    use bae_render::fixture::SceneBuilder;
    use bae_render::{
        AnimEvaluator, CompEvaluator, ExportFormat, ExportJob, ExportSettings, RenderPurpose,
        export_frames,
    };

    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    let mut app = bae_app::AppBuilder::new();
    app.add_module(bae_effects::EffectsModule);
    let app = app.build();
    let mut renderer = Renderer::new(gpu);
    renderer.sync_effects(&app.get_registry::<dyn bae_render::Effect>().unwrap());
    let dir = tempfile::tempdir().unwrap();
    for i in 0..2 {
        let left = 4 + 8 * i;
        let data = (0..48 * 8)
            .flat_map(|p| {
                let v = if (left..left + 8).contains(&(p % 48)) {
                    255
                } else {
                    0
                };
                [v, v, v, 255]
            })
            .collect();
        bae_render::png_out::write_rgba8(
            &dir.path().join(format!("motion_{i}.png")),
            &bae_gpu::transfer::ImageData {
                width: 48,
                height: 8,
                format: bae_gpu::wgpu::TextureFormat::Rgba8Unorm,
                bytes_per_row: 48 * 4,
                data,
            },
        )
        .unwrap();
    }
    let rate = FrameRate::FPS_30;
    let mut scene = SceneBuilder::new();
    let comp = scene
        .comp("main", 48, 8, rate, rate.frame_to_time(6))
        .unwrap();
    let asset = scene
        .footage_asset(
            "clip",
            &dir.path().join("motion_0.png"),
            None,
            Some([48, 8]),
        )
        .unwrap();
    scene
        .set(&asset, "kind", Value::Enum("sequence".into()))
        .unwrap();
    scene
        .set(&asset, "frame_rate", Value::FrameRate(rate))
        .unwrap();
    let layer = scene.layer(&comp, "clip", "footage", Some(&asset)).unwrap();
    scene.set(&layer, "stretch", Value::Float(200.0)).unwrap();
    scene
        .set(&comp, "frame_blending", Value::Bool(true))
        .unwrap();
    let mut sample = |mode: &str, frame| {
        scene
            .set(&layer, "frame_blending", Value::Enum(mode.into()))
            .unwrap();
        let request = RenderRequest {
            purpose: RenderPurpose::Preview,
            ..Default::default()
        };
        let evaluated = AnimEvaluator::new(None)
            .with_purpose(request.purpose)
            .evaluate(scene.stage(), &comp, rate.frame_to_time(frame))
            .unwrap();
        let output = renderer.render_frame(&evaluated, &request).unwrap();
        assert!(output.warnings.is_empty(), "{:?}", output.warnings);
        renderer
            .read_rgba8(&output, OutputAlpha::Straight)
            .unwrap()
            .data
    };
    let red = |image: &[u8], x: usize| image[(4 * 48 + x) * 4];
    let motion = sample("pixel_motion", 1);
    let mix = sample("frame_mix", 1);
    assert!(
        red(&motion, 14) > 200 && red(&motion, 6) < 55,
        "moving square: {} / {}",
        red(&motion, 14),
        red(&motion, 6)
    );
    assert!(red(&mix, 14).abs_diff(128) <= 1 && red(&mix, 6).abs_diff(128) <= 1);
    assert_eq!(red(&sample("pixel_motion", 0), 6), 255);
    assert_eq!(red(&sample("pixel_motion", 3), 6), 0);
    assert_eq!(sample("pixel_motion", 1), motion);
    let file = dir.path().join("export.png");
    let job = ExportJob {
        stage: scene.stage(),
        comp: &comp,
        times: &[(Some(1), rate.frame_to_time(1))],
        rate,
        request: &RenderRequest {
            purpose: RenderPurpose::Final,
            ..Default::default()
        },
        settings: &ExportSettings::new(ExportFormat::Png, &file),
        encoders: None,
    };
    let out = export_frames(
        &mut renderer,
        &mut AnimEvaluator::new(None),
        &job,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(bae_render::png_out::read_rgba8(&file).unwrap().2, motion);
}

#[test]
fn time_blend_uses_the_previous_surface_and_fx_pastes_the_downstream_copy() {
    let Some(mut r) = renderer() else { return };
    for paired in [false, true] {
        let scene = |time: Time| {
            let mut comp = RenderComp::new(path("/comps/main"), 16, 8);
            comp.time = time;
            let mut layer = solid(
                CLIP,
                16,
                if time == Time::ZERO {
                    [1., 0., 0., 1.]
                } else {
                    [0., 0., 1., 1.]
                },
            );
            layer.time = time;
            layer.effects = if paired {
                let mut tint = effect(
                    "paint",
                    "ADBE Fill",
                    vec![(2, Value::Color([0., 1., 0., 1.]))],
                );
                tint.enabled = time == Time::ZERO;
                vec![
                    effect(
                        "paste",
                        "CC Time Blend FX",
                        vec![
                            (1, Value::Int(2)),
                            (3, Value::Float(0.5)),
                            (4, Value::Int(2)),
                        ],
                    ),
                    tint,
                    effect("copy", "CC Time Blend FX", vec![]),
                ]
            } else {
                vec![effect(
                    "blend",
                    "CC Time Blend",
                    vec![(2, Value::Float(0.5)), (3, Value::Int(2))],
                )]
            };
            comp.layers.push(layer);
            comp
        };
        let _first = pixels(&mut r, scene, 0.);
        let second = pixels(&mut r, scene, 1. / 30.);
        let expected = if paired {
            [0, 128, 128, 255]
        } else {
            [128, 0, 128, 255]
        };
        for (channel, value) in second[..4].iter().zip(expected) {
            assert!(
                channel.abs_diff(value) <= 1,
                "second frame {:?}",
                &second[..4]
            );
        }

        assert_eq!(&pixels(&mut r, scene, 4. / 30.)[..4], &[0, 0, 255, 255]);
    }
}
