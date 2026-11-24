use super::{Case, HD};
use bae_model::Value;

pub(super) fn at_size(out: &mut Vec<Case>, size: [u32; 2], tag: &str) {
    for b in [10.0, 25.0, 100.0, 300.0] {
        out.push(Case {
            name: format!("{tag} gaussian blurriness {b}"),
            match_name: "ADBE Gaussian Blur 2",
            params: vec![(1, Value::Float(b)), (3, Value::Bool(false))],
            size,
        });
    }
    for (r, n) in [(5.0, 1), (20.0, 3), (100.0, 3)] {
        out.push(Case {
            name: format!("{tag} fast box blur r{r} x{n}"),
            match_name: "ADBE Box Blur2",
            params: vec![(1, Value::Float(r)), (2, Value::Int(n))],
            size,
        });
    }
}

pub(super) fn spatial(out: &mut Vec<Case>) {
    for (effect, params) in [
        ("ADBE Motion Blur", vec![(2, Value::Float(100.0))]),
        ("ADBE Radial Blur", vec![(1, Value::Float(25.0))]),
        (
            "ADBE Unsharp Mask2",
            vec![(1, Value::Float(100.0)), (2, Value::Float(50.0))],
        ),
    ] {
        out.push(Case {
            name: format!("1080p {effect}"),
            match_name: effect,
            params,
            size: HD,
        });
    }
}
