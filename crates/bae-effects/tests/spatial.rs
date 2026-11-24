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
    ];
    let mut count = 0;
    for fixture in [
        include_str!("../../../docs/reference/ae/effects/blur-and-sharpen.json"),
        include_str!("../../../docs/reference/ae/effects/stylize.json"),
        include_str!("../../../docs/reference/ae/effects/perspective.json"),
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
                    assert_eq!(
                        serde_json::json!(
                            bae_effects::effects::spatial::options_zh(name, def.id).unwrap()
                        ),
                        serde_json::json!(options)
                    );
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
