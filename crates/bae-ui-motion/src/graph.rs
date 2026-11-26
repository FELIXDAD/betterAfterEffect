//! Graph Editor (AE-UI-077 … AE-UI-083, AE-ANI-059 … AE-ANI-063): the Timeline's
//! time graph as value and speed graphs of the shown properties.
//!
//! - **Shown properties** (Show Properties menu): the selected properties, the
//!   animated properties of the selected layers, the Graph Editor Set — any union.
//! - **Graph type**: Auto-Select (speed graph for spatial properties, value graph for
//!   the others), Edit Value Graph, Edit Speed Graph; the other type can be drawn
//!   behind as the reference graph.
//! - **Editing**: click / Shift-click / marquee selection, Alt-click a segment selects
//!   the property's keys; dragging keys (value graph: time and value, speed graph:
//!   time), Bezier handles (speed and influence; Alt splits a continuous key), the
//!   transform box around several keys (move, scale about the opposite edge or with
//!   Cmd/Ctrl about the centre); snapping; Cmd/Ctrl-click toggles Linear ↔ Auto Bezier.
//!   Drags are previewed with [`bae_motion::apply_key_edits`] and committed as one
//!   `key.edit` when released (one undo step).
//! - **Button bar**: Show Properties, Graph Type and Options, transform box, Snap,
//!   Auto-Zoom Graph Height, Fit Selection, Fit All, Separate Dimensions, Edit Selected
//!   Keyframes, Hold / Linear / Auto Bezier, Easy Ease / In / Out.
//!
//! Vertical scale: each property's graph has its own range (fitted continuously while
//! Auto-Zoom Graph Height is on), so curves of different units stay editable together;
//! the axis labels show the focused property's units.

use std::collections::{HashMap, HashSet};

use bae_anim::{Curve, CurveKind, LayerTiming};
use bae_model::{AttrSpec, Ease, Interpolation, Keyframe, PrimPath, Stage, Time, Value};
use bae_motion::query::{self, CompInfo};
use bae_motion::{KeyEdit, KeyRef};
use bae_ui::Accessible as _;
use bae_ui::theme::{self, color};
use bae_ui::widgets;
use bae_ui::{Invocation, PanelContext, tr};
use egui::{Align2, Color32, CursorIcon, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use serde_json::json;

use crate::actions::{self, ids};
use crate::session::SharedSession;
use crate::timeline::TimeAxis;

/// Height of the button bar at the bottom of the Graph Editor.
pub(crate) const BAR_H: f32 = 24.0;
/// Pick radius of keys and handles, in points.
const PICK: f32 = 6.0;
/// Snap distance, in points.
const SNAP_PX: f32 = 8.0;
/// Vertical padding of the plot inside the graph area.
const VPAD: f32 = 14.0;
/// Gap between the selected keys and the transform box.
const BOX_MARGIN: f32 = 4.0;

/// Graph Type and Options ▸ the graph type radio group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GraphType {
    /// Speed graph for spatial properties, value graph for the others.
    Auto,
    Value,
    Speed,
}

/// Graph Editor options (Show Properties and Graph Type and Options menus, button
/// bar toggles), with AE's defaults.
#[derive(Clone, Debug)]
pub(crate) struct GraphSettings {
    pub show_selected: bool,
    pub show_animated: bool,
    pub show_set: bool,
    pub graph_type: GraphType,
    pub show_reference: bool,
    pub show_waveforms: bool,
    pub show_in_out: bool,
    pub show_markers: bool,
    pub show_tooltips: bool,
    pub show_expression_editor: bool,
    pub allow_between_frames: bool,
    pub transform_box: bool,
    pub snap: bool,
    pub auto_zoom: bool,
}

impl Default for GraphSettings {
    fn default() -> Self {
        GraphSettings {
            show_selected: true,
            show_animated: true,
            show_set: false,
            graph_type: GraphType::Auto,
            show_reference: true,
            show_waveforms: true,
            show_in_out: true,
            show_markers: true,
            show_tooltips: true,
            show_expression_editor: false,
            allow_between_frames: false,
            transform_box: true,
            snap: false,
            auto_zoom: true,
        }
    }
}

/// What the Graph Editor asks the Timeline to do with the shared time axis.
#[derive(Debug, Default)]
pub(crate) struct GraphOut {
    /// Fit the time axis to this comp-time range.
    pub fit_time: Option<(Time, Time)>,
    /// Zoom the time axis (Alt+wheel), in zoom-slider units.
    pub zoom_by: f32,
}

/// (property, graph is a speed graph) → vertical range.
type RangeKey = (PrimPath, String, bool);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    In,
    Out,
}

/// Which box handle: -1 / 0 / 1 for left / none / right and top / none / bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BoxHandle {
    hx: i8,
    hy: i8,
}

#[derive(Clone, Debug)]
enum GDrag {
    /// Keys follow the pointer (also: dragging inside the transform box).
    Keys {
        origin: Pos2,
        grabbed: KeyRef,
        dim: usize,
        keys: Vec<KeyRef>,
    },
    Handle {
        key: KeyRef,
        dim: usize,
        side: Side,
        split: bool,
        /// The other selected keys: their handles on the same side follow.
        keys: Vec<KeyRef>,
    },
    Scale {
        origin: Pos2,
        handle: BoxHandle,
        bounds: Rect,
        keys: Vec<KeyRef>,
    },
    Marquee {
        origin: Pos2,
        additive: bool,
    },
}

/// Graph Editor state of a Timeline panel (view state; nothing here is saved in the
/// project).
#[derive(Default)]
pub(crate) struct GraphEditor {
    pub(crate) settings: GraphSettings,
    ranges: HashMap<RangeKey, (f64, f64)>,
    drag: Option<GDrag>,
    /// Value graph: the dimensions picked per selected keyframe (Scale X but not Y …).
    sel_dims: HashSet<(KeyRef, usize)>,
    /// The property whose units the vertical axis shows.
    focus: Option<(PrimPath, String)>,
    /// Expression being typed in the expression editor.
    expr_buf: Option<(PrimPath, String, String)>,
    /// The keyframe right-clicked for the context menu.
    menu_key: Option<KeyRef>,
    /// Sampled graphs by (property, speed graph): reused while the keyframes and
    /// the view stay the same, so idle frames and drags resample only what changed.
    cache: HashMap<RangeKey, CacheEntry>,
    /// Last frame's geometry, for tests.
    #[cfg(test)]
    pub(crate) last: TestGeometry,
}

/// Where the last frame drew things (tests drive the editor with these).
#[cfg(test)]
#[derive(Default)]
pub(crate) struct TestGeometry {
    pub keys: Vec<(KeyRef, usize, Pos2)>,
    pub handles: Vec<(KeyRef, usize, bool, Pos2)>,
    pub selection_box: Option<Rect>,
    pub vaxes: Vec<((PrimPath, String), VAxis)>,
    pub buttons: Vec<(String, Rect)>,
    /// Layer markers drawn (x).
    pub markers: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Shown properties and their numbers.

/// One property drawn in the Graph Editor.
#[derive(Clone, Debug)]
pub(crate) struct Prop {
    pub path: PrimPath,
    pub attr: String,
    pub layer: PrimPath,
    /// Keyframes as stored (or with the drag preview applied).
    pub spec: AttrSpec,
    /// One temporal ease drives the distance along a motion path.
    pub spatial: bool,
    /// Dimensions drawn (2 for Position / Scale of 2D layers).
    pub dims: usize,
    /// Colours: one shared ease, no per-channel handles.
    pub shared: bool,
    /// Drawn as a speed graph.
    pub speed: bool,
    pub clamp: Option<(f64, f64)>,
    pub timing: LayerTiming,
    /// Index among the shown properties (curve colour of 1D properties).
    pub index: usize,
}

impl Prop {
    fn comp(&self, t: Time) -> Time {
        self.timing.comp_time(t)
    }

    fn key_time(&self, t: Time) -> Time {
        self.timing.layer_time(t)
    }

    /// Keyframe seconds per comp second (100 / stretch).
    fn rate(&self) -> f64 {
        let s = if self.timing.stretch.abs() < 1.0 {
            100.0
        } else {
            self.timing.stretch
        };
        100.0 / s.abs()
    }

    fn id(&self) -> (PrimPath, String) {
        (self.path.clone(), self.attr.clone())
    }

    fn range_key(&self) -> RangeKey {
        (self.path.clone(), self.attr.clone(), self.speed)
    }

    fn curve(&self) -> Option<Curve> {
        let kind = if self.spatial {
            CurveKind::Spatial
        } else {
            CurveKind::PerDimension
        };
        let c = Curve::new(&self.spec.keyframes, kind)?;
        Some(match self.clamp {
            Some((lo, hi)) => c.with_clamp(lo, hi),
            None => c,
        })
    }

    /// Stored ease entries per side (1 for spatial properties and colours).
    fn ease_dims(&self) -> usize {
        if self.spatial || self.shared {
            1
        } else {
            self.spec
                .keyframes
                .values()
                .next()
                .map_or(1, |k| k.value.dimensions().max(1))
        }
    }

    /// Whether handles of this property can be edited in the current graph.
    fn has_handles(&self) -> bool {
        !self.shared && (self.speed || !self.spatial)
    }

    /// Number of curves drawn for the current graph type.
    fn curves(&self) -> usize {
        if self.speed && (self.spatial || self.shared) {
            1
        } else {
            self.dims
        }
    }
}

/// Numeric components of a value (time remap in seconds).
pub(crate) fn comps(v: &Value) -> Option<Vec<f64>> {
    Some(match v {
        Value::Float(x) => vec![*x],
        Value::Int(x) => vec![*x as f64],
        Value::Vec2(x) => x.to_vec(),
        Value::Vec3(x) => x.to_vec(),
        Value::Color(x) => x.to_vec(),
        Value::Time(t) => vec![t.as_seconds()],
        _ => return None,
    })
}

/// `template` with its components replaced.
pub(crate) fn recompose(template: &Value, c: &[f64]) -> Value {
    let g = |i: usize| c.get(i).copied().unwrap_or(0.0);
    match template {
        Value::Float(_) => Value::Float(g(0)),
        Value::Int(_) => Value::Int(g(0).round() as i64),
        Value::Vec2(_) => Value::Vec2([g(0), g(1)]),
        Value::Vec3(_) => Value::Vec3([g(0), g(1), g(2)]),
        Value::Color(_) => Value::Color([g(0), g(1), g(2), g(3)]),
        Value::Time(_) => Value::Time(Time::from_seconds(g(0))),
        other => other.clone(),
    }
}

/// Every animated, numeric property under `layer` (layer attributes, masks,
/// effects, shape contents), in a stable order.
fn animated_props(stage: &Stage, layer: &PrimPath, out: &mut Vec<(PrimPath, String)>) {
    let mut stack = vec![layer.clone()];
    while let Some(p) = stack.pop() {
        if let Some(v) = stage.prim(&p) {
            let mut names: Vec<&str> = v.attr_names().into_iter().collect();
            names.sort_unstable();
            for name in names {
                if v.attr(name).is_some_and(|s| s.is_animated()) {
                    out.push((p.clone(), name.to_owned()));
                }
            }
        }
        let mut children = stage.children(&p);
        children.reverse();
        stack.extend(children);
    }
}

/// The properties the Graph Editor shows (AE-UI-078), in timeline order.
pub(crate) fn shown_props(
    stage: &Stage,
    comp: &PrimPath,
    session: &SharedSession,
    settings: &GraphSettings,
) -> Vec<Prop> {
    let (layers, props, keys, set) = {
        let s = session.lock();
        (
            s.selected_layers.clone(),
            s.selected_props.clone(),
            s.selected_keys.clone(),
            s.graph_set.clone(),
        )
    };
    let mut wanted: Vec<(PrimPath, String)> = Vec::new();
    let mut add = |id: (PrimPath, String)| {
        if !wanted.contains(&id) {
            wanted.push(id);
        }
    };
    if settings.show_selected {
        for p in &props {
            add(p.clone());
        }
        for k in &keys {
            add((k.path.clone(), k.attr.clone()));
        }
    }
    if settings.show_animated {
        for l in &layers {
            let mut v = Vec::new();
            animated_props(stage, l, &mut v);
            for id in v {
                add(id);
            }
        }
    }
    if settings.show_set {
        for id in &set {
            add(id.clone());
        }
    }
    // Timeline order: by layer, then as gathered.
    let order = query::comp_layers(stage, comp);
    let owners: Vec<Option<usize>> = wanted
        .iter()
        .map(|(path, _)| {
            let layer = query::owning_layer(stage, path)?;
            order.iter().position(|l| *l == layer)
        })
        .collect();
    let mut picked: Vec<(usize, usize)> = owners
        .iter()
        .enumerate()
        .filter_map(|(i, o)| o.map(|li| (li, i)))
        .collect();
    picked.sort_by_key(|(li, i)| (*li, *i));
    let mut out: Vec<Prop> = Vec::with_capacity(picked.len());
    for (li, i) in picked {
        let (path, attr) = &wanted[i];
        if let Some(p) = make_prop(stage, path, attr, &order[li], settings, out.len()) {
            out.push(p);
        }
    }
    out
}

fn make_prop(
    stage: &Stage,
    path: &PrimPath,
    attr: &str,
    layer: &PrimPath,
    settings: &GraphSettings,
    index: usize,
) -> Option<Prop> {
    let spec = query::attr_spec(stage, path, attr)?.clone();
    let first = spec.keyframes.values().next()?;
    let n = comps(&first.value)?.len();
    let spatial = bae_anim::schema::curve_kind(attr, &spec) == CurveKind::Spatial;
    let shared = matches!(first.value, Value::Color(_));
    let three_d = crate::common::is_3d(stage, layer);
    let dims = if n == 3 && !three_d && attr.starts_with("transform:") {
        2
    } else {
        n
    };
    let ty = stage.prim(path).and_then(|v| v.prim_type().cloned());
    let speed = match settings.graph_type {
        GraphType::Auto => spatial,
        GraphType::Value => false,
        GraphType::Speed => true,
    };
    Some(Prop {
        path: path.clone(),
        attr: attr.to_owned(),
        layer: layer.clone(),
        spatial,
        dims,
        shared,
        speed,
        clamp: bae_anim::schema::clamp_range(ty.as_ref(), attr),
        timing: query::key_timing(stage, path),
        index,
        spec,
    })
}

// ---------------------------------------------------------------------------
// Sampling.

/// One polyline of a graph in (x in points, graph units).
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub dim: usize,
    pub pts: Vec<(f32, f64)>,
}

/// A property's sampled graph (curves and key values) for one graph type.
#[derive(Debug, Default)]
pub(crate) struct Sampled {
    pub pieces: Vec<Piece>,
    key_vals: Vec<(Time, Vec<f64>)>,
    /// Data range of `pieces` (computed once, used by every fitted frame).
    range: Option<(f64, f64)>,
}

impl Sampled {
    fn new(pieces: Vec<Piece>, key_vals: Vec<(Time, Vec<f64>)>) -> Self {
        let range = data_range(&pieces, std::iter::empty());
        Self {
            pieces,
            key_vals,
            range,
        }
    }

    /// Data range including `extra` values.
    fn range_with(&self, extra: impl Iterator<Item = f64>) -> Option<(f64, f64)> {
        extra.filter(|v| v.is_finite()).fold(self.range, |r, v| {
            Some(r.map_or((v, v), |(a, b)| (a.min(v), b.max(v))))
        })
    }
}

/// What a [`Sampled`] graph depends on besides the keyframes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Stamp {
    timing: LayerTiming,
    axis: (u32, u32, Time, Time),
    range: (u32, u32),
    dims: usize,
    spatial: bool,
    shared: bool,
    clamp: Option<(f64, f64)>,
}

struct CacheEntry {
    spec: AttrSpec,
    stamp: Stamp,
    data: std::rc::Rc<Sampled>,
    used: bool,
}

fn sorted(a: Time, b: Time) -> (Time, Time) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Value of dimension `d` at keyframe time `t`.
fn value_at(curve: &Curve, t: Time, dims: usize) -> Vec<f64> {
    let mut c = comps(&curve.sample(t)).unwrap_or_default();
    c.resize(dims.max(c.len()), 0.0);
    c
}

/// Speeds (graph units per comp second) at keyframe time `t` inside the segment
/// `[ta, tb]`: one per drawn curve.
fn speed_at(p: &Prop, curve: &Curve, t: Time, ta: Time, tb: Time) -> Vec<f64> {
    let span = (tb - ta).0.max(1);
    let h = Time((Time::FLICKS_PER_SECOND / 10_000).min(span / 1000).max(1));
    let (lo, hi) = if t - h < ta {
        (t, (t + h).min(tb))
    } else if t + h > tb {
        ((t - h).max(ta), t)
    } else {
        (t - h, t + h)
    };
    let dt = (hi - lo).as_seconds();
    if dt <= 0.0 {
        return vec![0.0; p.curves()];
    }
    let a = value_at(curve, lo, p.dims);
    let b = value_at(curve, hi, p.dims);
    let rate = p.rate();
    if p.spatial || p.shared {
        let d: f64 = (0..p.dims.min(a.len()))
            .map(|i| {
                let k = if p.shared { 255.0 } else { 1.0 };
                ((b[i] - a[i]) * k).powi(2)
            })
            .sum::<f64>()
            .sqrt();
        vec![d / dt * rate]
    } else {
        (0..p.dims).map(|i| (b[i] - a[i]) / dt * rate).collect()
    }
}

/// The curves of `p` (compiled as `curve`) over the visible x range
/// `[left, right]`: value graph or speed graph (`speed`), sampled about every two
/// points.
pub(crate) fn sample(
    p: &Prop,
    curve: &Curve,
    axis: &TimeAxis,
    left: f32,
    right: f32,
    speed: bool,
) -> Vec<Piece> {
    let keys: Vec<(Time, &Keyframe)> = p.spec.keyframes.iter().map(|(t, k)| (*t, k)).collect();
    let n = p.curves();
    let mut out: Vec<Piece> = Vec::new();
    let mut push = |dim: usize, pts: Vec<(f32, f64)>| {
        if pts.len() >= 2 {
            out.push(Piece { dim, pts });
        }
    };
    let x_of = |t: Time| axis.x(p.comp(t));
    if !speed {
        let first = &keys[0];
        let last = &keys[keys.len() - 1];
        let v0 = value_at(curve, first.0, p.dims);
        let vn = value_at(curve, last.0, p.dims);
        let (x0, xn) = (x_of(first.0), x_of(last.0));
        for d in 0..n {
            if x0.min(xn) > left {
                let (xa, va) = if x0 < xn { (x0, v0[d]) } else { (xn, vn[d]) };
                push(d, vec![(left, va), (xa, va)]);
            }
            if x0.max(xn) < right {
                let (xb, vb) = if x0 < xn { (xn, vn[d]) } else { (x0, v0[d]) };
                push(d, vec![(xb, vb), (right, vb)]);
            }
        }
    }
    for w in keys.windows(2) {
        let ((ta, a), (tb, b)) = (w[0], w[1]);
        let (xa, xb) = (x_of(ta), x_of(tb));
        let (xl, xr) = (xa.min(xb).max(left), xa.max(xb).min(right));
        if xl > xr {
            continue;
        }
        let hold_out = a.interp_out == Interpolation::Hold;
        let hold_in = b.interp_in == Interpolation::Hold;
        if speed {
            if hold_out || hold_in {
                continue;
            }
        } else if hold_out || hold_in {
            let va = value_at(curve, ta, p.dims);
            let vb = value_at(curve, tb, p.dims);
            for d in 0..n {
                let pts = if hold_out {
                    vec![(xa, va[d]), (xb, va[d]), (xb, vb[d])]
                } else {
                    vec![(xa, va[d]), (xa, vb[d]), (xb, vb[d])]
                };
                push(d, pts);
            }
            continue;
        }
        let steps = (((xr - xl) / 2.0).ceil() as usize).clamp(1, 1024);
        let (lo, hi) = sorted(ta, tb);
        let mut per: Vec<Vec<(f32, f64)>> = vec![Vec::with_capacity(steps + 1); n];
        for j in 0..=steps {
            let x = xl + (xr - xl) * j as f32 / steps as f32;
            // Exact key times at the ends, so segments meet at the keys.
            let t = if j == 0 && (xl - xa.min(xb)).abs() < 0.01 {
                if xa <= xb { ta } else { tb }
            } else if j == steps && (xr - xa.max(xb)).abs() < 0.01 {
                if xa <= xb { tb } else { ta }
            } else {
                p.key_time(axis.t(x)).clamp(lo, hi)
            };
            let v = if speed {
                speed_at(p, curve, t, lo, hi)
            } else {
                value_at(curve, t, p.dims)
            };
            for (d, pts) in per.iter_mut().enumerate() {
                pts.push((x, v.get(d).copied().unwrap_or(0.0)));
            }
        }
        for (d, pts) in per.into_iter().enumerate() {
            push(d, pts);
        }
    }
    out
}

/// Data range of pieces (and extra values).
fn data_range(pieces: &[Piece], extra: impl Iterator<Item = f64>) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for v in pieces
        .iter()
        .flat_map(|p| p.pts.iter().map(|(_, v)| *v))
        .chain(extra)
    {
        if v.is_finite() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// A range with room around the data (and never empty).
fn padded((lo, hi): (f64, f64)) -> (f64, f64) {
    let span = hi - lo;
    // Flat when the spread is rounding noise next to the values (a linear
    // segment's speed samples differ by ~1e-9): fitting it would blow the noise
    // up to the full height (#552).
    if span.abs() <= 1e-9 * lo.abs().max(hi.abs()).max(1.0) {
        let m = lo.abs().max(1.0) * 0.1;
        (lo - m, hi + m)
    } else {
        (lo - span * 0.05, hi + span * 0.05)
    }
}

/// Vertical mapping of one graph.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VAxis {
    pub lo: f64,
    pub hi: f64,
    pub top: f32,
    pub bottom: f32,
}

impl VAxis {
    fn new(range: (f64, f64), rect: Rect) -> Self {
        VAxis {
            lo: range.0,
            hi: range.1,
            top: rect.top() + VPAD,
            bottom: rect.bottom() - VPAD,
        }
    }

    pub fn y(&self, v: f64) -> f32 {
        let f = (v - self.lo) / (self.hi - self.lo).max(1e-12);
        self.bottom - (f as f32) * (self.bottom - self.top)
    }

    pub fn v(&self, y: f32) -> f64 {
        let f = f64::from((self.bottom - y) / (self.bottom - self.top).max(1.0));
        self.lo + f * (self.hi - self.lo)
    }

    /// Graph units per point.
    pub fn per_point(&self) -> f64 {
        (self.hi - self.lo) / f64::from((self.bottom - self.top).max(1.0))
    }
}

// ---------------------------------------------------------------------------
// Keys and handles.

/// A key drawn in a graph: one point per drawn curve.
#[derive(Clone, Debug)]
struct KeyPt {
    key: KeyRef,
    prop: usize,
    dim: usize,
    pos: Pos2,
}

#[derive(Clone, Debug)]
struct HandlePt {
    key: KeyRef,
    prop: usize,
    dim: usize,
    side: Side,
    anchor: Pos2,
    pos: Pos2,
}

/// Ease of side `side`, dimension `d` (falling back to the first entry / default).
fn ease_of(k: &Keyframe, side: Side, d: usize) -> Ease {
    let e = match side {
        Side::In => &k.ease_in,
        Side::Out => &k.ease_out,
    };
    e.get(d).or_else(|| e.first()).copied().unwrap_or_default()
}

/// Graph values at each key (value graph: the key's components; speed graph: the
/// speed leaving the key, arriving for the last key, 0 next to Hold).
fn key_values(p: &Prop, curve: &Curve, speed: bool) -> Vec<(Time, Vec<f64>)> {
    let keys: Vec<(Time, &Keyframe)> = p.spec.keyframes.iter().map(|(t, k)| (*t, k)).collect();
    let n = if speed && (p.spatial || p.shared) {
        1
    } else {
        p.dims
    };
    let mut out = Vec::with_capacity(keys.len());
    for (i, (t, k)) in keys.iter().enumerate() {
        let ys: Vec<f64> = if speed {
            let seg = if i + 1 < keys.len() {
                Some((*t, keys[i + 1].0, k.interp_out, keys[i + 1].1.interp_in))
            } else if i > 0 {
                Some((keys[i - 1].0, *t, keys[i - 1].1.interp_out, k.interp_in))
            } else {
                None
            };
            match seg {
                Some((a, b, o, inn)) if o != Interpolation::Hold && inn != Interpolation::Hold => {
                    let (lo, hi) = sorted(a, b);
                    speed_at(p, curve, *t, lo, hi)
                }
                _ => vec![0.0; n],
            }
        } else {
            let mut c = comps(&k.value).unwrap_or_default();
            c.truncate(p.dims);
            c
        };
        out.push((*t, ys));
    }
    out
}

/// Key points of `p` in its graph, from [`key_values`].
fn key_points(
    p: &Prop,
    pi: usize,
    axis: &TimeAxis,
    va: &VAxis,
    values: &[(Time, Vec<f64>)],
) -> Vec<KeyPt> {
    let mut out = Vec::new();
    for (t, ys) in values {
        let x = axis.x(p.comp(*t));
        for (d, v) in ys.iter().enumerate().take(p.curves()) {
            out.push(KeyPt {
                key: KeyRef::new(p.path.clone(), p.attr.clone(), *t),
                prop: pi,
                dim: d,
                pos: pos2(x, va.y(*v)),
            });
        }
    }
    out
}

/// Bezier handles of the selected keys of `p` (AE-ANI-060 / AE-ANI-061).
fn handle_points(
    p: &Prop,
    pi: usize,
    axis: &TimeAxis,
    va: &VAxis,
    selected: &[KeyRef],
) -> Vec<HandlePt> {
    if !p.has_handles() {
        return Vec::new();
    }
    let keys: Vec<(Time, &Keyframe)> = p.spec.keyframes.iter().map(|(t, k)| (*t, k)).collect();
    let rate = p.rate();
    let mut out = Vec::new();
    for (i, (t, k)) in keys.iter().enumerate() {
        let kr = KeyRef::new(p.path.clone(), p.attr.clone(), *t);
        if !selected.contains(&kr) {
            continue;
        }
        let vals = comps(&k.value).unwrap_or_default();
        for (side, neighbour) in [
            (Side::Out, keys.get(i + 1)),
            (Side::In, i.checked_sub(1).and_then(|j| keys.get(j))),
        ] {
            let interp = match side {
                Side::In => k.interp_in,
                Side::Out => k.interp_out,
            };
            let Some((tn, _)) = neighbour else {
                continue;
            };
            if interp != Interpolation::Bezier {
                continue;
            }
            let dt = (*tn - *t).as_seconds().abs();
            let sign = if side == Side::Out { 1.0 } else { -1.0 };
            for d in 0..p.curves() {
                let e = ease_of(k, side, d);
                let inf = (e.influence / 100.0).clamp(0.0, 1.0);
                let ht = *t + Time::from_seconds(sign * inf * dt);
                let x = axis.x(p.comp(ht));
                let (anchor_y, y) = if p.speed {
                    (va.y(e.speed * rate), va.y(e.speed * rate))
                } else {
                    let v = vals.get(d).copied().unwrap_or(0.0);
                    (va.y(v), va.y(v + sign * e.speed * inf * dt))
                };
                out.push(HandlePt {
                    key: kr.clone(),
                    prop: pi,
                    dim: d,
                    side,
                    anchor: pos2(axis.x(p.comp(*t)), anchor_y),
                    pos: pos2(x, y),
                });
            }
        }
    }
    out
}

/// The ease that dragging the `side` handle of key `key` (dimension `dim`) to the
/// pointer at key time `tp` and graph value `vp` gives.
pub(crate) fn handle_ease(
    p: &Prop,
    key: Time,
    dim: usize,
    side: Side,
    tp: Time,
    vp: f64,
) -> Option<Ease> {
    let (t, k, neighbour) = handle_segment(p, key, side)?;
    let dt = (neighbour - t).as_seconds().abs().max(1e-9);
    // Distance of the pointer from the key towards the neighbour, in key seconds.
    let toward = (tp - t).as_seconds() * (neighbour - t).as_seconds().signum();
    let toward = toward.max(dt * 0.001);
    let influence = (toward / dt * 100.0).clamp(0.1, 100.0);
    let speed = if p.speed {
        let s = vp / p.rate();
        if p.spatial { s.max(0.0) } else { s }
    } else {
        let v = comps(&k.value)?.get(dim).copied()?;
        match side {
            Side::Out => (vp - v) / toward,
            Side::In => (v - vp) / toward,
        }
    };
    Some(Ease { speed, influence })
}

/// The key at `key` and the neighbouring key time on `side`.
fn handle_segment(p: &Prop, key: Time, side: Side) -> Option<(Time, &Keyframe, Time)> {
    let mut before = p.spec.keyframes.range(..key);
    let mut after = p.spec.keyframes.range(key..);
    let (t, k) = after.next()?;
    if *t != key {
        return None;
    }
    let neighbour = match side {
        Side::Out => *after.next()?.0,
        Side::In => *before.next_back()?.0,
    };
    Some((*t, k, neighbour))
}

/// Average speed of the segment on `side` of key `key` in dimension `dim` (graph
/// units per key second, signed the way handle speeds are): the speed a straight
/// line would have. Spatial properties use the straight distance.
fn segment_speed(p: &Prop, key: Time, dim: usize, side: Side) -> Option<f64> {
    let (t, k, neighbour) = handle_segment(p, key, side)?;
    let dt = (neighbour - t).as_seconds().abs().max(1e-9);
    let a = comps(&k.value)?;
    let b = comps(&p.spec.keyframes.get(&neighbour)?.value)?;
    let delta = |i: usize| b.get(i).copied().unwrap_or(0.0) - a.get(i).copied().unwrap_or(0.0);
    if p.spatial {
        let n = p.dims.min(a.len());
        return Some((0..n).map(|i| delta(i).powi(2)).sum::<f64>().sqrt() / dt);
    }
    let d = delta(dim);
    Some(match side {
        Side::Out => d / dt,
        Side::In => -d / dt,
    })
}

/// The edit that sets the `side` handle of key `key` to `eases` (dimension, ease).
/// A continuous key turns its other handle to the same speed unless `split`;
/// Auto Bezier keys become continuous Bezier.
pub(crate) fn set_handle(
    p: &Prop,
    key: Time,
    side: Side,
    split: bool,
    eases: &[(usize, Ease)],
) -> Option<KeyEdit> {
    let (t, k, _) = handle_segment(p, key, side)?;
    let n = p.ease_dims();
    let full = |side: Side| -> Vec<Ease> { (0..n).map(|j| ease_of(k, side, j)).collect() };
    let mut this = full(side);
    for (dim, e) in eases {
        this[(*dim).min(n - 1)] = *e;
    }
    let continuous = k.flags.temporal_continuous && !split;
    let mut edit = KeyEdit::new(KeyRef::new(p.path.clone(), p.attr.clone(), t));
    let opposite = match side {
        Side::In => Side::Out,
        Side::Out => Side::In,
    };
    let other_interp = match opposite {
        Side::In => k.interp_in,
        Side::Out => k.interp_out,
    };
    let mut other = None;
    if continuous && other_interp == Interpolation::Bezier {
        let mut o = full(opposite);
        for (dim, e) in eases {
            o[(*dim).min(n - 1)].speed = e.speed;
        }
        other = Some(o);
    }
    match side {
        Side::Out => {
            edit.interp_out = Some(Interpolation::Bezier);
            edit.ease_out = Some(this);
            edit.ease_in = other;
        }
        Side::In => {
            edit.interp_in = Some(Interpolation::Bezier);
            edit.ease_in = Some(this);
            edit.ease_out = other;
        }
    }
    edit.auto_bezier = Some(false);
    edit.continuous = Some(continuous);
    Some(edit)
}

/// Dragging the `side` handle of key `key` (dimension `dim`) to the pointer at key
/// time `tp` and graph value `vp` (AE-ANI-060 / AE-ANI-061).
#[cfg(test)]
pub(crate) fn drag_handle(
    p: &Prop,
    key: Time,
    dim: usize,
    side: Side,
    split: bool,
    tp: Time,
    vp: f64,
) -> Option<KeyEdit> {
    let e = handle_ease(p, key, dim, side, tp, vp)?;
    set_handle(p, key, side, split, &[(dim, e)])
}

/// Rounded to the comp's frame grid.
fn round_frame(info: &CompInfo, t: Time) -> Time {
    info.frame_rate.snap(t + Time(info.frame_duration().0 / 2))
}

/// The nearest candidate within `tolerance` of `t`.
pub(crate) fn snap_to(t: Time, candidates: &[Time], tolerance: Time) -> Option<Time> {
    candidates
        .iter()
        .copied()
        .filter(|c| (*c - t).0.abs() <= tolerance.0)
        .min_by_key(|c| (*c - t).0.abs())
}

// ---------------------------------------------------------------------------
// Drawing helpers.

const RED: Color32 = Color32::from_rgb(0xe0, 0x4f, 0x4f);
const GREEN: Color32 = Color32::from_rgb(0x6c, 0xc4, 0x4a);
const BLUE: Color32 = Color32::from_rgb(0x4f, 0x8d, 0xe8);
const KEY_SELECTED: Color32 = Color32::from_rgb(0xf2, 0xd0, 0x3b);
const SNAP: Color32 = Color32::from_rgb(0xff, 0x8c, 0x00);
const REFERENCE: Color32 = Color32::from_gray(0x5c);
/// Curve colours of one-dimensional properties, by shown index.
const PALETTE: [Color32; 5] = [
    Color32::from_rgb(0xd8, 0x5c, 0xc8),
    Color32::from_rgb(0x4f, 0xc8, 0xd8),
    Color32::from_rgb(0xe8, 0xa0, 0x4f),
    Color32::from_rgb(0xa8, 0xd8, 0x4f),
    Color32::from_rgb(0xb0, 0x8c, 0xf0),
];

fn curve_color(p: &Prop, dim: usize) -> Color32 {
    if p.curves() == 1 {
        return PALETTE[p.index % PALETTE.len()];
    }
    match dim {
        0 => RED,
        1 => GREEN,
        2 => BLUE,
        _ => Color32::from_gray(0xc0),
    }
}

/// Unit drawn after a value of `attr`.
fn unit(attr: &str) -> String {
    let u: &str = match attr {
        "transform:scale" | "transform:opacity" | "opacity" | "roundness" => "%",
        "transform:rotation"
        | "transform:rotation_x"
        | "transform:rotation_y"
        | "rotation"
        | "transform:skew"
        | "transform:skew_axis"
        | "transform:orientation" => "°",
        // AE's Chinese UI says 像素 (#197).
        a if a.contains("position") || a.contains("anchor") || a == "size" => {
            return format!(" {}", tr("px"));
        }
        _ => "",
    };
    u.to_owned()
}

pub(crate) fn fmt_num(v: f64) -> String {
    // No "-0" for values that are zero up to rounding.
    let v = if v.abs() < 1e-9 { 0.0 } else { v };
    let a = v.abs();
    if a >= 100.0 || v == v.round() {
        format!("{v:.0}")
    } else if a >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

/// A 1-2-5 tick step giving about `count` ticks over `span`.
fn nice_step(span: f64, count: f64) -> f64 {
    let raw = (span / count.max(1.0)).abs().max(1e-12);
    let mag = 10f64.powf(raw.log10().floor());
    let f = raw / mag;
    let n = if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    n * mag
}

fn polyline(p: &egui::Painter, piece: &Piece, va: &VAxis, stroke: Stroke) {
    let pts: Vec<Pos2> = piece
        .pts
        .iter()
        .map(|(x, v)| pos2(*x, va.y(*v).clamp(-1.0e5, 1.0e5)))
        .collect();
    p.add(Shape::line(pts, stroke));
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t).distance(p)
}

/// The eight box handles: (position, which).
fn box_handles(r: Rect) -> [(Pos2, BoxHandle); 8] {
    let h = |hx: i8, hy: i8| {
        let x = match hx {
            -1 => r.left(),
            1 => r.right(),
            _ => r.center().x,
        };
        let y = match hy {
            -1 => r.top(),
            1 => r.bottom(),
            _ => r.center().y,
        };
        (pos2(x, y), BoxHandle { hx, hy })
    };
    [
        h(-1, -1),
        h(0, -1),
        h(1, -1),
        h(-1, 0),
        h(1, 0),
        h(-1, 1),
        h(0, 1),
        h(1, 1),
    ]
}

// ---------------------------------------------------------------------------
// The editor.

/// What lies under the pointer, in pick order.
enum Hit {
    Handle(HandlePt),
    BoxHandle(BoxHandle),
    Key(KeyPt),
    InBox,
    Segment(usize),
    Nothing,
}

/// Per-frame geometry of the shown graphs.
struct Frame {
    props: Vec<Prop>,
    vaxes: Vec<VAxis>,
    pieces: Vec<std::rc::Rc<Sampled>>,
    /// Reference graphs (the other graph type), when shown.
    reference: Vec<Option<std::rc::Rc<Sampled>>>,
    keys: Vec<KeyPt>,
    handles: Vec<HandlePt>,
    /// Tight bounds of the selected keys (screen); the transform box is drawn
    /// [`BOX_MARGIN`] outside.
    selection_box: Option<Rect>,
}

impl GraphEditor {
    /// Is `(key, dim)` picked? Keys selected elsewhere (layer bar mode) count as
    /// picked in every dimension.
    fn picked(&self, key: &KeyRef, dim: usize) -> bool {
        let any = self.sel_dims.iter().any(|(k, _)| k == key);
        !any || self.sel_dims.contains(&(key.clone(), dim))
    }

    fn range_for(&self, p: &Prop, data: &Sampled, fit: bool) -> (f64, f64) {
        let stored = self.ranges.get(&p.range_key()).copied();
        match (fit, stored) {
            (false, Some(r)) => r,
            _ => padded(data.range.unwrap_or((0.0, 1.0))),
        }
    }

    /// Edits the current drag makes with the pointer at `pointer`, and where it
    /// snapped (vertical line x, horizontal line y).
    #[allow(clippy::too_many_arguments)]
    fn drag_edits(
        &self,
        props: &[Prop],
        vaxes: &[VAxis],
        keys_now: &[KeyPt],
        info: &CompInfo,
        axis: &TimeAxis,
        pointer: Pos2,
        mods: egui::Modifiers,
        snap_times: &[Time],
    ) -> (Vec<KeyEdit>, Option<f32>, Option<f32>) {
        let find = |k: &KeyRef| {
            props
                .iter()
                .position(|p| p.path == k.path && p.attr == k.attr)
        };
        let snapping = self.settings.snap != mods.command;
        let mut vline = None;
        let mut hline = None;
        let mut edits: Vec<KeyEdit> = Vec::new();
        match &self.drag {
            Some(GDrag::Keys {
                origin,
                grabbed,
                dim,
                keys,
            }) => {
                let Some(gi) = find(grabbed) else {
                    return (edits, None, None);
                };
                let gp = &props[gi];
                let mut d = pointer - *origin;
                if mods.shift {
                    if d.x.abs() > d.y.abs() {
                        d.y = 0.0;
                    } else {
                        d.x = 0.0;
                    }
                }
                let ct0 = gp.comp(grabbed.time);
                let mut nct = ct0 + Time((f64::from(d.x) * axis.per_point()) as i64);
                if !self.settings.allow_between_frames {
                    nct = round_frame(info, nct);
                }
                if snapping {
                    let tol = Time((f64::from(SNAP_PX) * axis.per_point()) as i64);
                    if let Some(s) = snap_to(nct, snap_times, tol) {
                        nct = s;
                        vline = Some(axis.x(s));
                    }
                }
                if d.x == 0.0 {
                    nct = ct0;
                }
                let dct = nct - ct0;
                // Value change of the grabbed point, snapped to other key values.
                let mut dy = d.y;
                if !gp.speed && dy != 0.0 {
                    let va = &vaxes[gi];
                    let v0 = gp
                        .spec
                        .keyframes
                        .get(&grabbed.time)
                        .and_then(|k| comps(&k.value))
                        .and_then(|c| c.get(*dim).copied())
                        .unwrap_or(0.0);
                    let nv = v0 - f64::from(dy) * va.per_point();
                    if snapping {
                        let targets = keys_now.iter().filter(|kp| {
                            kp.prop == gi && kp.dim == *dim && !keys.contains(&kp.key)
                        });
                        let y = va.y(nv);
                        if let Some(t) = targets
                            .map(|kp| kp.pos.y)
                            .filter(|ty| (ty - y).abs() <= SNAP_PX)
                            .min_by(|a, b| (a - y).abs().total_cmp(&(b - y).abs()))
                        {
                            dy = t - va.y(v0);
                            hline = Some(t);
                        }
                    }
                }
                for k in keys {
                    let Some(pi) = find(k) else {
                        continue;
                    };
                    let p = &props[pi];
                    let Some(kf) = p.spec.keyframes.get(&k.time) else {
                        continue;
                    };
                    let mut e = KeyEdit::new(k.clone());
                    if dct != Time::ZERO {
                        let nt = p.key_time(p.comp(k.time) + dct);
                        e.time = Some(nt);
                        if kf.flags.roving {
                            e.roving = Some(false);
                        }
                    }
                    if !p.speed && dy != 0.0 {
                        let dv = -f64::from(dy) * vaxes[pi].per_point();
                        if let Some(mut c) = comps(&kf.value) {
                            for (j, cj) in c.iter_mut().enumerate().take(p.dims) {
                                let move_it = if k == grabbed {
                                    j == *dim
                                        || (self.sel_dims.contains(&(k.clone(), j))
                                            && self.sel_dims.contains(&(k.clone(), *dim)))
                                } else {
                                    self.picked(k, j)
                                };
                                if move_it {
                                    *cj += dv;
                                }
                            }
                            e.value = Some(recompose(&kf.value, &c));
                        }
                    }
                    if e.time.is_some() || e.value.is_some() {
                        edits.push(e);
                    }
                }
            }
            Some(GDrag::Handle {
                key,
                dim,
                side,
                split,
                keys,
            }) => {
                if let Some(pi) = find(key) {
                    let p = &props[pi];
                    let va = &vaxes[pi];
                    let tp = p.key_time(axis.t(pointer.x));
                    let mut vp = va.v(pointer.y);
                    if !p.speed && mods.shift {
                        // Shift keeps the handle level (speed 0).
                        if let Some(v) = p
                            .spec
                            .keyframes
                            .get(&key.time)
                            .and_then(|k| comps(&k.value))
                            .and_then(|c| c.get(*dim).copied())
                        {
                            vp = v;
                        }
                    } else if p.speed && snapping && (pointer.y - va.y(0.0)).abs() <= SNAP_PX {
                        // Snap to the 0 speed line.
                        vp = 0.0;
                        hline = Some(va.y(0.0));
                    }
                    if let Some(ease) = handle_ease(p, key.time, *dim, *side, tp, vp) {
                        edits.extend(
                            self.follow_handle(props, key, *dim, *side, *split, ease, keys),
                        );
                    }
                }
            }
            Some(GDrag::Scale {
                origin,
                handle,
                bounds,
                keys,
            }) => {
                let about_center = mods.command;
                let fixed = |lo: f32, hi: f32, h: i8| {
                    if about_center {
                        (lo + hi) / 2.0
                    } else if h < 0 {
                        hi
                    } else {
                        lo
                    }
                };
                let start = |lo: f32, hi: f32, h: i8| if h < 0 { lo } else { hi };
                let fx = fixed(bounds.left(), bounds.right(), handle.hx);
                let fy = fixed(bounds.top(), bounds.bottom(), handle.hy);
                let scale = |h: i8, p: f32, o: f32, f: f32, s: f32| -> f32 {
                    if h == 0 || (s - f).abs() < 1e-3 {
                        1.0
                    } else {
                        (s + (p - o) - f) / (s - f)
                    }
                };
                let mut sx = scale(
                    handle.hx,
                    pointer.x,
                    origin.x,
                    fx,
                    start(bounds.left(), bounds.right(), handle.hx),
                );
                let mut sy = scale(
                    handle.hy,
                    pointer.y,
                    origin.y,
                    fy,
                    start(bounds.top(), bounds.bottom(), handle.hy),
                );
                if mods.shift && handle.hx != 0 && handle.hy != 0 {
                    let s = sx.abs().max(sy.abs());
                    sx = s * sx.signum();
                    sy = s * sy.signum();
                }
                for k in keys {
                    let Some(pi) = find(k) else {
                        continue;
                    };
                    let p = &props[pi];
                    let Some(kf) = p.spec.keyframes.get(&k.time) else {
                        continue;
                    };
                    let pts: Vec<&KeyPt> = keys_now.iter().filter(|kp| kp.key == *k).collect();
                    let Some(first) = pts.first() else {
                        continue;
                    };
                    let mut e = KeyEdit::new(k.clone());
                    let nx = fx + (first.pos.x - fx) * sx;
                    let mut nct = axis.t(nx);
                    if !self.settings.allow_between_frames {
                        nct = round_frame(info, nct);
                    }
                    let nt = p.key_time(nct);
                    if nt != k.time {
                        e.time = Some(nt);
                    }
                    if !p.speed
                        && (sy - 1.0).abs() > 1e-6
                        && let Some(mut c) = comps(&kf.value)
                    {
                        for kp in &pts {
                            if self.picked(k, kp.dim) {
                                let ny = fy + (kp.pos.y - fy) * sy;
                                if let Some(cj) = c.get_mut(kp.dim) {
                                    *cj = vaxes[pi].v(ny);
                                }
                            }
                        }
                        e.value = Some(recompose(&kf.value, &c));
                    }
                    if e.time.is_some() || e.value.is_some() {
                        edits.push(e);
                    }
                }
            }
            _ => {}
        }
        (edits, vline, hline)
    }

    /// Edits for a handle drag that gives `ease` to the `side` handle of `key` in
    /// `dim`: the same handle of every other selected key (and the key's other
    /// selected dimensions) takes the same influence and the same speed relative
    /// to its segment's average speed, so all the curves keep one shape.
    #[allow(clippy::too_many_arguments)]
    fn follow_handle(
        &self,
        props: &[Prop],
        key: &KeyRef,
        dim: usize,
        side: Side,
        split: bool,
        ease: Ease,
        selected: &[KeyRef],
    ) -> Vec<KeyEdit> {
        let find = |k: &KeyRef| props.iter().find(|p| p.path == k.path && p.attr == k.attr);
        let Some(p0) = find(key) else {
            return Vec::new();
        };
        let ratio = segment_speed(p0, key.time, dim, side)
            .filter(|a| a.abs() > 1e-9)
            .map(|a| ease.speed / a);
        let mut edits = Vec::new();
        let mut keys: Vec<&KeyRef> = vec![key];
        keys.extend(selected.iter().filter(|k| *k != key));
        for k in keys {
            let Some(p) = find(k) else {
                continue;
            };
            if !p.has_handles() {
                continue;
            }
            let Some((_, kf, _)) = handle_segment(p, k.time, side) else {
                continue;
            };
            let interp = match side {
                Side::In => kf.interp_in,
                Side::Out => kf.interp_out,
            };
            let mut eases = Vec::new();
            for d in 0..p.ease_dims() {
                if k == key && d == dim {
                    eases.push((d, ease));
                } else if interp == Interpolation::Bezier && self.picked(k, d) {
                    let mut e = ease_of(kf, side, d);
                    e.influence = ease.influence;
                    if let Some(r) = ratio
                        && let Some(a) = segment_speed(p, k.time, d, side)
                        && a.abs() > 1e-9
                    {
                        e.speed = if p.spatial { (r * a).max(0.0) } else { r * a };
                    }
                    eases.push((d, e));
                }
            }
            if eases.is_empty() {
                continue;
            }
            if let Some(e) = set_handle(p, k.time, side, split, &eases) {
                edits.push(e);
            }
        }
        edits
    }

    fn hit(&self, f: &Frame, pos: Pos2, mods: egui::Modifiers) -> Hit {
        if let Some(h) = f
            .handles
            .iter()
            .filter(|h| h.pos.distance(pos) <= PICK)
            .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos)))
        {
            return Hit::Handle(h.clone());
        }
        if let Some(r) = f.selection_box.map(|r| r.expand(BOX_MARGIN)) {
            for (hp, which) in box_handles(r) {
                if (hp - pos).abs().max_elem() <= 4.0 {
                    return Hit::BoxHandle(which);
                }
            }
        }
        if let Some(k) = f
            .keys
            .iter()
            .filter(|k| k.pos.distance(pos) <= PICK)
            .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos)))
        {
            return Hit::Key(k.clone());
        }
        if f.selection_box
            .is_some_and(|r| r.expand(BOX_MARGIN).contains(pos))
        {
            return Hit::InBox;
        }
        if mods.alt {
            for (pi, sampled) in f.pieces.iter().enumerate() {
                let va = &f.vaxes[pi];
                for piece in &sampled.pieces {
                    for w in piece.pts.windows(2) {
                        let a = pos2(w[0].0, va.y(w[0].1));
                        let b = pos2(w[1].0, va.y(w[1].1));
                        if dist_to_segment(pos, a, b) <= 4.0 {
                            return Hit::Segment(pi);
                        }
                    }
                }
            }
        }
        Hit::Nothing
    }

    /// The Graph Editor in `rect` (graph area and button bar).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        axis: &TimeAxis,
        rect: Rect,
        now: Time,
        waveform: &mut dyn FnMut(&Ui, &PrimPath, Rect),
    ) -> GraphOut {
        let mut out = GraphOut::default();
        let stage = cx.stage;
        let bar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - BAR_H), rect.max);
        let mut area = Rect::from_min_max(rect.min, pos2(rect.right(), bar.top()));
        let mut props = shown_props(stage, &info.path, session, &self.settings);
        // Keys of locked layers cannot be selected or edited (AE).
        let editable = |p: &Prop| query::locked_layer_of(stage, &p.path).is_none();
        if self.settings.show_expression_editor {
            let strip = Rect::from_min_max(pos2(area.left(), area.bottom() - 22.0), area.max);
            area.max.y = strip.top();
            self.expression_editor(ui, cx, session, &props, strip);
        }
        ui.painter().rect_filled(area, 0.0, color::TIMELINE_BG);
        let selected = session.lock().selected_keys.clone();
        self.sel_dims.retain(|(k, _)| selected.contains(k));
        if self
            .focus
            .as_ref()
            .is_none_or(|f| !props.iter().any(|p| p.id() == *f))
        {
            self.focus = props.first().map(Prop::id);
        }

        let resp = ui
            .interact(area, ui.id().with("graph_editor"), Sense::click_and_drag())
            .labeled(egui::WidgetType::Other, tr("Graph Editor"));
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let mods = ui.input(|i| i.modifiers);
        let released = ui.input(|i| i.pointer.any_released());

        // Snap targets (comp times): CTI, comp start / end, work area, In / Out points,
        // other keyframes.
        let dragged: Vec<KeyRef> = match &self.drag {
            Some(GDrag::Keys { keys, .. }) => keys.clone(),
            _ => Vec::new(),
        };
        let mut snap_times = vec![
            now,
            Time::ZERO,
            info.duration,
            info.work_area.start,
            info.work_area.end,
        ];
        let mut layers_seen: Vec<PrimPath> = Vec::new();
        for p in &props {
            if !layers_seen.contains(&p.layer) {
                layers_seen.push(p.layer.clone());
                let t = query::layer_timing(stage, &p.layer);
                snap_times.push(t.in_point);
                snap_times.push(t.out_point);
            }
            for t in p.spec.keyframes.keys() {
                let kr = KeyRef::new(p.path.clone(), p.attr.clone(), *t);
                if !dragged.contains(&kr) {
                    snap_times.push(p.comp(*t));
                }
            }
        }

        // Geometry from the stored keyframes.
        for e in self.cache.values_mut() {
            e.used = false;
        }
        let fit = self.settings.auto_zoom && self.drag.is_none();
        let mut frame = self.geometry(&props, axis, area, &selected, fit);

        // Drag preview: the edits the drag would make now, applied locally.
        let mut edits: Vec<KeyEdit> = Vec::new();
        let (mut vline, mut hline) = (None, None);
        if self.drag.is_some()
            && let Some(ptr) = pointer
        {
            let (e, v, h) = self.drag_edits(
                &frame.props,
                &frame.vaxes,
                &frame.keys,
                info,
                axis,
                ptr,
                mods,
                &snap_times,
            );
            (edits, vline, hline) = (e, v, h);
            if !edits.is_empty() {
                for p in &mut props {
                    let mine: Vec<&KeyEdit> = edits
                        .iter()
                        .filter(|e| e.key.path == p.path && e.key.attr == p.attr)
                        .collect();
                    if !mine.is_empty() {
                        p.spec = bae_motion::apply_key_edits(&p.attr, &p.spec, &mine).0;
                    }
                }
                let moved: Vec<KeyRef> = selected
                    .iter()
                    .map(|k| {
                        edits
                            .iter()
                            .find(|e| e.key == *k)
                            .and_then(|e| e.time)
                            .map_or(k.clone(), |t| {
                                KeyRef::new(k.path.clone(), k.attr.clone(), t)
                            })
                    })
                    .collect();
                frame = self.geometry(&props, axis, area, &moved, false);
            }
        }
        // Remember the ranges (frozen while dragging and with Auto-Zoom off).
        for (p, va) in frame.props.iter().zip(&frame.vaxes) {
            self.ranges.insert(p.range_key(), (va.lo, va.hi));
        }
        let shown: HashSet<RangeKey> = frame.props.iter().map(Prop::range_key).collect();
        self.ranges.retain(|k, _| shown.contains(k));
        self.cache.retain(|_, e| e.used);

        let _markers = self.paint(ui, stage, &frame, axis, area, &selected, waveform);
        #[cfg(test)]
        {
            self.last.markers = _markers;
        }
        let p = ui.painter().with_clip_rect(area);
        if let Some(x) = vline {
            p.vline(x, area.y_range(), Stroke::new(1.0, SNAP));
        }
        if let Some(y) = hline {
            p.hline(area.x_range(), y, Stroke::new(1.0, SNAP));
        }
        if let (Some(GDrag::Marquee { origin, .. }), Some(ptr)) = (&self.drag, pointer) {
            let r = Rect::from_two_pos(*origin, ptr).intersect(area);
            p.rect(
                r,
                0.0,
                Color32::from_rgba_unmultiplied(88, 149, 237, 30),
                Stroke::new(1.0, color::ACCENT),
                egui::StrokeKind::Inside,
            );
        }

        // Hover: cursor and tooltip.
        if self.drag.is_none()
            && let Some(hp) = resp.hover_pos()
        {
            match self.hit(&frame, hp, mods) {
                Hit::Handle(_) | Hit::Key(_) => ui.ctx().set_cursor_icon(CursorIcon::PointingHand),
                Hit::InBox => ui.ctx().set_cursor_icon(CursorIcon::Move),
                Hit::BoxHandle(h) => ui.ctx().set_cursor_icon(match (h.hx, h.hy) {
                    (0, _) => CursorIcon::ResizeVertical,
                    (_, 0) => CursorIcon::ResizeHorizontal,
                    (a, b) if a == b => CursorIcon::ResizeNwSe,
                    _ => CursorIcon::ResizeNeSw,
                }),
                _ => {}
            }
            if self.settings.show_tooltips
                && let Hit::Key(k) = self.hit(&frame, hp, egui::Modifiers::NONE)
            {
                let p = &frame.props[k.prop];
                let text = key_tooltip(stage, info, p, &k);
                resp.clone().on_hover_ui_at_pointer(|ui| {
                    ui.label(text);
                });
            }
        }

        // Wheel: Alt = time zoom; with Auto-Zoom off, plain = vertical pan and
        // Cmd/Ctrl = vertical zoom.
        if resp.hovered() {
            let trackpad = cfg!(target_os = "macos") && ui.input(|i| i.events.iter().any(|event| matches!(event, egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta, .. } if delta.y != 0.0)));
            let dy = if trackpad {
                crate::viewer::wheel(ui).pan.y
            } else {
                ui.input(|i| i.smooth_scroll_delta.y)
            };
            if trackpad && dy != 0.0 && !mods.alt {
                self.settings.auto_zoom = false;
            }
            if dy != 0.0 {
                if mods.alt {
                    out.zoom_by = dy * 0.002;
                } else if !self.settings.auto_zoom {
                    for (p, va) in frame.props.iter().zip(&frame.vaxes) {
                        let (lo, hi) = (va.lo, va.hi);
                        let r = if mods.command {
                            let f = (1.0 - f64::from(dy) * 0.003).clamp(0.2, 5.0);
                            let c = (lo + hi) / 2.0;
                            (c + (lo - c) * f, c + (hi - c) * f)
                        } else {
                            let d = f64::from(dy) * va.per_point();
                            (lo + d, hi + d)
                        };
                        self.ranges.insert(p.range_key(), r);
                    }
                }
            }
        }

        // Clicks.
        if resp.clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            match self.hit(&frame, pos, mods) {
                Hit::Key(k) if editable(&frame.props[k.prop]) => {
                    self.focus = Some(frame.props[k.prop].id());
                    if mods.command {
                        let selected = session.lock().selected_keys.clone();
                        toggle_interpolation(cx, &frame.props[k.prop], &k.key, &selected);
                    } else {
                        let mut s = session.lock();
                        if mods.shift {
                            if let Some(i) = s.selected_keys.iter().position(|x| *x == k.key) {
                                s.selected_keys.remove(i);
                                self.sel_dims.retain(|(x, _)| *x != k.key);
                            } else {
                                s.selected_keys.push(k.key.clone());
                                self.sel_dims.insert((k.key.clone(), k.dim));
                            }
                        } else {
                            s.selected_keys = vec![k.key.clone()];
                            self.sel_dims = [(k.key.clone(), k.dim)].into_iter().collect();
                        }
                    }
                }
                Hit::Segment(pi) if editable(&frame.props[pi]) => {
                    let p = &frame.props[pi];
                    self.focus = Some(p.id());
                    let all: Vec<KeyRef> = p
                        .spec
                        .keyframes
                        .keys()
                        .map(|t| KeyRef::new(p.path.clone(), p.attr.clone(), *t))
                        .collect();
                    let mut s = session.lock();
                    if !mods.shift {
                        s.selected_keys.clear();
                        self.sel_dims.clear();
                    }
                    for k in all {
                        if !s.selected_keys.contains(&k) {
                            s.selected_keys.push(k);
                        }
                    }
                }
                Hit::Nothing if !mods.shift => {
                    session.lock().selected_keys.clear();
                    self.sel_dims.clear();
                }
                _ => {}
            }
        }
        if resp.secondary_clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            self.menu_key = None;
            if let Hit::Key(k) = self.hit(&frame, pos, egui::Modifiers::NONE) {
                let mut s = session.lock();
                if !s.selected_keys.contains(&k.key) {
                    s.selected_keys = vec![k.key.clone()];
                    self.sel_dims = [(k.key.clone(), k.dim)].into_iter().collect();
                }
                self.menu_key = Some(k.key.clone());
            }
        }
        // Right-click on a keyframe: the keyframe menu; elsewhere the Keyframe
        // Assistant and Toggle Hold items for the selection.
        let menu_key = self.menu_key.clone();
        let settings = &mut self.settings;
        crate::common::context_menu(&resp, &mut |ui| match &menu_key {
            Some(k) => crate::timeline::key_menu(ui, cx, session, k),
            // Empty graph space: AE's view options.
            None => area_menu(ui, settings),
        });

        // Drag start.
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        {
            let shown_selected: Vec<KeyRef> = {
                let s = session.lock();
                s.selected_keys
                    .iter()
                    .filter(|k| {
                        frame
                            .props
                            .iter()
                            .any(|p| p.path == k.path && p.attr == k.attr && editable(p))
                    })
                    .cloned()
                    .collect()
            };
            self.drag = match self.hit(&frame, origin, mods) {
                Hit::Handle(h) if editable(&frame.props[h.prop]) => {
                    self.focus = Some(frame.props[h.prop].id());
                    Some(GDrag::Handle {
                        key: h.key,
                        dim: h.dim,
                        side: h.side,
                        split: mods.alt,
                        keys: shown_selected.clone(),
                    })
                }
                Hit::BoxHandle(handle) => frame.selection_box.map(|bounds| GDrag::Scale {
                    origin,
                    handle,
                    bounds,
                    keys: shown_selected.clone(),
                }),
                Hit::Key(k) if editable(&frame.props[k.prop]) => {
                    self.focus = Some(frame.props[k.prop].id());
                    let mut s = session.lock();
                    if !s.selected_keys.contains(&k.key) {
                        if mods.shift {
                            s.selected_keys.push(k.key.clone());
                        } else {
                            s.selected_keys = vec![k.key.clone()];
                            self.sel_dims.clear();
                        }
                        self.sel_dims.insert((k.key.clone(), k.dim));
                    }
                    let keys: Vec<KeyRef> = s
                        .selected_keys
                        .iter()
                        .filter(|x| {
                            frame
                                .props
                                .iter()
                                .any(|p| p.path == x.path && p.attr == x.attr && editable(p))
                        })
                        .cloned()
                        .collect();
                    Some(GDrag::Keys {
                        origin,
                        grabbed: k.key,
                        dim: k.dim,
                        keys,
                    })
                }
                Hit::InBox => frame
                    .keys
                    .iter()
                    .find(|kp| shown_selected.contains(&kp.key))
                    .map(|kp| GDrag::Keys {
                        origin,
                        grabbed: kp.key.clone(),
                        dim: kp.dim,
                        keys: shown_selected.clone(),
                    }),
                _ => Some(GDrag::Marquee {
                    origin,
                    additive: mods.shift,
                }),
            };
        }
        // Marquee selects the key points inside.
        if let (Some(GDrag::Marquee { origin, additive }), Some(ptr)) = (&self.drag, pointer) {
            let r = Rect::from_two_pos(*origin, ptr);
            let inside: Vec<&KeyPt> = frame
                .keys
                .iter()
                .filter(|k| r.contains(k.pos) && editable(&frame.props[k.prop]))
                .collect();
            let mut s = session.lock();
            if !*additive {
                s.selected_keys.clear();
                self.sel_dims.clear();
            }
            for k in inside {
                if !s.selected_keys.contains(&k.key) {
                    s.selected_keys.push(k.key.clone());
                }
                self.sel_dims.insert((k.key.clone(), k.dim));
            }
        }
        // Release: one key.edit for the whole drag.
        if released
            && let Some(drag) = self.drag.take()
            && !matches!(drag, GDrag::Marquee { .. })
            && !edits.is_empty()
        {
            let moved: Vec<(KeyRef, Time)> = edits
                .iter()
                .filter_map(|e| e.time.map(|t| (e.key.clone(), t)))
                .collect();
            cx.invoke_with("key.edit", json!({ "edits": edits }));
            let mut s = session.lock();
            for k in &mut s.selected_keys {
                if let Some((_, t)) = moved.iter().find(|(m, _)| m == k) {
                    k.time = *t;
                }
            }
            // Panels drawn before the edit runs keep the moved keys selected.
            let landing: Vec<KeyRef> = moved
                .iter()
                .map(|(k, t)| KeyRef::new(k.path.clone(), k.attr.clone(), *t))
                .collect();
            s.expect(crate::session::Pending::Keys(landing));
            self.sel_dims = self
                .sel_dims
                .drain()
                .map(|(k, d)| match moved.iter().find(|(m, _)| *m == k) {
                    Some((_, t)) => (KeyRef::new(k.path.clone(), k.attr.clone(), *t), d),
                    None => (k, d),
                })
                .collect();
        }

        self.button_bar(ui, cx, session, info, bar, &frame, &mut out);
        #[cfg(test)]
        {
            self.last.keys = frame
                .keys
                .iter()
                .map(|k| (k.key.clone(), k.dim, k.pos))
                .collect();
            self.last.handles = frame
                .handles
                .iter()
                .map(|h| (h.key.clone(), h.dim, h.side == Side::Out, h.pos))
                .collect();
            self.last.selection_box = frame.selection_box;
            self.last.vaxes = frame
                .props
                .iter()
                .zip(&frame.vaxes)
                .map(|(p, v)| (p.id(), *v))
                .collect();
        }
        out
    }

    /// The sampled graph of `p` (cached).
    fn sampled(
        &mut self,
        p: &Prop,
        axis: &TimeAxis,
        area: Rect,
        speed: bool,
    ) -> Option<std::rc::Rc<Sampled>> {
        let stamp = Stamp {
            timing: p.timing,
            axis: (
                axis.left.to_bits(),
                axis.width.to_bits(),
                axis.start,
                axis.span,
            ),
            range: (area.left().to_bits(), area.right().to_bits()),
            dims: p.dims,
            spatial: p.spatial,
            shared: p.shared,
            clamp: p.clamp,
        };
        let key = (p.path.clone(), p.attr.clone(), speed);
        if let Some(e) = self.cache.get_mut(&key)
            && e.stamp == stamp
            && e.spec == p.spec
        {
            e.used = true;
            return Some(e.data.clone());
        }
        let curve = p.curve()?;
        let data = std::rc::Rc::new(Sampled::new(
            sample(p, &curve, axis, area.left(), area.right(), speed),
            key_values(p, &curve, speed),
        ));
        self.cache.insert(
            key,
            CacheEntry {
                spec: p.spec.clone(),
                stamp,
                data: data.clone(),
                used: true,
            },
        );
        Some(data)
    }

    /// Sampled curves, vertical axes, key points, handles and the transform box.
    fn geometry(
        &mut self,
        props: &[Prop],
        axis: &TimeAxis,
        area: Rect,
        selected: &[KeyRef],
        fit: bool,
    ) -> Frame {
        let mut f = Frame {
            props: props.to_vec(),
            vaxes: Vec::with_capacity(props.len()),
            pieces: Vec::with_capacity(props.len()),
            reference: Vec::with_capacity(props.len()),
            keys: Vec::new(),
            handles: Vec::new(),
            selection_box: None,
        };
        for (pi, p) in props.iter().enumerate() {
            let Some(data) = self.sampled(p, axis, area, p.speed) else {
                f.vaxes.push(VAxis::new((0.0, 1.0), area));
                f.pieces.push(Default::default());
                f.reference.push(None);
                continue;
            };
            let reference = if self.settings.show_reference && !(p.shared && !p.speed) {
                self.sampled(p, axis, area, !p.speed)
            } else {
                None
            };
            let mut range = self.range_for(p, &data, fit);
            if fit {
                // Keep the handles of selected keys in view too.
                let va = VAxis::new(range, area);
                let hs = handle_points(p, pi, axis, &va, selected);
                let extra = hs.iter().map(|h| va.v(h.pos.y));
                if let Some(r) = data.range_with(extra) {
                    range = padded(r);
                }
            }
            let va = VAxis::new(range, area);
            f.keys.extend(key_points(p, pi, axis, &va, &data.key_vals));
            f.handles.extend(handle_points(p, pi, axis, &va, selected));
            f.vaxes.push(va);
            f.pieces.push(data);
            f.reference.push(reference);
        }
        if self.settings.transform_box {
            let pts: Vec<Pos2> = f
                .keys
                .iter()
                .filter(|k| selected.contains(&k.key) && self.picked(&k.key, k.dim))
                .map(|k| k.pos)
                .collect();
            let distinct = pts.iter().any(|p| p.distance(pts[0]) > 0.5);
            if pts.len() >= 2 && distinct {
                f.selection_box = Some(Rect::from_points(&pts));
            }
        }
        f
    }

    /// Draws the graphs; returns the x of the layer markers drawn.
    #[allow(clippy::too_many_arguments)]
    fn paint(
        &self,
        ui: &Ui,
        stage: &Stage,
        f: &Frame,
        axis: &TimeAxis,
        area: Rect,
        selected: &[KeyRef],
        waveform: &mut dyn FnMut(&Ui, &PrimPath, Rect),
    ) -> Vec<f32> {
        let mut layers: Vec<&PrimPath> = Vec::new();
        for prop in &f.props {
            if !layers.contains(&&prop.layer) {
                layers.push(&prop.layer);
            }
        }
        // Audio waveforms of the shown layers, behind everything (AE).
        if self.settings.show_waveforms {
            for layer in &layers {
                waveform(ui, layer, area);
            }
        }
        let p = ui.painter().with_clip_rect(area);
        let focus = f
            .props
            .iter()
            .position(|x| Some(x.id()) == self.focus)
            .or(if f.props.is_empty() { None } else { Some(0) });
        // Grid and labels of the focused property.
        if let Some(fi) = focus {
            let va = &f.vaxes[fi];
            let fp = &f.props[fi];
            let step = nice_step(va.hi - va.lo, f64::from((va.bottom - va.top) / 40.0));
            let mut v = (va.lo / step).ceil() * step;
            let suffix = if fp.speed {
                format!("{}/{}", unit(&fp.attr).trim(), tr("sec"))
            } else {
                unit(&fp.attr)
            };
            let mut n = 0;
            while v <= va.hi && n < 200 {
                if v.abs() < step * 1e-6 {
                    v = 0.0;
                }
                let y = va.y(v);
                p.hline(
                    area.x_range(),
                    y,
                    Stroke::new(1.0, Color32::from_gray(0x30)),
                );
                p.text(
                    pos2(area.left() + 3.0, y - 1.0),
                    Align2::LEFT_BOTTOM,
                    format!("{}{}", fmt_num(v), suffix),
                    theme::small(),
                    color::TEXT_DIM,
                );
                v += step;
                n += 1;
            }
        }
        // Layer In / Out points as braces.
        if self.settings.show_in_out {
            let mut seen: Vec<&PrimPath> = Vec::new();
            for prop in &f.props {
                if seen.contains(&&prop.layer) {
                    continue;
                }
                seen.push(&prop.layer);
                let t = query::layer_timing(stage, &prop.layer);
                for (time, glyph) in [(t.in_point, "{"), (t.out_point, "}")] {
                    p.text(
                        pos2(axis.x(time), area.top() + 2.0),
                        Align2::CENTER_TOP,
                        glyph,
                        theme::font(16.0),
                        color::TEXT_DIM,
                    );
                }
            }
        }
        // Layer markers along the bottom (AE).
        let mut marker_xs = Vec::new();
        if self.settings.show_markers {
            for layer in &layers {
                for (m, t) in bae_motion::markers::markers_of(stage, layer) {
                    let x = axis.x(query::to_comp_time(stage, &m, t));
                    let b = area.bottom() - 2.0;
                    p.add(egui::Shape::convex_polygon(
                        vec![pos2(x, b - 7.0), pos2(x + 4.0, b), pos2(x - 4.0, b)],
                        color::TEXT_DIM,
                        Stroke::NONE,
                    ));
                    marker_xs.push(x);
                }
            }
        }
        // Reference graphs (the other graph type), fitted to the area, grey.
        for (i, reference) in f.reference.iter().enumerate() {
            let Some(other) = reference else {
                continue;
            };
            let Some(r) = other.range else {
                continue;
            };
            let va = VAxis::new(padded(r), area);
            for piece in &other.pieces {
                polyline(&p, piece, &va, Stroke::new(1.0, REFERENCE));
            }
            if Some(i) == focus {
                for v in [va.hi, va.lo] {
                    p.text(
                        pos2(area.right() - 3.0, va.y(v)),
                        Align2::RIGHT_CENTER,
                        fmt_num(v),
                        theme::small(),
                        REFERENCE,
                    );
                }
            }
        }
        // Curves.
        for (pi, prop) in f.props.iter().enumerate() {
            for piece in &f.pieces[pi].pieces {
                polyline(
                    &p,
                    piece,
                    &f.vaxes[pi],
                    Stroke::new(1.5, curve_color(prop, piece.dim)),
                );
            }
        }
        // Handles.
        for h in &f.handles {
            p.line_segment([h.anchor, h.pos], Stroke::new(1.0, KEY_SELECTED));
            p.circle_filled(h.pos, 3.0, KEY_SELECTED);
        }
        // Keys: selected = solid yellow, semi-selected = hollow yellow, others the
        // curve colour.
        for k in &f.keys {
            let r = Rect::from_center_size(k.pos, vec2(6.0, 6.0));
            let sel = selected.contains(&k.key);
            if sel && self.picked(&k.key, k.dim) {
                p.rect_filled(r, 0.0, KEY_SELECTED);
            } else if sel {
                p.rect_stroke(
                    r,
                    0.0,
                    Stroke::new(1.0, KEY_SELECTED),
                    egui::StrokeKind::Inside,
                );
            } else {
                p.rect_filled(r, 0.0, curve_color(&f.props[k.prop], k.dim));
            }
        }
        // Transform box.
        if let Some(r) = f.selection_box.map(|r| r.expand(BOX_MARGIN)) {
            p.rect_stroke(
                r,
                0.0,
                Stroke::new(1.0, color::TEXT_STRONG),
                egui::StrokeKind::Middle,
            );
            for (hp, _) in box_handles(r) {
                p.rect_filled(
                    Rect::from_center_size(hp, vec2(5.0, 5.0)),
                    0.0,
                    color::TEXT_STRONG,
                );
            }
        }
        if f.props.is_empty() {
            p.text(
                area.center(),
                Align2::CENTER_CENTER,
                tr("Select a property to show its graph"),
                theme::body(),
                color::TEXT_DIM,
            );
        }
        marker_xs
    }
}

fn key_tooltip(stage: &Stage, info: &CompInfo, p: &Prop, k: &KeyPt) -> String {
    let layer = query::display_name_of(stage, &p.layer);
    let ty = stage.prim(&p.path).and_then(|v| v.prim_type().cloned());
    let name = crate::common::attr_label(ty.as_ref(), &p.attr, p.dims == 3);
    let value = p
        .spec
        .keyframes
        .get(&k.key.time)
        .and_then(|kf| comps(&kf.value))
        .map(|c| {
            c.iter()
                .take(p.dims)
                .map(|v| fmt_num(*v))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    format!(
        "{layer}: {name}\n{}  {value}{}",
        crate::common::timecode(info, p.comp(k.key.time)),
        unit(&p.attr)
    )
}

/// Cmd/Ctrl-click a key: Linear ↔ Auto Bezier; any other Bezier → Linear (AE-ANI-049),
/// on every selected key when the clicked one is selected.
fn toggle_interpolation(cx: &mut PanelContext<'_>, p: &Prop, key: &KeyRef, selected: &[KeyRef]) {
    let Some(k) = p.spec.keyframes.get(&key.time) else {
        return;
    };
    let keys = crate::timeline::clicked_set(selected, key);
    let edits = crate::timeline::click_conversion(k, &keys, false);
    cx.invoke_with("key.edit", json!({ "edits": edits }));
}

/// Keys → one interpolation for both sides (Graph Editor Hold / Linear / Auto Bezier
/// buttons).
pub(crate) fn convert_keys(cx: &mut PanelContext<'_>, keys: &[KeyRef], to: &str) {
    if keys.is_empty() {
        return;
    }
    let edits: Vec<KeyEdit> = keys
        .iter()
        .map(|k| {
            let mut e = KeyEdit::new(k.clone());
            match to {
                "auto_bezier" => e.auto_bezier = Some(true),
                other => {
                    let i = if other == "hold" {
                        Interpolation::Hold
                    } else {
                        Interpolation::Linear
                    };
                    e.interp_in = Some(i);
                    e.interp_out = Some(i);
                    e.auto_bezier = Some(false);
                    e.continuous = Some(false);
                }
            }
            e
        })
        .collect();
    cx.invoke_with("key.edit", json!({ "edits": edits }));
}

// ---------------------------------------------------------------------------
// Button bar (AE-UI-080) and expression editor.

#[derive(Clone, Copy)]
enum Glyph {
    Eye,
    GraphType,
    TransformBox,
    Snap,
    AutoZoom,
    FitSelection,
    FitAll,
    Separate,
    EditKeys,
    Hold,
    Linear,
    AutoBezier,
    EasyEase,
    EaseIn,
    EaseOut,
}

fn paint_glyph(p: &egui::Painter, r: Rect, g: Glyph, c: Color32) {
    let s = Stroke::new(1.2, c);
    let at = |x: f32, y: f32| pos2(r.left() + x * r.width(), r.top() + y * r.height());
    let line = |pts: &[(f32, f32)]| {
        p.add(Shape::line(
            pts.iter().map(|(x, y)| at(*x, *y)).collect(),
            s,
        ));
    };
    let curve = |a: (f32, f32), b: (f32, f32), c1: (f32, f32), c2: (f32, f32)| {
        let pts: Vec<Pos2> = (0..=12)
            .map(|i| {
                let t = i as f32 / 12.0;
                let u = 1.0 - t;
                let x = u * u * u * a.0
                    + 3.0 * u * u * t * c1.0
                    + 3.0 * u * t * t * c2.0
                    + t * t * t * b.0;
                let y = u * u * u * a.1
                    + 3.0 * u * u * t * c1.1
                    + 3.0 * u * t * t * c2.1
                    + t * t * t * b.1;
                at(x, y)
            })
            .collect();
        p.add(Shape::line(pts, s));
    };
    let diamond = |x: f32, y: f32| {
        let c0 = at(x, y);
        let d = r.width() * 0.14;
        p.add(Shape::convex_polygon(
            vec![
                c0 + vec2(0.0, -d),
                c0 + vec2(d, 0.0),
                c0 + vec2(0.0, d),
                c0 + vec2(-d, 0.0),
            ],
            c,
            Stroke::NONE,
        ));
    };
    match g {
        Glyph::Eye => bae_ui::icons::paint(p, r, bae_ui::icons::Icon::Eye, c),
        Glyph::GraphType => bae_ui::icons::paint(p, r, bae_ui::icons::Icon::GraphEditor, c),
        Glyph::Snap => bae_ui::icons::paint(p, r, bae_ui::icons::Icon::SnapEdges, c),
        Glyph::TransformBox => {
            p.rect_stroke(r.shrink(3.5), 0.0, s, egui::StrokeKind::Middle);
            for (x, y) in [(0.22, 0.22), (0.78, 0.22), (0.22, 0.78), (0.78, 0.78)] {
                p.rect_filled(Rect::from_center_size(at(x, y), vec2(3.0, 3.0)), 0.0, c);
            }
        }
        Glyph::AutoZoom => {
            line(&[(0.5, 0.12), (0.5, 0.88)]);
            line(&[(0.35, 0.27), (0.5, 0.12), (0.65, 0.27)]);
            line(&[(0.35, 0.73), (0.5, 0.88), (0.65, 0.73)]);
            line(&[(0.15, 0.5), (0.3, 0.5)]);
            line(&[(0.7, 0.5), (0.85, 0.5)]);
        }
        Glyph::FitSelection | Glyph::FitAll => {
            for (x0, x1, y) in [
                (0.12, 0.3, 0.15),
                (0.7, 0.88, 0.15),
                (0.12, 0.3, 0.85),
                (0.7, 0.88, 0.85),
            ] {
                line(&[(x0, y), (x1, y)]);
            }
            line(&[(0.12, 0.15), (0.12, 0.35)]);
            line(&[(0.88, 0.15), (0.88, 0.35)]);
            line(&[(0.12, 0.65), (0.12, 0.85)]);
            line(&[(0.88, 0.65), (0.88, 0.85)]);
            if matches!(g, Glyph::FitAll) {
                curve((0.25, 0.7), (0.75, 0.3), (0.5, 0.7), (0.5, 0.3));
            } else {
                diamond(0.38, 0.6);
                diamond(0.62, 0.4);
            }
        }
        Glyph::Separate => {
            line(&[(0.15, 0.85), (0.15, 0.15)]);
            line(&[(0.15, 0.85), (0.85, 0.85)]);
            line(&[(0.35, 0.3), (0.6, 0.65)]);
            line(&[(0.6, 0.3), (0.35, 0.65)]);
        }
        Glyph::EditKeys => {
            diamond(0.4, 0.5);
            line(&[(0.62, 0.42), (0.75, 0.58), (0.88, 0.42)]);
        }
        Glyph::Hold => {
            line(&[(0.1, 0.75), (0.5, 0.75), (0.5, 0.25), (0.9, 0.25)]);
            diamond(0.5, 0.75);
        }
        Glyph::Linear => {
            line(&[(0.1, 0.85), (0.9, 0.15)]);
            diamond(0.5, 0.5);
        }
        Glyph::AutoBezier => {
            curve((0.1, 0.8), (0.9, 0.8), (0.35, 0.05), (0.65, 0.05));
            diamond(0.5, 0.36);
        }
        Glyph::EasyEase => {
            curve((0.08, 0.8), (0.5, 0.5), (0.35, 0.8), (0.4, 0.5));
            curve((0.5, 0.5), (0.92, 0.2), (0.6, 0.5), (0.65, 0.2));
            diamond(0.5, 0.5);
        }
        Glyph::EaseIn => {
            curve((0.1, 0.8), (0.55, 0.5), (0.4, 0.8), (0.45, 0.5));
            line(&[(0.55, 0.5), (0.9, 0.5)]);
            diamond(0.55, 0.5);
        }
        Glyph::EaseOut => {
            line(&[(0.1, 0.5), (0.45, 0.5)]);
            curve((0.45, 0.5), (0.9, 0.2), (0.55, 0.5), (0.6, 0.2));
            diamond(0.45, 0.5);
        }
    }
}

/// A button of the bar; `on` = toggled on (None for plain buttons).
fn bar_button(
    ui: &mut Ui,
    rect: Rect,
    label: &str,
    enabled: bool,
    on: Option<bool>,
    glyph: Glyph,
) -> egui::Response {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    if !enabled {
        child.disable();
    }
    let id = ui.id().with(("graph_button", label));
    let resp = child.interact(rect, id, Sense::click());
    let resp = match on {
        Some(on) => resp.labeled_toggle(egui::WidgetType::Button, label, on),
        None => resp.labeled(egui::WidgetType::Button, label),
    };
    let p = ui.painter();
    if on == Some(true) {
        p.rect_filled(rect.shrink(1.0), 2.0, color::BUTTON_ON);
    } else if enabled && resp.hovered() {
        p.rect_filled(rect.shrink(1.0), 2.0, color::HOVER);
    }
    let c = if !enabled {
        color::TEXT_DISABLED
    } else if on == Some(true) || resp.hovered() {
        color::TEXT_STRONG
    } else {
        color::ICON
    };
    paint_glyph(
        p,
        Rect::from_center_size(rect.center(), vec2(14.0, 14.0)),
        glyph,
        c,
    );
    resp.on_hover_text(label)
}

/// Right-click on the graph's empty space (AE 26, 14 entries; `None` is a
/// separator).
pub(crate) const AREA_MENU: [Option<&str>; 17] = [
    Some("Auto-Zoom Graph Height"),
    Some("Show Selected Properties"),
    None,
    Some("Show Animated Properties"),
    Some("Show Graph Editor Set"),
    Some("Auto-Select Graph Type"),
    Some("Edit Value Graph"),
    None,
    Some("Edit Speed Graph"),
    Some("Show Reference Graph"),
    Some("Snap"),
    Some("Show Audio Waveforms"),
    Some("Show Layer In/Out Points"),
    None,
    Some("Show Layer Markers"),
    Some("Show Graph Tool Tips"),
    Some("Show Expression Editor"),
];

fn area_menu(ui: &mut Ui, s: &mut GraphSettings) {
    ui.set_min_width(240.0);
    for entry in AREA_MENU {
        let Some(title) = entry else {
            ui.separator();
            continue;
        };
        let label = tr(title);
        let graph_type = match title {
            "Auto-Select Graph Type" => Some(GraphType::Auto),
            "Edit Value Graph" => Some(GraphType::Value),
            "Edit Speed Graph" => Some(GraphType::Speed),
            _ => None,
        };
        if let Some(t) = graph_type {
            if ui.radio(s.graph_type == t, label).clicked() {
                s.graph_type = t;
                ui.close();
            }
            continue;
        }
        let on = match title {
            "Auto-Zoom Graph Height" => &mut s.auto_zoom,
            "Show Selected Properties" => &mut s.show_selected,
            "Show Animated Properties" => &mut s.show_animated,
            "Show Graph Editor Set" => &mut s.show_set,
            "Show Reference Graph" => &mut s.show_reference,
            "Snap" => &mut s.snap,
            "Show Audio Waveforms" => &mut s.show_waveforms,
            "Show Layer In/Out Points" => &mut s.show_in_out,
            "Show Layer Markers" => &mut s.show_markers,
            "Show Graph Tool Tips" => &mut s.show_tooltips,
            _ => &mut s.show_expression_editor,
        };
        if menu_check(ui, on, label) {
            ui.close();
        }
    }
}

fn menu_check(ui: &mut Ui, on: &mut bool, label: &str) -> bool {
    widgets::checkbox(ui, on, label).clicked()
}

const POSITION_FAMILY: [&str; 4] = [
    "transform:position",
    "transform:position_x",
    "transform:position_y",
    "transform:position_z",
];

impl GraphEditor {
    #[cfg_attr(not(test), allow(clippy::needless_pass_by_ref_mut))]
    fn button(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        label: &str,
        enabled: bool,
        on: Option<bool>,
        glyph: Glyph,
    ) -> egui::Response {
        #[cfg(test)]
        self.last.buttons.push((label.to_owned(), rect));
        bar_button(ui, rect, label, enabled, on, glyph)
    }

    #[allow(clippy::too_many_arguments)]
    fn button_bar(
        &mut self,
        ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        bar: Rect,
        f: &Frame,
        out: &mut GraphOut,
    ) {
        let stage = cx.stage;
        ui.painter().rect_filled(bar, 0.0, color::PANEL);
        ui.painter().hline(
            bar.x_range(),
            bar.top() + 0.5,
            Stroke::new(1.0, color::SEPARATOR),
        );
        let (selected, sel_props, sel_layers) = {
            let s = session.lock();
            (
                s.selected_keys.clone(),
                s.selected_props.clone(),
                s.selected_layers.clone(),
            )
        };
        let shown_sel: Vec<KeyRef> = selected
            .iter()
            .filter(|k| f.props.iter().any(|p| p.path == k.path && p.attr == k.attr))
            .cloned()
            .collect();
        let has_keys = !selected.is_empty();
        #[cfg(test)]
        self.last.buttons.clear();
        let mut x = bar.left() + 4.0;
        let mut next = |gap: f32| {
            x += gap;
            let r = Rect::from_min_size(pos2(x, bar.top() + 1.0), vec2(22.0, BAR_H - 2.0));
            x += 24.0;
            r
        };

        // Show Properties.
        let r = next(0.0);
        let resp = self.button(
            ui,
            r,
            tr("Choose which properties are shown in the graph editor"),
            true,
            None,
            Glyph::Eye,
        );
        crate::common::popup_menu(&resp, &mut |ui| {
            ui.set_min_width(220.0);
            let s = &mut self.settings;
            menu_check(ui, &mut s.show_selected, tr("Show Selected Properties"));
            menu_check(ui, &mut s.show_animated, tr("Show Animated Properties"));
            menu_check(ui, &mut s.show_set, tr("Show Graph Editor Set"));
        });
        // Graph Type and Options.
        let r = next(0.0);
        let resp = self.button(
            ui,
            r,
            tr("Choose graph type and options"),
            true,
            None,
            Glyph::GraphType,
        );
        crate::common::popup_menu(&resp, &mut |ui| {
            ui.set_min_width(240.0);
            let s = &mut self.settings;
            for (t, label) in [
                (GraphType::Auto, "Auto-Select Graph Type"),
                (GraphType::Value, "Edit Value Graph"),
                (GraphType::Speed, "Edit Speed Graph"),
            ] {
                if ui.radio(s.graph_type == t, tr(label)).clicked() {
                    s.graph_type = t;
                }
            }
            ui.separator();
            menu_check(ui, &mut s.show_reference, tr("Show Reference Graph"));
            menu_check(ui, &mut s.show_waveforms, tr("Show Audio Waveforms"));
            menu_check(ui, &mut s.show_in_out, tr("Show Layer In/Out Points"));
            menu_check(ui, &mut s.show_markers, tr("Show Layer Markers"));
            menu_check(ui, &mut s.show_tooltips, tr("Show Graph Tool Tips"));
            menu_check(
                ui,
                &mut s.show_expression_editor,
                tr("Show Expression Editor"),
            );
            menu_check(
                ui,
                &mut s.allow_between_frames,
                tr("Allow Keyframes Between Frames"),
            );
        });
        // Transform box, Snap, Auto-Zoom.
        let r = next(6.0);
        if self
            .button(
                ui,
                r,
                tr("Show Transform Box when multiple keys are selected"),
                true,
                Some(self.settings.transform_box),
                Glyph::TransformBox,
            )
            .clicked()
        {
            self.settings.transform_box = !self.settings.transform_box;
        }
        let r = next(0.0);
        if self
            .button(
                ui,
                r,
                tr("Snap"),
                true,
                Some(self.settings.snap),
                Glyph::Snap,
            )
            .clicked()
        {
            self.settings.snap = !self.settings.snap;
        }
        let r = next(0.0);
        if self
            .button(
                ui,
                r,
                tr("Auto-zoom graph height"),
                true,
                Some(self.settings.auto_zoom),
                Glyph::AutoZoom,
            )
            .clicked()
        {
            self.settings.auto_zoom = !self.settings.auto_zoom;
        }
        // Fit Selection / Fit All.
        let r = next(6.0);
        if self
            .button(
                ui,
                r,
                tr("Fit selection to view"),
                !shown_sel.is_empty(),
                None,
                Glyph::FitSelection,
            )
            .clicked()
        {
            self.fit(f, Some(&shown_sel), out);
        }
        let r = next(0.0);
        if self
            .button(
                ui,
                r,
                tr("Fit all graphs to view"),
                !f.props.is_empty(),
                None,
                Glyph::FitAll,
            )
            .clicked()
        {
            self.fit(f, None, out);
        }
        // Separate Dimensions: the Position of the selected (or shown) properties.
        let mut targets: Vec<PrimPath> = Vec::new();
        let candidates = sel_props
            .iter()
            .cloned()
            .chain(f.props.iter().map(Prop::id))
            .chain(
                sel_layers
                    .iter()
                    .filter(|_| sel_props.is_empty() && f.props.is_empty())
                    .map(|l| (l.clone(), "transform:position".to_owned())),
            );
        for (path, attr) in candidates {
            if POSITION_FAMILY.contains(&attr.as_str())
                && query::is_layer(stage, &path)
                && !query::is_locked(stage, &path)
                && !targets.contains(&path)
            {
                targets.push(path);
            }
        }
        let separated = !targets.is_empty()
            && targets
                .iter()
                .all(|l| bae_motion::dimensions::is_separated(stage, l));
        let r = next(6.0);
        if self
            .button(
                ui,
                r,
                tr("Separate Dimensions"),
                !targets.is_empty(),
                Some(separated),
                Glyph::Separate,
            )
            .clicked()
        {
            cx.invoke_with(
                "transform.separate_dimensions",
                json!({"layers": targets, "separated": !separated}),
            );
            // The selected Position becomes X / Y Position and back.
            let mut s = session.lock();
            s.selected_props
                .retain(|(_, a)| !POSITION_FAMILY.contains(&a.as_str()));
        }
        // Edit Selected Keyframes.
        let r = next(6.0);
        let resp = self.button(
            ui,
            r,
            tr("Edit selected keyframes"),
            has_keys,
            None,
            Glyph::EditKeys,
        );
        let first = session.lock().selected_keys.first().cloned();
        crate::common::popup_menu(&resp, &mut |ui| {
            // AE: the keyframe context menu for the selected keyframes.
            if let Some(k) = &first {
                crate::timeline::key_menu(ui, cx, session, k);
            }
        });
        // Interpolation.
        for (gap, glyph, label, to) in [
            (
                6.0,
                Glyph::Hold,
                "Convert selected keyframes to Hold",
                "hold",
            ),
            (
                0.0,
                Glyph::Linear,
                "Convert selected keyframes to Linear",
                "linear",
            ),
            (
                0.0,
                Glyph::AutoBezier,
                "Convert selected keyframes to Auto Bezier",
                "auto_bezier",
            ),
        ] {
            let r = next(gap);
            if self
                .button(ui, r, tr(label), has_keys, None, glyph)
                .clicked()
            {
                convert_keys(cx, &selected, to);
            }
        }
        // Easy Ease.
        for (gap, glyph, label, mode) in [
            (6.0, Glyph::EasyEase, "Easy Ease", "easy_ease"),
            (0.0, Glyph::EaseIn, "Easy Ease In", "ease_in"),
            (0.0, Glyph::EaseOut, "Easy Ease Out", "ease_out"),
        ] {
            let r = next(gap);
            if self
                .button(ui, r, tr(label), has_keys, None, glyph)
                .clicked()
            {
                actions::handle(
                    &Invocation::with_params(ids::EASE, json!({ "mode": mode })),
                    cx,
                    session,
                );
            }
        }
        let _ = info;
    }

    /// Fit the time axis (and, with Auto-Zoom off, each graph's height) to the
    /// selected keys or to every key.
    fn fit(&mut self, f: &Frame, only: Option<&[KeyRef]>, out: &mut GraphOut) {
        let mut times: Vec<Time> = Vec::new();
        for kp in &f.keys {
            if only.is_none_or(|o| o.contains(&kp.key)) {
                times.push(f.props[kp.prop].comp(kp.key.time));
            }
        }
        if let (Some(lo), Some(hi)) = (times.iter().min(), times.iter().max()) {
            out.fit_time = Some((*lo, *hi));
        }
        if self.settings.auto_zoom {
            return;
        }
        for (pi, p) in f.props.iter().enumerate() {
            let va = &f.vaxes[pi];
            let vals: Vec<f64> = f
                .keys
                .iter()
                .filter(|kp| kp.prop == pi && only.is_none_or(|o| o.contains(&kp.key)))
                .map(|kp| va.v(kp.pos.y))
                .collect();
            let range = if only.is_some() {
                vals.iter().fold(None, |r: Option<(f64, f64)>, v| {
                    Some(r.map_or((*v, *v), |(a, b)| (a.min(*v), b.max(*v))))
                })
            } else {
                f.pieces[pi].range_with(vals.into_iter())
            };
            if let Some(r) = range {
                self.ranges.insert(p.range_key(), padded(r));
            }
        }
    }

    /// Graph Type and Options ▸ Show Expression Editor: the expression of the focused
    /// property, editable at the bottom of the Graph Editor.
    fn expression_editor(
        &mut self,
        ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        props: &[Prop],
        strip: Rect,
    ) {
        ui.painter().rect_filled(strip, 0.0, color::PANEL);
        let stage = cx.stage;
        let sel = session.lock().selected_props.clone();
        let target = sel
            .iter()
            .chain(self.focus.iter())
            .cloned()
            .chain(props.iter().map(Prop::id))
            .find(|(p, a)| query::attr_spec(stage, p, a).is_some_and(|s| s.expression.is_some()));
        let Some((path, attr)) = target else {
            ui.painter().text(
                pos2(strip.left() + 6.0, strip.center().y),
                Align2::LEFT_CENTER,
                tr("No expression on the focused property"),
                theme::small(),
                color::TEXT_DIM,
            );
            return;
        };
        let Some(expr) = query::attr_spec(stage, &path, &attr).and_then(|s| s.expression.clone())
        else {
            return;
        };
        let editing = self
            .expr_buf
            .as_ref()
            .is_some_and(|(p, a, _)| *p == path && *a == attr);
        let mut buf = if editing {
            self.expr_buf
                .as_ref()
                .map(|e| e.2.clone())
                .unwrap_or_default()
        } else {
            expr.source.clone()
        };
        let (font, text, bg) = crate::common::expression_style(cx.resources);
        let r = ui
            .put(
                strip.shrink2(vec2(4.0, 1.0)),
                egui::TextEdit::singleline(&mut buf)
                    .font(font)
                    .text_color(text)
                    .background_color(bg),
            )
            .labeled(egui::WidgetType::TextEdit, tr("Expression"));
        if r.changed() || r.has_focus() {
            self.expr_buf = Some((path.clone(), attr.clone(), buf.clone()));
        }
        if r.lost_focus() {
            self.expr_buf = None;
            if buf != expr.source {
                cx.invoke_with(
                    "attr.set_expression",
                    json!({"path": path, "attr": attr, "source": buf, "enabled": expr.enabled}),
                );
            }
        }
    }
}

/// Chinese strings of this module (keys are the English text, see
/// `bae_ui::i18n`).
pub(crate) const ZH: &[(&str, &str)] = &[
    // Graph Editor (AE-UI-077 … 083): AE's Simplified Chinese wording.
    ("Graph Editor", "图表编辑器"),
    ("Graph Editor Set", "图表编辑器集"),
    (
        "Choose which properties are shown in the graph editor",
        "选择图表编辑器中显示的属性",
    ),
    ("Show Selected Properties", "显示选择的属性"),
    ("Show Animated Properties", "显示动画属性"),
    ("Show Graph Editor Set", "显示图表编辑器集"),
    ("Choose graph type and options", "选择图表类型和选项"),
    ("Auto-Select Graph Type", "自动选择图表类型"),
    ("Edit Value Graph", "编辑值图表"),
    ("Edit Speed Graph", "编辑速度图表"),
    ("Show Reference Graph", "显示参考图表"),
    ("Show Audio Waveforms", "显示音频波形"),
    ("Show Layer In/Out Points", "显示图层的入点/出点"),
    ("Show Layer Markers", "显示图层标记"),
    ("Show Graph Tool Tips", "显示图表工具技巧"),
    ("Show Expression Editor", "显示表达式编辑器"),
    ("Allow Keyframes Between Frames", "允许帧之间的关键帧"),
    (
        "Show Transform Box when multiple keys are selected",
        "选择多个关键帧时，显示变换框",
    ),
    ("Snap", "对齐"),
    ("Auto-zoom graph height", "自动缩放图表高度"),
    ("Fit selection to view", "使选择适于查看"),
    ("Fit all graphs to view", "使所有图表适于查看"),
    ("Separate Dimensions", "单独尺寸"),
    ("Edit selected keyframes", "编辑选定的关键帧"),
    (
        "Convert selected keyframes to Hold",
        "将选定的关键帧转换为定格",
    ),
    (
        "Convert selected keyframes to Linear",
        "将选定的关键帧转换为线性",
    ),
    (
        "Convert selected keyframes to Auto Bezier",
        "将选定的关键帧转换为自动贝塞尔曲线",
    ),
    ("sec", "秒"),
    ("X Position", "X 位置"),
    ("Y Position", "Y 位置"),
    ("Z Position", "Z 位置"),
    (
        "Select a property to show its graph",
        "选择属性以显示其图表",
    ),
    (
        "No expression on the focused property",
        "当前属性没有表达式",
    ),
    ("Edit Keyframes", "编辑关键帧"),
];

/// Registers this module's Chinese strings (`MotionUiModule::build`).
pub(crate) fn register(_app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(attr: &str, keys: Vec<(f64, Keyframe)>, spatial: bool, speed: bool) -> Prop {
        let mut spec = AttrSpec::default();
        for (t, k) in keys {
            spec.keyframes.insert(Time::from_seconds(t), k);
        }
        let dims = spec
            .keyframes
            .values()
            .next()
            .and_then(|k| comps(&k.value))
            .map_or(1, |c| c.len());
        Prop {
            path: PrimPath::new("/l").unwrap(),
            attr: attr.into(),
            layer: PrimPath::new("/l").unwrap(),
            spec,
            spatial,
            dims,
            shared: false,
            speed,
            clamp: None,
            timing: LayerTiming::default(),
            index: 0,
        }
    }

    fn bez(v: impl Into<Value>, continuous: bool) -> Keyframe {
        let mut k = Keyframe::linear(v);
        k.interp_in = Interpolation::Bezier;
        k.interp_out = Interpolation::Bezier;
        k.ease_in = vec![Ease::default()];
        k.ease_out = vec![Ease::default()];
        k.flags.temporal_continuous = continuous;
        k
    }

    #[test]
    fn value_graph_handle_maps_to_speed_and_influence() {
        // 0 → 100 over 2 s; out handle of the first key dragged to (0.5 s, 50).
        let p = prop(
            "transform:rotation",
            vec![(0.0, bez(0.0, true)), (2.0, bez(100.0, true))],
            false,
            false,
        );
        let e = drag_handle(
            &p,
            Time::ZERO,
            0,
            Side::Out,
            false,
            Time::from_seconds(0.5),
            50.0,
        )
        .unwrap();
        let out = e.ease_out.unwrap()[0];
        assert!((out.influence - 25.0).abs() < 1e-9);
        assert!((out.speed - 100.0).abs() < 1e-9);
        // Continuous: the incoming side takes the same speed, keeps its influence.
        let inn = e.ease_in.unwrap()[0];
        assert_eq!((inn.speed, inn.influence), (100.0, Ease::DEFAULT_INFLUENCE));
        // In handle of the last key at (1.5 s, 50): speed (100 − 50) / 0.5.
        let e = drag_handle(
            &p,
            Time::from_seconds(2.0),
            0,
            Side::In,
            true,
            Time::from_seconds(1.5),
            50.0,
        )
        .unwrap();
        let inn = e.ease_in.unwrap()[0];
        assert!((inn.speed - 100.0).abs() < 1e-9 && (inn.influence - 25.0).abs() < 1e-9);
        assert!(e.ease_out.is_none(), "split: the other side is untouched");
        assert_eq!(e.continuous, Some(false));
        // Influence is clamped to 100 % past the neighbour.
        let e = drag_handle(
            &p,
            Time::ZERO,
            0,
            Side::Out,
            false,
            Time::from_seconds(3.0),
            0.0,
        )
        .unwrap();
        assert_eq!(e.ease_out.unwrap()[0].influence, 100.0);
    }

    #[test]
    fn speed_graph_handle_and_linear_speed() {
        let p = prop(
            "transform:position",
            vec![
                (0.0, bez([0.0, 0.0, 0.0], false)),
                (1.0, bez([300.0, 400.0, 0.0], false)),
            ],
            true,
            true,
        );
        // Speed graph: the pointer height is the speed; spatial speeds are ≥ 0.
        let e = drag_handle(
            &p,
            Time::ZERO,
            0,
            Side::Out,
            false,
            Time::from_seconds(0.25),
            -20.0,
        )
        .unwrap();
        let out = e.ease_out.unwrap()[0];
        assert_eq!(out.speed, 0.0);
        assert!((out.influence - 25.0).abs() < 1e-9);
        // A linear segment has constant speed: 500 px / 1 s.
        let lin = prop(
            "transform:position",
            vec![
                (0.0, Keyframe::linear([0.0, 0.0, 0.0])),
                (1.0, Keyframe::linear([300.0, 400.0, 0.0])),
            ],
            true,
            true,
        );
        let axis = TimeAxis {
            left: 0.0,
            width: 1016.0,
            start: Time::ZERO,
            span: Time::from_seconds(1.0),
        };
        let pieces = sample(&lin, &lin.curve().unwrap(), &axis, 0.0, 1016.0, true);
        assert_eq!(pieces.len(), 1);
        for (_, v) in &pieces[0].pts {
            assert!((v - 500.0).abs() < 1e-3, "{v}");
        }
    }

    /// #552: a linear segment's constant speed draws as a level line, not as its
    /// rounding noise fitted to the full height.
    #[test]
    fn constant_speed_draws_level() {
        let lin = prop(
            "transform:position",
            vec![
                (62.0 / 30.0, Keyframe::linear([960.0, 540.0, 0.0])),
                (242.0 / 30.0, Keyframe::linear([1500.0, 540.0, 0.0])),
            ],
            true,
            true,
        );
        let axis = TimeAxis {
            left: 0.0,
            width: 1500.0,
            start: Time::ZERO,
            span: Time::from_seconds(10.0),
        };
        let c = lin.curve().unwrap();
        let data = Sampled::new(
            sample(&lin, &c, &axis, 0.0, 1500.0, true),
            key_values(&lin, &c, true),
        );
        let va = VAxis::new(
            padded(data.range.unwrap()),
            Rect::from_min_size(Pos2::ZERO, vec2(1500.0, 300.0)),
        );
        let ys: Vec<f32> = data.pieces[0].pts.iter().map(|(_, v)| va.y(*v)).collect();
        let (lo, hi) = ys
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), y| (a.min(*y), b.max(*y)));
        assert!(hi - lo < 0.5, "a level line, not spikes: {lo}..{hi}");
        assert!((va.v(lo) - 90.0).abs() < 1e-6);
    }

    #[test]
    fn hold_segments_step_in_the_value_graph() {
        let mut a = Keyframe::linear(0.0);
        a.interp_out = Interpolation::Hold;
        let p = prop(
            "transform:opacity",
            vec![(0.0, a), (1.0, Keyframe::linear(100.0))],
            false,
            false,
        );
        let axis = TimeAxis {
            left: 0.0,
            width: 116.0,
            start: Time::ZERO,
            span: Time::from_seconds(1.0),
        };
        let c = p.curve().unwrap();
        let pieces = sample(&p, &c, &axis, 0.0, 116.0, false);
        let step = pieces
            .iter()
            .find(|pc| pc.pts.len() == 3)
            .expect("hold step");
        assert_eq!(step.pts[0].1, 0.0);
        assert_eq!(step.pts[1].1, 0.0);
        assert_eq!(step.pts[2].1, 100.0);
        // No speed curve across a hold.
        assert!(sample(&p, &c, &axis, 0.0, 116.0, true).is_empty());
    }

    #[test]
    fn snapping_and_ticks() {
        let c = [Time(100), Time(200)];
        assert_eq!(snap_to(Time(190), &c, Time(15)), Some(Time(200)));
        assert_eq!(snap_to(Time(150), &c, Time(15)), None);
        assert_eq!(nice_step(100.0, 10.0), 10.0);
        assert_eq!(nice_step(7.0, 10.0), 1.0);
        assert_eq!(nice_step(300.0, 4.0), 100.0);
        let va = VAxis {
            lo: 0.0,
            hi: 100.0,
            top: 0.0,
            bottom: 100.0,
        };
        assert_eq!(va.y(25.0), 75.0);
        assert!((va.v(75.0) - 25.0).abs() < 1e-9);
    }
}
