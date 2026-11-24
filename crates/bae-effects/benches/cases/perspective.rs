use super::{Case, HD};
use bae_model::Value;

pub(super) fn register(out: &mut Vec<Case>) {
    out.push(Case {
        name: "1080p ADBE Drop Shadow".into(),
        match_name: "ADBE Drop Shadow",
        params: vec![(5, Value::Float(100.0))],
        size: HD,
    });
}
