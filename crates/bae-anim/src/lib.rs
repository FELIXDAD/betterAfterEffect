//! BAE animation evaluation (workstream 03, ARCHITECTURE §5).
//!
//! Given a [`Stage`] and a time, compute the final state of every layer of a comp,
//! matching After Effects' numbers:
//!
//! - [`evaluate_comp`] → [`EvaluatedComp`]: one comp at one comp time, flattened —
//!   world matrices, opacity, visibility, source times, masks, effect parameters,
//!   shape contents, cameras and lights. The renderer (04) only reads this.
//! - [`evaluate_attribute`] → [`Value`]: one attribute of any prim (UI, agents).
//! - [`Evaluator`]: the same with compiled comps cached between frames; feed it every
//!   [`StageEvent`](bae_model::StageEvent) through [`Evaluator::invalidate`].
//! - [`Curve`]: keyframe interpolation on its own (linear, Bezier speed/influence,
//!   hold, spatial Bezier with arc-length timing, roving keyframes).
//!
//! **Time bases.** `evaluate_comp` takes comp time. Keyframes of a layer and of the
//! prims below it are in layer time (`docs/spec/data-model.md` §10); the conversion
//! (start time, stretch) is done here, see [`time`].
//!
//! Spec references (`AE-ANI-nnn`) point to `docs/spec/ae/transform-keyframes-expressions.md`.
//! This crate has no GPU, window or platform dependencies.

mod bezier;
mod bounds;
mod curve;
pub mod edit;
pub mod effect_history;
mod error;
mod eval;
mod evaluated;
mod expr;
mod reads;
mod samples;
pub mod schema;
pub mod time;
mod transform;

pub use bae_model::{Stage, Time, Value};
pub use bounds::{SourceBounds, SourceRect, set_source_bounds};
pub use curve::{Curve, CurveKind};
pub use error::EvalError;
pub use eval::{AttrOverride, Evaluator, layer_timing};
pub use evaluated::{
    AssetInfo, AttrError, CameraState, EvaluatedComp, EvaluatedEffect, EvaluatedLayer,
    EvaluatedMask, EvaluatedPrim, FrameBlending, Iris, LayerSource, LayerSwitches, LightFalloff,
    LightState, LightType, MaterialState, TextPathOptions, TrackMatte,
};
pub use expr::prewarm as prewarm_expressions;

/// Parse an editor draft using the expression compiler, without executing it.
pub fn expression_syntax_error(source: &str) -> Option<String> {
    expr::rewrite::rewrite(source).err().map(|e| e.to_string())
}
pub use reads::{ExprRead, ReadKind, reads_touched};
pub use samples::{ImageSamples, SampleKey, SampleRequests};
pub use time::{LayerTiming, MotionBlur};
pub use transform::{AutoOrientRotation, Transform};

/// Re-exported so consumers use the same `glam` as the matrices in [`EvaluatedLayer`].
pub use glam;

use bae_model::PrimPath;

/// Evaluate `comp` at comp time `time`.
///
/// Compiles the comp from scratch; use an [`Evaluator`] to reuse the compiled form
/// across frames.
pub fn evaluate_comp(
    stage: &Stage,
    comp: &PrimPath,
    time: Time,
) -> Result<EvaluatedComp, EvalError> {
    Evaluator::new().evaluate_comp(stage, comp, time)
}

/// Evaluate one attribute of `prim` at `time` (the time of the comp containing the
/// prim; converted to layer time for layers and the prims below them).
pub fn evaluate_attribute(
    stage: &Stage,
    prim: &PrimPath,
    attr: &str,
    time: Time,
) -> Result<Value, EvalError> {
    Evaluator::new().evaluate_attribute(stage, prim, attr, time)
}

/// AE's default comp camera zoom when no camera layer is active: a 50 mm lens on
/// 36 mm film, `width * 50 / 36` pixels (AE-CMP-403).
pub fn default_camera_zoom(comp_width: u32) -> f64 {
    comp_width as f64 * 50.0 / 36.0
}
