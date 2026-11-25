mod common;
use bae_model::Value;
use bae_render::IRect;
use common::*;

#[test]
fn projection_silhouettes_and_spotlight() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(64, 48);
    let input = Pixels::new(rect, |_, _| [0.8, 0.4, 0.2, 1.]);
    for (name, params) in [
        ("CC Sphere", vec![(6, Value::Float(12.))]),
        ("CC Cylinder", vec![]),
    ] {
        let out = run(&gpu, F32, builtin(name), params, &input, rect, 1.);
        assert!(out.get(32, 24)[3] > 0.99, "{name}");
        assert_eq!(out.get(0, 0)[3], 0.);
    }
    let light = run(
        &gpu,
        F32,
        builtin("CC Spotlight"),
        vec![(8, Value::Int(1))],
        &input,
        rect,
        1.,
    );
    assert!(light.get(32, 24)[3] > light.get(0, 47)[3]);
}

#[test]
fn input_region_matches_the_sampling_operation() {
    let layer = IRect::new(-8, 4, 120, 68);
    let tile = IRect::new(10, 20, 14, 24);
    for (name, values, expected) in [
        ("CC Sphere", vec![], layer),
        ("CC Cylinder", vec![], layer),
        ("CC Spotlight", vec![], tile),
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
