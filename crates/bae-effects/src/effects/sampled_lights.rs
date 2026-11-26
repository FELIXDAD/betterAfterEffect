//! Comp lights in layer coordinates for the Cycore surface shaders.
use bae_model::{EffectDef, Value};
use bae_render::{RenderComp, RenderEffect, RenderLayer, light::LightKind};
use glam::{DMat4, DVec3};

const LIGHTS_PARAM: u32 = u32::MAX - 1;
const MAX_LIGHTS: usize = 24;

pub(crate) fn collect(
    def: &EffectDef,
    instance: &mut RenderEffect,
    comp: &RenderComp,
    layer: &RenderLayer,
) {
    let Some(using) = def.params.iter().find(|p| p.name == "Using") else {
        return;
    };
    if !matches!(
        instance.param(using.id),
        Some(Value::Int(2) | Value::Float(2.0))
    ) {
        return;
    }
    let m = layer.transform;
    let world = layer.matrix_3d.unwrap_or_else(|| {
        DMat4::from_cols(
            m.matrix2.x_axis.extend(0.).extend(0.),
            m.matrix2.y_axis.extend(0.).extend(0.),
            DVec3::Z.extend(0.),
            m.translation.extend(0.).extend(1.),
        )
    });
    if world.determinant().abs() < 1e-12 {
        return;
    }
    let local = world.inverse();
    let index = comp.layers.iter().position(|l| l.path == layer.path);
    let mut rows = Vec::new();
    for light in comp
        .lights
        .iter()
        .filter(|l| l.scope_below.is_none_or(|n| index.is_some_and(|i| i > n)))
        .take(MAX_LIGHTS)
    {
        let kind = match light.kind {
            LightKind::Environment => continue,
            LightKind::Ambient => 0.,
            LightKind::Parallel => 1.,
            LightKind::Point => 2.,
            LightKind::Spot => 3.,
        };
        let position = local.transform_point3(light.position());
        let direction = local
            .transform_vector3(light.direction())
            .normalize_or_zero();
        rows.extend(position.to_array());
        rows.push(kind);
        rows.extend(direction.to_array());
        rows.push(f64::from(light.cone_angle.to_radians() * 0.5).cos());
        rows.extend(light.color.map(|c| f64::from(c * light.intensity * 0.01)));
        rows.push(f64::from(light.cone_feather) * 0.01);
    }
    instance.params.push((LIGHTS_PARAM, Value::FloatList(rows)));
}

pub(super) fn write(instance: &RenderEffect, u: &mut crate::gpu::UniformWriter, header: usize) {
    if let Some(Value::FloatList(lights)) = instance.param(LIGHTS_PARAM) {
        u.f32(header + 64 * 16, (lights.len() / 12) as f32);
        for (i, &v) in lights.iter().enumerate() {
            u.f32(header + 65 * 16 + i * 4, v as f32);
        }
    }
}
