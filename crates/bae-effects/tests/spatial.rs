//! Registration, geometry and GPU pixel contracts for blur, perspective and stylize effects.
mod common;
use bae_model::Value;
use bae_render::{EffectParams, IRect};
use common::*;

#[test]
fn catalog_matches_ae_dump() {
    let mn = [
        "ADBE Box Blur2",
        "ADBE Motion Blur",
        "ADBE Radial Blur",
        "ADBE Sharpen",
        "ADBE Unsharp Mask2",
        "ADBE Glo2",
        "ADBE Drop Shadow",
        "ADBE Bevel Alpha",
        "ADBE Find Edges",
        "ADBE Mosaic",
        "ADBE Posterize",
        "ADBE Threshold2",
        "ADBE Threshold",
        "ADBE Emboss",
        "ADBE Color Emboss",
        "ADBE Cartoonify",
        "ADBE Roughen Edges",
        "ADBE Tile",
        "ADBE Strobe",
        "ADBE Radial Shadow",
    ];
    let mut count = 0;
    for fixture in [
        include_str!("../../../docs/reference/ae/effects/blur-and-sharpen.json"),
        include_str!("../../../docs/reference/ae/effects/stylize.json"),
        include_str!("../../../docs/reference/ae/effects/perspective.json"),
        include_str!("../../../docs/reference/ae/effects/obsolete-hidden.json"),
    ] {
        let data: serde_json::Value = serde_json::from_str(fixture).unwrap();
        for e in data["effects"].as_array().unwrap() {
            let name = e["matchName"].as_str().unwrap();
            if !mn.contains(&name) {
                continue;
            }
            count += 1;
            let b = builtin(name);
            assert_eq!(b.info.name_zh, e["displayName"]);
            let props = e["properties"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["matchName"] != "ADBE Effect Built In Params");
            assert_eq!(b.def().params.len(), props.clone().count());
            for (idx, pr) in props.enumerate() {
                let def = &b.def().params[idx];
                assert_eq!(def.match_name.as_deref(), pr["matchName"].as_str());
                assert_eq!(b.info.param_name_zh(def.id), pr["name"].as_str());
                let expected = &pr["defaultValue"];
                match def.default.as_ref() {
                    Some(Value::Float(v)) => {
                        assert!((v - expected.as_f64().unwrap()).abs() < 0.000001)
                    }
                    Some(Value::Int(v)) => assert_eq!(Some(*v), expected.as_i64()),
                    Some(Value::Bool(v)) => assert_eq!(Some(*v as i64), expected.as_i64()),
                    Some(Value::Vec2(v)) => assert_eq!(
                        serde_json::json!(v),
                        serde_json::json!(
                            expected
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|n| n.as_f64().unwrap())
                                .collect::<Vec<_>>()
                        )
                    ),
                    Some(Value::Color(v)) => {
                        for (a, b) in v.iter().zip(expected.as_array().unwrap()) {
                            assert_eq!(*a, b.as_f64().unwrap());
                        }
                    }
                    None => assert!(expected.is_null()),
                    other => panic!("unexpected {other:?}"),
                }
                if let bae_model::ParamKind::Slider {
                    valid_min,
                    valid_max,
                    ..
                } = def.kind
                {
                    assert_eq!(valid_min, pr["minValue"].as_f64().unwrap());
                    assert_eq!(valid_max, pr["maxValue"].as_f64().unwrap());
                }
                if let Some(options) = pr["dropdownOptions"].as_array() {
                    let bae_model::ParamKind::Popup { options: labels } = &def.kind else {
                        panic!("{name}: parameter {} is not a popup", def.id);
                    };
                    assert_eq!(labels.len(), options.len());
                    if let Some(zh) = bae_effects::effects::spatial::options_zh(name, def.id) {
                        assert_eq!(serde_json::json!(zh), serde_json::json!(options));
                    } else {
                        let zh: toml::Table = include_str!("../packages/strobe/locales/zh-CN.toml")
                            .parse()
                            .unwrap();
                        let table = zh["effect"]["strobe"]["choices"]
                            [if def.id == 6 { "mode" } else { "operation" }]
                        .as_table()
                        .unwrap();
                        assert_eq!(table.len(), options.len());
                        for (index, expected) in options.iter().enumerate() {
                            assert_eq!(table[&(index + 1).to_string()].as_str(), expected.as_str());
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, mn.len());
}

#[test]
fn shadow_bounds_and_shifted_alpha() {
    let rect = IRect::new(-2, 3, 18, 18);
    let params = vec![
        (3, Value::Float(90.)),
        (4, Value::Float(3.)),
        (6, Value::Bool(true)),
        (1, Value::Color([1., 0., 0., 1.])),
    ];
    let fx = builtin("ADBE Drop Shadow");
    let inst = instance(&fx.info.match_name, params.clone());
    let out = fx.effect().output_bounds(&EffectParams::new(&inst), rect);
    assert_eq!(out, IRect::new(1, 3, 21, 18));
    let Some(gpu) = context() else { return };
    let input = test_image(rect);
    let expected = Pixels::new(out, |x, y| {
        let a = input.get(x - 3, y)[3] * 0.5;
        [a, 0., 0., a]
    });
    let got = run(&gpu, F32, fx, params, &input, out, 1.);
    assert_close(&got, &expected, out, 1e-6, "shadow only");
}

#[test]
fn mosaic_averages_premultiplied_rgba_and_posterize_preserves_alpha() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-3, 2, 13, 14);
    let input = Pixels::new(rect, |x, _| {
        if x % 2 == 0 {
            [0.; 4]
        } else {
            [0.1, 0.2, 0.4, 0.5]
        }
    });
    for format in [F16, F32] {
        let mosaic = run(
            &gpu,
            format,
            builtin("ADBE Mosaic"),
            vec![(1, Value::Int(2)), (2, Value::Int(1))],
            &input,
            rect,
            1.,
        );
        assert_close(
            &mosaic,
            &Pixels::new(rect, |_, _| [0.05, 0.1, 0.2, 0.25]),
            rect,
            0.001,
            "mosaic average",
        );
        let poster = run(
            &gpu,
            format,
            builtin("ADBE Posterize"),
            vec![(1, Value::Int(3))],
            &input,
            rect,
            1.,
        );
        let expected = Pixels::new(rect, |x, _| {
            if x % 2 == 0 {
                [0.; 4]
            } else {
                [0., 0.25, 0.5, 0.5]
            }
        });
        assert_close(&poster, &expected, rect, 0.001, "posterized straight RGB");
    }
}

#[test]
fn all_spatial_filters_render_consistent_tiles() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-8, 1, 56, 52);
    let tile = IRect::new(17, 17, 29, 31);
    let input = test_image(rect);
    for (name, params) in [
        ("ADBE Motion Blur", vec![(1, Value::Float(36.))]),
        ("ADBE Radial Blur", vec![(1, Value::Float(20.))]),
        ("ADBE Sharpen", vec![(1, Value::Int(85))]),
        ("ADBE Find Edges", vec![]),
        ("ADBE Bevel Alpha", vec![]),
        (
            "ADBE Unsharp Mask2",
            vec![(1, Value::Float(90.)), (2, Value::Float(10.))],
        ),
        ("ADBE Drop Shadow", vec![(5, Value::Float(35.))]),
        ("ADBE Glo2", vec![(3, Value::Float(30.))]),
        ("ADBE Mosaic", vec![]),
    ] {
        let fx = builtin(name);
        let inst = instance(name, params.clone());
        let p = EffectParams::new(&inst);
        let read = fx.effect().input_roi(&p, tile).intersect(&rect);
        let crop = Pixels::new(read, |x, y| input.get(x, y));
        let full = run_depth(&gpu, F32, fx, params.clone(), &input, rect, 1., false);
        let part = run_depth(&gpu, F32, fx, params, &crop, tile, 1., false);
        assert_close(&part, &full, tile, 0.00001, name);
        assert!(
            full.data
                .iter()
                .zip(&input.data)
                .any(|(a, b)| (a[0] - b[0]).abs() > 0.005),
            "{name} did not affect red"
        );
        for px in &part.data {
            assert!(
                px[3] >= 0.
                    && px[3] <= 1.
                    && px[..3].iter().all(|v| *v >= 0. && *v <= px[3] + 1e-6),
                "{name}: {px:?}"
            );
        }
    }
}

#[test]
fn unsharp_retains_hdr_only_in_float_projects() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(32, 8);
    let input = Pixels::new(rect, |x, _| {
        if x >= 16 {
            [1.2, 0.6, 0.2, 0.8]
        } else {
            [0.1, 0.1, 0.1, 0.8]
        }
    });
    let fx = builtin("ADBE Unsharp Mask2");
    for format in [F16, F32] {
        let params = vec![(1, Value::Float(100.)), (2, Value::Float(2.))];
        let hdr = run_depth(&gpu, format, fx, params.clone(), &input, rect, 1., true);
        let bounded = run_depth(&gpu, format, fx, params, &input, rect, 1., false);
        assert!(hdr.get(16, 4)[0] > 1.2);
        assert!((bounded.get(16, 4)[0] - 0.8).abs() < 0.001);
    }
}

#[test]
fn threshold_and_emboss_preserve_premultiplied_alpha() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(8, 4);
    let input = Pixels::new(rect, |x, _| {
        let a = 0.5;
        let v = if x < 4 { 0.125 } else { 0.375 };
        [v, v, v, a]
    });
    for format in [F16, F32] {
        let threshold = run(
            &gpu,
            format,
            builtin("ADBE Threshold2"),
            vec![],
            &input,
            rect,
            1.,
        );
        let expected = Pixels::new(rect, |x, _| {
            let v = if x < 4 { 0. } else { 0.5 };
            [v, v, v, 0.5]
        });
        assert_close(&threshold, &expected, rect, 0.001, "threshold");
        for name in ["ADBE Emboss", "ADBE Color Emboss"] {
            let embossed = run(
                &gpu,
                format,
                builtin(name),
                vec![(1, Value::Float(90.))],
                &input,
                rect,
                1.,
            );
            assert!(embossed.data.iter().all(|pixel| pixel[3] == 0.5));
            assert_ne!(embossed.get(3, 1), embossed.get(1, 1));
            let blended = run(
                &gpu,
                format,
                builtin(name),
                vec![(4, Value::Float(100.))],
                &input,
                rect,
                1.,
            );
            assert_close(&blended, &input, rect, 0.001, "emboss blend");
        }
    }
}

#[test]
fn motion_tile_can_repeat_mirror_and_extend_around_layer_center() {
    let fx = builtin("ADBE Tile");
    let rect = IRect::new(-2, 1, 2, 3);
    let params = vec![(4, Value::Float(300.)), (6, Value::Bool(true))];
    let inst = instance("ADBE Tile", params.clone());
    let out = fx.effect().output_bounds(&EffectParams::new(&inst), rect);
    assert_eq!(out, IRect::new(-6, 1, 6, 3));
    let Some(gpu) = context() else { return };
    let input = Pixels::new(rect, |x, y| [(x + 2) as f32 / 4., y as f32 / 4., 0., 1.]);
    let expected = Pixels::new(out, |x, y| {
        let x = (x + 2).rem_euclid(8);
        input.get(x.min(7 - x) - 2, y)
    });
    let got = run(&gpu, F16, fx, params, &input, out, 1.);
    assert_close(&got, &expected, out, 0.001, "mirrored tiles");
}

#[test]
fn roughen_cycle_and_cartoon_use_the_same_pixels_in_cropped_roi() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-10, 0, 80, 70);
    let roi = IRect::new(13, 0, 42, 40);
    let input = Pixels::new(rect, |x, y| {
        if x < 14 || !(6..60).contains(&y) {
            [0.; 4]
        } else {
            [0.2, 0.7, 0.3, 1.]
        }
    });
    for (name, params) in [
        (
            "ADBE Roughen Edges",
            vec![(9, Value::Float(75.)), (12, Value::Bool(true))],
        ),
        ("ADBE Cartoonify", vec![]),
    ] {
        let fx = builtin(name);
        let inst = instance(name, params.clone());
        let read = fx
            .effect()
            .input_roi(&EffectParams::new(&inst), roi)
            .intersect(&rect);
        let cropped = Pixels::new(read, |x, y| input.get(x, y));
        let full = run(&gpu, F16, fx, params.clone(), &input, rect, 1.);
        let tile = run(&gpu, F16, fx, params.clone(), &cropped, roi, 1.);
        assert_close(&full, &tile, roi, 0.002, name);
        if name == "ADBE Roughen Edges" {
            let looped = run(
                &gpu,
                F16,
                fx,
                vec![(9, Value::Float(435.)), (12, Value::Bool(true))],
                &input,
                rect,
                1.,
            );
            assert_close(&full, &looped, rect, 0.002, "roughen one revolution");
            let later = run(
                &gpu,
                F16,
                fx,
                vec![(9, Value::Float(150.)), (12, Value::Bool(true))],
                &input,
                rect,
                1.,
            );
            assert!(max_diff(&full, &later, rect).0 > 0.05);
        }
    }
}

#[test]
fn strobe_respects_layer_time_and_scales_premultiplied_pixels() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(4, 4);
    let input = Pixels::new(rect, |_, _| [0.125, 0.25, 0.375, 0.5]);
    let image = input.upload(&gpu, F32);
    let fx = builtin("ADBE Strobe");
    let inst = instance(
        "ADBE Strobe",
        vec![(2, Value::Float(25.)), (6, Value::Int(2))],
    );
    let mut target = bae_effects::Standalone::new(gpu.clone(), F32);
    for (time, factor) in [(0., 0.25), (0.499, 0.25), (0.5, 1.), (1.1, 0.25)] {
        let params = EffectParams {
            time: bae_model::Time::from_seconds(time),
            ..EffectParams::new(&inst)
        };
        let output = fx.render(&mut target, &params, &image, rect).unwrap();
        target.finish().unwrap();
        let got = Pixels::download(&gpu, &output);
        let want = input.map(|c| c.map(|v| v * factor));
        assert_close(&got, &want, rect, 0.004, "strobe phase");
    }

    let random = instance(
        "ADBE Strobe",
        vec![
            (3, Value::Float(0.25)),
            (5, Value::Float(100.)),
            (6, Value::Int(2)),
        ],
    );
    let mut count = 0;
    for frame in 0..30 {
        let params = EffectParams {
            time: bae_model::FrameRate::FPS_30.frame_to_time(frame),
            ..EffectParams::new(&random)
        };
        let output = fx.render(&mut target, &params, &image, rect).unwrap();
        target.finish().unwrap();
        count += usize::from(Pixels::download(&gpu, &output).get(0, 0)[3] == 0.0);
    }
    assert!(
        (1..30).contains(&count),
        "phase jitter should vary between frames"
    );
}

#[test]
fn radial_shadow_resize_preserves_projection_outside_the_layer() {
    let rect = IRect::from_size(10, 10);
    let ray = IRect::new(16, 5, 17, 6);
    let fx = builtin("ADBE Radial Shadow");
    let mut params = vec![
        (3, Value::Vec2([1., 1.])),
        (4, Value::Float(50.)),
        (8, Value::Bool(true)),
    ];
    let inst = instance("ADBE Radial Shadow", params.clone());
    assert_eq!(
        fx.effect().output_bounds(&EffectParams::new(&inst), rect),
        rect
    );
    params.push((9, Value::Bool(true)));
    let inst = instance("ADBE Radial Shadow", params.clone());
    assert!(
        fx.effect()
            .output_bounds(&EffectParams::new(&inst), rect)
            .contains(&ray)
    );
    let Some(gpu) = context() else { return };
    let input = Pixels::new(rect, |_, _| [0.25, 0.5, 0.75, 1.]);
    let got = run(&gpu, F32, fx, params, &input, ray, 1.);
    let shadow = Pixels::new(ray, |_, _| [0., 0., 0., 127. / 255.]);
    assert_close(&got, &shadow, ray, 0.001, "projected outside source bounds");
}

#[test]
fn stylize_repeat_and_rgb_threshold() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(8, 6);
    let pattern = Pixels::new(rect, |x, _| [x as f32 / 8., 0.25, 0.75, 1.]);
    let tile = IRect::new(8, 0, 16, 6);
    let output = run(
        &gpu,
        F32,
        builtin("CC RepeTile"),
        vec![(1, Value::Float(8.))],
        &pattern,
        tile,
        1.,
    );
    assert_eq!(output.get(10, 3), pattern.get(2, 3));
    let rgb = run(
        &gpu,
        F32,
        builtin("CC Threshold RGB"),
        vec![],
        &pattern,
        rect,
        1.,
    );
    assert_eq!(rgb.get(2, 3), [0., 0., 1., 1.]);
    let original = run(
        &gpu,
        F32,
        builtin("CS Vignette"),
        vec![(1, Value::Float(0.))],
        &pattern,
        rect,
        1.,
    );
    let rect = IRect::from_size(64, 48);
    let gray = Pixels::new(rect, |_, _| [0.5, 0.5, 0.5, 1.]);
    let shaded = run(
        &gpu,
        F32,
        builtin("CS Vignette"),
        vec![(1, Value::Float(250.)), (3, Value::Vec2([40., 24.]))],
        &gray,
        rect,
        1.,
    );
    assert!(shaded.get(40, 24)[0] > shaded.get(0, 0)[0] + 0.02);
    assert_close(&original, &pattern, pattern.rect, 0.00001, "zero vignette");
}
