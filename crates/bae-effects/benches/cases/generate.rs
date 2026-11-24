use super::{Case, HD};

pub(super) fn register(out: &mut Vec<Case>) {
    out.push(Case {
        name: "1080p keying-matte cell pattern".into(),
        match_name: "ADBE Cell Pattern",
        params: vec![],
        size: HD,
    });
}
