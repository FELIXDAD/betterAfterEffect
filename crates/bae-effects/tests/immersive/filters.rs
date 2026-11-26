use crate::common::*;
use bae_model::Value;
use bae_render::IRect;

#[test]
fn blur_and_glow_cross_the_seam_without_leaking_to_the_other_eye() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(64, 64);
    let input = Pixels::new(rect, |x, y| {
        let v = if x < 4 && y < 32 { 1.0 } else { 0.0 };
        [v, v, v, 1.0]
    });
    for (name, params) in [
        (
            "Mettle SkyBox Blur",
            vec![(3, Value::Int(2)), (2, Value::Float(7.))],
        ),
        (
            "Mettle SkyBox Glow",
            vec![(8, Value::Int(2)), (3, Value::Float(7.))],
        ),
    ] {
        let out = run(&gpu, F32, builtin(name), params, &input, rect, 1.);
        assert!(out.get(63, 16)[0] > 0.05, "{name}");
        assert_eq!(out.get(63, 48)[0], 0.0, "{name}");
    }
}

#[test]
fn denoise_removes_impulses_and_sharpen_increases_edge_contrast() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(64, 32);
    let input = Pixels::new(rect, |x, y| {
        let v = if (x, y) == (16, 16) {
            1.0
        } else if x < 32 {
            0.25
        } else {
            0.75
        };
        [v, v, v, 1.0]
    });
    let clean = run(
        &gpu,
        F32,
        builtin("Mettle SkyBox Denoise"),
        vec![(2, Value::Int(2)), (3, Value::Float(1.))],
        &input,
        rect,
        1.,
    );
    assert!((clean.get(16, 16)[0] - 0.25).abs() < 0.01);
    assert!((clean.get(31, 16)[0] - 0.25).abs() < 0.01);
    let sharp = run(
        &gpu,
        F32,
        builtin("Mettle SkyBox Sharpen"),
        vec![(2, Value::Float(50.))],
        &input,
        rect,
        1.,
    );
    assert!(sharp.get(31, 16)[0] < 0.24 && sharp.get(32, 16)[0] > 0.76);
}
