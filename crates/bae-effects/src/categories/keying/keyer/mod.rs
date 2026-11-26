pub(crate) mod definitions;
use crate::effects::{image_grid, sampled::Shader, temporal};
use crate::{Builtin, EffectTarget, TargetEffect, blur, params::Params, rust_builtin};
use bae_model::{EffectDef, ParamKind, Time, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError,
    effect::{LayerDependency, MaskSelect},
};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Keylight,
    Cleaner,
    Soft,
    Hard,
}

macro_rules! shader {
    ($name:literal) => {
        Shader {
            label: concat!("bae.effects.keying.", $name),
            source: concat!(
                include_str!("../../../effects/sampled.wgsl"),
                include_str!(concat!($name, ".wgsl"))
            ),
        }
    };
}
pub(crate) fn prewarm(
    rt: &crate::gpu::Runtime,
    format: bae_gpu::wgpu::TextureFormat,
) -> Result<(), RenderError> {
    for shader in [shader!("keylight"), shader!("refine")] {
        shader.kernel(rt, format)?;
    }
    Ok(())
}

struct Keyer {
    def: EffectDef,
    kind: Kind,
}

pub(crate) fn builtin(kind: Kind) -> Builtin {
    let (builder, id, zh, keyword) = match kind {
        Kind::Keylight => (
            definitions::keylight(),
            "keylight",
            "Keylight (1.2)",
            "绿幕",
        ),
        Kind::Cleaner => (
            definitions::cleaner(),
            "key-cleaner",
            "Key Cleaner",
            "抠像清理",
        ),
        Kind::Soft => (
            definitions::soft(),
            "refine-soft-matte",
            "调整柔和遮罩",
            "边缘",
        ),
        Kind::Hard => (
            definitions::hard(),
            "refine-hard-matte",
            "调整实边遮罩",
            "边缘",
        ),
    };
    let (def, names) = builder.build();
    rust_builtin(
        (Arc::new(Keyer { def, kind }), names),
        id,
        zh,
        &[id, keyword],
    )
}

fn mask(p: &EffectParams<'_>, id: u32) -> Option<MaskSelect> {
    match p.value(id) {
        Some(Value::PrimRef(path)) => Some(MaskSelect::One(path.clone())),
        _ => None,
    }
}

mod guided;
mod screen;

impl Keyer {
    fn reference(&self, p: &EffectParams<'_>) -> Option<LayerDependency> {
        let v = Params::new(&self.def, p);
        let frames = match self.kind {
            Kind::Cleaner if v.b(2) => -1.,
            Kind::Soft if v.popup(35) > 2 || v.b(8) => -1.,
            Kind::Hard if v.f(16) > 0. || v.b(8) => -1.,
            _ => return None,
        };

        temporal::before_effect(
            p,
            p.comp_time
                + Time::from_seconds(frames * p.time_step.as_seconds() * p.layer_time_scale),
        )
    }
}
impl Effect for Keyer {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        self.reference(p).into_iter().collect()
    }
    fn mask_dependencies(&self, p: &EffectParams<'_>) -> Vec<MaskSelect> {
        self.def
            .params
            .iter()
            .filter(|d| d.kind == ParamKind::Path)
            .filter_map(|d| mask(p, d.id))
            .collect()
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        let v = Params::new(&self.def, p);
        if self.kind == Kind::Keylight {
            return screen::read(v, roi);
        }
        roi.inflate(2 * guided::radius(self.kind, v).ceil() as i32)
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
// Run the shared blur planner in render-grid texels, then restore layer coordinates.
fn filter(
    target: &mut dyn EffectTarget,
    image: &GpuImage,
    plan: blur::Plan,
) -> Result<GpuImage, RenderError> {
    let grid = image.pixel_scale;
    let mut texels = image.clone();
    texels.rect = image_grid::pixel_rect(image.rect, 1. / grid);
    texels.pixel_scale = 1.;
    let mut out = plan.encode(target, &texels)?;
    out.rect = image_grid::pixel_rect(out.rect, grid);
    out.pixel_scale = grid;
    Ok(out)
}

impl TargetEffect for Keyer {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        if self.kind == Kind::Keylight {
            return screen::encode(target, v, input, roi);
        }
        let reference = self
            .reference(p)
            .and_then(|r| target.effect_layer(&r))
            .cloned();
        guided::encode(target, self.kind, v, input, reference.as_ref(), roi)
    }
}
