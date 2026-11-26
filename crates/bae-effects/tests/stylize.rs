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

#[test]
fn burn_and_progressive_scans_reach_their_endpoints() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(32, 16);
    let image = Pixels::new(r, |x, y| [x as f32 / 32., y as f32 / 16., 0.25, 1.]);
    let clear = Pixels::new(r, |_, _| [0.; 4]);
    for (name, start, end) in [
        ("CC Burn Film", &image, &clear),
        ("CS BlockLoad", &clear, &image),
    ] {
        for (t, want) in [(0., start), (100., end)] {
            let result = run(
                &gpu,
                F32,
                builtin(name),
                vec![(1, Value::Float(t))],
                &image,
                r,
                1.,
            );
            assert_close(&result, want, r, 0.0001, name);
        }
    }
}

#[test]
fn honeycomb_relief_and_flow_use_their_sample_coordinates() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(48, 32);
    let input = Pixels::new(r, |x, y| [x as f32 / 48., y as f32 / 32., 0.3, 1.]);
    for (name, values) in [
        ("CS HexTile", vec![(2, Value::Float(6.))]),
        ("CC Plastic", vec![(5, Value::Float(85.))]),
        ("CC Mr. Smoothie", vec![(6, Value::Float(180.))]),
    ] {
        let fx = builtin(name);
        let before = run(&gpu, F32, fx, vec![], &input, r, 1.);
        let after = run(&gpu, F32, fx, values, &input, r, 1.);
        let (d, _, _) = max_diff(&before, &after, r);
        assert!(d.is_finite() && d > 0.001, "{name} control produced {d}");
        assert!(after.get(20, 16)[3] > 0.9, "{name} lost the source alpha");
    }
}

#[test]
fn plastic_requests_scene_data_only_for_comp_lights() {
    let fx = builtin("CC Plastic");
    for (using, expected) in [(1, false), (2, true)] {
        let instance = instance("CC Plastic", vec![(26, Value::Int(using))]);
        assert_eq!(
            fx.effect()
                .uses_scene(&bae_render::EffectParams::new(&instance)),
            expected
        );
    }
}
