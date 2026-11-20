//! Comp evaluation: compile a comp once (resolve opinions, decode keyframes into
//! [`Curve`]s, resolve parents and mattes), then sample it at any time.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use bae_model::{
    AttrSpec, BezierPath, FrameRate, LayerKind, PrimPath, PrimType, PrimView, Stage, StageEvent,
    Time, Value,
};
use glam::{DMat4, DVec3};

use crate::curve::Curve;
use crate::error::EvalError;
use crate::evaluated::{
    AssetInfo, CameraState, EvaluatedComp, EvaluatedEffect, EvaluatedLayer, EvaluatedMask,
    EvaluatedPrim, LayerSource, LayerSwitches, LightFalloff, LightState, LightType, TrackMatte,
};
use crate::schema::{
    self, asset, camera as cam, comp as c, effect as fx, layer as l, light as li, mask as mk,
    transform as tr,
};
use crate::time::{LayerTiming, MotionBlur, frame_rate_from_value, nested_comp_time};
use crate::transform::{AutoOrientRotation, Transform, drop_z, look_along, orient_x_along};

/// An animatable attribute, compiled.
#[derive(Clone, Debug)]
enum Prop {
    Static(Value),
    Curve(Box<Curve>),
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
            Some(spec) => Prop::from_spec(spec, prim_type, attr)
                .unwrap_or_else(|| Prop::Static(default_of(prim_type, attr).unwrap_or(fallback))),
            None => Prop::Static(default_of(prim_type, attr).unwrap_or(fallback)),
        }
    }

    fn from_spec(spec: &AttrSpec, prim_type: Option<&PrimType>, attr: &str) -> Option<Prop> {
        if let Some(curve) = Curve::new(&spec.keyframes, schema::curve_kind(attr, spec)) {
            let curve = match schema::clamp_range(prim_type, attr) {
                Some((lo, hi)) => curve.with_clamp(lo, hi),
                None => curve,
            };
            return Some(Prop::Curve(Box::new(curve)));
        }
        spec.value.clone().map(Prop::Static)
    }

    fn sample(&self, t: Time) -> Value {
        match self {
            Prop::Static(v) => v.clone(),
            Prop::Curve(c) => c.sample(t),
        }
    }

    fn f64(&self, t: Time) -> f64 {
        match self {
            Prop::Static(v) => num(v),
            Prop::Curve(c) => num(&c.sample(t)),
        }
    }

    fn vec3(&self, t: Time) -> DVec3 {
        match self {
            Prop::Static(v) => vec3(v),
            Prop::Curve(c) => vec3(&c.sample(t)),
        }
    }
}

fn default_of(prim_type: Option<&PrimType>, attr: &str) -> Option<Value> {
    schema::default_value(prim_type?, attr)
}

fn num(v: &Value) -> f64 {
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
struct CompiledComp {
    path: PrimPath,
    width: u32,
    height: u32,
    pixel_aspect: f64,
    frame_rate: FrameRate,
    duration: Time,
    bg_color: [f64; 4],
    motion_blur: MotionBlur,
    layers: Vec<CompiledLayer>,
    /// Prims whose changes invalidate this compiled comp (the comp and its sources).
    deps: Vec<PrimPath>,
}

#[derive(Debug)]
enum Position {
    Combined(Prop),
    Separate(Box<[Prop; 3]>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AutoOrient {
    Off,
    AlongPath,
    /// Cameras/lights: towards the point of interest; 3D AV layers: towards camera.
    Towards,
}

#[derive(Debug)]
struct CompiledTransform {
    anchor: Prop,
    position: Position,
    scale: Prop,
    orientation: Prop,
    rotation_x: Prop,
    rotation_y: Prop,
    rotation: Prop,
    opacity: Prop,
    point_of_interest: Prop,
}

#[derive(Debug)]
enum CompiledSource {
    None,
    Asset(PrimPath, AssetInfo),
    Comp {
        path: PrimPath,
        rate: FrameRate,
        preserve: bool,
    },
}

#[derive(Debug)]
struct CompiledMask {
    path: PrimPath,
    mode: String,
    inverted: bool,
    shape: Prop,
    feather: Prop,
    opacity: Prop,
    expansion: Prop,
}

#[derive(Debug)]
struct CompiledEffect {
    path: PrimPath,
    match_name: String,
    enabled: bool,
    params: Vec<(String, Prop)>,
}

#[derive(Debug)]
struct CompiledPrim {
    path: PrimPath,
    prim_type: Option<PrimType>,
    attrs: Vec<(String, Prop)>,
    children: Vec<CompiledPrim>,
}

#[derive(Debug)]
struct CompiledCamera {
    zoom: Prop,
    depth_of_field: bool,
    focus_distance: Prop,
    aperture: Prop,
    blur_level: Prop,
}

#[derive(Debug)]
struct CompiledLight {
    light_type: LightType,
    intensity: Prop,
    color: Prop,
    cone_angle: Prop,
    cone_feather: Prop,
    falloff: LightFalloff,
    radius: Prop,
    falloff_distance: Prop,
    casts_shadows: bool,
    shadow_darkness: Prop,
    shadow_diffusion: Prop,
}

#[derive(Debug)]
struct CompiledLayer {
    path: PrimPath,
    name: String,
    kind: LayerKind,
    timing: LayerTiming,
    switches: LayerSwitches,
    source: CompiledSource,
    time_remap: Option<Prop>,
    parent: Option<usize>,
    blend_mode: String,
    track_matte: TrackMatte,
    track_matte_layer: Option<usize>,
    auto_orient: AutoOrient,
    transform: CompiledTransform,
    masks: Vec<CompiledMask>,
    effects: Vec<CompiledEffect>,
    contents: Vec<CompiledPrim>,
    text: Option<Prop>,
    text_animators: Vec<CompiledPrim>,
    camera: Option<CompiledCamera>,
    light: Option<CompiledLight>,
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
        adaptive_limit: MotionBlur::default().adaptive_limit,
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

    Ok(CompiledComp {
        path: path.clone(),
        width: int(c::WIDTH, 1920.0).max(0.0) as u32,
        height: int(c::HEIGHT, 1080.0).max(0.0) as u32,
        pixel_aspect: int(c::PIXEL_ASPECT, 1.0),
        frame_rate,
        duration,
        bg_color,
        motion_blur,
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
        solo: setting_bool(v, lt, l::SOLO),
        shy: setting_bool(v, lt, l::SHY),
        guide: setting_bool(v, lt, l::GUIDE),
        adjustment: setting_bool(v, lt, l::ADJUSTMENT_LAYER) || kind == LayerKind::Adjustment,
        three_d: three_d || matches!(kind, LayerKind::Camera | LayerKind::Light),
        motion_blur: setting_bool(v, lt, l::MOTION_BLUR),
        collapse_transformation: setting_bool(v, lt, l::COLLAPSE_TRANSFORMATION),
        preserve_transparency: setting_bool(v, lt, l::PRESERVE_TRANSPARENCY),
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
                .and_then(|s| Prop::from_spec(s, ty, l::TIME_REMAP))
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
                    Some((id.to_owned(), Prop::from_spec(spec, Some(et), name)?))
                })
                .collect();
            Some(CompiledEffect {
                match_name: setting_str(&ev, et, fx::MATCH_NAME).unwrap_or_default(),
                enabled: setting(&ev, et, fx::ENABLED)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                params,
                path: ep,
            })
        })
        .collect();
    let contents = child_prims(l::CONTENTS)
        .into_iter()
        .filter_map(|cp| compile_generic(stage, &cp, 0))
        .collect();
    let text = (kind == LayerKind::Text)
        .then(|| {
            v.attr(l::TEXT_SOURCE)
                .and_then(|s| Prop::from_spec(s, ty, l::TEXT_SOURCE))
        })
        .flatten();
    let text_animators = child_prims(l::ANIMATORS)
        .into_iter()
        .filter_map(|cp| compile_generic(stage, &cp, 0))
        .collect();

    Ok(CompiledLayer {
        name: v
            .value(l::NAME)
            .and_then(|n| n.as_str())
            .unwrap_or(v.name())
            .to_owned(),
        kind,
        timing: layer_timing(v, comp_duration),
        switches,
        source,
        time_remap,
        parent: None,
        blend_mode: setting_str(v, lt, l::BLEND_MODE).unwrap_or_else(|| "normal".into()),
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
        contents,
        text,
        text_animators,
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
    if let Some(t) = &prim_type {
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
                Some(spec) => Prop::from_spec(spec, prim_type.as_ref(), name)?,
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
    Some(CompiledPrim {
        path: path.clone(),
        prim_type,
        attrs,
        children,
    })
}

// ---------------------------------------------------------------------------
// Sampling
// ---------------------------------------------------------------------------

impl CompiledComp {
    fn evaluate(&self, time: Time) -> EvaluatedComp {
        let n = self.layers.len();
        let has_solo = self
            .layers
            .iter()
            .any(|l| l.switches.solo && l.switches.enabled);

        // Pass 1: everything that does not depend on other layers.
        let mut out: Vec<EvaluatedLayer> = self
            .layers
            .iter()
            .map(|layer| {
                let lt = layer.timing.layer_time(time);
                let active = layer.timing.is_active(time);
                let visible =
                    layer.switches.enabled && active && (!has_solo || layer.switches.solo);
                let tf = sample_transform(layer, lt);
                EvaluatedLayer {
                    path: layer.path.clone(),
                    name: layer.name.clone(),
                    kind: layer.kind,
                    timing: layer.timing,
                    layer_time: lt,
                    active,
                    visible,
                    switches: layer.switches,
                    source: sample_source(layer, lt),
                    motion_blur_times: if self.motion_blur.enabled && layer.switches.motion_blur {
                        self.motion_blur.sample_times(
                            time,
                            self.frame_rate,
                            self.motion_blur.samples,
                        )
                    } else {
                        Vec::new()
                    },
                    parent: layer.parent,
                    opacity: (tf.opacity / 100.0).clamp(0.0, 1.0),
                    local_matrix: DMat4::IDENTITY,
                    world_matrix: DMat4::IDENTITY,
                    transform: tf,
                    blend_mode: layer.blend_mode.clone(),
                    track_matte: layer.track_matte,
                    track_matte_layer: layer.track_matte_layer,
                    masks: layer.masks.iter().map(|m| sample_mask(m, lt)).collect(),
                    effects: layer.effects.iter().map(|e| sample_effect(e, lt)).collect(),
                    contents: layer
                        .contents
                        .iter()
                        .map(|p| sample_generic(p, lt))
                        .collect(),
                    text: layer.text.as_ref().and_then(|p| match p.sample(lt) {
                        Value::TextDocument(doc) => Some(doc),
                        _ => None,
                    }),
                    text_animators: layer
                        .text_animators
                        .iter()
                        .map(|p| sample_generic(p, lt))
                        .collect(),
                    camera: None,
                    light: None,
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
            self.world_matrix(i, &mut out, &mut world, camera, &mut visiting);
        }
        // Camera / light state needs the final world matrices.
        for (i, layer) in self.layers.iter().enumerate() {
            let lt = out[i].layer_time;
            let wm = out[i].world_matrix;
            let poi = layer.transform.point_of_interest.vec3(lt);
            if let Some(cm) = &layer.camera {
                out[i].camera = Some(CameraState {
                    two_node: layer.auto_orient == AutoOrient::Towards,
                    zoom: cm.zoom.f64(lt),
                    depth_of_field: cm.depth_of_field,
                    focus_distance: cm.focus_distance.f64(lt),
                    aperture: cm.aperture.f64(lt),
                    blur_level: cm.blur_level.f64(lt),
                    point_of_interest: poi,
                    world_matrix: wm,
                });
            }
            if let Some(lg) = &layer.light {
                out[i].light = Some(LightState {
                    light_type: lg.light_type,
                    intensity: lg.intensity.f64(lt),
                    color: match lg.color.sample(lt) {
                        Value::Color(c) => c,
                        _ => [1.0; 4],
                    },
                    cone_angle: lg.cone_angle.f64(lt),
                    cone_feather: lg.cone_feather.f64(lt),
                    falloff: lg.falloff,
                    radius: lg.radius.f64(lt),
                    falloff_distance: lg.falloff_distance.f64(lt),
                    casts_shadows: lg.casts_shadows,
                    shadow_darkness: lg.shadow_darkness.f64(lt),
                    shadow_diffusion: lg.shadow_diffusion.f64(lt),
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
            layers: out,
            camera,
            errors: Vec::new(),
        }
    }

    /// World matrix of layer `i` (memoised in `world`; parent cycles are cut).
    fn world_matrix(
        &self,
        i: usize,
        out: &mut [EvaluatedLayer],
        world: &mut [Option<DMat4>],
        camera: Option<usize>,
        visiting: &mut Vec<usize>,
    ) -> DMat4 {
        if let Some(m) = world[i] {
            return m;
        }
        visiting.push(i);
        let layer = &self.layers[i];
        let parent_world = match layer.parent {
            Some(p) if !visiting.contains(&p) => {
                Some(self.world_matrix(p, out, world, camera, visiting))
            }
            _ => None,
        };
        let lt = out[i].layer_time;
        let three_d = out[i].switches.three_d;
        let mut tf = out[i].transform;
        tf.auto_orient = match (layer.auto_orient, layer.kind) {
            (AutoOrient::Off, _) => AutoOrientRotation::None,
            (AutoOrient::AlongPath, kind) => along_path(layer, lt, three_d, kind),
            (AutoOrient::Towards, LayerKind::Camera | LayerKind::Light) => {
                look_along(layer.transform.point_of_interest.vec3(lt) - tf.position)
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
            // 2D layers see their parent chain flattened (AE-ANI-016).
            Some(p) => drop_z(p) * local,
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

fn sample_transform(layer: &CompiledLayer, lt: Time) -> Transform {
    let t = &layer.transform;
    let three_d = layer.switches.three_d;
    let mut position = match &t.position {
        Position::Combined(p) => p.vec3(lt),
        Position::Separate(xyz) => DVec3::new(xyz[0].f64(lt), xyz[1].f64(lt), xyz[2].f64(lt)),
    };
    let mut anchor = t.anchor.vec3(lt);
    let scale = match t.scale.sample(lt) {
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
            t.orientation.vec3(lt)
        } else {
            DVec3::ZERO
        },
        rotation: DVec3::new(
            if three_d { t.rotation_x.f64(lt) } else { 0.0 },
            if three_d { t.rotation_y.f64(lt) } else { 0.0 },
            t.rotation.f64(lt),
        ),
        opacity: t.opacity.f64(lt),
        auto_orient: AutoOrientRotation::None,
    }
}

/// Orient Along Path (AE-ANI-019/020): direction of travel of Position at `lt`.
fn along_path(
    layer: &CompiledLayer,
    lt: Time,
    three_d: bool,
    kind: LayerKind,
) -> AutoOrientRotation {
    let d = position_direction(layer, lt);
    let Some(d) = d else {
        return AutoOrientRotation::None;
    };
    if !three_d {
        if d.x == 0.0 && d.y == 0.0 {
            return AutoOrientRotation::None;
        }
        return AutoOrientRotation::Angle(d.y.atan2(d.x).to_degrees());
    }
    match kind {
        LayerKind::Camera | LayerKind::Light => look_along(d),
        _ => orient_x_along(d),
    }
}

/// Direction of motion of a layer's position: a central difference of ±1/1000 s
/// (clamped into the keyframe range so the first/last segments keep their tangent).
fn position_direction(layer: &CompiledLayer, lt: Time) -> Option<DVec3> {
    let range = match &layer.transform.position {
        Position::Combined(Prop::Curve(c)) => c.time_range(),
        Position::Separate(xyz) => {
            let ranges: Vec<(Time, Time)> = xyz
                .iter()
                .filter_map(|p| match p {
                    Prop::Curve(c) => Some(c.time_range()),
                    Prop::Static(_) => None,
                })
                .collect();
            if ranges.is_empty() {
                return None;
            }
            (
                ranges.iter().map(|r| r.0).min().expect("non-empty"),
                ranges.iter().map(|r| r.1).max().expect("non-empty"),
            )
        }
        Position::Combined(Prop::Static(_)) => return None,
    };
    if range.0 >= range.1 {
        return None;
    }
    let h = Time(Time::FLICKS_PER_SECOND / 1000);
    let a = lt.clamp(range.0, range.1 - h);
    let pos = |t: Time| match &layer.transform.position {
        Position::Combined(p) => p.vec3(t),
        Position::Separate(xyz) => DVec3::new(xyz[0].f64(t), xyz[1].f64(t), xyz[2].f64(t)),
    };
    let d = pos(a + h) - pos(a);
    (d.length_squared() > 0.0).then_some(d)
}

fn sample_source(layer: &CompiledLayer, lt: Time) -> LayerSource {
    let source_time = match &layer.time_remap {
        Some(p) => match p.sample(lt) {
            Value::Time(t) => t,
            other => Time::from_seconds(num(&other)),
        },
        None => lt,
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

fn sample_mask(m: &CompiledMask, lt: Time) -> EvaluatedMask {
    EvaluatedMask {
        path: m.path.clone(),
        mode: m.mode.clone(),
        inverted: m.inverted,
        shape: match m.shape.sample(lt) {
            Value::Bezier(b) => b,
            _ => BezierPath::default(),
        },
        feather: match m.feather.sample(lt) {
            Value::Vec2(f) => f,
            Value::Float(f) => [f, f],
            _ => [0.0; 2],
        },
        opacity: (m.opacity.f64(lt) / 100.0).clamp(0.0, 1.0),
        expansion: m.expansion.f64(lt),
    }
}

fn sample_effect(e: &CompiledEffect, lt: Time) -> EvaluatedEffect {
    EvaluatedEffect {
        path: e.path.clone(),
        match_name: e.match_name.clone(),
        enabled: e.enabled,
        params: e
            .params
            .iter()
            .map(|(id, p)| (id.clone(), p.sample(lt)))
            .collect(),
    }
}

fn sample_generic(p: &CompiledPrim, lt: Time) -> EvaluatedPrim {
    EvaluatedPrim {
        path: p.path.clone(),
        prim_type: p.prim_type.clone(),
        attrs: p
            .attrs
            .iter()
            .map(|(n, v)| (n.clone(), v.sample(lt)))
            .collect::<BTreeMap<_, _>>(),
        children: p.children.iter().map(|c| sample_generic(c, lt)).collect(),
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
        let compiled = match self.comps.get(comp) {
            Some(c) => c.clone(),
            None => {
                let c = Arc::new(compile_comp(stage, comp)?);
                self.comps.insert(comp.clone(), c.clone());
                c
            }
        };
        let mut out = compiled.evaluate(time);
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
            Some(spec) => Prop::from_spec(spec, prim_type, attr),
            None => None,
        }
        .or_else(|| default_of(prim_type, attr).map(Prop::Static))
        .ok_or_else(|| EvalError::NoValue {
            prim: prim.clone(),
            attr: attr.to_owned(),
        })?;
        Ok(prop.sample(local))
    }

    /// Drop compiled comps that `event` may have changed.
    pub fn invalidate(&mut self, event: &StageEvent) {
        self.comps
            .retain(|_, c| !c.deps.iter().any(|d| event.touches(d)));
    }

    /// Drop every compiled comp.
    pub fn clear(&mut self) {
        self.comps.clear();
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
