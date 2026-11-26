//! Comp evaluation: from the project to a [`RenderComp`].
//!
//! The real evaluator is workstream 03 (`bae-anim`: AE easing, spatial Beziers,
//! expressions, time remapping, motion blur samples). Until it lands,
//! [`MockEvaluator`] reads a [`Stage`] directly, following the prim schema v1
//! conventions (`docs/spec/data-model.md` §10):
//!
//! * layers are the children of `<comp>/layers`, first child on top (direct
//!   `Layer` children of the comp are accepted too);
//! * layer keyframes are in layer time: `layer time = (t − start_time) × 100 / stretch`;
//! * `source` points at an `Asset` (solid: `solid_color`, `width`, `height`;
//!   footage: `file:*`) or at a `Comp` (pre-comp, evaluated at layer time);
//! * `transform:*` in AE units (pixels, percent, degrees), `parent` chains;
//! * `in_point` ≤ t < `out_point` (none = comp duration), `enabled`, `solo`,
//!   `guide`, `blend_mode`, `track_matte` + `track_matte_layer` (none = layer above),
//!   `preserve_transparency` (not in schema v1 yet; read when present);
//! * effects are `Effect` children (or children of `<layer>/effects`) with
//!   `param:NNNN` attributes; masks are `Mask` children (or under `<layer>/masks`).
//!
//! Simplifications (03 does these properly): keyframes interpolate linearly
//! (Bezier easing is treated as linear, hold is honoured), expressions are
//! ignored (static value / keyframes are used), no time remapping or motion blur.

use std::path::PathBuf;
use std::sync::Arc;

use bae_model::{
    AttrSpec, FrameRate, Interpolation, PrimPath, PrimType, PrimView, Stage, StageEvent, Time,
    Value, schema,
};
use glam::{DAffine2, DVec2};

use crate::assets::AssetFiles;
use crate::blend::{BlendMode, MaskMode, TrackMatteMode};
use crate::error::RenderError;
use crate::graph::MAX_DEPTH;
use crate::input::{
    FootageRef, LayerContent, RenderComp, RenderEffect, RenderLayer, RenderMask, TrackMatte,
};
use crate::request::RenderPurpose;

/// Other frames of a comp for effects that read them, from an evaluator and
/// the stage ([`crate::Renderer::render_frame_with`]).
pub struct EvaluatorFrames<'a> {
    /// The stage.
    pub stage: &'a Stage,
    /// The evaluator.
    pub evaluator: &'a mut dyn CompEvaluator,
}

impl crate::graph::CompFrames for EvaluatorFrames<'_> {
    fn animated_controls(&self, effect: &PrimPath, ids: &[u32]) -> bool {
        ids.is_empty()
            || self.stage.prim(effect).is_some_and(|fx| {
                ids.iter().any(|id| {
                    fx.attr(&bae_model::effect::param_attr(*id))
                        .is_some_and(|a| a.is_animated() || a.expression.is_some())
                })
            })
    }

    fn parameters_at(
        &mut self,
        instance: &RenderEffect,
        ids: &[u32],
        frame: &crate::graph::EffectFrame,
        times: &[Time],
    ) -> Vec<crate::effect::ParameterSample> {
        if times.is_empty() {
            return Vec::new();
        }
        let Some(view) = self.stage.prim(&instance.path) else {
            return Vec::new();
        };
        let mut samples: Vec<_> = times
            .iter()
            .map(|&time| crate::effect::ParameterSample {
                time,
                values: Vec::new(),
            })
            .collect();
        for (id, current) in &instance.params {
            if !ids.is_empty() && !ids.contains(id) {
                continue;
            }
            let name = bae_model::effect::param_attr(*id);
            let Some(attr) = view.attr(&name) else {
                continue;
            };
            if !attr.is_animated() && attr.expression.is_none() {
                continue;
            }
            let curve =
                bae_anim::Curve::new(&attr.keyframes, bae_anim::schema::curve_kind(&name, attr));
            for sample in &mut samples {
                let value = if attr.expression.is_some() {
                    self.evaluator.attribute(
                        self.stage,
                        &instance.path,
                        &name,
                        frame.comp_at(sample.time),
                    )
                } else {
                    curve.as_ref().map(|curve| curve.sample(sample.time))
                };
                sample
                    .values
                    .push((*id, value.unwrap_or_else(|| current.clone())));
            }
        }
        samples
    }

    fn scalar_track(
        &mut self,
        effect: &PrimPath,
        id: u32,
        frame: &crate::graph::EffectFrame,
    ) -> Option<Arc<crate::effect::ScalarTrack>> {
        let name = bae_model::effect::param_attr(id);
        let attr = self.stage.prim(effect)?.attr(&name)?;
        if !attr.is_animated() && attr.expression.is_none() {
            return None;
        }
        self.evaluator
            .integral(self.stage, effect, &name, frame, attr)
    }

    fn comp_at(&mut self, comp: &PrimPath, time: Time) -> Result<Option<RenderComp>, RenderError> {
        self.evaluator.evaluate(self.stage, comp, time).map(Some)
    }
}

/// Produces evaluated comps. Implemented by [`MockEvaluator`] and by
/// [`crate::anim::AnimEvaluator`] (the adapter over 03's `bae-anim`).
///
/// The stage is passed per call so one evaluator (with its compiled caches) can
/// live across edits; forward every [`StageEvent`] to [`CompEvaluator::invalidate`].
pub trait CompEvaluator {
    /// Evaluates `comp` of `stage` at comp time `time`.
    fn evaluate(
        &mut self,
        stage: &Stage,
        comp: &PrimPath,
        time: Time,
    ) -> Result<RenderComp, RenderError>;

    /// Evaluate a single effect attribute at composition time.
    fn attribute(
        &mut self,
        stage: &Stage,
        path: &PrimPath,
        name: &str,
        time: Time,
    ) -> Option<Value> {
        bae_anim::Evaluator::new()
            .evaluate_attribute(stage, path, name, time)
            .ok()
    }

    /// Cumulative scalar samples for effects that integrate a parameter over time.
    fn integral(
        &mut self,
        stage: &Stage,
        path: &PrimPath,
        name: &str,
        frame: &crate::graph::EffectFrame,
        attr: &AttrSpec,
    ) -> Option<Arc<crate::effect::ScalarTrack>> {
        crate::parameter_track::prepare(&mut Arc::default(), attr, frame, |t| {
            self.attribute(stage, path, name, t)?.as_f64()
        })
    }

    /// Drops whatever `event` makes stale. Default: nothing is cached.
    fn invalidate(&mut self, _event: &StageEvent) {}

    /// Problems found while evaluating since the last call (unsupported features).
    fn take_warnings(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// Evaluates a [`Stage`] with static values and linear keyframes.
#[derive(Debug)]
pub struct MockEvaluator {
    assets: AssetFiles,
    purpose: RenderPurpose,
    warnings: Vec<String>,
}

impl MockEvaluator {
    /// An evaluator whose footage paths resolve against `base_dir` (the
    /// project's directory, [`bae_model::Project::base_dir`]).
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        Self {
            assets: AssetFiles::new(base_dir),
            purpose: RenderPurpose::Final,
            warnings: Vec::new(),
        }
    }

    /// Uses `assets` to find footage files (e.g. [`AssetFiles::for_project`]).
    #[must_use]
    pub fn with_assets(mut self, assets: AssetFiles) -> Self {
        self.assets = assets;
        self
    }

    /// Preview or final (guide layers render only in a top-level preview).
    #[must_use]
    pub fn with_purpose(mut self, purpose: RenderPurpose) -> Self {
        self.purpose = purpose;
        self
    }
}

/// One evaluation over a stage.
struct MockPass<'a> {
    stage: &'a Stage,
    assets: &'a AssetFiles,
    purpose: RenderPurpose,
    warnings: &'a mut Vec<String>,
}

impl<'a> MockPass<'a> {
    fn warn(&mut self, message: String) {
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    fn comp(
        &mut self,
        path: &PrimPath,
        time: Time,
        depth: usize,
    ) -> Result<RenderComp, RenderError> {
        if depth > MAX_DEPTH {
            return Err(RenderError::Cycle {
                path: path.clone(),
                max: MAX_DEPTH,
            });
        }
        let stage = self.stage;
        let view = stage
            .prim(path)
            .filter(|v| v.is_a(&PrimType::COMP))
            .ok_or_else(|| RenderError::NotAComp(path.clone()))?;
        let width = int(&view, "width", time)
            .unwrap_or(1920)
            .clamp(0, i64::from(u32::MAX)) as u32;
        let height = int(&view, "height", time)
            .unwrap_or(1080)
            .clamp(0, i64::from(u32::MAX)) as u32;
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidSize {
                path: path.clone(),
                width,
                height,
            });
        }
        let duration = sample(&view, "duration", time)
            .and_then(|v| v.as_time())
            .unwrap_or(Time::from_flicks(10 * Time::FLICKS_PER_SECOND));
        let frame_rate = match sample(&view, "frame_rate", time) {
            Some(Value::FrameRate(r)) => r,
            _ => FrameRate::FPS_30,
        };
        let mut comp = RenderComp {
            path: path.clone(),
            time,
            width,
            height,
            pixel_aspect: float(&view, "pixel_aspect", time).unwrap_or(1.0),
            frame_rate,
            background: color(&view, "bg_color", time).unwrap_or([0.0, 0.0, 0.0, 1.0]),
            shutter: [
                float(&view, "shutter_angle", time).unwrap_or(180.0),
                float(&view, "shutter_phase", time).unwrap_or(-90.0),
            ],
            layers: Vec::new(),
            // The mock evaluator draws every layer flat and unblurred.
            camera: None,
            lights: Vec::new(),
            motion_blur: None,
        };

        let layer_paths = layer_paths(stage, path);
        let views: Vec<PrimView<'a>> = layer_paths.iter().filter_map(|p| stage.prim(p)).collect();
        let any_solo = views
            .iter()
            .any(|l| boolean(l, "solo", time).unwrap_or(false));
        for (index, layer) in views.iter().enumerate() {
            let start = sample(layer, "start_time", time)
                .and_then(|v| v.as_time())
                .unwrap_or(Time::ZERO);
            let stretch = float(layer, "stretch", time).unwrap_or(100.0);
            let local = layer_time(time, start, stretch);
            let in_point = sample(layer, "in_point", time)
                .and_then(|v| v.as_time())
                .unwrap_or(Time::ZERO);
            let out_point = sample(layer, "out_point", time)
                .and_then(|v| v.as_time())
                .unwrap_or(duration);
            let solo_ok = !any_solo || boolean(layer, "solo", time).unwrap_or(false);
            let guide = boolean(layer, "guide", time).unwrap_or(false);
            let guide_ok = !guide || (depth == 0 && self.purpose == RenderPurpose::Preview);
            let kind = string(layer, "kind", local).unwrap_or_else(|| "solid".to_owned());

            let mut out = RenderLayer::new(layer.path().clone(), LayerContent::None);
            out.time = local;
            out.start_time = start;
            out.time_scale = if stretch == 0.0 { 1.0 } else { stretch / 100.0 };
            out.in_point = in_point;
            out.active = in_point <= time && time < out_point && solo_ok && guide_ok;
            out.visible = boolean(layer, "enabled", local).unwrap_or(true);
            out.adjustment =
                kind == "adjustment" || boolean(layer, "adjustment_layer", local).unwrap_or(false);
            out.transform = self.world_transform(&views, index, time, 0);
            out.opacity =
                (float(layer, "transform:opacity", local).unwrap_or(100.0) / 100.0) as f32;
            out.blend_mode = match string(layer, "blend_mode", local) {
                Some(token) => BlendMode::from_token(&token).unwrap_or_else(|| {
                    self.warn(format!("{}: unknown blend mode `{token}`", layer.path()));
                    BlendMode::Normal
                }),
                None => BlendMode::Normal,
            };
            out.preserve_transparency =
                boolean(layer, "preserve_transparency", local).unwrap_or(false);
            out.track_matte = string(layer, "track_matte", local)
                .and_then(|t| TrackMatteMode::from_token(&t))
                .and_then(|mode| {
                    let matte_index = match sample(layer, "track_matte_layer", local) {
                        Some(Value::PrimRef(p)) => layer_paths.iter().position(|lp| *lp == p),
                        _ => index.checked_sub(1),
                    };
                    matte_index.map(|layer| TrackMatte { layer, mode })
                });
            if out.active {
                out.content = self.content(layer, &kind, local, depth)?;
                if boolean(layer, "effects_enabled", local).unwrap_or(true) {
                    out.effects = self.effects(layer, local);
                }
                out.masks = self.masks(layer, local);
            }
            if layer_has_expressions(layer) {
                self.warn(format!(
                    "{}: expressions are ignored by the mock evaluator (03 evaluates them)",
                    layer.path()
                ));
            }
            comp.layers.push(out);
        }
        Ok(comp)
    }

    /// Layer space → comp space, including the parent chain.
    fn world_transform(
        &self,
        views: &[PrimView<'_>],
        index: usize,
        time: Time,
        depth: usize,
    ) -> DAffine2 {
        let layer = &views[index];
        let start = sample(layer, "start_time", time)
            .and_then(|v| v.as_time())
            .unwrap_or(Time::ZERO);
        let stretch = float(layer, "stretch", time).unwrap_or(100.0);
        let local = layer_time(time, start, stretch);
        let anchor = vec2(layer, "transform:anchor_point", local).unwrap_or(DVec2::ZERO);
        let position = if boolean(layer, "transform:separate_dimensions", local).unwrap_or(false) {
            DVec2::new(
                float(layer, "transform:position_x", local).unwrap_or(0.0),
                float(layer, "transform:position_y", local).unwrap_or(0.0),
            )
        } else {
            vec2(layer, "transform:position", local).unwrap_or(DVec2::ZERO)
        };
        let scale = vec2(layer, "transform:scale", local).unwrap_or(DVec2::splat(100.0)) / 100.0;
        let rotation = float(layer, "transform:rotation", local)
            .unwrap_or(0.0)
            .to_radians();
        let local_matrix = DAffine2::from_translation(position)
            * DAffine2::from_angle(rotation)
            * DAffine2::from_scale(scale)
            * DAffine2::from_translation(-anchor);
        let parent = match sample(layer, "parent", local) {
            Some(Value::PrimRef(p)) => views.iter().position(|v| *v.path() == p),
            _ => None,
        };
        match parent {
            Some(p) if p != index && depth < MAX_DEPTH => {
                self.world_transform(views, p, time, depth + 1) * local_matrix
            }
            _ => local_matrix,
        }
    }

    fn content(
        &mut self,
        layer: &PrimView<'_>,
        kind: &str,
        local: Time,
        depth: usize,
    ) -> Result<LayerContent, RenderError> {
        // Adjustment layers keep their source: its alpha is their coverage.
        if matches!(kind, "null" | "camera" | "light" | "audio") {
            return Ok(LayerContent::None);
        }
        let stage = self.stage;
        let source = match sample(layer, "source", local) {
            Some(Value::PrimRef(p) | Value::AssetRef(p)) => p,
            _ => {
                if matches!(kind, "shape" | "text") {
                    self.warn(format!(
                        "{}: {kind} layers are drawn by 05 (not yet)",
                        layer.path()
                    ));
                } else {
                    self.warn(format!("{}: layer has no source", layer.path()));
                }
                return Ok(LayerContent::None);
            }
        };
        let Some(src) = stage.prim(&source) else {
            self.warn(format!("{}: source {source} does not exist", layer.path()));
            return Ok(LayerContent::None);
        };
        if src.is_a(&PrimType::COMP) {
            let nested = self.comp(&source, local, depth + 1)?;
            return Ok(LayerContent::Comp(Arc::new(nested)));
        }
        if !src.is_a(&PrimType::ASSET) {
            self.warn(format!(
                "{}: source {source} is not a comp or asset",
                layer.path()
            ));
            return Ok(LayerContent::None);
        }
        let asset_kind = string(&src, "kind", Time::ZERO).unwrap_or_else(|| "footage".to_owned());
        let w = int(&src, "width", Time::ZERO)
            .unwrap_or(0)
            .clamp(0, i64::from(u32::MAX)) as u32;
        let h = int(&src, "height", Time::ZERO)
            .unwrap_or(0)
            .clamp(0, i64::from(u32::MAX)) as u32;
        match asset_kind.as_str() {
            "solid" => Ok(LayerContent::Solid {
                color: color(&src, "solid_color", Time::ZERO).unwrap_or([0.5, 0.5, 0.5, 1.0]),
                width: w,
                height: h,
            }),
            "footage" | "sequence" => {
                let file = match self.assets.file(&src) {
                    Ok(file) => file,
                    Err(message) => {
                        self.warn(format!("{}: {message}", layer.path()));
                        None
                    }
                };
                Ok(LayerContent::Footage(FootageRef {
                    asset: source,
                    file,
                    size: (w > 0 && h > 0).then_some([w, h]),
                    source_time: local,
                    sequence: None,
                    blend: None,
                }))
            }
            _ => Ok(LayerContent::None),
        }
    }

    fn effects(&self, layer: &PrimView<'_>, local: Time) -> Vec<RenderEffect> {
        let stage = self.stage;
        children_of_type(stage, layer.path(), "effects", &PrimType::EFFECT)
            .into_iter()
            .filter_map(|path| {
                let fx = stage.prim(&path)?;
                let mut params: Vec<(u32, Value)> = fx
                    .attr_names()
                    .into_iter()
                    .filter_map(|name| {
                        let id = bae_model::effect::param_id(name)?;
                        Some((id, sample(&fx, name, local)?))
                    })
                    .collect();
                params.sort_by_key(|(id, _)| *id);
                Some(RenderEffect {
                    effect: string(&fx, "match_name", local).unwrap_or_default(),
                    enabled: boolean(&fx, "enabled", local).unwrap_or(true),
                    params,
                    path,
                })
            })
            .collect()
    }

    fn masks(&self, layer: &PrimView<'_>, local: Time) -> Vec<RenderMask> {
        let stage = self.stage;
        children_of_type(stage, layer.path(), "masks", &PrimType::MASK)
            .into_iter()
            .filter_map(|path| {
                let m = stage.prim(&path)?;
                let Some(Value::Bezier(shape)) = sample(&m, "path", local) else {
                    return None;
                };
                let feather = vec2(&m, "feather", local).unwrap_or(DVec2::ZERO);
                Some(RenderMask {
                    shape,
                    mode: string(&m, "mode", local)
                        .and_then(|t| MaskMode::from_token(&t))
                        .unwrap_or_default(),
                    inverted: boolean(&m, "inverted", local).unwrap_or(false),
                    opacity: (float(&m, "opacity", local).unwrap_or(100.0) / 100.0) as f32,
                    feather: [feather.x as f32, feather.y as f32],
                    expansion: float(&m, "expansion", local).unwrap_or(0.0) as f32,
                    path,
                })
            })
            .collect()
    }
}

impl CompEvaluator for MockEvaluator {
    fn evaluate(
        &mut self,
        stage: &Stage,
        comp: &PrimPath,
        time: Time,
    ) -> Result<RenderComp, RenderError> {
        MockPass {
            stage,
            assets: &self.assets,
            purpose: self.purpose,
            warnings: &mut self.warnings,
        }
        .comp(comp, time, 0)
    }

    fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

/// `(t − start) × 100 / stretch`, rounded to whole flicks.
pub fn layer_time(comp_time: Time, start: Time, stretch_percent: f64) -> Time {
    let offset = comp_time.saturating_sub(start);
    if stretch_percent == 100.0 || stretch_percent == 0.0 {
        return offset;
    }
    Time::from_flicks((offset.flicks() as f64 * 100.0 / stretch_percent).round() as i64)
}

/// Layer prims of a comp, top first.
pub fn layer_paths(stage: &Stage, comp: &PrimPath) -> Vec<PrimPath> {
    let mut out = Vec::new();
    for child in stage.children(comp) {
        if child.name() == "layers" {
            out.extend(
                stage
                    .children(&child)
                    .into_iter()
                    .filter(|p| stage.prim(p).is_some_and(|v| v.is_a(&PrimType::LAYER))),
            );
        } else if stage.prim(&child).is_some_and(|v| v.is_a(&PrimType::LAYER)) {
            out.push(child);
        }
    }
    out
}

fn children_of_type(
    stage: &Stage,
    parent: &PrimPath,
    folder: &str,
    ty: &PrimType,
) -> Vec<PrimPath> {
    let mut out = Vec::new();
    for child in stage.children(parent) {
        let Some(view) = stage.prim(&child) else {
            continue;
        };
        if view.is_a(ty) {
            out.push(child);
        } else if child.name() == folder {
            out.extend(
                stage
                    .children(&child)
                    .into_iter()
                    .filter(|p| stage.prim(p).is_some_and(|v| v.is_a(ty))),
            );
        }
    }
    out
}

fn layer_has_expressions(layer: &PrimView<'_>) -> bool {
    layer
        .attr_names()
        .into_iter()
        .filter_map(|n| layer.attr(n))
        .any(|a| a.expression.as_ref().is_some_and(|e| e.enabled))
}

/// The value of `attr` at `t`: keyframes (linear; hold honoured), else the static
/// value, else the schema default.
pub fn sample(view: &PrimView<'_>, attr: &str, t: Time) -> Option<Value> {
    match view.attr(attr) {
        Some(spec) if !spec.keyframes.is_empty() => interpolate(spec, t),
        Some(spec) if spec.value.is_some() => spec.value.clone(),
        _ => view
            .prim_type()
            .and_then(|ty| schema::attr_def(ty, attr))
            .and_then(|def| def.default.to_value()),
    }
}

fn interpolate(spec: &AttrSpec, t: Time) -> Option<Value> {
    let before = spec.keyframes.range(..=t).next_back();
    let after = spec
        .keyframes
        .range((std::ops::Bound::Excluded(t), std::ops::Bound::Unbounded))
        .next();
    match (before, after) {
        (Some((_, k)), None) => Some(k.value.clone()),
        (None, Some((_, k))) => Some(k.value.clone()),
        (None, None) => None,
        (Some((t0, k0)), Some((t1, k1))) => {
            if k0.interp_out == Interpolation::Hold {
                return Some(k0.value.clone());
            }
            let f = (t - *t0).flicks() as f64 / (*t1 - *t0).flicks() as f64;
            Some(lerp_value(&k0.value, &k1.value, f))
        }
    }
}

fn lerp_value(a: &Value, b: &Value, f: f64) -> Value {
    let l = |x: f64, y: f64| x + (y - x) * f;
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => Value::Float(l(*x, *y)),
        (Value::Int(x), Value::Int(y)) => Value::Float(l(*x as f64, *y as f64)),
        (Value::Vec2(x), Value::Vec2(y)) => Value::Vec2([l(x[0], y[0]), l(x[1], y[1])]),
        (Value::Vec3(x), Value::Vec3(y)) => {
            Value::Vec3([l(x[0], y[0]), l(x[1], y[1]), l(x[2], y[2])])
        }
        (Value::Color(x), Value::Color(y)) => {
            Value::Color([l(x[0], y[0]), l(x[1], y[1]), l(x[2], y[2]), l(x[3], y[3])])
        }
        _ => a.clone(),
    }
}

fn float(view: &PrimView<'_>, attr: &str, t: Time) -> Option<f64> {
    sample(view, attr, t)?.as_f64()
}

fn int(view: &PrimView<'_>, attr: &str, t: Time) -> Option<i64> {
    match sample(view, attr, t)? {
        Value::Int(i) => Some(i),
        Value::Float(f) => Some(f.round() as i64),
        _ => None,
    }
}

fn boolean(view: &PrimView<'_>, attr: &str, t: Time) -> Option<bool> {
    sample(view, attr, t)?.as_bool()
}

fn string(view: &PrimView<'_>, attr: &str, t: Time) -> Option<String> {
    sample(view, attr, t)?.as_str().map(str::to_owned)
}

fn vec2(view: &PrimView<'_>, attr: &str, t: Time) -> Option<DVec2> {
    match sample(view, attr, t)? {
        Value::Vec2([x, y]) | Value::Vec3([x, y, _]) => Some(DVec2::new(x, y)),
        Value::Float(v) => Some(DVec2::splat(v)),
        _ => None,
    }
}

fn color(view: &PrimView<'_>, attr: &str, t: Time) -> Option<[f32; 4]> {
    match sample(view, attr, t)? {
        Value::Color(c) => Some(c.map(|v| v as f32)),
        _ => None,
    }
}
