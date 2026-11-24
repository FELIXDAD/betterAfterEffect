use super::{Case, HD};
use bae_model::Value;

pub(super) fn at_size(out: &mut Vec<Case>, size: [u32; 2], tag: &str) {
    for (name, params) in [
        ("ADBE Tint", vec![(3, Value::Float(80.0))]),
        ("ADBE Tritone", vec![(4, Value::Float(20.0))]),
        ("ADBE Easy Levels2", vec![(5, Value::Float(1.4))]),
        ("ADBE Pro Levels2", vec![(6, Value::Float(1.4))]),
        (
            "ADBE HUE SATURATION",
            vec![(4, Value::Float(30.0)), (5, Value::Float(20.0))],
        ),
        (
            "ADBE Brightness & Contrast 2",
            vec![(1, Value::Float(30.0)), (2, Value::Float(20.0))],
        ),
        ("ADBE Exposure2", vec![(3, Value::Float(0.5))]),
    ] {
        out.push(Case {
            name: format!("{tag} {name}"),
            match_name: name,
            params,
            size,
        });
    }
    if size == HD {
        for (name, effect) in [
            ("auto levels", "ADBE AutoLevels"),
            ("auto color", "ADBE AutoColor"),
            ("equalize", "ADBE Equalize"),
            ("colorama", "APC Colorama"),
        ] {
            out.push(Case {
                name: format!("1080p color-3 {name}"),
                match_name: effect,
                params: vec![],
                size: HD,
            });
        }
    }
}

pub(super) fn shadow_highlight(out: &mut Vec<Case>) {
    out.push(Case {
        name: "1080p shadow/highlight auto".into(),
        match_name: "ADBE ShadowHighlight",
        params: vec![],
        size: HD,
    });
}
