use bae_render::{camera::Projection, effect::EffectScene};
use glam::{DMat4, DVec2, DVec3, DVec4};

/// Projects full-resolution particle positions to homogeneous layer pixels for
/// both the GPU sprites and the composition viewer. Positions start at the
/// layer's top-left content bound. Divide x and y by w after near-plane clipping.
pub fn particle_world_projection(
    size: DVec2,
    origin: DVec2,
    scene: Option<&EffectScene>,
    param: impl Fn(u32) -> f64,
) -> DMat4 {
    if let Some(projection) = scene.and_then(|s| scene_projection(s, origin)) {
        return projection;
    }
    let center = size * 0.5;
    let focal = center.x / (param(37).to_radians() * 0.5).tan().max(0.01);
    let camera = DMat4::from_rotation_z(param(36).to_radians())
        * DMat4::from_rotation_y(-param(35).to_radians())
        * DMat4::from_rotation_x(param(34).to_radians())
        * DMat4::from_translation(-center.extend(0.));
    let depth = focal * param(33).max(0.01);
    // z carries the internal focal length for sprite sizing. A composition
    // camera sizes sprites using its projected layer-space x axis instead.
    DMat4::from_cols(
        DVec4::X * focal,
        DVec4::Y * focal,
        DVec4::new(center.x, center.y, 0., 1.),
        DVec4::new(center.x * depth, center.y * depth, focal, depth),
    ) * camera
}

pub(super) fn scene_projection(scene: &EffectScene, origin: DVec2) -> Option<DMat4> {
    let camera = scene.camera?;
    let view = camera.view();
    let inverse = scene.placement.inverse();
    let center = DVec2::from_array(scene.size.map(f64::from)) * 0.5;
    let project = |v: DVec3| {
        let xy = scene.placement.transform_point2(v.truncate() + origin);
        let p = view.transform_point3(xy.extend(v.z));
        let (w, zoom) = match camera.projection {
            Projection::Perspective => (p.z, camera.zoom),
            Projection::Orthographic { scale } => (1., scale),
        };
        let xy = inverse.matrix2 * (p.truncate() * zoom + center * w)
            + (inverse.translation - origin) * w;
        DVec4::new(xy.x, xy.y, 0., w)
    };
    let base = project(DVec3::ZERO);
    Some(DMat4::from_cols(
        project(DVec3::X) - base,
        project(DVec3::Y) - base,
        project(DVec3::Z) - base,
        base,
    ))
}
