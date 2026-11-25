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
