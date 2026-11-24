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
}
