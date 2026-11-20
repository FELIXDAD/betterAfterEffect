//! Adapter from workstream 03's evaluator (`bae-anim`) to the renderer's input.
//!
//! `bae_anim::Evaluator` does the animation work (AE easing, spatial paths,
//! parenting, layer time, time remapping, later expressions); this module maps
//! its [`EvaluatedComp`] onto [`RenderComp`]: world matrices become 2D affines,
//! sources are resolved (solid assets, footage files; nested comps arrive
//! already evaluated at their mapped time), tokens become enums.

use std::path::PathBuf;
use std::sync::Arc;

use bae_anim::{EvaluatedComp, EvaluatedLayer, Evaluator, LayerSource};
use bae_model::{AssetFile, AssetResolver, LayerKind, PrimPath, Stage, StageEvent, Time};
use glam::{DAffine2, DMat4, DVec2};

use crate::blend::{BlendMode, MaskMode, TrackMatteMode};
use crate::error::RenderError;
use crate::eval::CompEvaluator;
use crate::graph::MAX_DEPTH;
use crate::input::{
    FootageRef, LayerContent, RenderComp, RenderEffect, RenderLayer, RenderMask, TrackMatte,
};
use crate::request::RenderPurpose;

/// [`CompEvaluator`] backed by `bae_anim::Evaluator` (keeps compiled comps across
/// frames; forward [`StageEvent`]s to [`CompEvaluator::invalidate`]).
#[derive(Debug)]
pub struct AnimEvaluator {
    inner: Evaluator,
    assets: AssetResolver,
    purpose: RenderPurpose,
    warnings: Vec<String>,
}

impl AnimEvaluator {
    /// An evaluator whose footage paths resolve against `base_dir` (the
    /// project's directory).
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        Self {
            inner: Evaluator::new(),
            assets: AssetResolver::new(base_dir, Vec::new()),
            purpose: RenderPurpose::Final,
            warnings: Vec::new(),
        }
    }

    /// Preview or final (guide layers render only in a top-level preview).
    #[must_use]
    pub fn with_purpose(mut self, purpose: RenderPurpose) -> Self {
        self.purpose = purpose;
        self
    }

    fn warn(&mut self, message: String) {
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    fn eval(
        &mut self,
        stage: &Stage,
        comp: &PrimPath,
        time: Time,
        depth: usize,
    ) -> Result<RenderComp, RenderError> {
        if depth > MAX_DEPTH {
            return Err(RenderError::Cycle {
                path: comp.clone(),
                max: MAX_DEPTH,
            });
        }
        let ec = self
            .inner
            .evaluate_comp(stage, comp, time)
            .map_err(|e| RenderError::Other(format!("{comp}: {e}")))?;
        self.convert(stage, &ec, depth)
    }

    /// Converts one evaluated comp (nested comps are evaluated on demand).
    pub fn convert(
        &mut self,
        stage: &Stage,
        ec: &EvaluatedComp,
        depth: usize,
    ) -> Result<RenderComp, RenderError> {
        for e in &ec.errors {
            self.warn(format!("{}.{}: {}", e.prim, e.attr, e.message));
        }
        let mut rc = RenderComp {
            path: ec.path.clone(),
            time: ec.time,
            width: ec.width,
            height: ec.height,
            pixel_aspect: ec.pixel_aspect,
            frame_rate: ec.frame_rate,
            background: ec.bg_color.map(|v| v as f32),
            layers: Vec::with_capacity(ec.layers.len()),
        };
        for layer in &ec.layers {
            let out = self.layer(stage, ec, layer, depth)?;
            rc.layers.push(out);
        }
        Ok(rc)
    }

    fn layer(
        &mut self,
        stage: &Stage,
        ec: &EvaluatedComp,
        l: &EvaluatedLayer,
        depth: usize,
    ) -> Result<RenderLayer, RenderError> {
        let guide_ok = !l.switches.guide || (depth == 0 && self.purpose == RenderPurpose::Preview);
        let mut out = RenderLayer::new(l.path.clone(), LayerContent::None);
        out.time = l.layer_time;
        out.active = l.active && guide_ok;
        out.visible = l.visible;
        out.adjustment = l.switches.adjustment || l.kind == LayerKind::Adjustment;
        if l.switches.three_d {
            self.warn(format!(
                "{}: 3D layers are drawn flat (no camera projection yet)",
                l.path
            ));
        }
        out.transform = affine_2d(&l.world_matrix);
        out.opacity = l.opacity.clamp(0.0, 1.0) as f32;
        out.blend_mode = BlendMode::from_token(&l.blend_mode).unwrap_or_else(|| {
            self.warn(format!("{}: unknown blend mode `{}`", l.path, l.blend_mode));
            BlendMode::Normal
        });
        out.preserve_transparency = l.switches.preserve_transparency;
        out.track_matte = matte_mode(l.track_matte)
            .and_then(|mode| l.track_matte_layer.map(|layer| TrackMatte { layer, mode }));
        if out.active {
            out.content = self.content(stage, ec, l, depth)?;
            out.masks = l
                .masks
                .iter()
                .map(|m| RenderMask {
                    path: m.path.clone(),
                    shape: m.shape.clone(),
                    mode: MaskMode::from_token(&m.mode).unwrap_or_default(),
                    inverted: m.inverted,
                    opacity: m.opacity.clamp(0.0, 1.0) as f32,
                    feather: m.feather.map(|v| v as f32),
                    expansion: m.expansion as f32,
                })
                .collect();
            out.effects = l
                .effects
                .iter()
                .map(|e| RenderEffect {
                    path: e.path.clone(),
                    effect: e.match_name.clone(),
                    enabled: e.enabled,
                    params: e
                        .params
                        .iter()
                        .filter_map(|(id, v)| Some((id.parse().ok()?, v.clone())))
                        .collect(),
                })
                .collect();
        }
        Ok(out)
    }

    fn content(
        &mut self,
        stage: &Stage,
        _ec: &EvaluatedComp,
        l: &EvaluatedLayer,
        depth: usize,
    ) -> Result<LayerContent, RenderError> {
        if matches!(
            l.kind,
            LayerKind::Null
                | LayerKind::Camera
                | LayerKind::Light
                | LayerKind::Audio
                | LayerKind::Adjustment
        ) {
            return Ok(LayerContent::None);
        }
        match &l.source {
            LayerSource::None => {
                if matches!(l.kind, LayerKind::Shape | LayerKind::Text) {
                    self.warn(format!(
                        "{}: {} layers are drawn by 05 (not yet)",
                        l.path,
                        l.kind.as_str()
                    ));
                }
                Ok(LayerContent::None)
            }
            LayerSource::Comp { comp, nested, .. } => match nested {
                // bae-anim evaluates nested comps itself (at the mapped time).
                Some(nested) => {
                    let rc = self.convert(stage, nested, depth + 1)?;
                    Ok(LayerContent::Comp(Arc::new(rc)))
                }
                None => {
                    self.warn(format!("{}: pre-comp {comp} is missing or cyclic", l.path));
                    Ok(LayerContent::None)
                }
            },
            LayerSource::Asset { asset, time, info } => Ok(match info.kind.as_str() {
                "solid" => LayerContent::Solid {
                    color: info
                        .solid_color
                        .unwrap_or([0.5, 0.5, 0.5, 1.0])
                        .map(|v| v as f32),
                    width: info.width,
                    height: info.height,
                },
                "footage" | "sequence" => LayerContent::Footage(FootageRef {
                    asset: asset.clone(),
                    file: stage
                        .prim(asset)
                        .and_then(|view| AssetFile::from_prim(&view))
                        .and_then(|f| self.assets.resolve(&f).path().map(|p| p.to_path_buf())),
                    size: (info.width > 0 && info.height > 0).then_some([info.width, info.height]),
                    source_time: *time,
                }),
                _ => LayerContent::None,
            }),
        }
    }
}

impl CompEvaluator for AnimEvaluator {
    fn evaluate(
        &mut self,
        stage: &Stage,
        comp: &PrimPath,
        time: Time,
    ) -> Result<RenderComp, RenderError> {
        self.eval(stage, comp, time, 0)
    }

    fn invalidate(&mut self, event: &StageEvent) {
        self.inner.invalidate(event);
    }

    fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

fn matte_mode(m: bae_anim::TrackMatte) -> Option<TrackMatteMode> {
    match m {
        bae_anim::TrackMatte::None => None,
        bae_anim::TrackMatte::Alpha => Some(TrackMatteMode::Alpha),
        bae_anim::TrackMatte::AlphaInverted => Some(TrackMatteMode::AlphaInverted),
        bae_anim::TrackMatte::Luma => Some(TrackMatteMode::Luma),
        bae_anim::TrackMatte::LumaInverted => Some(TrackMatteMode::LumaInverted),
    }
}

/// The 2D part of a layer → comp matrix (orthographic; z is dropped).
pub fn affine_2d(m: &DMat4) -> DAffine2 {
    DAffine2::from_cols(
        DVec2::new(m.x_axis.x, m.x_axis.y),
        DVec2::new(m.y_axis.x, m.y_axis.y),
        DVec2::new(m.w_axis.x, m.w_axis.y),
    )
}
