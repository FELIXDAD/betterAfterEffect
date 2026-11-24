//! AE disk IDs and plugin controls, projective bounds, sampling and tile invariance.
mod common;
use bae_model::{ParamKind, Value};
use bae_render::{EffectParams, IRect};
use common::*;

#[test]
fn definitions_follow_the_ae26_dump() {
    let json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/reference/ae/effects/distort.json"
    ))
    .unwrap();
    for effect in [
        "Geometry2",
        "Turbulent Displace",
        "Wave Warp",
        "Bulge",
        "Twirl",
        "Polar Coordinates",
        "Corner Pin",
        "Offset",
        "Mirror",
        "Spherize",
        "Ripple",
        "Optics Compensation",
    ] {
        let fx = builtin(&format!("ADBE {effect}"));
        let name = &fx.def().match_name;
        let e = json["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["matchName"] == name.as_str())
            .unwrap();
        assert_eq!(e["displayName"], fx.info.name_zh);
        let list: Vec<_> = e["properties"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["matchName"] != "ADBE Effect Built In Params")
            .collect();
        assert_eq!(list.len(), fx.def().params.len());
        for (p, r) in fx.def().params.iter().zip(list) {
            assert_eq!(p.match_name.as_deref(), r["matchName"].as_str());
            assert_eq!(fx.info.param_name_zh(p.id), r["name"].as_str());
            let v = &r["defaultValue"];
            match &p.default {
                Some(Value::Vec2(a)) => {
                    assert_eq!(*a, [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()])
                }
                Some(Value::Float(x)) => assert_eq!(Some(*x), v.as_f64()),
                Some(Value::Int(x)) => assert_eq!(Some(*x), v.as_i64()),
                Some(Value::Bool(x)) => assert_eq!(i64::from(*x), v.as_i64().unwrap()),
                None => assert!(v.is_null()),
                other => panic!("{other:?}"),
            }
            match &p.kind {
                ParamKind::Slider {
                    valid_min,
                    valid_max,
                    ..
                } => {
                    assert_eq!(Some(*valid_min), r["minValue"].as_f64());
                    assert_eq!(Some(*valid_max), r["maxValue"].as_f64());
                }
                ParamKind::Popup { options } => {
                    let zh = bae_effects::effects::distort::options_zh(name, p.id).unwrap();
                    assert_eq!(zh.len(), options.len());
                    assert_eq!(serde_json::json!(zh), r["dropdownOptions"]);
                }
                _ => {}
            }
        }
    }
}

#[test]
fn translated_corners_and_transform_bounds() {
    let r = IRect::from_size(64, 32);
    for (fx, params) in [
        ("ADBE Geometry2", vec![(2, Value::Vec2([30., 20.]))]),
        (
            "ADBE Corner Pin",
            vec![
                (1, Value::Vec2([-2., 4.])),
                (2, Value::Vec2([62., 4.])),
                (3, Value::Vec2([-2., 36.])),
                (4, Value::Vec2([62., 36.])),
            ],
        ),
    ] {
        let effect = instance(fx, params);
        assert_eq!(
            builtin(fx)
                .effect()
                .output_bounds(&EffectParams::new(&effect), r),
            IRect::new(-2, 4, 62, 36)
        );
    }
}

#[test]
fn wrap_reflection_and_hdr_opacity() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(32, 16);
    let input = test_image(r);
    for format in [F16, F32] {
        for (name, params) in [
            ("ADBE Offset", vec![(1, Value::Vec2([20., 10.]))]),
            ("ADBE Mirror", vec![(1, Value::Vec2([16., 8.]))]),
            ("ADBE Geometry2", vec![(8, Value::Float(50.))]),
            ("ADBE Corner Pin", vec![]),
        ] {
            let result = run(&gpu, format, builtin(name), params, &input, r, 1.);
            let want = Pixels::new(r, |x, y| match name {
                "ADBE Offset" => input.get((x - 4).rem_euclid(32), (y - 2).rem_euclid(16)),
                "ADBE Mirror" => input.get(x.min(32 - x), y),
                "ADBE Geometry2" => input.get(x, y).map(|c| 0.5 * c),
                _ => input.get(x, y),
            });
            assert_close(&result, &want, r, 0.001, name);
        }
    }
}

#[test]
fn warps_sample_the_same_source_for_tiles() {
    let Some(gpu) = context() else { return };
    let r = IRect::new(-5, 3, 59, 35);
    let tile = IRect::new(17, 8, 29, 21);
    let input = test_image(r);
    for fx in bae_effects::builtins()
        .iter()
        .filter(|f| f.def().category == "Distort")
    {
        let params = match fx.info.name.as_str() {
            "Twirl" | "Optics Compensation" | "Ripple" | "Spherize" => vec![(1, Value::Float(32.))],
            "Polar Coordinates" => vec![(1, Value::Float(1.))],
            _ => vec![],
        };
        let full = run(&gpu, F32, fx, params.clone(), &input, r, 0.5);
        let part = run(&gpu, F32, fx, params, &input, tile, 0.5);
        assert_close(&part, &full, tile, 0.0001, &fx.info.name);
    }
}

#[test]
fn turbulence_locks_motion_to_the_selected_axis_and_cycles() {
    let Some(gpu) = context() else { return };
    let fx = builtin("ADBE Turbulent Displace");
    let r = IRect::from_size(96, 64);
    let input = Pixels::new(r, |x, y| [x as f32 / 96., y as f32 / 64., 0., 1.]);
    let render = |mode, turn, seed| {
        run(
            &gpu,
            F32,
            fx,
            vec![
                (1, Value::Int(mode)),
                (2, Value::Float(16.)),
                (3, Value::Float(20.)),
                (6, Value::Float(turn)),
                (8, Value::Bool(true)),
                (9, Value::Int(2)),
                (10, Value::Int(seed)),
                (12, Value::Int(1)),
            ],
            &input,
            r,
            1.,
        )
    };
    for (mode, unchanged) in [(9, 0), (10, 1)] {
        let a = render(mode, 0., 0);
        let middle = render(mode, 360., 0);
        let looped = render(mode, 720., 0);
        let seeded = render(mode, 0., 7);
        let inner = IRect::new(16, 16, 80, 48);
        for y in inner.y0..inner.y1 {
            for x in inner.x0..inner.x1 {
                assert!((a.get(x, y)[unchanged] - input.get(x, y)[unchanged]).abs() < 1e-5);
            }
        }
        assert!(max_diff(&a, &middle, inner).0 > 0.01);
        assert!(max_diff(&a, &seeded, inner).0 > 0.01);
        assert_close(&a, &looped, inner, 1e-5, "cycle evolution");
    }
}

#[test]
fn collapsed_transform_has_zero_coverage_and_integer_output_is_premultiplied() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(9, 7);
    let fx = builtin("ADBE Geometry2");
    let input = Pixels::new(r, |_, _| [1.5, 0.25, 0.1, 0.5]);
    let zero = run(&gpu, F32, fx, vec![(3, Value::Float(0.))], &input, r, 1.);
    assert_close(&zero, &Pixels::new(r, |_, _| [0.; 4]), r, 0., "zero scale");
    for fmt in [F32, F16] {
        let bounded = run_depth(&gpu, fmt, fx, vec![], &input, r, 1., false);
        let hdr = run_depth(&gpu, fmt, fx, vec![], &input, r, 1., true);
        assert_eq!(bounded.get(4, 3)[0], 0.5);
        assert_eq!(hdr.get(4, 3)[0], 1.5);
    }
}
