use super::{Case, HD};
use bae_model::Value;

pub(super) fn register(out: &mut Vec<Case>) {
    for choke in [10.5, -100.0] {
        out.push(Case {
            name: format!("1080p simple choker {choke}"),
            match_name: "ADBE Simple Choker",
            params: vec![(2, Value::Float(choke))],
            size: HD,
        });
    }
    out.push(Case {
        name: "1080p keying-matte matte choker".into(),
        match_name: "ADBE Matte Choker",
        params: vec![],
        size: HD,
    });
}
