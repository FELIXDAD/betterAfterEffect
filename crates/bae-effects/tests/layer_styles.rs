mod common;

use bae_effects::get;
use bae_render::IRect;
use common::{F32, Pixels, context, run};

#[test]
fn photoshop_styles_follow_the_alpha_edge() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(32, 32);
    let input = Pixels::new(rect, |x, y| {
        if (8..24).contains(&x) && (8..24).contains(&y) {
            [0.35, 0.35, 0.35, 1.]
        } else {
            [0.; 4]
        }
    });
    for name in [
        "Drop Shadow",
        "Inner Shadow",
        "Outer Glow",
        "Inner Glow",
        "Bevel Emboss",
    ] {
        let fx = get(&format!("ADBE PSL {name}")).unwrap();
        let out = run(&gpu, F32, fx, vec![], &input, rect, 1.);
        assert!(out.data.iter().flatten().all(|v| v.is_finite()));
        if matches!(name, "Drop Shadow" | "Outer Glow") {
            assert!(out.get(24, 18)[3] > 0., "{name}");
            assert_eq!(out.get(15, 15), input.get(15, 15));
        } else {
            assert_eq!(out.get(25, 18)[3], 0., "{name}");
            assert!(
                out.data
                    .iter()
                    .zip(&input.data)
                    .any(|(a, b)| (a[0] - b[0]).abs() > 0.02),
                "{name}"
            );
        }
    }
}
