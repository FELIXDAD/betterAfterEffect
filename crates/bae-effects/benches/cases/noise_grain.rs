use super::{Case, HD};
use bae_model::Value;

pub(super) fn register(out: &mut Vec<Case>) {
    for (effect, params) in [
        ("ADBE Fractal Noise", vec![]),
        ("VISINF Grain Implant", vec![(21, Value::Int(3))]),
    ] {
        out.push(Case {
            name: format!("1080p {effect}"),
            match_name: effect,
            params,
            size: HD,
        });
    }
    for (name, effect, params) in [
        ("median r1", "ADBE PS Median", vec![(1, Value::Int(1))]),
        ("median r8", "ADBE PS Median", vec![(1, Value::Int(8))]),
        (
            "dust r12",
            "ADBE Dust & Scratches",
            vec![(1, Value::Int(12)), (2, Value::Float(0.1))],
        ),
    ] {
        out.push(Case {
            name: format!("1080p keying-matte {name}"),
            match_name: effect,
            params,
            size: HD,
        });
    }
}
