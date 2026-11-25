pub(crate) mod definitions;
use crate::effects::{image_grid, layer_input, sampled::Shader, temporal};
use crate::{Builtin, EffectTarget, TargetEffect, params::Params, rust_builtin};
use bae_model::{EffectDef, Time};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};
use std::sync::Arc;

const MATCH: Shader = Shader {
    label: "bae.effects.match-grain",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("hash.wgsl"),
        include_str!("../../../../packages/add-grain/shaders/field.wgsl"),
        include_str!("grain.wgsl"),
        include_str!("match.wgsl")
    ),
};
const REMOVE: Shader = Shader {
    label: "bae.effects.remove-grain",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("grain.wgsl"),
        include_str!("remove.wgsl")
    ),
};
const PROFILE: Shader = Shader {
    label: "bae.effects.grain-patches",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("profile.wgsl")
    ),
};
const REDUCE: Shader = Shader {
    label: "bae.effects.grain-profile",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("reduce.wgsl")
    ),
};
pub(crate) fn prewarm(
    rt: &crate::gpu::Runtime,
    format: bae_gpu::wgpu::TextureFormat,
) -> Result<(), RenderError> {
    for shader in [MATCH, REMOVE, PROFILE, REDUCE] {
        shader.kernel(rt, format)?;
    }
    Ok(())
}

struct Grain {
    def: EffectDef,
    matching: bool,
}
pub(crate) fn builtins() -> [Builtin; 2] {
    [
        (true, definitions::match_grain(), "match-grain", "匹配颗粒"),
        (
            false,
            definitions::remove_grain(),
            "remove-grain",
            "移除颗粒",
        ),
    ]
    .map(|(matching, b, id, zh)| {
        let (def, names) = b.reference_points().build();
        rust_builtin(
            (Arc::new(Grain { def, matching }), names),
            id,
            zh,
            &[id, zh],
        )
    })
}
impl Grain {
    fn inputs(&self, p: &EffectParams<'_>) -> [Option<LayerDependency>; 3] {
        let v = Params::new(&self.def, p);
        let time = Time::from_seconds(
            v.f(if self.matching { 141 } else { 58 }) * p.time_step.as_seconds(),
        );
        let reference = if self.matching {
            layer_input::dependency(p, 13, false).map(|mut dep| {
                dep.time = time;
                dep
            })
        } else {
            temporal::before_effect(p, time)
        };
        let matte = layer_input::dependency(p, if self.matching { 150 } else { 69 }, false);
        let previous = if !self.matching && v.b(22) {
            temporal::before_effect(p, p.comp_time - p.time_step)
        } else {
            None
        };
        [reference, matte, previous]
    }
}
impl Effect for Grain {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        self.inputs(p).into_iter().flatten().collect()
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        image_grid::layer_rect(p).union(&roi)
    }
    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, p, input, roi)
    }
}
impl TargetEffect for Grain {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        let [reference, matte, previous] = self
            .inputs(p)
            .map(|r| r.and_then(|r| target.effect_layer(&r)).cloned());
        if self.matching && reference.is_none() {
            return Ok(input.clone());
        }
        let shader = if self.matching { &MATCH } else { &REMOVE };
        let count = v
            .f(if self.matching { 10 } else { 3 })
            .round()
            .clamp(1., 10.) as u32;
        let profiles = 1 + u32::from(self.matching && v.f(118) > 0.);
        let patches = PROFILE.dispatch(
            target,
            v,
            IRect::from_size(16, count * profiles),
            input,
            [reference.as_ref(), None, None],
            [0, u32::from(self.matching)],
        )?;
        let stats = REDUCE.dispatch(
            target,
            v,
            IRect::from_size(profiles, 1),
            input,
            [Some(&patches), None, None],
            [0, count],
        )?;
        shader.dispatch(
            target,
            v,
            roi,
            input,
            [
                Some(&stats),
                matte.as_ref(),
                if self.matching {
                    reference.as_ref()
                } else {
                    previous.as_ref()
                },
            ],
            [0, 0],
        )
    }
}
