//! Read-only helpers over the composed stage, shared by the motion commands and the
//! motion workspace UI (`bae-ui-motion`): comp settings, the layers of a comp, layer
//! timing, display names and the time base of keyframes.
//!
//! Nothing here writes to the stage.

use bae_anim::LayerTiming;
use bae_model::schema::attr_def;
use bae_model::{AttrSpec, FrameRate, PrimPath, PrimType, PrimView, Stage, Time, TimeRange, Value};

/// Folder new comps go into when no parent is given (the convention used by
/// importers, fixtures and the agent guide: `/comps/<name>`).
pub const COMPS_FOLDER: &str = "/comps";
/// Folder for project items such as footage.
pub const ASSETS_FOLDER: &str = "/assets";
/// Folder for the solid items that solid and adjustment layers use (AE's "Solids").
pub const SOLIDS_FOLDER: &str = "/assets/solids";
/// Child folder of a comp holding its layers, topmost first.
pub const LAYERS: &str = "layers";
/// Child folder of a layer or shape group holding shape contents.
pub const CONTENTS: &str = "contents";
/// Child folder of a comp or layer holding its markers.
pub const MARKERS: &str = "markers";

/// The settings of a comp that the timeline and viewer need.
#[derive(Clone, Debug, PartialEq)]
pub struct CompInfo {
    pub path: PrimPath,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub pixel_aspect: f64,
    pub frame_rate: FrameRate,
    pub duration: Time,
    /// Work area in comp time, clamped to `[0, duration]`.
    pub work_area: TimeRange,
    pub bg_color: [f64; 4],
    /// Timecode of comp time 0 (AE "Start Timecode").
    pub display_start: Time,
}

impl CompInfo {
    /// Number of frames in the comp.
    pub fn frame_count(&self) -> i64 {
        self.frame_rate.time_to_frame(self.duration).max(1)
    }

    /// Duration of one frame.
    pub fn frame_duration(&self) -> Time {
        self.frame_rate.frame_duration()
    }
}

/// Static value of `attr` on `view`, falling back to the schema default.
/// Ignores keyframes (see [`attr_spec`] for animation).
pub fn static_value(view: &PrimView<'_>, attr: &str) -> Option<Value> {
    view.value(attr).cloned().or_else(|| {
        view.prim_type()
            .and_then(|t| attr_def(t, attr))
            .and_then(|d| d.default.to_value())
    })
}

/// The winning attribute opinion (value, keyframes, expression) of `attr`.
pub fn attr_spec<'a>(stage: &'a Stage, path: &PrimPath, attr: &str) -> Option<&'a AttrSpec> {
    stage.prim(path)?.attr(attr)
}

fn int(view: &PrimView<'_>, attr: &str, default: i64) -> i64 {
    match static_value(view, attr) {
        Some(Value::Int(v)) => v,
        Some(Value::Float(v)) => v as i64,
        _ => default,
    }
}

fn time(view: &PrimView<'_>, attr: &str) -> Option<Time> {
    static_value(view, attr).and_then(|v| v.as_time())
}

/// Display name: the `name` attribute, else the prim name.
pub fn display_name(view: &PrimView<'_>) -> String {
    match view.value("name") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => view.name().to_owned(),
    }
}

/// Display name of the prim at `path` (its path name when it does not exist).
pub fn display_name_of(stage: &Stage, path: &PrimPath) -> String {
    stage
        .prim(path)
        .map_or_else(|| path.name().to_owned(), |v| display_name(&v))
}

/// Comp settings, or `None` when `comp` is not a `Comp` prim.
pub fn comp_info(stage: &Stage, comp: &PrimPath) -> Option<CompInfo> {
    let view = stage.prim(comp)?;
    if !view.is_a(&PrimType::COMP) {
        return None;
    }
    let frame_rate = match static_value(&view, "frame_rate") {
        Some(Value::FrameRate(r)) if r.num > 0 && r.den > 0 => r,
        _ => FrameRate::FPS_30,
    };
    let duration = time(&view, "duration")
        .filter(|d| d.0 > 0)
        .unwrap_or(Time::from_seconds(10.0));
    let ws = time(&view, "work_area_start")
        .unwrap_or(Time::ZERO)
        .clamp(Time::ZERO, duration);
    let we = time(&view, "work_area_duration")
        .map_or(duration, |d| ws + d)
        .clamp(ws, duration);
    let bg_color = match static_value(&view, "bg_color") {
        Some(Value::Color(c)) => c,
        _ => [0.0, 0.0, 0.0, 1.0],
    };
    let pixel_aspect = static_value(&view, "pixel_aspect")
        .and_then(|v| v.as_f64())
        .filter(|v| *v > 0.0)
        .unwrap_or(1.0);
    Some(CompInfo {
        path: comp.clone(),
        name: display_name(&view),
        width: int(&view, "width", 1920).clamp(1, 30_000) as u32,
        height: int(&view, "height", 1080).clamp(1, 30_000) as u32,
        pixel_aspect,
        frame_rate,
        duration,
        work_area: TimeRange::new(ws, we),
        bg_color,
        display_start: time(&view, "display_start_time").unwrap_or(Time::ZERO),
    })
}

/// Every comp on the stage, in traversal order.
pub fn comps(stage: &Stage) -> Vec<PrimPath> {
    stage
        .traverse()
        .into_iter()
        .filter(|p| stage.prim(p).is_some_and(|v| v.is_a(&PrimType::COMP)))
        .collect()
}

/// The `layers` folder of a comp.
pub fn layers_folder(comp: &PrimPath) -> PrimPath {
    comp.child(LAYERS).expect("valid name")
}

/// Layers of a comp, topmost first (AE index 1 first).
pub fn comp_layers(stage: &Stage, comp: &PrimPath) -> Vec<PrimPath> {
    stage
        .children(&layers_folder(comp))
        .into_iter()
        .filter(|p| is_layer(stage, p))
        .collect()
}

/// The composition opened by double-clicking this pre-comp layer.
pub fn precomp_source(stage: &Stage, layer: &PrimPath) -> Option<PrimPath> {
    let view = stage.prim(layer)?;
    if view.value("kind")?.as_str()? != "pre_comp" {
        return None;
    }
    let source = view.value("source")?.as_prim_ref()?;
    stage
        .prim(source)?
        .is_a(&PrimType::COMP)
        .then(|| source.clone())
}

/// The composition source of each pre-comp layer, in layer order.
pub fn child_comps<'a>(stage: &'a Stage, comp: &PrimPath) -> impl Iterator<Item = PrimPath> + 'a {
    comp_layers(stage, comp)
        .into_iter()
        .filter_map(|layer| precomp_source(stage, &layer))
}

/// Whether using `source` inside `parent` would close a composition cycle.
pub fn would_nest_recursively(stage: &Stage, parent: &PrimPath, source: &PrimPath) -> bool {
    let mut stack = vec![source.clone()];
    let mut visited = std::collections::HashSet::new();
    while let Some(comp) = stack.pop() {
        if &comp == parent {
            return true;
        }
        if visited.insert(comp.clone()) {
            stack.extend(child_comps(stage, &comp));
        }
    }
    false
}

/// Whether `path` is a `Layer` prim.
pub fn is_layer(stage: &Stage, path: &PrimPath) -> bool {
    stage.prim(path).is_some_and(|v| v.is_a(&PrimType::LAYER))
}

/// Nearest ancestor-or-self of `path` of type `ty`.
pub fn ancestor_of_type(stage: &Stage, path: &PrimPath, ty: &PrimType) -> Option<PrimPath> {
    std::iter::once(path.clone())
        .chain(path.ancestors())
        .find(|p| !p.is_root() && stage.prim(p).is_some_and(|v| v.is_a(ty)))
}

/// The comp a layer (or anything below a comp) belongs to.
pub fn owning_comp(stage: &Stage, path: &PrimPath) -> Option<PrimPath> {
    ancestor_of_type(stage, path, &PrimType::COMP)
}

/// The layer a prim belongs to (itself for a layer).
pub fn owning_layer(stage: &Stage, path: &PrimPath) -> Option<PrimPath> {
    ancestor_of_type(stage, path, &PrimType::LAYER)
}

/// The layer's Lock switch is on (AE: it cannot be selected or edited until
/// unlocked).
pub fn is_locked(stage: &Stage, layer: &PrimPath) -> bool {
    stage
        .prim(layer)
        .and_then(|v| static_value(&v, "locked"))
        .and_then(|v| v.as_bool())
        == Some(true)
}

/// The locked layer `path` belongs to (itself, or the layer of a property,
/// shape or effect below it), if any.
pub fn locked_layer_of(stage: &Stage, path: &PrimPath) -> Option<PrimPath> {
    owning_layer(stage, path).filter(|l| is_locked(stage, l))
}

/// A layer's placement in comp time. An unset out point is the comp's end.
pub fn layer_timing(stage: &Stage, layer: &PrimPath) -> LayerTiming {
    let Some(view) = stage.prim(layer) else {
        return LayerTiming::default();
    };
    let comp_end = owning_comp(stage, layer)
        .and_then(|c| comp_info(stage, &c))
        .map_or(Time::MAX, |c| c.duration);
    let stretch = static_value(&view, "stretch")
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0);
    LayerTiming {
        start_time: time(&view, "start_time").unwrap_or(Time::ZERO),
        stretch,
        in_point: time(&view, "in_point").unwrap_or(Time::ZERO),
        out_point: time(&view, "out_point").unwrap_or(comp_end),
    }
}

/// Duration of a layer's time-based source (a pre-comp, or footage with a duration),
/// in source time; None for stills, solids, text, shapes and other layers without one
/// (AE: they can be trimmed without limit and have no slip-edit bar).
pub fn source_duration(stage: &Stage, layer: &PrimPath) -> Option<Time> {
    let view = stage.prim(layer)?;
    let source = view
        .value("source")
        .and_then(|v| v.as_prim_ref())
        .cloned()?;
    let src = stage.prim(&source)?;
    let d = time(&src, "duration")?;
    (d > Time::ZERO).then_some(d)
}

/// Comp-time span a layer's whole source covers (start, end), when it has one.
pub fn source_span(stage: &Stage, layer: &PrimPath) -> Option<(Time, Time)> {
    let d = source_duration(stage, layer)?;
    let t = layer_timing(stage, layer);
    let a = t.comp_time(Time::ZERO);
    let b = t.comp_time(d);
    Some((a.min(b), a.max(b)))
}

/// How keyframe times of the prim at `path` relate to comp time: the owning layer's
/// timing for layers and the prims below them (masks, effects, shape contents), the
/// identity for everything else (comp attributes are keyed in comp time).
pub fn key_timing(stage: &Stage, path: &PrimPath) -> LayerTiming {
    match owning_layer(stage, path) {
        Some(layer) => {
            let t = layer_timing(stage, &layer);
            LayerTiming {
                in_point: Time::MIN,
                out_point: Time::MAX,
                ..t
            }
        }
        None => LayerTiming::default(),
    }
}

/// Comp time → keyframe time of `path` (see [`key_timing`]).
pub fn to_key_time(stage: &Stage, path: &PrimPath, comp_time: Time) -> Time {
    key_timing(stage, path).layer_time(comp_time)
}

/// Keyframe time of `path` → comp time.
pub fn to_comp_time(stage: &Stage, path: &PrimPath, key_time: Time) -> Time {
    key_timing(stage, path).comp_time(key_time)
}

/// Layer kind token (`kind` attribute).
pub fn layer_kind(stage: &Stage, layer: &PrimPath) -> Option<String> {
    let view = stage.prim(layer)?;
    static_value(&view, "kind").and_then(|v| v.as_str().map(str::to_owned))
}

/// A prim name derived from a display name: characters other than letters, digits,
/// `_` and `-` become `_`; empty names become `item`.
pub fn sanitize_name(display: &str) -> String {
    let mut s: String = display
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    while s.starts_with('-') {
        s.remove(0);
    }
    if s.is_empty() { "item".to_owned() } else { s }
}

/// A child name of `parent` based on `base` that no sublayer uses yet
/// (`base`, `base_2`, `base_3` …).
pub fn unique_child_name(stage: &Stage, parent: &PrimPath, base: &str) -> String {
    let base = sanitize_name(base);
    let taken = |name: &str| {
        parent
            .child(name)
            .is_ok_and(|p| stage.has_prim(&p) || stage.sublayers().any(|s| s.has_spec(&p)))
    };
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|i| format!("{base}_{i}"))
        .find(|n| !taken(n))
        .expect("unbounded")
}

/// AE-style next default name: `"<base> N"` where N is one more than the highest
/// number already used by siblings named `"<base> n"` (e.g. "Shape Layer 3").
pub fn next_numbered_name(stage: &Stage, parent: &PrimPath, base: &str) -> String {
    let max = stage
        .children(parent)
        .iter()
        .filter_map(|p| {
            let name = display_name_of(stage, p);
            let rest = name.strip_prefix(base)?.trim_start();
            rest.parse::<u32>().ok()
        })
        .max()
        .unwrap_or(0);
    format!("{base} {}", max + 1)
}

/// Number of interpolated dimensions of an attribute's values (used for per-dimension
/// ease arrays). Spatial properties (position, anchor point) have one.
pub fn ease_dimensions(attr: &str, spec: &AttrSpec, value: &Value) -> usize {
    match bae_anim::schema::curve_kind(attr, spec) {
        bae_anim::CurveKind::Spatial => 1,
        bae_anim::CurveKind::PerDimension => value.dimensions().max(1),
    }
}

/// Value of an animated attribute at keyframe time `t` (no expressions), or the
/// static value / schema default when it has no keyframes.
pub fn sample_attr(stage: &Stage, path: &PrimPath, attr: &str, t: Time) -> Option<Value> {
    let view = stage.prim(path)?;
    match view.attr(attr) {
        Some(spec) if spec.is_animated() => {
            let curve =
                bae_anim::Curve::new(&spec.keyframes, bae_anim::schema::curve_kind(attr, spec))?;
            Some(curve.sample(t))
        }
        _ => static_value(&view, attr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_name("Shape Layer 1"), "Shape_Layer_1");
        assert_eq!(sanitize_name("形状图层 1"), "形状图层_1");
        assert_eq!(sanitize_name("--a.b"), "a_b");
        assert_eq!(sanitize_name("  "), "item");
    }
}
