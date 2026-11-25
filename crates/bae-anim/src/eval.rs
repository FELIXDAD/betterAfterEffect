//! Comp evaluation: compile a comp once (resolve opinions, decode keyframes into
//! [`Curve`]s, resolve parents and mattes), then sample it at any time.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use bae_model::{
    AttrSpec, BezierPath, FrameRate, LayerKind, PrimPath, PrimType, PrimView, Stage, StageEvent,
    Time, Value,
};
use glam::{DMat4, DVec3};

use rquickjs::Ctx;

use crate::curve::Curve;
use crate::error::EvalError;
use crate::evaluated::{
    AssetInfo, CameraState, EvaluatedComp, EvaluatedEffect, EvaluatedLayer, EvaluatedMask,
    EvaluatedPrim, FrameBlending, LayerSource, LayerSwitches, LightFalloff, LightState, LightType,
    TrackMatte,
};
use crate::expr;
use crate::schema::{
    self, asset, camera as cam, comp as c, effect as fx, layer as l, light as li, mask as mk,
    transform as tr,
};
use crate::time::{LayerTiming, MotionBlur, frame_rate_from_value, nested_comp_time};
use crate::transform::{AutoOrientRotation, Transform, look_along};

/// An animatable attribute, compiled.
#[derive(Clone, Debug)]
pub(crate) enum Prop {
    Static(Value),
    Curve(Box<Curve>),
    /// An enabled expression on top of a static value or keyframes.
    Expr(Box<ExprProp>),
}

/// An attribute with an expression: the pre-expression value and the source, plus the
/// attribute's identity (memo key, error reports).
#[derive(Clone, Debug)]
pub(crate) struct ExprProp {
    pub base: Prop,
    pub source: Arc<str>,
    pub prim: PrimPath,
    pub attr: Arc<str>,
    /// Range the result is clamped to (Opacity), as after interpolation.
    pub clamp: Option<(f64, f64)>,
    /// The source compiled for the Rust fast path, when it is one of its shapes.
    pub fast: Option<crate::expr::fast::Fast>,
    /// Random seed state after `prim` and `attr` (see `expr::seed_prefix`).
    pub seed: u32,
}

/// Where and when a property is sampled.
#[derive(Clone, Copy)]
pub(crate) struct Cx<'a, 'js> {
    /// Comp id in the expression scope (0 when there is none).
    pub comp: usize,
    /// Layer index in the comp.
    pub layer: usize,
    /// Comp time.
    pub ct: Time,
    /// Layer time (the keyframes' time base).
    pub lt: Time,
    /// The JavaScript context when expressions run; `None` evaluates every
    /// expression's pre-expression value.
    pub js: Option<&'a Ctx<'js>>,
}

impl Cx<'_, '_> {
    /// The same layer at another layer time.
    pub(crate) fn at_layer_time(&self, lt: Time, timing: &LayerTiming) -> Self {
        Cx {
            lt,
            ct: timing.comp_time(lt),
            ..*self
        }
    }
}

impl Prop {
    /// Compile the winning opinion of `attr` on `view`, falling back to the schema
    /// default of `prim_type`, then to `fallback`.
    fn read(
        view: &PrimView<'_>,
        prim_type: Option<&PrimType>,
        attr: &str,
        fallback: Value,
    ) -> Prop {
        match view.attr(attr) {
            Some(spec) => Prop::from_spec(spec, prim_type, view.path(), attr)
                .unwrap_or_else(|| Prop::Static(default_of(prim_type, attr).unwrap_or(fallback))),
            None => Prop::Static(default_of(prim_type, attr).unwrap_or(fallback)),
        }
    }

    fn from_spec(
        spec: &AttrSpec,
        prim_type: Option<&PrimType>,
        prim: &PrimPath,
        attr: &str,
    ) -> Option<Prop> {
        let base = Self::from_spec_no_expr(spec, prim_type, attr)
            .or_else(|| default_of(prim_type, attr).map(Prop::Static));
        match &spec.expression {
            Some(e) if e.enabled && !e.source.trim().is_empty() => {
                Some(Prop::Expr(Box::new(ExprProp {
                    base: base.unwrap_or(Prop::Static(Value::Float(0.0))),
                    source: e.source.as_str().into(),
                    prim: prim.clone(),
                    attr: attr.into(),
                    clamp: schema::clamp_range(prim_type, attr),
                    fast: crate::expr::fast::Fast::compile(&e.source),
                    seed: crate::expr::seed_prefix(prim.as_str(), attr),
                })))
            }
            _ => base,
        }
    }

    fn from_spec_no_expr(
        spec: &AttrSpec,
        prim_type: Option<&PrimType>,
        attr: &str,
    ) -> Option<Prop> {
        if let Some(curve) = Curve::new(&spec.keyframes, schema::curve_kind(attr, spec)) {
            let curve = match schema::clamp_range(prim_type, attr) {
                Some((lo, hi)) => curve.with_clamp(lo, hi),
                None => curve,
            };
            return Some(Prop::Curve(Box::new(curve)));
        }
        spec.value.clone().map(Prop::Static)
    }

    /// Value at `cx` (expressions applied when `cx.js` is set).
    pub(crate) fn sample(&self, cx: &Cx<'_, '_>) -> Value {
        match self {
            Prop::Static(v) => v.clone(),
            Prop::Curve(c) => c.sample(cx.lt),
            Prop::Expr(e) => match cx.js {
                Some(ctx) => crate::expr::evaluate(ctx, cx, e),
                None => e.base.sample(cx),
            },
        }
    }

    /// Value before expressions.
    pub(crate) fn sample_pre(&self, cx: &Cx<'_, '_>) -> Value {
        match self {
            Prop::Expr(e) => e.base.sample(cx),
            other => other.sample(cx),
        }
    }

    pub(crate) fn curve(&self) -> Option<&Curve> {
        match self {
            Prop::Curve(c) => Some(c),
            Prop::Expr(e) => e.base.curve(),
            Prop::Static(_) => None,
        }
    }

    fn has_expression(&self) -> bool {
        matches!(self, Prop::Expr(_))
    }

    fn f64(&self, cx: &Cx<'_, '_>) -> f64 {
        num(&self.sample(cx))
    }

    fn vec3(&self, cx: &Cx<'_, '_>) -> DVec3 {
        vec3(&self.sample(cx))
    }
}

fn default_of(prim_type: Option<&PrimType>, attr: &str) -> Option<Value> {
    schema::default_value(prim_type?, attr)
}

pub(crate) fn num(v: &Value) -> f64 {
    match v {
        Value::Float(x) => *x,
        Value::Int(x) => *x as f64,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Time(t) => t.as_seconds(),
        Value::Vec2(x) => x[0],
        Value::Vec3(x) => x[0],
        _ => 0.0,
    }
}

fn vec3(v: &Value) -> DVec3 {
    match v {
        Value::Vec3(x) => DVec3::from_array(*x),
        Value::Vec2(x) => DVec3::new(x[0], x[1], 0.0),
        Value::Float(x) => DVec3::new(*x, 0.0, 0.0),
        Value::Int(x) => DVec3::new(*x as f64, 0.0, 0.0),
        _ => DVec3::ZERO,
    }
}

/// Static (non-animatable) setting: the winning opinion's value or the schema default.
fn setting(view: &PrimView<'_>, prim_type: &PrimType, attr: &str) -> Option<Value> {
    view.value(attr)
        .cloned()
        .or_else(|| schema::default_value(prim_type, attr))
}

fn setting_bool(view: &PrimView<'_>, prim_type: &PrimType, attr: &str) -> bool {
    setting(view, prim_type, attr)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn setting_str(view: &PrimView<'_>, prim_type: &PrimType, attr: &str) -> Option<String> {
    setting(view, prim_type, attr).and_then(|v| v.as_str().map(str::to_owned))
}

fn setting_time(view: &PrimView<'_>, prim_type: &PrimType, attr: &str) -> Option<Time> {
    match setting(view, prim_type, attr)? {
        Value::Time(t) => Some(t),
        Value::Int(f) => Some(Time(f)),
        Value::Float(s) => Some(Time::from_seconds(s)),
        _ => None,
    }
}

fn setting_f64(view: &PrimView<'_>, prim_type: &PrimType, attr: &str) -> Option<f64> {
    setting(view, prim_type, attr).and_then(|v| v.as_f64())
}

fn setting_ref(view: &PrimView<'_>, attr: &str) -> Option<PrimPath> {
    view.value(attr).and_then(|v| v.as_prim_ref()).cloned()
}

// ---------------------------------------------------------------------------
// Compiled comp
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) struct CompiledComp {
    pub(crate) path: PrimPath,
    /// Display name (`name` attribute, falling back to the prim name).
    pub(crate) name: String,
    /// Some property of some layer has an enabled expression.
    pub(crate) has_expressions: bool,
    /// Some of them need JavaScript (not the Rust fast path), so they are
    /// warmed up before the pass (see `expr::warm_up`).
    pub(crate) has_js_expressions: bool,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixel_aspect: f64,
    pub(crate) frame_rate: FrameRate,
    pub(crate) duration: Time,
    pub(crate) bg_color: [f64; 4],
    pub(crate) motion_blur: MotionBlur,
    pub(crate) frame_blending: bool,
    pub(crate) layers: Vec<CompiledLayer>,
    /// Prims whose changes invalidate this compiled comp (the comp and its sources).
    pub(crate) deps: Vec<PrimPath>,
}

#[derive(Debug)]
pub(crate) enum Position {
    Combined(Prop),
    Separate(Box<[Prop; 3]>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum AutoOrient {
    Off,
    AlongPath,
    /// Cameras/lights: towards the point of interest; 3D AV layers: towards camera.
    Towards,
}

#[derive(Debug)]
pub(crate) struct CompiledTransform {
    pub(crate) anchor: Prop,
    pub(crate) position: Position,
    pub(crate) scale: Prop,
    pub(crate) orientation: Prop,
    pub(crate) rotation_x: Prop,
    pub(crate) rotation_y: Prop,
    pub(crate) rotation: Prop,
    pub(crate) opacity: Prop,
    pub(crate) point_of_interest: Prop,
}

#[derive(Debug)]
pub(crate) enum CompiledSource {
    None,
    Asset(PrimPath, AssetInfo),
    Comp {
        path: PrimPath,
        rate: FrameRate,
        preserve: bool,
    },
}

#[derive(Debug)]
pub(crate) struct CompiledMask {
    pub(crate) path: PrimPath,
    pub(crate) name: String,
    pub(crate) mode: String,
    pub(crate) inverted: bool,
    pub(crate) shape: Prop,
    pub(crate) feather: Prop,
    pub(crate) opacity: Prop,
    pub(crate) expansion: Prop,
}

#[derive(Debug)]
pub(crate) struct CompiledEffect {
    pub(crate) path: PrimPath,
    pub(crate) name: String,
    pub(crate) match_name: String,
    pub(crate) enabled: bool,
    pub(crate) params: Vec<(String, Prop)>,
    /// Child prims (the Paint effect's strokes).
    pub(crate) children: Vec<CompiledPrim>,
}

#[derive(Debug)]
pub(crate) struct CompiledPrim {
    pub(crate) path: PrimPath,
    pub(crate) prim_type: Option<PrimType>,
    pub(crate) attrs: Vec<(String, Prop)>,
    pub(crate) children: Vec<CompiledPrim>,
    /// Its evaluated form when nothing in it animates: every frame shares it
    /// (1000 shape layers used to rebuild every fill and path each frame).
    pub(crate) fixed: Option<EvaluatedPrim>,
}

/// A text layer's Path Options (AE: Text > Path Options), compiled.
#[derive(Debug)]
pub(crate) struct CompiledTextPath {
    pub(crate) mask: PrimPath,
    pub(crate) reverse: bool,
    pub(crate) perpendicular: bool,
    pub(crate) force_alignment: bool,
    pub(crate) first_margin: Prop,
    pub(crate) last_margin: Prop,
}

#[derive(Debug)]
pub(crate) struct CompiledCamera {
    pub(crate) zoom: Prop,
    pub(crate) depth_of_field: bool,
    pub(crate) focus_distance: Prop,
    pub(crate) aperture: Prop,
    pub(crate) blur_level: Prop,
}

#[derive(Debug)]
pub(crate) struct CompiledLight {
    pub(crate) light_type: LightType,
    pub(crate) intensity: Prop,
    pub(crate) color: Prop,
    pub(crate) cone_angle: Prop,
    pub(crate) cone_feather: Prop,
    pub(crate) falloff: LightFalloff,
    pub(crate) radius: Prop,
    pub(crate) falloff_distance: Prop,
    pub(crate) casts_shadows: bool,
    pub(crate) shadow_darkness: Prop,
    pub(crate) shadow_diffusion: Prop,
}

#[derive(Debug)]
pub(crate) struct CompiledLayer {
    pub(crate) path: PrimPath,
    /// Shared with every frame's evaluated layer (no per-frame copies).
    pub(crate) name: Arc<str>,
    pub(crate) kind: LayerKind,
    pub(crate) timing: LayerTiming,
    pub(crate) switches: LayerSwitches,
    pub(crate) source: CompiledSource,
    pub(crate) time_remap: Option<Prop>,
    pub(crate) parent: Option<usize>,
    pub(crate) blend_mode: Arc<str>,
    pub(crate) track_matte: TrackMatte,
    pub(crate) track_matte_layer: Option<usize>,
    pub(crate) auto_orient: AutoOrient,
    pub(crate) transform: CompiledTransform,
    pub(crate) masks: Vec<CompiledMask>,
    pub(crate) effects: Vec<CompiledEffect>,
    pub(crate) contents: Vec<CompiledPrim>,
    /// `contents` evaluated, when nothing in them animates.
    pub(crate) fixed_contents: Option<Arc<[EvaluatedPrim]>>,
    pub(crate) text: Option<Prop>,
    pub(crate) text_animators: Vec<CompiledPrim>,
    /// `text_animators` evaluated, when nothing in them animates.
    pub(crate) fixed_animators: Option<Arc<[EvaluatedPrim]>>,
    pub(crate) text_path: Option<CompiledTextPath>,
    pub(crate) camera: Option<CompiledCamera>,
    pub(crate) light: Option<CompiledLight>,
}

fn layer_kind(token: &str) -> LayerKind {
    match token {
        "shape" => LayerKind::Shape,
        "text" => LayerKind::Text,
        "solid" => LayerKind::Solid,
        "footage" => LayerKind::Footage,
        "null" => LayerKind::Null,
        "camera" => LayerKind::Camera,
        "light" => LayerKind::Light,
        "adjustment" => LayerKind::Adjustment,
        "pre_comp" => LayerKind::PreComp,
        "audio" => LayerKind::Audio,
        _ => LayerKind::Solid,
    }
}

/// Layers of a comp, top first: the children of its `layers` folder that are `Layer`s.
fn comp_layers(stage: &Stage, comp: &PrimPath) -> Vec<PrimPath> {
    let Ok(folder) = comp.child(c::LAYERS) else {
        return Vec::new();
    };
    stage
        .children(&folder)
        .into_iter()
        .filter(|p| stage.prim(p).is_some_and(|v| v.is_a(&PrimType::LAYER)))
        .collect()
}

/// Timing of a layer prim (used by `evaluate_attribute` without compiling the comp).
fn layer_timing(view: &PrimView<'_>, comp_duration: Time) -> LayerTiming {
    let t = &PrimType::LAYER;
    LayerTiming {
        start_time: setting_time(view, t, l::START_TIME).unwrap_or(Time::ZERO),
        stretch: setting_f64(view, t, l::STRETCH).unwrap_or(100.0),
        in_point: setting_time(view, t, l::IN_POINT).unwrap_or(Time::ZERO),
        out_point: setting_time(view, t, l::OUT_POINT).unwrap_or(comp_duration),
    }
}

fn comp_duration(view: &PrimView<'_>) -> Time {
    setting_time(view, &PrimType::COMP, c::DURATION).unwrap_or(Time::from_seconds(10.0))
}

fn compile_comp(stage: &Stage, path: &PrimPath) -> Result<CompiledComp, EvalError> {
    let view = stage
        .prim(path)
        .ok_or_else(|| EvalError::NoSuchPrim(path.clone()))?;
    if !view.is_a(&PrimType::COMP) {
        return Err(EvalError::NotAComp(path.clone()));
    }
    let ct = &PrimType::COMP;
    let int = |attr: &str, d: f64| setting_f64(&view, ct, attr).unwrap_or(d);
    let duration = comp_duration(&view);
    let frame_rate = setting(&view, ct, c::FRAME_RATE)
        .and_then(|v| match v {
            Value::FrameRate(r) => Some(r),
            other => frame_rate_from_value(&other),
        })
        .unwrap_or(FrameRate::FPS_30);
    let bg_color = match setting(&view, ct, c::BG_COLOR) {
        Some(Value::Color(c)) => c,
        _ => [0.0, 0.0, 0.0, 1.0],
    };
    let motion_blur = MotionBlur {
        enabled: setting_bool(&view, ct, c::MOTION_BLUR),
        shutter_angle: int(c::SHUTTER_ANGLE, 180.0),
        shutter_phase: int(c::SHUTTER_PHASE, -90.0),
        samples: int(c::MOTION_BLUR_SAMPLES, 16.0).max(1.0) as u32,
        adaptive_limit: int(c::MOTION_BLUR_ADAPTIVE_LIMIT, 128.0).clamp(16.0, 256.0) as u32,
    };

    let paths = comp_layers(stage, path);
    let index_of = |p: &PrimPath| paths.iter().position(|q| q == p);
    let mut deps = vec![path.clone()];
    let mut layers = Vec::with_capacity(paths.len());
    for (i, lp) in paths.iter().enumerate() {
        let lv = stage.prim(lp).expect("listed layer exists");
        let mut layer = compile_layer(stage, &lv, duration)?;
        layer.parent = setting_ref(&lv, l::PARENT)
            .and_then(|p| index_of(&p))
            .filter(|&p| p != i);
        if layer.track_matte != TrackMatte::None {
            layer.track_matte_layer = match setting_ref(&lv, l::TRACK_MATTE_LAYER) {
                Some(p) => index_of(&p),
                None => i.checked_sub(1),
            };
        }
        if let CompiledSource::Comp { path, .. } = &layer.source {
            deps.push(path.clone());
        }
        layers.push(layer);
    }

    let has_expressions = layers.iter().any(CompiledLayer::has_expressions);
    let has_js_expressions = has_expressions
        && layers
            .iter()
            .any(|l| l.expr_props().iter().any(|e| e.fast.is_none()));
    Ok(CompiledComp {
        name: view
            .value(schema::layer::NAME)
            .and_then(Value::as_str)
            .unwrap_or(view.name())
            .to_owned(),
        has_expressions,
        has_js_expressions,
        path: path.clone(),
        width: int(c::WIDTH, 1920.0).max(0.0) as u32,
        height: int(c::HEIGHT, 1080.0).max(0.0) as u32,
        pixel_aspect: int(c::PIXEL_ASPECT, 1.0),
        frame_rate,
        duration,
        bg_color,
        motion_blur,
        frame_blending: setting_bool(&view, ct, c::FRAME_BLENDING),
        layers,
        deps,
    })
}

fn compile_layer(
    stage: &Stage,
    v: &PrimView<'_>,
    comp_duration: Time,
) -> Result<CompiledLayer, EvalError> {
    let lt = &PrimType::LAYER;
    let ty = Some(lt);
    let kind = layer_kind(setting_str(v, lt, l::KIND).as_deref().unwrap_or("solid"));
    let three_d = setting_bool(v, lt, l::THREE_D);
    let switches = LayerSwitches {
        enabled: setting(v, lt, l::ENABLED)
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        effects: setting(v, lt, l::EFFECTS_ENABLED)
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        solo: setting_bool(v, lt, l::SOLO),
        shy: setting_bool(v, lt, l::SHY),
        guide: setting_bool(v, lt, l::GUIDE),
        adjustment: setting_bool(v, lt, l::ADJUSTMENT_LAYER) || kind == LayerKind::Adjustment,
        three_d: three_d || matches!(kind, LayerKind::Camera | LayerKind::Light),
        motion_blur: setting_bool(v, lt, l::MOTION_BLUR),
        collapse_transformation: setting_bool(v, lt, l::COLLAPSE_TRANSFORMATION),
        preserve_transparency: setting_bool(v, lt, l::PRESERVE_TRANSPARENCY),
        frame_blending: match setting_str(v, lt, l::FRAME_BLENDING).as_deref() {
            Some("frame_mix") => FrameBlending::FrameMix,
            Some("pixel_motion") => FrameBlending::PixelMotion,
            _ => FrameBlending::Off,
        },
    };

    let source = match setting_ref(v, l::SOURCE) {
        Some(p) => match stage.prim(&p) {
            Some(sv) if sv.is_a(&PrimType::COMP) => CompiledSource::Comp {
                rate: setting(&sv, &PrimType::COMP, c::FRAME_RATE)
                    .and_then(|v| match v {
                        Value::FrameRate(r) => Some(r),
                        other => frame_rate_from_value(&other),
                    })
                    .unwrap_or(FrameRate::FPS_30),
                preserve: setting_bool(&sv, &PrimType::COMP, c::PRESERVE_FRAME_RATE),
                path: p,
            },
            Some(av) => {
                let info = asset_info(&av);
                CompiledSource::Asset(p, info)
            }
            None => CompiledSource::Asset(
                p,
                AssetInfo {
                    kind: "missing".into(),
                    width: 0,
                    height: 0,
                    pixel_aspect: 1.0,
                    frame_rate: None,
                    duration: None,
                    solid_color: None,
                },
            ),
        },
        None => CompiledSource::None,
    };
    let time_remap = setting_bool(v, lt, l::TIME_REMAP_ENABLED)
        .then(|| {
            v.attr(l::TIME_REMAP)
                .and_then(|s| Prop::from_spec(s, ty, v.path(), l::TIME_REMAP))
        })
        .flatten();

    // Only an authored opinion counts here: without one, AE's per-kind default applies.
    let auto_orient = match v.value(l::AUTO_ORIENT).and_then(Value::as_str) {
        Some("along_path") => AutoOrient::AlongPath,
        Some("towards_point_of_interest") => AutoOrient::Towards,
        Some(_) => AutoOrient::Off,
        // No opinion: two-node cameras and spot/parallel lights aim at their POI.
        None => match kind {
            LayerKind::Camera if setting_str(v, lt, cam::TYPE).as_deref() != Some("one_node") => {
                AutoOrient::Towards
            }
            LayerKind::Light
                if matches!(
                    setting_str(v, lt, li::TYPE).as_deref(),
                    Some("spot") | Some("parallel") | None
                ) =>
            {
                AutoOrient::Towards
            }
            _ => AutoOrient::Off,
        },
    };

    let p = |attr: &str, fallback: Value| Prop::read(v, ty, attr, fallback);
    let position = if setting_bool(v, lt, tr::SEPARATE_DIMENSIONS) {
        Position::Separate(Box::new([
            p(tr::POSITION_X, Value::Float(0.0)),
            p(tr::POSITION_Y, Value::Float(0.0)),
            p(tr::POSITION_Z, Value::Float(0.0)),
        ]))
    } else {
        Position::Combined(p(tr::POSITION, Value::Vec3([0.0; 3])))
    };
    let transform = CompiledTransform {
        anchor: p(tr::ANCHOR_POINT, Value::Vec3([0.0; 3])),
        position,
        scale: p(tr::SCALE, Value::Vec3([100.0; 3])),
        orientation: p(tr::ORIENTATION, Value::Vec3([0.0; 3])),
        rotation_x: p(tr::ROTATION_X, Value::Float(0.0)),
        rotation_y: p(tr::ROTATION_Y, Value::Float(0.0)),
        rotation: p(tr::ROTATION, Value::Float(0.0)),
        opacity: p(tr::OPACITY, Value::Float(100.0)),
        point_of_interest: p(tr::POINT_OF_INTEREST, Value::Vec3([0.0; 3])),
    };

    let camera = (kind == LayerKind::Camera).then(|| CompiledCamera {
        zoom: p(cam::ZOOM, Value::Float(1777.8)),
        depth_of_field: setting_bool(v, lt, cam::DEPTH_OF_FIELD),
        focus_distance: p(cam::FOCUS_DISTANCE, Value::Float(1777.8)),
        aperture: p(cam::APERTURE, Value::Float(25.3)),
        blur_level: p(cam::BLUR_LEVEL, Value::Float(100.0)),
    });
    let light = (kind == LayerKind::Light).then(|| CompiledLight {
        light_type: match setting_str(v, lt, li::TYPE).as_deref() {
            Some("parallel") => LightType::Parallel,
            Some("point") => LightType::Point,
            Some("ambient") => LightType::Ambient,
            Some("environment") => LightType::Environment,
            _ => LightType::Spot,
        },
        intensity: p(li::INTENSITY, Value::Float(100.0)),
        color: p(li::COLOR, Value::Color([1.0; 4])),
        cone_angle: p(li::CONE_ANGLE, Value::Float(90.0)),
        cone_feather: p(li::CONE_FEATHER, Value::Float(50.0)),
        falloff: match setting_str(v, lt, li::FALLOFF).as_deref() {
            Some("smooth") => LightFalloff::Smooth,
            Some("inverse_square_clamped") => LightFalloff::InverseSquareClamped,
            _ => LightFalloff::None,
        },
        radius: p(li::RADIUS, Value::Float(500.0)),
        falloff_distance: p(li::FALLOFF_DISTANCE, Value::Float(500.0)),
        casts_shadows: setting_bool(v, lt, li::CASTS_SHADOWS),
        shadow_darkness: p(li::SHADOW_DARKNESS, Value::Float(100.0)),
        shadow_diffusion: p(li::SHADOW_DIFFUSION, Value::Float(0.0)),
    });

    let lpath = v.path().clone();
    let child_prims = |folder: &str| -> Vec<PrimPath> {
        lpath
            .child(folder)
            .map(|f| stage.children(&f))
            .unwrap_or_default()
    };
    let masks = child_prims(l::MASKS)
        .into_iter()
        .filter_map(|mp| {
            let mv = stage.prim(&mp)?;
            if !mv.is_a(&PrimType::MASK) {
                return None;
            }
            let mt = &PrimType::MASK;
            let mty = Some(mt);
            Some(CompiledMask {
                name: mv
                    .value(schema::effect::NAME)
                    .and_then(Value::as_str)
                    .unwrap_or(mv.name())
                    .to_owned(),
                mode: setting_str(&mv, mt, mk::MODE).unwrap_or_else(|| "add".into()),
                inverted: setting_bool(&mv, mt, mk::INVERTED),
                shape: Prop::read(&mv, mty, mk::PATH, Value::Bezier(BezierPath::default())),
                feather: Prop::read(&mv, mty, mk::FEATHER, Value::Vec2([0.0; 2])),
                opacity: Prop::read(&mv, mty, mk::OPACITY, Value::Float(100.0)),
                expansion: Prop::read(&mv, mty, mk::EXPANSION, Value::Float(0.0)),
                path: mp,
            })
        })
        .collect();
    let effects = child_prims(l::EFFECTS)
        .into_iter()
        .filter_map(|ep| {
            let ev = stage.prim(&ep)?;
            if !ev.is_a(&PrimType::EFFECT) {
                return None;
            }
            let et = &PrimType::EFFECT;
            let params = ev
                .attr_names()
                .into_iter()
                .filter_map(|name| {
                    let id = name.strip_prefix(fx::PARAM_PREFIX)?;
                    let spec = ev.attr(name)?;
                    Some((id.to_owned(), Prop::from_spec(spec, Some(et), &ep, name)?))
                })
                .collect();
            Some(CompiledEffect {
                name: ev
                    .value(fx::NAME)
                    .and_then(Value::as_str)
                    .unwrap_or(ev.name())
                    .to_owned(),
                match_name: setting_str(&ev, et, fx::MATCH_NAME).unwrap_or_default(),
                enabled: setting(&ev, et, fx::ENABLED)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                params,
                children: stage
                    .children(&ep)
                    .into_iter()
                    .filter_map(|cp| compile_generic(stage, &cp, 0))
                    .collect(),
                path: ep,
            })
        })
        .collect();
    let contents: Vec<CompiledPrim> = child_prims(l::CONTENTS)
        .into_iter()
        .filter_map(|cp| compile_generic(stage, &cp, 0))
        .collect();
    let text = (kind == LayerKind::Text)
        .then(|| {
            v.attr(l::TEXT_SOURCE)
                .and_then(|s| Prop::from_spec(s, ty, v.path(), l::TEXT_SOURCE))
        })
        .flatten();
    let text_animators: Vec<CompiledPrim> = child_prims(l::ANIMATORS)
        .into_iter()
        .filter_map(|cp| compile_generic(stage, &cp, 0))
        .collect();
    let text_path = (kind == LayerKind::Text)
        .then(|| setting_ref(v, l::TEXT_PATH))
        .flatten()
        .map(|mask| CompiledTextPath {
            mask,
            reverse: setting_bool(v, lt, l::TEXT_REVERSE_PATH),
            perpendicular: setting(v, lt, l::TEXT_PERPENDICULAR_TO_PATH)
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            force_alignment: setting_bool(v, lt, l::TEXT_FORCE_ALIGNMENT),
            first_margin: p(l::TEXT_FIRST_MARGIN, Value::Float(0.0)),
            last_margin: p(l::TEXT_LAST_MARGIN, Value::Float(0.0)),
        });

    Ok(CompiledLayer {
        name: v
            .value(l::NAME)
            .and_then(|n| n.as_str())
            .unwrap_or(v.name())
            .into(),
        kind,
        timing: layer_timing(v, comp_duration),
        switches,
        source,
        time_remap,
        parent: None,
        blend_mode: setting_str(v, lt, l::BLEND_MODE)
            .unwrap_or_else(|| "normal".into())
            .into(),
        track_matte: TrackMatte::from_token(
            setting_str(v, lt, l::TRACK_MATTE)
                .as_deref()
                .unwrap_or("none"),
        ),
        track_matte_layer: None,
        auto_orient,
        transform,
        masks,
        effects,
        fixed_contents: fixed_list(&contents),
        contents,
        text,
        fixed_animators: fixed_list(&text_animators),
        text_animators,
        text_path,
        camera,
        light,
        path: lpath,
    })
}

fn asset_info(av: &PrimView<'_>) -> AssetInfo {
    let at = &PrimType::ASSET;
    let kind = setting_str(av, at, asset::KIND).unwrap_or_else(|| "footage".into());
    let dim = |attr: &str| setting_f64(av, at, attr).unwrap_or(0.0).max(0.0) as u32;
    AssetInfo {
        width: dim(asset::WIDTH),
        height: dim(asset::HEIGHT),
        pixel_aspect: setting_f64(av, at, asset::PIXEL_ASPECT).unwrap_or(1.0),
        frame_rate: setting(av, at, asset::FRAME_RATE).and_then(|v| match v {
            Value::FrameRate(r) => Some(r),
            other => frame_rate_from_value(&other),
        }),
        duration: setting_time(av, at, asset::DURATION),
        solid_color: (kind == "solid").then(|| match setting(av, at, asset::SOLID_COLOR) {
            Some(Value::Color(c)) => c,
            _ => [0.5, 0.5, 0.5, 1.0],
        }),
        kind,
    }
}

/// Shape contents (and any other prim tree under a layer): every attribute compiled,
/// children recursively (prim folders such as a group's `contents` are flattened in).
fn compile_generic(stage: &Stage, path: &PrimPath, depth: usize) -> Option<CompiledPrim> {
    if depth > 64 {
        return None;
    }
    let v = stage.prim(path)?;
    let prim_type = v.prim_type().cloned();
    let mut names: Vec<&str> = v.attr_names();
    if let Some(t) = &prim_type
        && t != &PrimType::TEXT_ANIMATOR
    {
        // Animator properties are optional: only authored ones affect the text.
        // Animatable schema attributes without an opinion still get their default, so
        // consumers see a complete set.
        for d in bae_model::schema::attr_defs(t) {
            if d.animatable && !names.contains(&d.name) {
                names.push(d.name);
            }
        }
    }
    let attrs = names
        .into_iter()
        .filter_map(|name| {
            let prop = match v.attr(name) {
                Some(spec) => Prop::from_spec(spec, prim_type.as_ref(), path, name)?,
                None => Prop::Static(default_of(prim_type.as_ref(), name)?),
            };
            Some((name.to_owned(), prop))
        })
        .collect();
    let children = stage
        .children(path)
        .into_iter()
        .filter_map(|cp| compile_generic(stage, &cp, depth + 1))
        .collect();
    let mut prim = CompiledPrim {
        path: path.clone(),
        prim_type,
        attrs,
        children,
        fixed: None,
    };
    prim.fixed = prim.fixed_form();
    Some(prim)
}

/// `prims` evaluated, shared as is when none of them animates.
pub(crate) fn sample_prims(
    prims: &[CompiledPrim],
    fixed: Option<&Arc<[EvaluatedPrim]>>,
    cx: &Cx<'_, '_>,
) -> Arc<[EvaluatedPrim]> {
    match fixed {
        Some(f) => f.clone(),
        None => prims.iter().map(|p| sample_generic(p, cx)).collect(),
    }
}

/// The evaluated form of `prims` when none of them animates.
fn fixed_list(prims: &[CompiledPrim]) -> Option<Arc<[EvaluatedPrim]>> {
    prims
        .iter()
        .map(|p| p.fixed.clone())
        .collect::<Option<Vec<_>>>()
        .map(Into::into)
}

// ---------------------------------------------------------------------------
// Sampling
// ---------------------------------------------------------------------------

impl CompiledComp {
    pub(crate) fn evaluate(
        &self,
        time: Time,
        comp_id: usize,
        js: Option<&Ctx<'_>>,
    ) -> EvaluatedComp {
        let n = self.layers.len();
        let has_solo = self
            .layers
            .iter()
            .any(|l| l.switches.solo && l.switches.enabled);

        // Expressions: memoise them first so long chains stay shallow (see
        // warm_up). Fast-path ones read only their own property: no chains.
        if js.is_some() && self.has_js_expressions {
            let props: Vec<(usize, &ExprProp)> = self
                .layers
                .iter()
                .enumerate()
                .flat_map(|(i, l)| l.expr_props().into_iter().map(move |e| (i, e)))
                .filter(|(_, e)| e.fast.is_none())
                .collect();
            let cx_of = |i: usize| Cx {
                comp: comp_id,
                layer: i,
                ct: time,
                lt: self.layers[i].timing.layer_time(time),
                js,
            };
            expr::warm_up(
                props.len(),
                |k| {
                    let (i, e) = props[k];
                    expr::evaluate(js.expect("js"), &cx_of(i), e);
                },
                |k| {
                    let (i, e) = props[k];
                    expr::memoize_pre(&cx_of(i), e);
                },
            );
        }

        // Pass 1: everything that does not depend on other layers.
        let mut out: Vec<EvaluatedLayer> = self
            .layers
            .iter()
            .enumerate()
            .map(|(i, layer)| {
                let lt = layer.timing.layer_time(time);
                let cx = Cx {
                    comp: comp_id,
                    layer: i,
                    ct: time,
                    lt,
                    js,
                };
                let active = layer.timing.is_active(time);
                let visible =
                    layer.switches.enabled && active && (!has_solo || layer.switches.solo);
                let tf = sample_transform(layer, &cx);
                EvaluatedLayer {
                    path: layer.path.clone(),
                    name: layer.name.clone(),
                    kind: layer.kind,
                    timing: layer.timing,
                    layer_time: lt,
                    active,
                    visible,
                    switches: layer.switches,
                    source: sample_source(layer, &cx),
                    motion_blur_times: if self.motion_blur.enabled && layer.switches.motion_blur {
                        self.motion_blur.sample_times(
                            time,
                            self.frame_rate,
                            self.motion_blur.samples,
                        )
                    } else {
                        Vec::new()
                    },
                    frame_blend_span: (self.frame_blending
                        && layer.switches.frame_blending != FrameBlending::Off
                        && matches!(layer.source, CompiledSource::Asset(..)))
                    .then(|| source_span(layer, &cx, self.frame_rate.frame_duration())),
                    parent: layer.parent,
                    opacity: (tf.opacity / 100.0).clamp(0.0, 1.0),
                    local_matrix: DMat4::IDENTITY,
                    world_matrix: DMat4::IDENTITY,
                    transform: tf,
                    blend_mode: layer.blend_mode.clone(),
                    track_matte: layer.track_matte,
                    track_matte_layer: layer.track_matte_layer,
                    masks: layer.masks.iter().map(|m| sample_mask(m, &cx)).collect(),
                    effects: layer
                        .effects
                        .iter()
                        .map(|e| sample_effect(e, &cx))
                        .collect(),
                    contents: sample_prims(&layer.contents, layer.fixed_contents.as_ref(), &cx),
                    text: layer.text.as_ref().and_then(|p| match p.sample(&cx) {
                        Value::TextDocument(doc) => Some(doc),
                        _ => None,
                    }),
                    text_animators: sample_prims(
                        &layer.text_animators,
                        layer.fixed_animators.as_ref(),
                        &cx,
                    ),
                    text_path: layer.text_path.as_ref().map(|t| crate::TextPathOptions {
                        mask: t.mask.clone(),
                        reverse: t.reverse,
                        perpendicular: t.perpendicular,
                        force_alignment: t.force_alignment,
                        first_margin: t.first_margin.sample(&cx).as_f64().unwrap_or(0.0),
                        last_margin: t.last_margin.sample(&cx).as_f64().unwrap_or(0.0),
                    }),
                    camera: None,
                    light: None,
                    depends_on: Vec::new(),
                }
            })
            .collect();

        // Pass 2: matrices. Cameras and lights first (orient-towards-camera needs the
        // active camera), then everything else.
        let mut world: Vec<Option<DMat4>> = vec![None; n];
        let camera = out
            .iter()
            .position(|l| l.kind == LayerKind::Camera && l.switches.enabled && l.active);
        let is_cam_or_light =
            |i: &usize| matches!(self.layers[*i].kind, LayerKind::Camera | LayerKind::Light);
        let order: Vec<usize> = (0..n)
            .filter(is_cam_or_light)
            .chain((0..n).filter(|i| !is_cam_or_light(i)))
            .collect();
        let mut visiting = Vec::new();
        for i in order {
            self.world_matrix(i, &mut out, &mut world, camera, &mut visiting, comp_id, js);
        }
        // Camera / light state needs the final world matrices.
        for (i, layer) in self.layers.iter().enumerate() {
            let cx = Cx {
                comp: comp_id,
                layer: i,
                ct: time,
                lt: out[i].layer_time,
                js,
            };
            let wm = out[i].world_matrix;
            let poi = layer.transform.point_of_interest.vec3(&cx);
            if let Some(cm) = &layer.camera {
                out[i].camera = Some(CameraState {
                    two_node: layer.auto_orient == AutoOrient::Towards,
                    zoom: cm.zoom.f64(&cx),
                    depth_of_field: cm.depth_of_field,
                    focus_distance: cm.focus_distance.f64(&cx),
                    aperture: cm.aperture.f64(&cx),
                    blur_level: cm.blur_level.f64(&cx),
                    point_of_interest: poi,
                    world_matrix: wm,
                });
            }
            if let Some(lg) = &layer.light {
                out[i].light = Some(LightState {
                    light_type: lg.light_type,
                    intensity: lg.intensity.f64(&cx),
                    color: match lg.color.sample(&cx) {
                        Value::Color(c) => c,
                        _ => [1.0; 4],
                    },
                    cone_angle: lg.cone_angle.f64(&cx),
                    cone_feather: lg.cone_feather.f64(&cx),
                    falloff: lg.falloff,
                    radius: lg.radius.f64(&cx),
                    falloff_distance: lg.falloff_distance.f64(&cx),
                    casts_shadows: lg.casts_shadows,
                    shadow_darkness: lg.shadow_darkness.f64(&cx),
                    shadow_diffusion: lg.shadow_diffusion.f64(&cx),
                    point_of_interest: poi,
                    world_matrix: wm,
                });
            }
        }

        EvaluatedComp {
            path: self.path.clone(),
            time,
            width: self.width,
            height: self.height,
            pixel_aspect: self.pixel_aspect,
            frame_rate: self.frame_rate,
            duration: self.duration,
            bg_color: self.bg_color,
            motion_blur: self.motion_blur,
            frame_blending: self.frame_blending,
            layers: out,
            camera,
            errors: Vec::new(),
        }
    }

    /// World matrix of layer `i` (memoised in `world`; parent cycles are cut).
    #[allow(clippy::too_many_arguments)]
    fn world_matrix(
        &self,
        i: usize,
        out: &mut [EvaluatedLayer],
        world: &mut [Option<DMat4>],
        camera: Option<usize>,
        visiting: &mut Vec<usize>,
        comp_id: usize,
        js: Option<&Ctx<'_>>,
    ) -> DMat4 {
        if let Some(m) = world[i] {
            return m;
        }
        visiting.push(i);
        let layer = &self.layers[i];
        let parent_world = match layer.parent {
            Some(p) if !visiting.contains(&p) => {
                Some(self.world_matrix(p, out, world, camera, visiting, comp_id, js))
            }
            _ => None,
        };
        let cx = Cx {
            comp: comp_id,
            layer: i,
            ct: out[i].timing.comp_time(out[i].layer_time),
            lt: out[i].layer_time,
            js,
        };
        let three_d = out[i].switches.three_d;
        let mut tf = out[i].transform;
        tf.auto_orient = match (layer.auto_orient, layer.kind) {
            (AutoOrient::Off, _) => AutoOrientRotation::None,
            (AutoOrient::AlongPath, _) => along_path(layer, &cx, three_d, self.tick()),
            (AutoOrient::Towards, LayerKind::Camera | LayerKind::Light) => {
                look_along(layer.transform.point_of_interest.vec3(&cx) - tf.position)
            }
            (AutoOrient::Towards, _) if three_d => {
                // Orient towards the active camera (AE-ANI-021), in the parent's space.
                let cam_pos = match camera.and_then(|c| world[c]) {
                    Some(m) => m.w_axis.truncate(),
                    None => DVec3::new(
                        self.width as f64 / 2.0,
                        self.height as f64 / 2.0,
                        -(self.width as f64) * 50.0 / 36.0,
                    ),
                };
                let to_parent = parent_world.map_or(DMat4::IDENTITY, |m| m.inverse());
                let cam_local = to_parent.transform_point3(cam_pos);
                // The layer's front (local -z) faces the camera: +z points away.
                look_along(tf.position - cam_local)
            }
            _ => AutoOrientRotation::None,
        };
        let local = match layer.kind {
            LayerKind::Camera | LayerKind::Light => tf.camera_matrix(),
            _ => tf.matrix(three_d),
        };
        let m = match parent_world {
            Some(p) if three_d => p * local,
            // A 2D layer sees every ancestor as a 2D transform (position xy, Z rotation,
            // scale xy, anchor xy), 3D ones included: orientation, X/Y rotation and z
            // are ignored (measured in AE 26; AE-ANI-016 guessed an orthographic flatten).
            Some(_) => {
                let mut chain = Vec::new();
                let mut j = layer.parent;
                while let Some(k) = j {
                    if k == i || chain.contains(&k) {
                        break;
                    }
                    chain.push(k);
                    j = self.layers[k].parent;
                }
                let mut m2 = DMat4::IDENTITY;
                for &k in chain.iter().rev() {
                    m2 *= out[k].transform.matrix(false);
                }
                m2 * local
            }
            None => local,
        };
        out[i].transform = tf;
        out[i].local_matrix = local;
        out[i].world_matrix = m;
        world[i] = Some(m);
        visiting.pop();
        m
    }
}

fn sample_transform(layer: &CompiledLayer, cx: &Cx<'_, '_>) -> Transform {
    let t = &layer.transform;
    let three_d = layer.switches.three_d;
    let mut position = match &t.position {
        Position::Combined(p) => p.vec3(cx),
        Position::Separate(xyz) => DVec3::new(xyz[0].f64(cx), xyz[1].f64(cx), xyz[2].f64(cx)),
    };
    let mut anchor = t.anchor.vec3(cx);
    let scale = match t.scale.sample(cx) {
        Value::Vec2([x, y]) => DVec3::new(x, y, 100.0),
        other => vec3(&other),
    };
    if !three_d {
        position.z = 0.0;
        anchor.z = 0.0;
    }
    Transform {
        anchor_point: anchor,
        position,
        scale,
        orientation: if three_d {
            t.orientation.vec3(cx)
        } else {
            DVec3::ZERO
        },
        rotation: DVec3::new(
            if three_d { t.rotation_x.f64(cx) } else { 0.0 },
            if three_d { t.rotation_y.f64(cx) } else { 0.0 },
            t.rotation.f64(cx),
        ),
        opacity: t.opacity.f64(cx),
        auto_orient: AutoOrientRotation::None,
    }
}

/// Orient Along Path (AE-ANI-019/020): direction of travel of Position at `lt`.
fn along_path(
    layer: &CompiledLayer,
    cx: &Cx<'_, '_>,
    three_d: bool,
    tick: Time,
) -> AutoOrientRotation {
    let d = position_direction(layer, cx, tick);
    let Some(d) = d else {
        return AutoOrientRotation::None;
    };
    if !three_d {
        if d.x == 0.0 && d.y == 0.0 {
            return AutoOrientRotation::None;
        }
        return AutoOrientRotation::Angle(d.y.atan2(d.x).to_degrees());
    }
    // 3D layers, cameras and lights alike: local +z along the path, no roll
    // (measured in AE 26 for AV layers).
    look_along(d)
}

/// Direction of motion of a layer's position, as AE computes it: a forward difference
/// over one step of its 1/1024-frame time grid (`tick`, comp time) from the quantized
/// time, kept inside the keyframe range so the first/last segments keep their tangent.
fn position_direction(layer: &CompiledLayer, cx: &Cx<'_, '_>, tick: Time) -> Option<DVec3> {
    let range = match &layer.transform.position {
        Position::Combined(p) => p.curve()?.time_range(),
        Position::Separate(xyz) => {
            let ranges: Vec<(Time, Time)> = xyz
                .iter()
                .filter_map(|p| p.curve().map(Curve::time_range))
                .collect();
            if ranges.is_empty() {
                return None;
            }
            (
                ranges.iter().map(|r| r.0).min().expect("non-empty"),
                ranges.iter().map(|r| r.1).max().expect("non-empty"),
            )
        }
    };
    if range.0 >= range.1 {
        return None;
    }
    let q = Time((cx.ct.0 as f64 / tick.0 as f64).round() as i64 * tick.0);
    let (mut a, mut b) = (
        layer.timing.layer_time(q),
        layer.timing.layer_time(q + tick),
    );
    if a > b {
        std::mem::swap(&mut a, &mut b);
    }
    let h = b - a;
    if h.0 <= 0 || range.1 - range.0 < h {
        return None;
    }
    if a < range.0 {
        (a, b) = (range.0, range.0 + h);
    } else if b > range.1 {
        (a, b) = (range.1 - h, range.1);
    }
    let pos = |t: Time| {
        let cx = cx.at_layer_time(t, &layer.timing);
        match &layer.transform.position {
            Position::Combined(p) => p.vec3(&cx),
            Position::Separate(xyz) => {
                DVec3::new(xyz[0].f64(&cx), xyz[1].f64(&cx), xyz[2].f64(&cx))
            }
        }
    };
    let d = pos(b) - pos(a);
    (d.length_squared() > 0.0).then_some(d)
}

/// The source time one comp frame `frame` spans around now: the time remap's
/// change over the frame centred here, or the frame scaled by the stretch.
fn source_span(layer: &CompiledLayer, cx: &Cx<'_, '_>, frame: Time) -> Time {
    let Some(p) = &layer.time_remap else {
        let stretch = layer.timing.stretch.abs().max(1.0);
        return Time((frame.0 as f64 * 100.0 / stretch).round() as i64);
    };
    let at =
        |ct: Time| match p.sample(&cx.at_layer_time(layer.timing.layer_time(ct), &layer.timing)) {
            Value::Time(t) => t,
            other => Time::from_seconds(num(&other)),
        };
    let half = Time(frame.0 / 2);
    Time((at(cx.ct + half) - at(cx.ct - half)).0.abs())
}

fn sample_source(layer: &CompiledLayer, cx: &Cx<'_, '_>) -> LayerSource {
    let source_time = match &layer.time_remap {
        Some(p) => match p.sample(cx) {
            Value::Time(t) => t,
            other => Time::from_seconds(num(&other)),
        },
        None => cx.lt,
    };
    match &layer.source {
        CompiledSource::None => LayerSource::None,
        CompiledSource::Asset(a, info) => LayerSource::Asset {
            asset: a.clone(),
            time: source_time,
            info: info.clone(),
        },
        CompiledSource::Comp {
            path,
            rate,
            preserve,
        } => LayerSource::Comp {
            comp: path.clone(),
            time: nested_comp_time(source_time, *rate, *preserve),
            nested: None,
        },
    }
}

fn sample_mask(m: &CompiledMask, cx: &Cx<'_, '_>) -> EvaluatedMask {
    EvaluatedMask {
        path: m.path.clone(),
        mode: m.mode.clone(),
        inverted: m.inverted,
        shape: match m.shape.sample(cx) {
            Value::Bezier(b) => b,
            _ => BezierPath::default(),
        },
        feather: match m.feather.sample(cx) {
            Value::Vec2(f) => f,
            Value::Float(f) => [f, f],
            _ => [0.0; 2],
        },
        opacity: (m.opacity.f64(cx) / 100.0).clamp(0.0, 1.0),
        expansion: m.expansion.f64(cx),
    }
}

fn sample_effect(e: &CompiledEffect, cx: &Cx<'_, '_>) -> EvaluatedEffect {
    EvaluatedEffect {
        path: e.path.clone(),
        match_name: e.match_name.clone(),
        enabled: e.enabled,
        params: e
            .params
            .iter()
            .map(|(id, p)| (id.clone(), p.sample(cx)))
            .collect(),
        children: e.children.iter().map(|c| sample_generic(c, cx)).collect(),
    }
}

fn sample_generic(p: &CompiledPrim, cx: &Cx<'_, '_>) -> EvaluatedPrim {
    if let Some(fixed) = &p.fixed {
        return fixed.clone();
    }
    EvaluatedPrim {
        path: p.path.clone(),
        prim_type: p.prim_type.clone(),
        attrs: Arc::new(
            p.attrs
                .iter()
                .map(|(n, v)| (n.clone(), v.sample(cx)))
                .collect::<BTreeMap<_, _>>(),
        ),
        children: p.children.iter().map(|c| sample_generic(c, cx)).collect(),
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Evaluates comps and attributes, caching compiled comps between calls.
///
/// Keep one per thread (or per render job) and call [`Evaluator::invalidate`] with
/// every [`StageEvent`]; compiled comps that the event may affect are dropped and
/// recompiled on next use. The free functions [`crate::evaluate_comp`] and
/// [`crate::evaluate_attribute`] use a throwaway evaluator.
/// Pre-comp nesting deeper than this is not evaluated (deeper nesting in practice
/// means a reference loop through several comps).
const MAX_NESTING: usize = 64;

#[derive(Debug, Default)]
pub struct Evaluator {
    comps: HashMap<PrimPath, Arc<CompiledComp>>,
    /// Every comp of the stage (expressions can reach any comp); rebuilt after
    /// structural changes.
    comp_list: Option<Vec<PrimPath>>,
}

impl Evaluator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluate `comp` at comp time `time`.
    pub fn evaluate_comp(
        &mut self,
        stage: &Stage,
        comp: &PrimPath,
        time: Time,
    ) -> Result<EvaluatedComp, EvalError> {
        self.evaluate_nested(stage, comp, time, &mut Vec::new())
    }

    /// Evaluate `comp` and, recursively, the pre-comps its layers show (each at its
    /// mapped time). `stack` holds the comps being evaluated, to cut cycles.
    fn evaluate_nested(
        &mut self,
        stage: &Stage,
        comp: &PrimPath,
        time: Time,
        stack: &mut Vec<PrimPath>,
    ) -> Result<EvaluatedComp, EvalError> {
        let compiled = self.compiled(stage, comp)?;
        let mut out = if compiled.has_expressions {
            self.evaluate_with_expressions(stage, &compiled, time)
        } else {
            compiled.evaluate(time, 0, None)
        };
        stack.push(comp.clone());
        for layer in &mut out.layers {
            if let LayerSource::Comp { comp, time, nested } = &mut layer.source
                && !stack.contains(comp)
                && stack.len() < MAX_NESTING
            {
                *nested = self
                    .evaluate_nested(stage, comp, *time, stack)
                    .ok()
                    .map(Box::new);
            }
        }
        stack.pop();
        Ok(out)
    }

    fn compiled(&mut self, stage: &Stage, comp: &PrimPath) -> Result<Arc<CompiledComp>, EvalError> {
        if let Some(c) = self.comps.get(comp) {
            return Ok(c.clone());
        }
        let c = Arc::new(compile_comp(stage, comp)?);
        self.comps.insert(comp.clone(), c.clone());
        Ok(c)
    }

    /// Every comp of the stage, compiled (the scope expressions run in).
    fn all_comps(&mut self, stage: &Stage) -> Vec<Arc<CompiledComp>> {
        let list = self.comp_list.get_or_insert_with(|| {
            stage
                .traverse()
                .into_iter()
                .filter(|p| stage.prim(p).is_some_and(|v| v.is_a(&PrimType::COMP)))
                .collect()
        });
        let list = list.clone();
        list.iter()
            .filter_map(|p| self.compiled(stage, p).ok())
            .collect()
    }

    fn expression_scope(
        &mut self,
        stage: &Stage,
        compiled: &Arc<CompiledComp>,
    ) -> (expr::Scope, usize) {
        let mut comps = self.all_comps(stage);
        let id = match comps.iter().position(|c| Arc::ptr_eq(c, compiled)) {
            Some(i) => i,
            None => {
                comps.push(compiled.clone());
                comps.len() - 1
            }
        };
        (expr::Scope::new(comps), id)
    }

    fn evaluate_with_expressions(
        &mut self,
        stage: &Stage,
        compiled: &Arc<CompiledComp>,
        time: Time,
    ) -> EvaluatedComp {
        let (scope, id) = self.expression_scope(stage, compiled);
        let (mut out, result) = expr::with_scope(scope, || {
            expr::with_js(|ctx| compiled.evaluate(time, id, Some(ctx)))
        });
        if let Some(r) = result {
            out.errors.extend(
                r.errors
                    .into_iter()
                    .filter(|(c, _)| *c == id)
                    .map(|(_, e)| e),
            );
            for ((c1, l1), (c2, l2)) in r.deps {
                if c1 != id {
                    continue;
                }
                let path = r.comps[c2].layers[l2].path.clone();
                let deps = &mut out.layers[l1].depends_on;
                if !deps.contains(&path) {
                    deps.push(path);
                }
            }
        }
        out
    }

    /// Evaluate one attribute of any prim at `time`.
    ///
    /// `time` is the time of the comp containing the prim: for a layer and the prims
    /// below it (masks, effects, contents) it is converted to layer time first, exactly
    /// as [`Evaluator::evaluate_comp`] does. Missing opinions fall back to the schema
    /// default; [`EvalError::NoValue`] when there is neither.
    pub fn evaluate_attribute(
        &mut self,
        stage: &Stage,
        prim: &PrimPath,
        attr: &str,
        time: Time,
    ) -> Result<Value, EvalError> {
        let view = stage
            .prim(prim)
            .ok_or_else(|| EvalError::NoSuchPrim(prim.clone()))?;
        let local = match enclosing_layer(stage, prim) {
            Some((layer, comp)) => {
                let lv = stage.prim(&layer).expect("enclosing layer exists");
                let duration = stage
                    .prim(&comp)
                    .map_or(Time::from_seconds(10.0), |cv| comp_duration(&cv));
                layer_timing(&lv, duration).layer_time(time)
            }
            None => time,
        };
        let prim_type = view.prim_type();
        let prop = match view.attr(attr) {
            Some(spec) => Prop::from_spec(spec, prim_type, prim, attr),
            None => None,
        }
        .or_else(|| default_of(prim_type, attr).map(Prop::Static))
        .ok_or_else(|| EvalError::NoValue {
            prim: prim.clone(),
            attr: attr.to_owned(),
        })?;
        if let (Prop::Expr(_), Some((layer, comp))) = (&prop, enclosing_layer(stage, prim)) {
            return Ok(
                self.evaluate_expression_attribute(stage, &comp, &layer, prim, attr, time, &prop)
            );
        }
        let cx = Cx {
            comp: 0,
            layer: 0,
            ct: time,
            lt: local,
            js: None,
        };
        Ok(prop.sample(&cx))
    }

    /// `evaluate_attribute` for an attribute with an expression: runs it in the scope of
    /// its comp, like `evaluate_comp` would.
    #[allow(clippy::too_many_arguments)]
    fn evaluate_expression_attribute(
        &mut self,
        stage: &Stage,
        comp: &PrimPath,
        layer: &PrimPath,
        prim: &PrimPath,
        attr: &str,
        time: Time,
        fallback: &Prop,
    ) -> Value {
        let Ok(compiled) = self.compiled(stage, comp) else {
            return fallback.sample(&Cx {
                comp: 0,
                layer: 0,
                ct: time,
                lt: time,
                js: None,
            });
        };
        let index = compiled.layers.iter().position(|l| &l.path == layer);
        let (scope, id) = self.expression_scope(stage, &compiled);
        let (v, _) = expr::with_scope(scope, || {
            expr::with_js(|ctx| {
                let (li, lt) = match index {
                    Some(i) => (i, compiled.layers[i].timing.layer_time(time)),
                    None => (0, time),
                };
                let prop = index
                    .and_then(|i| compiled.layers[i].find_prop(prim.as_str(), attr))
                    .unwrap_or(fallback);
                prop.sample(&Cx {
                    comp: id,
                    layer: li,
                    ct: time,
                    lt,
                    js: Some(ctx),
                })
            })
        });
        v
    }

    /// Drop compiled comps that `event` may have changed.
    pub fn invalidate(&mut self, event: &StageEvent) {
        self.comps
            .retain(|_, c| !c.deps.iter().any(|d| event.touches(d)));
        let structural = event.changes.iter().any(|c| match c {
            bae_model::Change::AttrChanged { attr, .. } => attr == l::NAME,
            _ => true,
        });
        if structural {
            self.comp_list = None;
        }
    }

    /// Drop every compiled comp.
    pub fn clear(&mut self) {
        self.comps.clear();
        self.comp_list = None;
    }
}

/// The nearest ancestor-or-self `Layer` of `prim` and its comp
/// (`<comp>/layers/<layer>`).
fn enclosing_layer(stage: &Stage, prim: &PrimPath) -> Option<(PrimPath, PrimPath)> {
    let mut cur = Some(prim.clone());
    while let Some(p) = cur {
        if stage.prim(&p).is_some_and(|v| v.is_a(&PrimType::LAYER)) {
            let comp = p.parent()?.parent()?;
            return Some((p, comp));
        }
        cur = p.parent();
    }
    None
}

// ---------------------------------------------------------------------------
// Property lookup and transforms for expressions
// ---------------------------------------------------------------------------

impl CompiledPrim {
    fn find(&self, prim: &str, attr: &str) -> Option<&Prop> {
        if self.path.as_str() == prim {
            return self.attrs.iter().find(|(n, _)| n == attr).map(|(_, p)| p);
        }
        self.children.iter().find_map(|c| c.find(prim, attr))
    }

    fn has_expressions(&self) -> bool {
        self.attrs.iter().any(|(_, p)| p.has_expression())
            || self.children.iter().any(CompiledPrim::has_expressions)
    }

    /// What it evaluates to at any time, if nothing in it animates.
    fn fixed_form(&self) -> Option<EvaluatedPrim> {
        let attrs = self
            .attrs
            .iter()
            .map(|(n, p)| match p {
                Prop::Static(v) => Some((n.clone(), v.clone())),
                _ => None,
            })
            .collect::<Option<BTreeMap<_, _>>>()?;
        Some(EvaluatedPrim {
            path: self.path.clone(),
            prim_type: self.prim_type.clone(),
            attrs: Arc::new(attrs),
            children: fixed_list(&self.children)?,
        })
    }
}

impl CompiledLayer {
    /// The compiled property `attr` of `prim` (this layer or a prim below it).
    pub(crate) fn find_prop(&self, prim: &str, attr: &str) -> Option<&Prop> {
        if self.path.as_str() == prim {
            let t = &self.transform;
            return match attr {
                tr::ANCHOR_POINT => Some(&t.anchor),
                tr::POSITION => match &t.position {
                    Position::Combined(p) => Some(p),
                    Position::Separate(_) => None,
                },
                tr::POSITION_X | tr::POSITION_Y | tr::POSITION_Z => match &t.position {
                    Position::Separate(xyz) => Some(
                        &xyz[match attr {
                            tr::POSITION_X => 0,
                            tr::POSITION_Y => 1,
                            _ => 2,
                        }],
                    ),
                    Position::Combined(_) => None,
                },
                tr::SCALE => Some(&t.scale),
                tr::ORIENTATION => Some(&t.orientation),
                tr::ROTATION_X => Some(&t.rotation_x),
                tr::ROTATION_Y => Some(&t.rotation_y),
                tr::ROTATION => Some(&t.rotation),
                tr::OPACITY => Some(&t.opacity),
                tr::POINT_OF_INTEREST => Some(&t.point_of_interest),
                l::TIME_REMAP => self.time_remap.as_ref(),
                l::TEXT_SOURCE => self.text.as_ref(),
                l::TEXT_FIRST_MARGIN => self.text_path.as_ref().map(|t| &t.first_margin),
                l::TEXT_LAST_MARGIN => self.text_path.as_ref().map(|t| &t.last_margin),
                cam::ZOOM => self.camera.as_ref().map(|c| &c.zoom),
                cam::FOCUS_DISTANCE => self.camera.as_ref().map(|c| &c.focus_distance),
                cam::APERTURE => self.camera.as_ref().map(|c| &c.aperture),
                cam::BLUR_LEVEL => self.camera.as_ref().map(|c| &c.blur_level),
                li::INTENSITY => self.light.as_ref().map(|c| &c.intensity),
                li::COLOR => self.light.as_ref().map(|c| &c.color),
                li::CONE_ANGLE => self.light.as_ref().map(|c| &c.cone_angle),
                li::CONE_FEATHER => self.light.as_ref().map(|c| &c.cone_feather),
                li::RADIUS => self.light.as_ref().map(|c| &c.radius),
                li::FALLOFF_DISTANCE => self.light.as_ref().map(|c| &c.falloff_distance),
                li::SHADOW_DARKNESS => self.light.as_ref().map(|c| &c.shadow_darkness),
                li::SHADOW_DIFFUSION => self.light.as_ref().map(|c| &c.shadow_diffusion),
                _ => None,
            };
        }
        if let Some(m) = self.masks.iter().find(|m| m.path.as_str() == prim) {
            return match attr {
                mk::PATH => Some(&m.shape),
                mk::FEATHER => Some(&m.feather),
                mk::OPACITY => Some(&m.opacity),
                mk::EXPANSION => Some(&m.expansion),
                _ => None,
            };
        }
        if let Some(e) = self.effects.iter().find(|e| e.path.as_str() == prim) {
            let id = attr.strip_prefix(fx::PARAM_PREFIX)?;
            return e.params.iter().find(|(i, _)| i == id).map(|(_, p)| p);
        }
        self.contents
            .iter()
            .chain(&self.text_animators)
            .find_map(|c| c.find(prim, attr))
    }

    /// Every property of this layer (and the prims below it) that has an expression.
    pub(crate) fn expr_props(&self) -> Vec<&ExprProp> {
        let mut all: Vec<&Prop> = Vec::new();
        let t = &self.transform;
        match &t.position {
            Position::Combined(p) => all.push(p),
            Position::Separate(xyz) => all.extend(xyz.iter()),
        }
        all.extend([
            &t.anchor,
            &t.scale,
            &t.orientation,
            &t.rotation_x,
            &t.rotation_y,
            &t.rotation,
            &t.opacity,
            &t.point_of_interest,
        ]);
        all.extend(self.time_remap.iter());
        all.extend(self.text.iter());
        for m in &self.masks {
            all.extend([&m.shape, &m.feather, &m.opacity, &m.expansion]);
        }
        for e in &self.effects {
            all.extend(e.params.iter().map(|(_, p)| p));
        }
        fn walk<'a>(p: &'a CompiledPrim, out: &mut Vec<&'a Prop>) {
            out.extend(p.attrs.iter().map(|(_, v)| v));
            for c in &p.children {
                walk(c, out);
            }
        }
        for c in self.contents.iter().chain(&self.text_animators) {
            walk(c, &mut all);
        }
        if let Some(c) = &self.camera {
            all.extend([&c.zoom, &c.focus_distance, &c.aperture, &c.blur_level]);
        }
        if let Some(t) = &self.text_path {
            all.extend([&t.first_margin, &t.last_margin]);
        }
        if let Some(l) = &self.light {
            all.extend([
                &l.intensity,
                &l.color,
                &l.cone_angle,
                &l.cone_feather,
                &l.radius,
                &l.falloff_distance,
                &l.shadow_darkness,
                &l.shadow_diffusion,
            ]);
        }
        all.into_iter()
            .filter_map(|p| match p {
                Prop::Expr(e) => Some(&**e),
                _ => None,
            })
            .collect()
    }

    fn has_expressions(&self) -> bool {
        let t = &self.transform;
        let position = match &t.position {
            Position::Combined(p) => p.has_expression(),
            Position::Separate(xyz) => xyz.iter().any(Prop::has_expression),
        };
        position
            || [
                &t.anchor,
                &t.scale,
                &t.orientation,
                &t.rotation_x,
                &t.rotation_y,
                &t.rotation,
                &t.opacity,
                &t.point_of_interest,
            ]
            .iter()
            .any(|p| p.has_expression())
            || self.time_remap.as_ref().is_some_and(Prop::has_expression)
            || self.text.as_ref().is_some_and(Prop::has_expression)
            || self.masks.iter().any(|m| {
                [&m.shape, &m.feather, &m.opacity, &m.expansion]
                    .iter()
                    .any(|p| p.has_expression())
            })
            || self
                .effects
                .iter()
                .any(|e| e.params.iter().any(|(_, p)| p.has_expression()))
            || self.contents.iter().any(CompiledPrim::has_expressions)
            || self
                .text_animators
                .iter()
                .any(CompiledPrim::has_expressions)
            || self.camera.as_ref().is_some_and(|c| {
                [&c.zoom, &c.focus_distance, &c.aperture, &c.blur_level]
                    .iter()
                    .any(|p| p.has_expression())
            })
    }

    /// Source size (footage, solid or pre-comp), for `width` / `height` in expressions.
    pub(crate) fn source_size(&self, comp: &CompiledComp) -> (u32, u32) {
        match &self.source {
            CompiledSource::Asset(_, info) => (info.width, info.height),
            CompiledSource::Comp { .. } | CompiledSource::None => (comp.width, comp.height),
        }
    }

    pub(crate) fn source_comp(&self) -> Option<&PrimPath> {
        match &self.source {
            CompiledSource::Comp { path, .. } => Some(path),
            _ => None,
        }
    }
}

impl CompiledComp {
    /// One step of AE's evaluation time grid: 1/1024 of a frame (measured in AE 26).
    pub(crate) fn tick(&self) -> Time {
        Time(((self.frame_rate.frame_duration().0 as f64) / 1024.0).round() as i64)
    }

    /// View matrix (comp space → camera space, camera looking along +z) and zoom of the
    /// active camera at comp time `ct`: the topmost enabled camera layer that is active,
    /// else AE's default camera at (w/2, h/2, −zoom), zoom = width·50/36.
    pub(crate) fn camera_at(&self, ct: Time, comp_id: usize, js: Option<&Ctx<'_>>) -> (DMat4, f64) {
        let cam = self.layers.iter().position(|l| {
            l.kind == LayerKind::Camera && l.switches.enabled && l.timing.is_active(ct)
        });
        match cam {
            Some(i) => {
                let layer = &self.layers[i];
                let cx = Cx {
                    comp: comp_id,
                    layer: i,
                    ct,
                    lt: layer.timing.layer_time(ct),
                    js,
                };
                let zoom = layer.camera.as_ref().map_or(1777.8, |c| c.zoom.f64(&cx));
                (self.world_at(i, ct, comp_id, js).inverse(), zoom)
            }
            None => {
                let zoom = crate::default_camera_zoom(self.width);
                let eye = DVec3::new(self.width as f64 / 2.0, self.height as f64 / 2.0, -zoom);
                (DMat4::from_translation(-eye), zoom)
            }
        }
    }

    /// Layer `i`'s world matrix (layer space → comp space) at comp time `ct`, for
    /// expression space transforms. Auto-orient towards the camera is not applied.
    pub(crate) fn world_at(
        &self,
        i: usize,
        ct: Time,
        comp_id: usize,
        js: Option<&Ctx<'_>>,
    ) -> DMat4 {
        let mut visiting = Vec::new();
        self.world_at_inner(i, ct, comp_id, js, false, &mut visiting)
    }

    /// `flat`: evaluate this layer as a 2D transform (it is an ancestor of a 2D layer).
    #[allow(clippy::too_many_arguments)]
    fn world_at_inner(
        &self,
        i: usize,
        ct: Time,
        comp_id: usize,
        js: Option<&Ctx<'_>>,
        flat: bool,
        visiting: &mut Vec<usize>,
    ) -> DMat4 {
        visiting.push(i);
        let layer = &self.layers[i];
        let cx = Cx {
            comp: comp_id,
            layer: i,
            ct,
            lt: layer.timing.layer_time(ct),
            js,
        };
        let three_d = layer.switches.three_d;
        let mut tf = sample_transform(layer, &cx);
        tf.auto_orient = match (layer.auto_orient, layer.kind) {
            (AutoOrient::AlongPath, _) => along_path(layer, &cx, three_d, self.tick()),
            (AutoOrient::Towards, LayerKind::Camera | LayerKind::Light) => {
                look_along(layer.transform.point_of_interest.vec3(&cx) - tf.position)
            }
            _ => AutoOrientRotation::None,
        };
        let local = match layer.kind {
            LayerKind::Camera | LayerKind::Light => tf.camera_matrix(),
            _ => tf.matrix(three_d),
        };
        let local = if flat { tf.matrix(false) } else { local };
        let m = match layer.parent {
            Some(p) if !visiting.contains(&p) => {
                self.world_at_inner(p, ct, comp_id, js, flat || !three_d, visiting) * local
            }
            _ => local,
        };
        visiting.pop();
        m
    }
}
