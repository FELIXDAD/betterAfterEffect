//! Legacy effect schemas come from the AE inventory; rendering uses the modern pipelines.
mod common;
use bae_effects::get;
use bae_model::{ParamKind, Value};
use bae_render::IRect;
use common::{F32, Pixels, assert_close, context, run};
use serde_json::Value as Json;

const COLOR: &[&str] = &[
    "ADBE Brightness & Contrast",
    "ADBE Alpha Levels2",
    "ADBE Alpha Levels3",
    "ADBE Easy Levels",
    "ADBE Pro Levels",
    "ADBE Exposure",
    "ADBE Photo Filter",
];

#[test]
fn color_aliases_match_the_legacy_reference() {
    check_schemas(COLOR);
}
#[test]
fn spatial_aliases_match_the_legacy_reference() {
    check_schemas(&[
        "ADBE Fast Blur",
        "ADBE Geometry",
        "ADBE Unsharp Mask",
        "ADBE Set Matte2",
        "ADBE PSL Solid Fill",
        "ADBE Spill Suppressor",
    ]);
}

#[test]
fn fast_blur_maps_direction_and_edges_to_box_blur() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(8, 6);
    let input = Pixels::new(rect, |x, y| {
        [u8::from(x == 3) as f32, 0., u8::from(y == 3) as f32, 1.]
    });
    for repeat in [false, true] {
        let legacy = vec![
            (1, Value::Float(2.)),
            (2, Value::Int(2)),
            (3, Value::Bool(repeat)),
        ];
        let modern = vec![
            (1, Value::Float(2.)),
            (2, Value::Int(3)),
            (3, Value::Int(2)),
            (4, Value::Bool(repeat)),
        ];
        let a = run(
            &gpu,
            F32,
            get("ADBE Fast Blur").unwrap(),
            legacy,
            &input,
            rect,
            1.,
        );
        let b = run(
            &gpu,
            F32,
            get("ADBE Box Blur2").unwrap(),
            modern,
            &input,
            rect,
            1.,
        );
        assert_close(&a, &b, rect, 0.00001, "Fast Blur");
    }
}

fn check_schemas(names: &[&str]) {
    let hidden: Json = serde_json::from_str(include_str!(
        "../../../docs/reference/ae/effects/obsolete-hidden.json"
    ))
    .unwrap();
    let visible: Json = serde_json::from_str(include_str!(
        "../../../docs/reference/ae/effects/obsolete.json"
    ))
    .unwrap();
    let mut errors = Vec::new();
    for &name in names {
        let fx = get(name).unwrap_or_else(|| panic!("missing {name}"));
        let data = [&hidden, &visible]
            .into_iter()
            .flat_map(|j| j["effects"].as_array().unwrap())
            .find(|e| e["matchName"] == name)
            .unwrap();
        assert!(fx.info.obsolete);
        assert_eq!(fx.info.name_zh, data["displayName"].as_str().unwrap());
        let props = data["properties"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["matchName"] != "ADBE Effect Built In Params");
        let params = fx.def().params.iter().filter(|p| p.match_name.is_some());
        assert_eq!(params.clone().count(), props.clone().count(), "{name}");
        for (param, prop) in params.zip(props) {
            let raw = prop["matchName"].as_str().unwrap();
            if param.match_name.as_deref() != Some(raw) {
                errors.push(format!("{raw}: disk order, got {:?}", param.match_name));
                continue;
            }
            if fx.info.param_name_zh(param.id) != prop["name"].as_str() {
                errors.push(format!(
                    "{raw}: label {:?} expected {}",
                    fx.info.param_name_zh(param.id),
                    prop["name"]
                ));
            }
            let value = &prop["defaultValue"];
            let ok = match &param.default {
                Some(Value::Float(x)) => Some(*x) == value.as_f64(),
                Some(Value::Int(x)) => Some(*x) == value.as_i64(),
                Some(Value::Bool(x)) => Some(i64::from(*x)) == value.as_i64(),
                Some(Value::Vec2(x)) => value
                    .as_array()
                    .is_some_and(|v| v.iter().map(Json::as_f64).eq(x.iter().copied().map(Some))),
                Some(Value::Color(x)) => value
                    .as_array()
                    .is_some_and(|v| v.iter().map(Json::as_f64).eq(x.iter().copied().map(Some))),
                None => value.is_null() || matches!(param.kind, ParamKind::Layer | ParamKind::Path),
                _ => false,
            };
            if !ok {
                errors.push(format!(
                    "{raw}: default {:?}, expected {value}",
                    param.default
                ));
            }
            if let ParamKind::Slider {
                valid_min,
                valid_max,
                ..
            } = param.kind
                && (Some(valid_min) != prop["minValue"].as_f64()
                    || Some(valid_max) != prop["maxValue"].as_f64())
            {
                errors.push(format!(
                    "{raw}: range {valid_min}..{valid_max}, expected {}..{}",
                    prop["minValue"], prop["maxValue"]
                ));
            }
            if let Some(opts) = prop["dropdownOptions"].as_array() {
                let actual = fx
                    .info
                    .param_options_zh
                    .iter()
                    .find(|(i, _)| *i == param.id)
                    .map(|(_, opts)| opts.as_slice());
                let english = match &param.kind {
                    ParamKind::Popup { options } => Some(options.as_slice()),
                    _ => None,
                };
                let actual = actual.or(english).unwrap_or_default();
                if !actual
                    .iter()
                    .map(String::as_str)
                    .eq(opts.iter().map(|s| s.as_str().unwrap()))
                {
                    errors.push(format!("{raw}: popup choices differ"));
                }
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

#[test]
fn alpha_levels_converts_8bit_controls_and_brightness_selects_legacy_mode() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(8, 4);
    let input = Pixels::new(rect, |x, _| {
        let alpha = (x + 1) as f32 / 8.;
        [0.4 * alpha, 0.2 * alpha, 0.1 * alpha, alpha]
    });
    for (old, old_params, new, new_params) in [
        (
            "ADBE Alpha Levels2",
            vec![(2, Value::Float(204.))],
            "ADBE Easy Levels2",
            vec![
                (1, Value::Int(5)),
                (4, Value::Float(0.8)),
                (8, Value::Int(1)),
                (9, Value::Int(1)),
            ],
        ),
        (
            "ADBE Brightness & Contrast",
            vec![(2, Value::Float(45.))],
            "ADBE Brightness & Contrast 2",
            vec![(2, Value::Float(45.)), (3, Value::Bool(true))],
        ),
    ] {
        let got = run(&gpu, F32, get(old).unwrap(), old_params, &input, rect, 1.);
        let expected = run(&gpu, F32, get(new).unwrap(), new_params, &input, rect, 1.);
        assert_close(&got, &expected, rect, 0.00001, old);
    }
}
