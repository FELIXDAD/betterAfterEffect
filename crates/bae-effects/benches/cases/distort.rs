use super::{Case, HD};
use bae_model::Value;

pub(super) fn register(out: &mut Vec<Case>) {
    for (effect, params) in [
        ("ADBE Turbulent Displace", vec![(5, Value::Float(4.))]),
        ("ADBE Wave Warp", vec![(8, Value::Int(3))]),
        (
            "ADBE Twirl",
            vec![(1, Value::Float(100.)), (2, Value::Float(80.))],
        ),
        (
            "ADBE Bulge",
            vec![
                (1, Value::Float(700.)),
                (2, Value::Float(400.)),
                (4, Value::Float(2.)),
                (6, Value::Int(2)),
            ],
        ),
        ("ADBE Ripple", vec![(1, Value::Float(80.))]),
        ("ADBE Optics Compensation", vec![(1, Value::Float(80.))]),
        (
            "ADBE Geometry2",
            vec![(7, Value::Float(20.)), (12, Value::Int(2))],
        ),
    ] {
        out.push(Case {
            name: format!("1080p distort {effect}"),
            match_name: effect,
            params,
            size: HD,
        });
    }
}
