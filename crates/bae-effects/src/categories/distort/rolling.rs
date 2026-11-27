//! Rolling-shutter readout correction from the shared dense motion estimator.
use crate::{
    Builtin, EffectTarget, TargetEffect,
    effects::{
        image_grid,
        sampled::Shader,
        temporal::{before_effect, flow},
    },
    gpu,
    params::Params,
    rust_builtin,
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, Time};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};
use std::sync::Arc;

#[path = "rolling_definition.rs"]
mod definition;
static SHADER: Shader = Shader {
    label: "bae.effects/rolling-shutter",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("../../effects/temporal/flow_sampling.wgsl"),
        include_str!("rolling.wgsl")
    ),
};
pub(super) fn prewarm(rt: &gpu::Runtime, fmt: wgpu::TextureFormat) -> Result<(), RenderError> {
    SHADER.kernel(rt, fmt)?;
    Ok(())
}
pub(super) fn builtin() -> Builtin {
    let (def, labels) = definition::definition().build();
    let mut item = rust_builtin(
        (Arc::new(Rolling { def }), labels),
        "rolling-shutter-repair",
        "果冻效应修复",
        &["rolling shutter", "jello", "果冻效应"],
    );
    item.info.param_options_zh = definition::options();
    item
}
struct Rolling {
    def: EffectDef,
}
fn neighbor(p: &EffectParams<'_>) -> (Time, f32) {
    if p.comp_time - p.time_step >= p.layer_in {
        (p.comp_time - p.time_step, -1.)
    } else {
        (p.comp_time + p.time_step, 1.)
    }
}
impl Effect for Rolling {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        roi.union(&image_grid::layer_rect(p))
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        if Params::new(&self.def, p).f(1) == 0. {
            return vec![];
        }
        before_effect(p, neighbor(p).0).into_iter().collect()
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
impl TargetEffect for Rolling {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        let Some(dep) = self.layer_dependencies(p).pop() else {
            return Ok(input.clone());
        };
        let Some(other) = target.effect_layer(&dep).cloned() else {
            return Ok(input.clone());
        };
        let grid = image_grid::grid_scale(p, input);
        let layer = image_grid::layer_rect(p).union(&roi);
        let detail = if v.popup(4) == 2 { v.f(6) } else { 20. };
        let field = flow::Pair {
            images: [input, &other],
            times: [p.comp_time, dep.time],
            layer: &dep.layer,
            roi: layer,
            grid,
            detail: detail * if v.b(5) { 2. } else { 1. },
            analysis: flow::Analysis::default(),
        }
        .forward(target)?;
        SHADER.dispatch_at(
            "main",
            target,
            v,
            roi,
            input,
            [Some(&field.image)],
            [0, 0],
            |w, h| {
                w.f32s(
                    h + 16 * 10,
                    &[
                        field.region.x0 as f32,
                        field.region.y0 as f32,
                        field.scale,
                        neighbor(p).1,
                    ],
                );
            },
        )
    }
}
