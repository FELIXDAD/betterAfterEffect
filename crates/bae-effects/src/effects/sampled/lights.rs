//! Comp lights in layer coordinates for the Cycore surface shaders (AE Lights).
use bae_render::{effect::EffectScene, light::LightKind};

use crate::gpu::UniformWriter;

const MAX_LIGHTS: usize = 24;

/// The scene's lights (only effects that asked for them get any,
/// `Effect::scene_lights`) after the parameter slots: the count at slot 64,
/// then 12 floats per light in layer coordinates.
pub(crate) fn write(scene: Option<&EffectScene>, u: &mut UniformWriter, header: usize) {
    let Some(scene) = scene.filter(|s| !s.lights.is_empty()) else {
        return;
    };
    if scene.world.determinant().abs() < 1e-12 {
        return;
    }
    let local = scene.world.inverse();
    let mut count = 0;
    for light in &scene.lights {
        if count == MAX_LIGHTS {
            break;
        }
        let kind = match light.kind {
            LightKind::Environment => continue,
            LightKind::Ambient => 0.,
            LightKind::Parallel => 1.,
            LightKind::Point => 2.,
            LightKind::Spot => 3.,
        };
        let p = local.transform_point3(light.position());
        let d = local
            .transform_vector3(light.direction())
            .normalize_or_zero();
        let [r, g, b] = light.color.map(|c| f64::from(c * light.intensity * 0.01));
        let row = [
            p.x,
            p.y,
            p.z,
            kind,
            d.x,
            d.y,
            d.z,
            f64::from(light.cone_angle.to_radians() * 0.5).cos(),
            r,
            g,
            b,
            f64::from(light.cone_feather) * 0.01,
        ];
        let at = header + 65 * 16 + count * 12 * 4;
        for (i, v) in row.into_iter().enumerate() {
            u.f32(at + i * 4, v as f32);
        }
        count += 1;
    }
    u.f32(header + 64 * 16, count as f32);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_render::light::{Falloff, Light};
    use glam::{DAffine2, DMat3, DMat4, DVec3};

    #[test]
    fn scene_lights_reach_the_uniforms_in_layer_coordinates() {
        let light = Light {
            kind: LightKind::Point,
            world: DMat4::from_translation(DVec3::new(110., 20., -50.)),
            point_of_interest: DVec3::ZERO,
            color: [1., 0.5, 0.],
            intensity: 200.,
            cone_angle: 90.,
            cone_feather: 50.,
            falloff: Falloff::None,
            radius: 0.,
            falloff_distance: 0.,
            casts_shadows: false,
            shadow_darkness: 0.,
            shadow_diffusion: 0.,
            scope_below: None,
        };
        let scene = EffectScene {
            projection: DMat3::IDENTITY,
            light: None,
            camera: None,
            size: [100, 100],
            placement: DAffine2::IDENTITY,
            layer_light: None,
            // The layer sits 10 px right of the comp origin.
            world: DMat4::from_translation(DVec3::new(10., 0., 0.)),
            lights: vec![light],
        };
        let mut u = UniformWriter::with_size(65 * 16 + 12 * 4);
        write(Some(&scene), &mut u, 0);
        let at =
            |offset: usize| f32::from_le_bytes(u.bytes[offset..offset + 4].try_into().unwrap());
        assert_eq!(at(64 * 16), 1.);
        let row: Vec<f32> = (0..12).map(|i| at(65 * 16 + i * 4)).collect();
        assert_eq!(&row[..4], &[100., 20., -50., 2.]);
        assert_eq!(&row[8..11], &[2., 1., 0.]);
        // An effect that did not ask for lights gets none.
        let mut none = UniformWriter::with_size(65 * 16 + 12 * 4);
        write(None, &mut none, 0);
        assert!(none.bytes.iter().all(|b| *b == 0));
    }
}
