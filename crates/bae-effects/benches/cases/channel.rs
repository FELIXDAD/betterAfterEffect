use super::{Case, HD};
use bae_model::Value;

pub(super) fn register(out: &mut Vec<Case>) {
    for radius in [20.0, 100.0, 1000.0] {
        out.push(Case {
            name: format!("1080p minimax r{radius}"),
            match_name: "ADBE Minimax",
            params: vec![(2, Value::Float(radius)), (3, Value::Int(2))],
            size: HD,
        });
    }
}
