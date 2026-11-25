mod common;
use bae_model::Value;
use bae_render::IRect;
use common::*;

#[test]
fn environment_pan_rotates_the_sampled_longitude() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(64, 32);
    let image = Pixels::new(r, |x, _| [x as f32 / 64., 0., 0., 1.]);
    let normal = run(&gpu, F32, builtin("CC Environment"), vec![], &image, r, 1.);
    let rotated = run(
        &gpu,
        F32,
        builtin("CC Environment"),
        vec![(2, Value::Float(90.))],
        &image,
        r,
        1.,
    );
    assert!((rotated.get(32, 16)[0] - normal.get(32, 16)[0] - 0.25).abs() < 0.02);
}

#[test]
fn environment_tracks_the_comp_camera() {
    use bae_model::PrimPath;
    use bae_render::{LayerContent, RenderComp, RenderLayer, camera::Camera};
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(64, 32);
    let mut comp = RenderComp::new(PrimPath::new("/comps/test").unwrap(), 64, 32);
    let mut camera = Camera::default_for(64, 32);
    camera.world *= glam::DMat4::from_rotation_y(std::f64::consts::FRAC_PI_2);
    comp.camera = Some(camera);
    let layer = RenderLayer::new(
        PrimPath::new("/comps/test/layers/env").unwrap(),
        LayerContent::None,
    );
    let fx = builtin("CC Environment");
    let mut inst = instance("CC Environment", vec![]);
    fx.effect().prepare_scene(&mut inst, &comp, &layer);
    let image = Pixels::new(r, |x, _| [x as f32 / 64., 0., 0., 1.]);
    let out = run(&gpu, F32, fx, inst.params, &image, r, 1.);
    assert!((out.get(32, 16)[0] - 0.75).abs() < 0.02);
}
