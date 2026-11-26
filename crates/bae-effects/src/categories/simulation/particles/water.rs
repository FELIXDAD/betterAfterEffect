use super::Kind;
use crate::{
    EffectTarget,
    effects::{layer_input, sampled::Shader},
    gpu,
    params::Params,
};
use bae_gpu::wgpu;
use bae_render::{GpuImage, IRect, RenderError};

static FLOW: Shader = Shader {
    label: "bae.effects.foam_flow_map",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("foam_flow.wgsl")
    ),
};
pub(super) fn flow_background(
    target: &mut dyn EffectTarget,
    p: Params<'_>,
    input: &GpuImage,
    map: &GpuImage,
    roi: IRect,
) -> Result<GpuImage, RenderError> {
    FLOW.dispatch(target, p, roi, input, [Some(map)], [0, 0])
}

static WAVE: Shader = Shader {
    label: "bae.effects.wave_world",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("wave_world.wgsl")
    ),
};
static CAUSTICS: Shader = Shader {
    label: "bae.effects.caustics",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("caustics.wgsl")
    ),
};
pub(super) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    FLOW.kernel(rt, format)?;
    WAVE.kernel(rt, format)?;
    CAUSTICS.kernel(rt, format)?;
    Ok(())
}
pub(super) fn dependency(
    p: &bae_render::EffectParams<'_>,
    id: u32,
    default_self: bool,
) -> Option<bae_render::effect::LayerDependency> {
    let mut dep = layer_input::dependency(p, id, default_self)?;
    if id == 10 && p.value(crate::catalog::layer_stage_id(10)).is_none() {
        dep.stage = bae_render::effect::LayerStage::EffectsAndMasks;
    }
    Some(dep)
}
pub(super) fn render(
    kind: Kind,
    target: &mut dyn EffectTarget,
    v: Params<'_>,
    input: &GpuImage,
    roi: IRect,
) -> Result<GpuImage, RenderError> {
    let map = |id, default_self| {
        dependency(v.p, id, default_self)
            .and_then(|d| target.effect_layer(&d))
            .cloned()
    };
    if kind == Kind::Wave {
        let ground = map(36, false);
        return WAVE.dispatch(target, v, roi, input, [ground.as_ref()], [0, 0]);
    }
    let bottom = map(2, true);
    let surface = map(10, false);
    let sky = map(70, false);
    // Use Comp Light: the light's values replace the manual light slots in
    // the uniforms (no copy of the effect instance every frame).
    let light = (v.popup(38) == 3)
        .then_some(v.p.scene)
        .flatten()
        .and_then(|scene| Some((scene, scene.layer_light?)));
    let scale = v.p.scale;
    let width = f64::from(v.p.layer_bounds.width().max(1));
    CAUSTICS.dispatch_at(
        "main",
        target,
        v,
        roi,
        input,
        [bottom.as_ref(), surface.as_ref(), sky.as_ref()],
        [0, 0],
        |u, header| {
            let Some((scene, light)) = light else {
                return;
            };
            let local = scene
                .placement
                .inverse()
                .transform_point2(light.position().truncate());
            let at = |id: usize| header + id * 16;
            let [r, g, b] = light.color;
            u.f32(at(30), light.intensity * 0.01)
                .f32s(at(32), &[r, g, b, 1.])
                .f32s(
                    at(34),
                    &[(local.x * scale) as f32, (local.y * scale) as f32],
                )
                .f32(at(36), (light.position().z.abs() / width) as f32)
                .f32(at(38), 1.);
        },
    )
}
