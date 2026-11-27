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
        ("pixel motion blur", "ADBE OFMotionBlur", vec![]),
        (
            "timewarp pixel motion frame 10.5",
            "ADBE Timewarp",
            vec![(2, Value::Int(2)), (4, Value::Float(10.5))],
        ),
        ("cc wide time 1+1", "CC Wide Time", vec![]),
    ] {
        out.push(Case {
            name: format!("1080p temporal {name}"),
            match_name,
            params,
            size: HD,
        });
    }
}
