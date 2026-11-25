pub(crate) mod definitions;
use super::{image_grid, sampled::Shader, temporal};
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
                include_str!("../sampled.wgsl"),
                include_str!(concat!($name, ".wgsl"))
            ),
        }
    };
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
            return image_grid::layer_rect(p).union(&roi);
        }
        let r = match self.kind {
            Kind::Cleaner => v.f(6),
            Kind::Soft => v.f(25) + v.f(27) + v.f(36) + v.f(2),
            Kind::Hard => v.f(24) + v.f(2) + 3.,
            _ => 0.,
        };
        roi.inflate((2. * r * p.scale).ceil() as i32 + 2)
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
fn soften(
    target: &mut dyn EffectTarget,
    image: &GpuImage,
    amount: f64,
    scale: f64,
) -> Result<GpuImage, RenderError> {
    let sigma = amount * scale * 0.5;
    if blur::radius(sigma) == 0 {
        return Ok(image.clone());
    }
    blur::plan_gaussian(image.rect, [sigma; 2], blur::Edge::Repeat, image.rect)
        .encode(target, image)
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
        let reference = self
            .reference(p)
            .and_then(|r| target.effect_layer(&r))
            .cloned();
        match self.kind {
            Kind::Keylight => {
                let mut masks = [None, None];
                for (i, id) in [23, 31].into_iter().enumerate() {
                    if let Some(image) = mask(p, id).and_then(|m| target.effect_mask(&m)).cloned() {
                        masks[i] = Some(soften(target, &image, v.f(id + 1), p.scale)?);
                    }
                }
                let blurred = soften(target, input, v.f(10), p.scale)?;
                shader!("keylight").dispatch(
                    target,
                    v,
                    roi,
                    input,
                    [masks[0].as_ref(), masks[1].as_ref(), Some(&blurred)],
                    [0, 0],
                )
            }
            kind => {
                let mode = match kind {
                    Kind::Cleaner => 0,
                    Kind::Soft => 1,
                    _ => 2,
                };
                let grid = image_grid::grid_scale(p, input);
                let bounds = image_grid::pixel_rect(input.rect, 1. / grid);
                let coefficients =
                    shader!("refine").dispatch(target, v, bounds, input, [None; 3], [mode, 1])?;
                shader!("refine").dispatch(
                    target,
                    v,
                    roi,
                    input,
                    [reference.as_ref(), Some(&coefficients), None],
                    [mode, 0],
                )
            }
        }
    }
}
