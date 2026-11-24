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

/// Mosaic and the other neighborhood-sampling style effects.
pub(super) fn mosaic(out: &mut Vec<Case>) {
    out.push(Case {
        name: "1080p ADBE Mosaic".into(),
        match_name: "ADBE Mosaic",
        params: vec![],
        size: HD,
    });
    for (name, match_name, params) in [
        ("cartoon default", "ADBE Cartoonify", vec![]),
        (
            "cartoon radius 40",
            "ADBE Cartoonify",
            vec![(2, Value::Float(40.))],
        ),
        ("roughen default", "ADBE Roughen Edges", vec![]),
        (
            "roughen border 500",
            "ADBE Roughen Edges",
            vec![(2, Value::Float(500.))],
        ),
        (
            "motion tile 40%",
            "ADBE Tile",
            vec![(2, Value::Float(40.)), (3, Value::Float(40.))],
        ),
    ] {
        out.push(Case {
            name: format!("1080p stylize {name}"),
            match_name,
            params,
            size: HD,
        });
    }
}
