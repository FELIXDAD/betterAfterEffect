use super::{Case, HD};
use bae_model::Value;

pub(super) fn glow(out: &mut Vec<Case>) {
    out.push(Case {
        name: "1080p ADBE Glo2".into(),
        match_name: "ADBE Glo2",
        params: vec![(3, Value::Float(100.0))],
        size: HD,
    });
}

pub(super) fn mosaic(out: &mut Vec<Case>) {
    out.push(Case {
        name: "1080p ADBE Mosaic".into(),
        match_name: "ADBE Mosaic",
        params: vec![],
        size: HD,
    });
}
