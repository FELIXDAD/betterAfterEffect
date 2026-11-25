mod common;
use bae_model::Value;
use bae_render::IRect;
use common::*;

#[test]
fn stylize_repeat_and_rgb_threshold() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(8, 6);
    let pattern = Pixels::new(rect, |x, _| [x as f32 / 8., 0.25, 0.75, 1.]);
    let tile = IRect::new(8, 0, 16, 6);
    let output = run(
        &gpu,
        F32,
        builtin("CC RepeTile"),
        vec![(1, Value::Float(8.))],
        &pattern,
        tile,
        1.,
    );
    assert_eq!(output.get(10, 3), pattern.get(2, 3));
    let rgb = run(
        &gpu,
        F32,
        builtin("CC Threshold RGB"),
        vec![],
        &pattern,
        rect,
        1.,
    );
    assert_eq!(rgb.get(2, 3), [0., 0., 1., 1.]);
    let original = run(
        &gpu,
        F32,
        builtin("CS Vignette"),
        vec![(1, Value::Float(0.))],
        &pattern,
        rect,
        1.,
    );
    let rect = IRect::from_size(64, 48);
    let gray = Pixels::new(rect, |_, _| [0.5, 0.5, 0.5, 1.]);
    let shaded = run(
        &gpu,
        F32,
        builtin("CS Vignette"),
        vec![(1, Value::Float(250.)), (3, Value::Vec2([40., 24.]))],
        &gray,
        rect,
        1.,
    );
    assert!(shaded.get(40, 24)[0] > shaded.get(0, 0)[0] + 0.02);
    assert_close(&original, &pattern, pattern.rect, 0.00001, "zero vignette");
}

#[test]
fn input_region_matches_the_sampling_operation() {
    let layer = IRect::new(-8, 4, 120, 68);
    let tile = IRect::new(10, 20, 14, 24);
    for (name, values, expected) in [
        ("CC Kaleida", vec![], layer),
        ("CC RepeTile", vec![], tile),
        ("CC Glass", vec![(6, Value::Float(5.))], tile.inflate(5)),
    ] {
        let effect = instance(name, values);
        let params = bae_render::EffectParams {
            layer_bounds: layer,
            ..bae_render::EffectParams::new(&effect)
        };
        assert_eq!(
            builtin(name).effect().input_roi(&params, tile),
            expected,
            "{name}"
        );
    }
}
