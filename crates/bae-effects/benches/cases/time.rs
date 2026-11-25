use super::{Case, HD};
use bae_model::Value;

pub(super) fn register(out: &mut Vec<Case>) {
    for (name, match_name, params) in [
        ("echo 8", "ADBE Echo", vec![(2, Value::Int(8))]),
        (
            "time displacement 0.25s",
            "ADBE Time Displacement",
            vec![(2, Value::Float(0.25)), (3, Value::Float(30.))],
        ),
        (
            "auto levels temporal 0.2s",
            "ADBE AutoLevels",
            vec![(1, Value::Float(0.2)), (2, Value::Bool(true))],
        ),
        (
            "shadow highlight temporal 0.2s",
            "ADBE ShadowHighlight",
            vec![(4, Value::Float(0.2)), (5, Value::Bool(true))],
        ),
    ] {
        out.push(Case {
            name: format!("1080p temporal {name}"),
            match_name,
            params,
            size: HD,
        });
    }
}
