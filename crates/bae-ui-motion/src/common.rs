//! Helpers shared by the motion panels: the session resource, a cached evaluator,
//! time formatting, property labels and value editors that turn edits into commands.

use std::sync::Arc;

use bae_anim::Evaluator;
use bae_app::Resources;
use bae_model::{LayerKind, PrimPath, PrimType, Stage, StageEvent, Time, Value};
use bae_motion::query::{self, CompInfo};
use bae_ui::Accessible as _;
use bae_ui::theme::{self, color};
use bae_ui::widgets::{self, HotNumber};
use bae_ui::{PanelContext, tr};
use egui::{Color32, Id, Ui, vec2};
use serde_json::json;

use crate::session::{Pending, Scrub, SharedSession};

/// The session resource (inserted by [`crate::MotionUiModule`]); a private one when
/// the app did not insert it (tests, other hosts).
/// Preferences the motion commands read (Default Spatial Interpolation to Linear),
/// pushed to them when they change. Every motion panel calls this once a frame
/// through [`session`]; it is an atomic compare when nothing changed.
fn sync_prefs(resources: &Resources) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEEN: AtomicU64 = AtomicU64::new(u64::MAX);
    let Some(store) = resources.get::<Arc<bae_prefs::PrefsStore>>() else {
        return;
    };
    let generation = store.generation();
    if SEEN.swap(generation, Ordering::Relaxed) != generation {
        bae_motion::set_default_spatial_linear(
            store
                .current()
                .bool(bae_prefs::keys::DEFAULT_SPATIAL_LINEAR),
        );
    }
}

/// Expression fields (timeline rows, Graph Editor): font size and colours from
/// Preferences > Scripting & Expressions > Expression Editor (AE defaults without a
/// preferences store). Returns (font, text colour, background).
pub(crate) fn expression_style(
    resources: &Resources,
) -> (egui::FontId, egui::Color32, egui::Color32) {
    let rgb = |c: [u8; 3]| egui::Color32::from_rgb(c[0], c[1], c[2]);
    match resources.get::<Arc<bae_prefs::PrefsStore>>() {
        Some(store) => {
            let p = store.display();
            (
                egui::FontId::monospace(p.int(bae_prefs::keys::EXPR_FONT_SIZE).clamp(6, 72) as f32),
                rgb(p.color("scripting_expressions.expr_color_default")),
                rgb(p.color("scripting_expressions.expr_color_background")),
            )
        }
        None => (
            egui::FontId::monospace(12.0),
            egui::Color32::from_rgb(0xcc, 0xcc, 0xcc),
            egui::Color32::from_rgb(0x1d, 0x1d, 0x1d),
        ),
    }
}

pub(crate) fn session(resources: &Resources) -> SharedSession {
    sync_prefs(resources);
    resources
        .get::<SharedSession>()
        .cloned()
        .unwrap_or_else(|| {
            thread_local! {
                static LOCAL: SharedSession = SharedSession::default();
            }
            LOCAL.with(Clone::clone)
        })
}

/// A panel evaluator that invalidates only the compositions touched by an edit.
#[derive(Default)]
pub(crate) struct EvalCache {
    eval: Evaluator,
    changes: StageChanges,
}

#[derive(Default)]
pub(crate) struct StageChanges {
    revision: Option<u64>,
}

impl StageChanges {
    /// Visit new edits once. A missing event means the project was replaced or
    /// the consumer fell behind the stage's retained history.
    pub(crate) fn sync(
        &mut self,
        stage: &Stage,
        mut apply: impl FnMut(Option<&StageEvent>),
    ) -> bool {
        let revision = stage.revision();
        if self.revision == Some(revision) {
            return false;
        }
        let previous = self.revision.replace(revision).unwrap_or(revision);
        if let Some(events) = stage.changes_since(previous) {
            for event in events {
                apply(Some(event));
            }
        } else {
            apply(None);
        }
        true
    }
}

impl EvalCache {
    pub(crate) fn get(&mut self, stage: &Stage) -> &mut Evaluator {
        self.changes.sync(stage, |event| match event {
            Some(event) => self.eval.invalidate(event),
            None => self.eval.clear(),
        });
        &mut self.eval
    }
}

/// `0:00:01:00` timecode of a comp time (AE's default display).
pub(crate) fn timecode(info: &CompInfo, t: Time) -> String {
    let frame = info.frame_rate.time_to_frame(t + info.display_start);
    widgets::timecode(frame, info.frame_rate.as_f64())
}

/// A duration as timecode (from 0, ignoring the comp's start timecode).
pub(crate) fn duration_code(info: &CompInfo, d: Time) -> String {
    widgets::timecode(info.frame_rate.time_to_frame(d), info.frame_rate.as_f64())
}

/// Frame number of a comp time.
pub(crate) fn frame_of(info: &CompInfo, t: Time) -> i64 {
    info.frame_rate.time_to_frame(t)
}

/// English label of a property (displayed through `tr`).
/// Integer properties AE shows as a dropdown: their values and names.
pub(crate) fn int_choices(attr: &str) -> Option<&'static [(i64, &'static str)]> {
    match attr {
        "material:casts_shadows" | "material:accepts_shadows" => {
            Some(&[(0, "Off"), (1, "On"), (2, "Only")])
        }
        "camera:iris_shape" => Some(&[
            (1, "Fast Rectangle"),
            (3, "Triangle"),
            (4, "Square"),
            (5, "Pentagon"),
            (6, "Hexagon"),
            (7, "Heptagon"),
            (8, "Octagon"),
            (9, "Nonagon"),
            (10, "Decagon"),
        ]),
        _ => None,
    }
}

pub(crate) fn attr_label(prim_type: Option<&PrimType>, attr: &str, three_d: bool) -> String {
    let fixed = match attr {
        "audio:levels" => "Audio Levels",
        "time_remap" => "Time Remap",
        "transform:anchor_point" => "Anchor Point",
        "transform:position" => "Position",
        "transform:position_x" => "X Position",
        "transform:position_y" => "Y Position",
        "transform:position_z" => "Z Position",
        "transform:scale" => "Scale",
        "transform:rotation" if three_d => "Z Rotation",
        "transform:rotation" => "Rotation",
        "transform:rotation_x" => "X Rotation",
        "transform:rotation_y" => "Y Rotation",
        "transform:orientation" => "Orientation",
        "transform:opacity" => "Opacity",
        "transform:point_of_interest" => "Point of Interest",
        "transform:skew" => "Skew",
        "transform:skew_axis" => "Skew Axis",
        "text:source" => "Source Text",
        "text:path" => "Path",
        "text:reverse_path" => "Reverse Path",
        "text:perpendicular_to_path" => "Perpendicular To Path",
        "text:force_alignment" => "Force Alignment",
        "text:first_margin" => "First Margin",
        "text:last_margin" => "Last Margin",
        // Text animators (AE: Animator N > Range Selector N, properties).
        "animator:anchor_point" => "Anchor Point",
        "animator:position" => "Position",
        "animator:scale" => "Scale",
        "animator:skew" => "Skew",
        "animator:rotation" => "Rotation",
        "animator:opacity" => "Opacity",
        "animator:fill_color" => "Fill Color",
        "animator:stroke_color" => "Stroke Color",
        "animator:stroke_width" => "Stroke Width",
        "animator:tracking" => "Tracking Amount",
        "animator:line_spacing" => "Line Spacing",
        "animator:blur" => "Blur",
        // AE's Chinese UI says 起始 here but 开始 for Trim Paths.
        "start" if prim_type == Some(&PrimType::TEXT_SELECTOR) => {
            return match bae_ui::i18n::lang() {
                bae_ui::i18n::Lang::Zh => "起始".into(),
                _ => "Start".into(),
            };
        }
        "end" if prim_type == Some(&PrimType::TEXT_SELECTOR) => "End",
        "offset" if prim_type == Some(&PrimType::TEXT_SELECTOR) => "Offset",
        "units" => "Units",
        "based_on" => "Based On",
        "mode" if prim_type == Some(&PrimType::TEXT_SELECTOR) => "Mode",
        "amount" => "Amount",
        "shape" if prim_type == Some(&PrimType::TEXT_SELECTOR) => "Shape",
        "smoothness" => "Smoothness",
        "ease_high" => "Ease High",
        "ease_low" => "Ease Low",
        "randomize_order" => "Randomize Order",
        "random_seed" => "Random Seed",
        "size" => "Size",
        "position" => "Position",
        "roundness" => "Roundness",
        "points" => "Points",
        "rotation" => "Rotation",
        "inner_radius" => "Inner Radius",
        "outer_radius" => "Outer Radius",
        "inner_roundness" => "Inner Roundness",
        "outer_roundness" => "Outer Roundness",
        "color" => "Color",
        "opacity" if prim_type == Some(&PrimType::MASK) => "Mask Opacity",
        "opacity" => "Opacity",
        "width" => "Stroke Width",
        "path" if prim_type == Some(&PrimType::MASK) => "Mask Path",
        "path" => "Path",
        "feather" => "Mask Feather",
        "expansion" => "Mask Expansion",
        "camera:zoom" => "Zoom",
        "camera:depth_of_field" => "Depth of Field",
        "camera:focus_distance" => "Focus Distance",
        "camera:aperture" => "Aperture",
        "camera:blur_level" => "Blur Level",
        "camera:iris_shape" => "Iris Shape",
        "camera:iris_rotation" => "Iris Rotation",
        "camera:iris_roundness" => "Iris Roundness",
        "camera:iris_aspect_ratio" => "Iris Aspect Ratio",
        "camera:iris_diffraction_fringe" => "Iris Diffraction Fringe",
        "camera:highlight_gain" => "Highlight Gain",
        "camera:highlight_threshold" => "Highlight Threshold",
        "camera:highlight_saturation" => "Highlight Saturation",
        "light:intensity" => "Intensity",
        "light:color" => "Color",
        "light:cone_angle" => "Cone Angle",
        "light:cone_feather" => "Cone Feather",
        "light:falloff" => "Falloff",
        "light:radius" => "Radius",
        "light:falloff_distance" => "Falloff Distance",
        "light:casts_shadows" | "material:casts_shadows" => "Casts Shadows",
        "light:shadow_darkness" => "Shadow Darkness",
        "light:shadow_diffusion" => "Shadow Diffusion",
        "material:light_transmission" => "Light Transmission",
        "material:accepts_shadows" => "Accepts Shadows",
        "material:accepts_lights" => "Accepts Lights",
        // AE's Chinese UI calls the material's Ambient 周围 (the light type is 环境).
        "material:ambient" => {
            return match bae_ui::i18n::lang() {
                bae_ui::i18n::Lang::Zh => "周围".into(),
                _ => "Ambient".into(),
            };
        }
        "material:diffuse" => "Diffuse",
        "material:specular_intensity" => "Specular Intensity",
        "material:specular_shininess" => "Specular Shininess",
        "material:metal" => "Metal",
        other => {
            // Effect parameters and unknown attributes: a readable form of the name.
            let s = other.rsplit(':').next().unwrap_or(other).replace('_', " ");
            let mut c = s.chars();
            return match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => s,
            };
        }
    };
    tr(fixed).to_owned()
}

/// Unit drawn after a value.
fn suffix(attr: &str) -> &'static str {
    match attr {
        "transform:scale" | "transform:opacity" | "opacity" | "roundness" | "inner_roundness"
        | "outer_roundness" => "%",
        "transform:rotation"
        | "transform:rotation_x"
        | "transform:rotation_y"
        | "rotation"
        | "transform:skew"
        | "transform:skew_axis" => "°",
        _ => "",
    }
}

fn decimals(attr: &str) -> usize {
    match attr {
        "transform:opacity" | "opacity" | "points" => 0,
        _ => 1,
    }
}

/// What a value editor did this frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Edit {
    None,
    /// Being dragged: preview only.
    Live(Value),
    /// Finished: run the command.
    Commit(Value),
}

enum Num {
    None,
    Live(f64),
    Commit(f64),
}

/// A hot number whose scrubbed value is kept until the drag ends (one command per drag).
fn number(
    ui: &mut Ui,
    id: Id,
    model: f64,
    attr: &str,
    suffix: &str,
    name: &str,
    tint: Color32,
) -> Num {
    let key = id.with("scrub");
    let mut v = ui.data(|d| d.get_temp::<f64>(key)).unwrap_or(model);
    let resp = ui
        .push_id(id, |ui| {
            ui.add(
                HotNumber::new(&mut v)
                    .tint(tint)
                    .decimals(decimals(attr))
                    .speed(if attr.ends_with("opacity") { 1.0 } else { 0.5 })
                    .suffix(suffix)
                    .label(name),
            )
        })
        .inner;
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<f64>(key));
        return Num::Commit(v);
    }
    if resp.dragged() {
        ui.data_mut(|d| d.insert_temp(key, v));
        return Num::Live(v);
    }
    if resp.changed() {
        return Num::Commit(v);
    }
    Num::None
}

/// Values an enabled expression drives are red (AE).
pub(crate) const EXPRESSION_VALUE: Color32 = Color32::from_rgb(0xe0, 0x5a, 0x5a);

/// Editor for one property value (hot numbers, color swatch, checkbox, text).
/// `dims` limits vector components (2 for 2D layers). `name` is what screen
/// readers call the property ("Layer “Solid 1” · Position"); components get
/// X / Y / Z after it (issue #128).
pub(crate) fn value_editor(
    ui: &mut Ui,
    id: Id,
    attr: &str,
    value: &Value,
    dims: usize,
    name: &str,
    expression: bool,
) -> Edit {
    let tint = if expression {
        EXPRESSION_VALUE
    } else {
        color::ACCENT
    };
    let axis = |i: usize| bae_ui::a11y::context(&[name, ["X", "Y", "Z"][i.min(2)]]);
    let mut out = Edit::None;
    let mut apply = |n: Num, f: &dyn Fn(f64) -> Value| match n {
        Num::None => {}
        Num::Live(v) => out = Edit::Live(f(v)),
        Num::Commit(v) => out = Edit::Commit(f(v)),
    };
    ui.spacing_mut().item_spacing.x = 4.0;
    match value {
        Value::Float(v) if attr.ends_with("rotation") || attr == "rotation" => {
            // AE shows rotation as revolutions + degrees: 1x+30.0°.
            let revs = (v / 360.0).trunc();
            let deg = v - revs * 360.0;
            let n = number(
                ui,
                id.with(0),
                revs,
                "points",
                "x+",
                &bae_ui::a11y::context(&[name, tr("Revolutions")]),
                tint,
            );
            apply(n, &|r| Value::Float(r.trunc() * 360.0 + deg));
            let n = number(
                ui,
                id.with(1),
                deg,
                attr,
                "°",
                &bae_ui::a11y::context(&[name, tr("Degrees")]),
                tint,
            );
            apply(n, &|d| Value::Float(revs * 360.0 + d));
        }
        Value::Float(v) => {
            let n = number(ui, id, *v, attr, suffix(attr), name, tint);
            apply(n, &|x| Value::Float(x));
        }
        Value::Int(v) => {
            let n = number(ui, id, *v as f64, "points", "", name, tint);
            apply(n, &|x| Value::Int(x.round() as i64));
        }
        Value::Vec2(v) | Value::Vec3([v @ .., _])
            if matches!(value, Value::Vec2(_)) || dims == 2 =>
        {
            let v = [v[0], v[1]];
            let z = match value {
                Value::Vec3(a) => Some(a[2]),
                _ => None,
            };
            let linked = attr.ends_with("scale");
            for i in 0..2 {
                let n = number(
                    ui,
                    id.with(i),
                    v[i],
                    attr,
                    if i == 1 { suffix(attr) } else { "" },
                    &axis(i),
                    tint,
                );
                apply(n, &|x| {
                    let mut w = v;
                    if linked && v[i] != 0.0 {
                        let f = x / v[i];
                        w = [v[0] * f, v[1] * f];
                    }
                    w[i] = x;
                    match z {
                        Some(z) => Value::Vec3([
                            w[0],
                            w[1],
                            if linked && v[i] != 0.0 {
                                z * x / v[i]
                            } else {
                                z
                            },
                        ]),
                        None => Value::Vec2(w),
                    }
                });
                if i == 0 {
                    ui.label(",");
                }
            }
        }
        Value::Vec3(v) => {
            let v = *v;
            let linked = attr.ends_with("scale");
            for i in 0..3 {
                let n = number(
                    ui,
                    id.with(i),
                    v[i],
                    attr,
                    if i == 2 { suffix(attr) } else { "" },
                    &axis(i),
                    tint,
                );
                apply(n, &|x| {
                    let mut w = v;
                    if linked && v[i] != 0.0 {
                        let f = x / v[i];
                        w = [v[0] * f, v[1] * f, v[2] * f];
                    }
                    w[i] = x;
                    Value::Vec3(w)
                });
                if i < 2 {
                    ui.label(",");
                }
            }
        }
        Value::Color(c) => {
            let key = id.with("color");
            let mut rgba = ui.data(|d| d.get_temp::<[f32; 4]>(key)).unwrap_or([
                c[0] as f32,
                c[1] as f32,
                c[2] as f32,
                c[3] as f32,
            ]);
            let resp = ui
                .color_edit_button_rgba_unmultiplied(&mut rgba)
                .labeled(egui::WidgetType::ColorButton, name);
            let to_value = |c: [f32; 4]| Value::Color(c.map(f64::from));
            if resp.changed() {
                ui.data_mut(|d| d.insert_temp(key, rgba));
                out = Edit::Live(to_value(rgba));
            } else if ui.data(|d| d.get_temp::<[f32; 4]>(key)).is_some()
                && ui.input(|i| i.pointer.any_released())
            {
                ui.data_mut(|d| d.remove::<[f32; 4]>(key));
                out = Edit::Commit(to_value(rgba));
            }
        }
        Value::Bool(b) => {
            let mut b2 = *b;
            if widgets::checkbox(ui, &mut b2, "")
                .labeled_toggle(egui::WidgetType::Checkbox, name, b2)
                .changed()
            {
                out = Edit::Commit(Value::Bool(b2));
            }
        }
        Value::TextDocument(doc) => {
            let key = id.with("text");
            let mut text = ui
                .data(|d| d.get_temp::<String>(key))
                .unwrap_or_else(|| doc.text.clone());
            let resp = ui
                .add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(160.0)
                        .text_color(color::TEXT_STRONG),
                )
                .labeled(egui::WidgetType::TextEdit, name);
            if resp.changed() {
                ui.data_mut(|d| d.insert_temp(key, text.clone()));
            }
            if resp.lost_focus() {
                ui.data_mut(|d| d.remove::<String>(key));
                if text != doc.text {
                    let mut d = (**doc).clone();
                    d.set_text(&text);
                    out = Edit::Commit(Value::TextDocument(Box::new(d)));
                }
            }
        }
        Value::Enum(s) | Value::String(s) => {
            ui.label(egui::RichText::new(s).color(color::TEXT_DIM));
        }
        _ => {}
    }
    out
}

/// Turn an edit into a command (or a live preview) for `attr` of `path` at comp
/// time `t`.
pub(crate) fn apply_edit(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    path: &PrimPath,
    attr: &str,
    t: Time,
    edit: Edit,
) {
    match edit {
        Edit::None => {}
        Edit::Live(value) => {
            let peers = edit_peers(cx.stage, session, path, attr);
            let peers = peer_values(cx.stage, path, attr, t, &value, &peers, true);
            session.lock().scrub = Some(Scrub {
                path: path.clone(),
                attr: attr.to_owned(),
                value,
                peers,
                time: t,
                committed: None,
            });
        }
        Edit::Commit(value) => {
            // A drag moves the others by as much; a typed value sets them all.
            let dragged = session
                .lock()
                .scrub
                .as_ref()
                .is_some_and(|s| s.path == *path && s.attr == attr && s.committed.is_none());
            let peers = edit_peers(cx.stage, session, path, attr);
            let peers = peer_values(cx.stage, path, attr, t, &value, &peers, dragged);
            // Shown as committed until the command has run.
            session.lock().scrub = Some(Scrub {
                path: path.clone(),
                attr: attr.to_owned(),
                value: value.clone(),
                peers: peers.clone(),
                time: t,
                committed: Some((cx.stage.revision(), std::time::Instant::now())),
            });
            let edits: Vec<serde_json::Value> = std::iter::once((path.clone(), value))
                .chain(peers)
                .map(|(p, v)| {
                    let time = query::to_key_time(cx.stage, &p, t);
                    json!({"path": p, "attr": attr, "time": time, "value": v})
                })
                .collect();
            if let [one] = &edits[..] {
                cx.invoke_with("attr.set_at", one.clone());
            } else {
                // One undo step for all of them.
                cx.invoke_with("attr.set_many", json!({ "edits": edits }));
            }
        }
    }
}

/// The same property as `attr` of `path` elsewhere in the selection, which an
/// edit of it changes too (AE): on the other selected layers when `path`'s
/// layer is one of several selected, and the same property selected on other
/// layers or effects. Locked layers are left alone.
pub(crate) fn edit_peers(
    stage: &Stage,
    session: &SharedSession,
    path: &PrimPath,
    attr: &str,
) -> Vec<PrimPath> {
    let Some(layer) = query::owning_layer(stage, path) else {
        return Vec::new();
    };
    let mut out: Vec<PrimPath> = Vec::new();
    {
        let s = session.lock();
        if s.selected_layers.len() > 1 && s.selected_layers.contains(&layer) {
            out.extend(
                s.selected_layers
                    .iter()
                    .filter(|l| **l != layer)
                    .filter_map(|l| counterpart(stage, &layer, path, l)),
            );
        }
        if s.selected_props.iter().any(|(p, a)| p == path && a == attr) {
            out.extend(
                s.selected_props
                    .iter()
                    .filter(|(p, a)| a == attr && same_owner(stage, path, p))
                    .map(|(p, _)| p.clone()),
            );
        }
    }
    let mut seen = std::collections::HashSet::new();
    // Text properties only on text layers.
    let text = |p: &PrimPath| query::layer_kind(stage, p).as_deref() == Some("text");
    out.retain(|p| {
        p != path
            && query::locked_layer_of(stage, p).is_none()
            && (!attr.starts_with("text:") || text(p))
            && seen.insert(p.clone())
    });
    out
}

fn match_name(stage: &Stage, path: &PrimPath) -> Option<String> {
    let view = stage.prim(path)?;
    query::static_value(&view, "match_name").and_then(|v| v.as_str().map(str::to_owned))
}

/// Whether `a` and `b` hold the same properties: the same prim type and the
/// same effect.
fn same_owner(stage: &Stage, a: &PrimPath, b: &PrimPath) -> bool {
    let ty = |p: &PrimPath| stage.prim(p).and_then(|v| v.prim_type().cloned());
    ty(a).is_some() && ty(a) == ty(b) && match_name(stage, a) == match_name(stage, b)
}

/// `path` (of `layer`) on `other`: the same prim below it, else (effects) the
/// same instance of the same effect (the second Gaussian Blur → the second).
fn counterpart(
    stage: &Stage,
    layer: &PrimPath,
    path: &PrimPath,
    other: &PrimPath,
) -> Option<PrimPath> {
    let rel = path.as_str().strip_prefix(layer.as_str())?;
    if let Ok(p) = PrimPath::new(&format!("{other}{rel}"))
        && stage.has_prim(&p)
        && same_owner(stage, path, &p)
    {
        return Some(p);
    }
    let m = match_name(stage, path)?;
    let same = |l: &PrimPath| {
        bae_motion::effect::effects_of(stage, l)
            .into_iter()
            .filter(|e| match_name(stage, e).as_deref() == Some(m.as_str()))
            .collect::<Vec<_>>()
    };
    let nth = same(layer).iter().position(|e| e == path)?;
    same(other).into_iter().nth(nth)
}

/// The peers' new values for `value` of `path`: offset by the same amount
/// (`relative`, a drag) or the same value; peers whose value has another type
/// are left out.
fn peer_values(
    stage: &Stage,
    path: &PrimPath,
    attr: &str,
    t: Time,
    value: &Value,
    peers: &[PrimPath],
    relative: bool,
) -> Vec<(PrimPath, Value)> {
    let at = |p: &PrimPath| query::sample_attr(stage, p, attr, query::to_key_time(stage, p, t));
    let from = at(path);
    peers
        .iter()
        .filter_map(|p| {
            let base = at(p)?;
            if std::mem::discriminant(&base) != std::mem::discriminant(value) {
                return None;
            }
            let v = match (relative, &from) {
                (true, Some(from)) => offset(&base, from, value),
                _ => value.clone(),
            };
            Some((p.clone(), v))
        })
        .collect()
}

/// `base` moved by `to - from` (numbers and vectors; other values become `to`).
fn offset(base: &Value, from: &Value, to: &Value) -> Value {
    match (base, from, to) {
        (Value::Float(b), Value::Float(f), Value::Float(t)) => Value::Float(b + (t - f)),
        (Value::Int(b), Value::Int(f), Value::Int(t)) => Value::Int(b + (t - f)),
        (Value::Vec2(b), Value::Vec2(f), Value::Vec2(t)) => {
            Value::Vec2(std::array::from_fn(|i| b[i] + (t[i] - f[i])))
        }
        (Value::Vec3(b), Value::Vec3(f), Value::Vec3(t)) => {
            Value::Vec3(std::array::from_fn(|i| b[i] + (t[i] - f[i])))
        }
        _ => to.clone(),
    }
}

/// The mask a text layer's characters follow (Path Options > Path), if it
/// still exists.
pub(crate) fn text_path_mask(stage: &Stage, layer: &PrimPath) -> Option<PrimPath> {
    let view = stage.prim(layer)?;
    let mask = view.value("text:path")?.as_prim_ref()?.clone();
    stage.has_prim(&mask).then_some(mask)
}

/// The Path popup of Path Options: None or one of the layer's masks.
pub(crate) fn text_path_picker(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    layer: &PrimPath,
    id: egui::Id,
) {
    let stage = cx.stage;
    let current = text_path_mask(stage, layer);
    let masks: Vec<PrimPath> = layer
        .child("masks")
        .ok()
        .map(|m| {
            stage
                .children(&m)
                .into_iter()
                .filter(|m| stage.prim(m).is_some_and(|v| v.is_a(&PrimType::MASK)))
                .collect()
        })
        .unwrap_or_default();
    let name = |m: &PrimPath| query::display_name_of(stage, m);
    let shown = current.as_ref().map_or_else(|| tr("None").to_owned(), name);
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(140.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), tr("None")).clicked() {
                chosen = Some(None);
            }
            for m in &masks {
                if ui
                    .selectable_label(current.as_ref() == Some(m), name(m))
                    .clicked()
                {
                    chosen = Some(Some(m.clone()));
                }
            }
        });
    if let Some(mask) = chosen.filter(|m| *m != current) {
        cx.invoke_with("text.set_path", json!({"layer": layer, "mask": mask}));
    }
}

/// The current value of an attribute at comp time `t` (keyframes and expressions
/// applied), or its static value / default.
pub(crate) fn current_value(
    eval: &mut Evaluator,
    stage: &Stage,
    path: &PrimPath,
    attr: &str,
    t: Time,
) -> Option<Value> {
    eval.evaluate_attribute(stage, path, attr, t)
        .ok()
        .or_else(|| query::static_value(&stage.prim(path)?, attr))
}

/// Localized AE-style default name for a new item: `"<base> N"`, one more than
/// the highest N among the existing siblings and the items commands are about
/// to create (several Cmd+Y in one frame get Solid 1, Solid 2 …).
pub(crate) fn numbered(
    stage: &Stage,
    session: &SharedSession,
    parent: &PrimPath,
    base: &str,
) -> String {
    let base = tr(base);
    let existing = query::next_numbered_name(stage, parent, base);
    let next_existing = number_after(&existing, base).unwrap_or(1);
    let reserved = session
        .lock()
        .reserved_names(parent)
        .filter_map(|n| number_after(&n, base))
        .max()
        .map_or(0, |n| n + 1);
    format!("{base} {}", next_existing.max(reserved))
}

/// `N` of `"<base> N"`.
fn number_after(name: &str, base: &str) -> Option<u32> {
    name.strip_prefix(base)?.trim().parse().ok()
}

/// New Composition: open Composition Settings for a new comp in `folder`
/// (default /comps); OK creates it and opens it (AE).
pub(crate) fn new_comp(cx: &PanelContext<'_>, session: &SharedSession, folder: Option<&PrimPath>) {
    crate::dialogs::new_comp(cx, session, folder.cloned());
}

/// Create a layer in the active comp (above the selected layer, like AE) and select
/// it once it exists. `extra` adds `layer.create` parameters.
pub(crate) fn new_layer(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    kind: LayerKind,
    extra: serde_json::Value,
) {
    let stage = cx.stage;
    let (comp, index) = {
        let s = session.lock();
        let Some(comp) = s.active_comp.clone() else {
            return;
        };
        let layers = query::comp_layers(stage, &comp);
        let index = s
            .selected_layers
            .iter()
            .filter_map(|l| layers.iter().position(|x| x == l))
            .min()
            .unwrap_or(0);
        (comp, index)
    };
    let folder = query::layers_folder(&comp);
    let base = match kind {
        LayerKind::Shape => "Shape Layer",
        LayerKind::Solid => "Solid",
        LayerKind::Null => "Null",
        LayerKind::Camera => "Camera",
        LayerKind::Light => "Light",
        LayerKind::Adjustment => "Adjustment Layer",
        LayerKind::Text => "Text",
        _ => "Layer",
    };
    let name = match (kind, extra.get("text").and_then(|t| t.as_str())) {
        // A name from a dialog (Solid Settings).
        _ if extra.get("name").and_then(|n| n.as_str()).is_some() => {
            extra["name"].as_str().unwrap_or_default().to_owned()
        }
        (LayerKind::Text, Some(t)) => t.to_owned(),
        _ => numbered(stage, session, &folder, base),
    };
    let Ok(path) = folder.child(&query::unique_child_name(stage, &folder, &name)) else {
        return;
    };
    let mut params = json!({"comp": comp, "kind": kind, "name": name, "index": index});
    // Preferences: the kind's default label; new layers start at the comp's
    // start, or at the current time when that option is off.
    let prefs = crate::prefs::current(cx.resources);
    params["label"] = json!(crate::prefs::layer_label(&prefs, kind));
    if !crate::prefs::layers_at_comp_start(&prefs) {
        params["start"] = json!(session.lock().time(&comp));
    }
    if let (Some(p), serde_json::Value::Object(m)) = (params.as_object_mut(), extra) {
        p.extend(m);
    }
    cx.invoke_with("layer.create", params);
    let mut s = session.lock();
    s.expect(Pending::Layer(path.clone()));
    s.reserve_name(path, name);
}

/// The comp info of the session's active comp.
pub(crate) fn active_comp(stage: &Stage, session: &SharedSession) -> Option<CompInfo> {
    let comp = session.lock().active_comp.clone()?;
    query::comp_info(stage, &comp)
}

/// Whether a layer is 3D (`three_d` switch; cameras and lights always are).
pub(crate) fn is_3d(stage: &Stage, layer: &PrimPath) -> bool {
    let Some(v) = stage.prim(layer) else {
        return false;
    };
    query::static_value(&v, "three_d").and_then(|v| v.as_bool()) == Some(true)
        || matches!(
            query::layer_kind(stage, layer).as_deref(),
            Some("camera" | "light")
        )
}

/// An inline rename field (AE: Enter or clicking elsewhere commits, Esc cancels).
/// Returns `None` while editing, `Some(None)` when cancelled, `Some(Some(text))`
/// when committed. Focus is requested once, when the field first appears.
pub(crate) fn rename_field(
    ui: &mut Ui,
    rect: egui::Rect,
    buf: &mut String,
    id: Id,
) -> Option<Option<String>> {
    let started = ui.data(|d| d.get_temp::<bool>(id)).is_none();
    let r = ui
        .put(
            rect,
            egui::TextEdit::singleline(buf)
                .id(id.with("field"))
                .text_color(color::TEXT_STRONG),
        )
        .labeled(egui::WidgetType::TextEdit, tr("Name"));
    if started {
        r.request_focus();
        // AE selects the whole name: typing replaces it.
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), r.id) {
            let end = egui::text::CCursor::new(buf.chars().count());
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    end,
                )));
            state.store(ui.ctx(), r.id);
        }
        ui.data_mut(|d| d.insert_temp(id, true));
        return None;
    }
    let (enter, escape) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
        )
    });
    let result = if escape {
        Some(None)
    } else if enter || r.lost_focus() || !r.has_focus() {
        Some(Some(buf.clone()))
    } else {
        None
    };
    if result.is_some() {
        ui.data_mut(|d| d.remove::<bool>(id));
    }
    result
}

/// A centered hint in an empty panel.
pub(crate) fn empty_hint(ui: &mut Ui, title: &str, body: &str) {
    ui.add_space((ui.available_height() / 2.0 - 30.0).max(8.0));
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(title).color(color::TEXT_STRONG));
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(body)
                .color(color::TEXT_DIM)
                .font(theme::small()),
        );
    });
}

/// A small text button in AE style (dark field).
pub(crate) fn text_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(color::TEXT_STRONG))
            .min_size(vec2(0.0, 20.0)),
    )
}

// Menus through non-generic entry points: egui's popup and menu code is generic over
// the closure and costs several KiB per closure type (about 14 KiB for a submenu), so
// the timeline and Graph Editor menus share one instantiation each.

/// `resp.context_menu(add)`.
pub(crate) fn context_menu(resp: &egui::Response, add: &mut dyn FnMut(&mut Ui)) {
    resp.context_menu(|ui| add(ui));
}

/// `egui::Popup::menu(resp).show(add)`.
pub(crate) fn popup_menu(resp: &egui::Response, add: &mut dyn FnMut(&mut Ui)) {
    egui::Popup::menu(resp).show(|ui| add(ui));
}

/// `widgets::menu_button(ui, title, add)` (a submenu inside a menu).
pub(crate) fn submenu(ui: &mut Ui, title: &str, add: &mut dyn FnMut(&mut Ui)) {
    widgets::menu_button(ui, title, |ui| add(ui));
}

/// Values displayed by the three text panels over the viewer selection.
pub(crate) struct SelectedStyles<'a> {
    pub doc: &'a bae_model::TextDocument,
    pub range: std::ops::Range<usize>,
}

impl SelectedStyles<'_> {
    pub fn character<T: PartialEq>(&self, get: impl Fn(&bae_model::TextStyle) -> T) -> Option<T> {
        let first = get(self.doc.style_at(self.range.start));
        self.doc
            .runs
            .iter()
            .filter(|r| r.start > self.range.start && r.start < self.range.end)
            .all(|r| get(&r.style) == first)
            .then_some(first)
    }

    pub fn paragraph<T: PartialEq>(
        &self,
        get: impl Fn(&bae_model::ParagraphStyle) -> T,
    ) -> Option<T> {
        let first = get(self.doc.paragraph_at(self.range.start));
        self.doc
            .paragraphs
            .iter()
            .filter(|r| r.start > self.range.start && r.start < self.range.end)
            .all(|r| get(&r.style) == first)
            .then_some(first)
    }
}
