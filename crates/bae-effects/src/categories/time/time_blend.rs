use crate::{
    Builtin, EffectTarget, Registry, TargetEffect, catalog::DefBuilder, effects::sampled::Shader,
    gpu, params::Params, rust_builtin,
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, PrimPath};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderEffect, RenderError};
use std::sync::Arc;

static BLEND: Shader = Shader {
    label: "bae.effects.cc_time_blend",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("../../shaders/generated_blend.wgsl"),
        include_str!("time_blend.wgsl")
    ),
};
pub(super) fn register(r: &mut Registry) {
    r.prewarm.push(prewarm);
    r.effects.extend([builtin(false), builtin(true)]);
}
fn prewarm(rt: &gpu::Runtime, f: wgpu::TextureFormat) -> Result<(), RenderError> {
    BLEND.kernel(rt, f)?;
    Ok(())
}
const TRANSFER: [&str; 18] = [
    "Blend",
    "Composite Over",
    "Composite Under",
    "Screen",
    "Add",
    "Soft Light",
    "Hard Light",
    "Lighten",
    "Darken",
    "Multiply",
    "Difference",
    "Color",
    "Overlay",
    "Luminosity",
    "(-",
    "Blend - Classic",
    "Composite Over - Classic",
    "Composite Under - Classic",
];

struct TimeBlend {
    def: EffectDef,
    paired: bool,
}
fn builtin(paired: bool) -> Builtin {
    let (id, slug) = if paired {
        ("CC Time Blend FX", "cc-time-blend-fx")
    } else {
        ("CC Time Blend", "cc-time-blend")
    };
    let shift = u32::from(paired);
    let mut b = DefBuilder::new(id, id, "Obsolete");
    if paired {
        b = b.popup(1, "Instance", "Instance", 1, &["Copy", "Paste"]);
    }
    let (def, names) = b
        .popup(1 + shift, "Transfer", "Transfer", 1, &TRANSFER)
        .slider(
            2 + shift,
            "Accumulation",
            "Accumulation",
            0.75,
            [0., 1.],
            [0., 1.],
            3,
            "none",
        )
        .popup(
            3 + shift,
            "Clear To",
            "Clear To",
            1,
            &["Transparent", "Current Frame"],
        )
        .build();
    rust_builtin(
        (Arc::new(TimeBlend { def, paired }), names),
        slug,
        id,
        &["feedback", "echo", "时间混合", "残影"],
    )
}
impl Effect for TimeBlend {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn full_surface(&self) -> bool {
        true
    }
    fn feedback_source(&self, p: &EffectParams<'_>, stack: &[RenderEffect]) -> Option<PrimPath> {
        let own = &p.effect.path;
        if self.paired
            && Params::new(&self.def, p).popup(1) == 2
            && let Some(i) = stack.iter().position(|fx| fx.path == *own)
            && let Some(next) = stack[i + 1..].iter().find(|fx| {
                fx.enabled
                    && fx.effect == self.def.match_name
                    && EffectParams::new(fx).popup(1).unwrap_or(1) == 1
            })
        {
            return Some(next.path.clone());
        }
        Some(own.clone())
    }
    fn render(
        &self,
        t: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(t, p, input, roi)
    }
}
impl TargetEffect for TimeBlend {
    fn encode(
        &self,
        t: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        if roi.is_empty() || (self.paired && v.popup(1) == 1) {
            return Ok(input.clone());
        }
        BLEND.dispatch(
            t,
            v,
            roi,
            input,
            [p.previous_frame],
            [u32::from(self.paired), 0],
        )
    }
}
