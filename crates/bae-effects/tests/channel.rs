mod common;
use bae_model::Value;
use bae_render::IRect;
use common::*;

#[test]
fn channel_aliases_preserve_alpha_and_share_blend_operations() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(4, 3);
    let c = Pixels::new(r, |_, _| [0.125, 0.25, 0.375, 0.5]);
    let render = |name: &str, params| run(&gpu, F32, builtin(name), params, &c, r, 1.);
    let swap = render(
        "ADBE Set Channels",
        vec![(2, Value::Int(3)), (6, Value::Int(1))],
    );
    assert_eq!(swap.get(1, 1), [0.375, 0.25, 0.125, 0.5]);
    let gain = render(
        "ADBE Arithmetic",
        vec![(1, Value::Int(4)), (2, Value::Float(51.))],
    );
    assert!((gain.get(1, 1)[0] - 0.225).abs() < 0.00001);
    let opaque = render("ADBE Solid Composite", vec![]);
    assert_eq!(opaque.get(1, 1), [0.625, 0.75, 0.875, 1.]);
}

#[test]
fn input_region_matches_the_sampling_operation() {
    let layer = IRect::new(-8, 4, 120, 68);
    let tile = IRect::new(10, 20, 14, 24);
    for (name, values, expected) in [
        ("ADBE Set Channels", vec![], tile),
        ("ADBE Arithmetic", vec![], tile),
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
