mod common;
use bae_model::Value;
use bae_render::IRect;
use common::*;

/// CC Environment over a 64 x 32 horizontal ramp used as its own environment.
fn environment(
    params: Vec<(u32, Value)>,
    camera: Option<bae_render::camera::Camera>,
) -> Option<Pixels> {
    use bae_effects::Standalone;
    use bae_render::{EffectParams, effect::EffectScene};
    let gpu = context()?;
    let r = IRect::from_size(64, 32);
    let image = Pixels::new(r, |x, _| [x as f32 / 64., 0., 0., 1.]).upload(&gpu, F32);
    let scene = camera.map(|camera| EffectScene {
        projection: glam::DMat3::IDENTITY,
        light: None,
        camera: Some(camera),
        size: [64, 32],
        placement: glam::DAffine2::IDENTITY,
        layer_light: None,
    });
    let inst = instance("CC Environment", params);
    let p = EffectParams {
        scene: scene.as_ref(),
        ..EffectParams::new(&inst)
    };
    let fx = builtin("CC Environment");
    let dep = fx.effect().layer_dependencies(&p).pop().unwrap();
    let mut target = Standalone::new(gpu.clone(), F32);
    target.set_effect_inputs(vec![(dep, image.clone())], vec![]);
    let out = fx.render(&mut target, &p, &image, r).unwrap();
    target.finish().unwrap();
    Some(Pixels::download(&gpu, &out))
}

#[test]
fn environment_pan_rotates_the_sampled_longitude() {
    let Some(normal) = environment(vec![], None) else {
        return;
    };
    let rotated = environment(vec![(2, Value::Float(90.))], None).unwrap();
    assert!((rotated.get(32, 16)[0] - normal.get(32, 16)[0] - 0.25).abs() < 0.02);
}

#[test]
fn environment_tracks_the_comp_camera() {
    let mut camera = bae_render::camera::Camera::default_for(64, 32);
    camera.world *= glam::DMat4::from_rotation_y(std::f64::consts::FRAC_PI_2);
    let Some(out) = environment(vec![], Some(camera)) else {
        return;
    };
    assert!((out.get(32, 16)[0] - 0.75).abs() < 0.02);
}

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
fn bevel_edges_light_angle_zero_is_up_and_turns_clockwise() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(40, 40);
    let input = Pixels::new(rect, |_, _| [0.5, 0.5, 0.5, 1.]);
    // AE's default -60°: the top and left edges are lit.
    let out = run(
        &gpu,
        F32,
        builtin("ADBE Bevel Edges"),
        vec![],
        &input,
        rect,
        1.,
    );
    let [left, right, top, bottom] =
        [(0, 20), (39, 20), (20, 0), (20, 39)].map(|(x, y)| out.get(x, y)[0]);
    assert!(left > 0.5 && top > 0.5, "lit: left {left}, top {top}");
    assert!(
        right < 0.5 && bottom < 0.5,
        "shaded: right {right}, bottom {bottom}"
    );
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
