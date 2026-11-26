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
    let mut lighting;
    let scene_params;
    let params = if v.popup(38) == 3
        && let Some(scene) = v.p.scene
        && let Some(light) = scene.layer_light
    {
        lighting = v.p.effect.clone();
        lighting
            .params
            .retain(|(id, _)| !matches!(id, 30 | 32 | 34 | 36 | 38));
        let local = scene
            .placement
            .inverse()
            .transform_point2(light.position().truncate());
        let width = f64::from(v.p.layer_bounds.width().max(1));
        let [r, g, b] = light.color.map(f64::from);
        use bae_model::Value;
        lighting.params.extend([
            (30, Value::Float(f64::from(light.intensity) * 0.01)),
            (32, Value::Color([r, g, b, 1.])),
            (34, Value::Vec2(local.to_array())),
            (36, Value::Float(light.position().z.abs() / width)),
            (38, Value::Int(1)),
        ]);
        scene_params = bae_render::EffectParams {
            effect: &lighting,
            ..*v.p
        };
        Params::new(v.def, &scene_params)
    } else {
        v
    };
    CAUSTICS.dispatch(
        target,
        params,
        roi,
        input,
        [bottom.as_ref(), surface.as_ref(), sky.as_ref()],
        [0, 0],
    )
}
