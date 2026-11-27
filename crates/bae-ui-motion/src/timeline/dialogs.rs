//! Keyframe dialogs: Keyframe Interpolation (AE-ANI-048, Cmd/Ctrl+Alt+K) and
//! Keyframe Velocity (AE-ANI-047, Cmd/Ctrl+Shift+K), opened from the Animation menu
//! actions and the keyframe context menu. OK runs one `key.edit` (one undo step);
//! Esc or Cancel closes without a command.
//!
//! The dialog bodies (`body`, `*_edits`) do not depend on the window around them;
//! for now the Timeline hosts them in a modal window, and they can move to the
//! shell's module dialog host (`bae_ui::dialog`, #166) unchanged.

use bae_model::{AttrSpec, Ease};
use bae_motion::KeyEdit;

use super::*;

/// An open keyframe dialog (held in the session, drawn by the Timeline).
#[derive(Clone, Debug)]
pub enum KeyDialog {
    Interpolation(InterpolationDialog),
    Velocity(VelocityDialog),
    Marker(Box<super::markers::MarkerDialog>),
    Sequence(SequenceDialog),
}

/// What closing a dialog body asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Close {
    Ok,
    Cancel,
}

// ---------------------------------------------------------------------------
// Keyframe Interpolation.

/// Temporal Interpolation menu, in AE's order.
pub(crate) const TEMPORAL: [&str; 6] = [
    "Current Settings",
    "Linear",
    "Bezier",
    "Continuous Bezier",
    "Auto Bezier",
    "Hold",
];
/// Spatial Interpolation menu.
pub(crate) const SPATIAL: [&str; 5] = [
    "Current Settings",
    "Linear",
    "Bezier",
    "Continuous Bezier",
    "Auto Bezier",
];
/// Roving menu.
pub(crate) const ROVING: [&str; 3] = ["Current Settings", "Rove Across Time", "Lock To Time"];

#[derive(Clone, Debug)]
pub struct InterpolationDialog {
    pub keys: Vec<KeyRef>,
    /// Index into [`TEMPORAL`].
    pub temporal: usize,
    /// Index into [`SPATIAL`].
    pub spatial: usize,
    /// Index into [`ROVING`].
    pub roving: usize,
}

impl InterpolationDialog {
    pub fn new(keys: Vec<KeyRef>) -> Self {
        Self {
            keys,
            temporal: 0,
            spatial: 0,
            roving: 0,
        }
    }
}

fn is_spatial(attr: &str, spec: &AttrSpec) -> bool {
    bae_anim::schema::curve_kind(attr, spec) == bae_anim::CurveKind::Spatial
}

/// The keys grouped by property, with each property's keyframes.
fn by_property<'a>(
    stage: &'a Stage,
    keys: &'a [KeyRef],
) -> Vec<(&'a PrimPath, &'a str, &'a AttrSpec, Vec<&'a KeyRef>)> {
    let mut out: Vec<(&PrimPath, &str, &AttrSpec, Vec<&KeyRef>)> = Vec::new();
    for k in keys {
        if let Some(entry) = out
            .iter_mut()
            .find(|(p, a, _, _)| **p == k.path && *a == k.attr)
        {
            entry.3.push(k);
        } else if let Some(spec) = query::attr_spec(stage, &k.path, &k.attr) {
            out.push((&k.path, k.attr.as_str(), spec, vec![k]));
        }
    }
    out
}

/// The `key.edit` edits for the dialog's choices. A method newly applied to a side
/// starts from AE's automatic values (Bezier from Linear keeps the curve of Auto
/// Bezier); "Current Settings" leaves that aspect alone. Spatial interpolation and
/// roving apply to spatial properties only; roving never to a property's first or
/// last keyframe.
pub(crate) fn interpolation_edits(stage: &Stage, d: &InterpolationDialog) -> Vec<KeyEdit> {
    let mut edits = Vec::new();
    for (_, attr, spec, keys) in by_property(stage, &d.keys) {
        let spatial = is_spatial(attr, spec);
        // AE's automatic values for these keys (temporal eases and spatial tangents).
        let auto = {
            let autos: Vec<KeyEdit> = keys
                .iter()
                .map(|k| {
                    let mut e = KeyEdit::new((*k).clone());
                    e.auto_bezier = Some(true);
                    if spatial {
                        e.spatial_auto_bezier = Some(true);
                    }
                    e
                })
                .collect();
            let refs: Vec<&KeyEdit> = autos.iter().collect();
            bae_motion::apply_key_edits(attr, spec, &refs).0
        };
        let first = spec.keyframes.keys().next().copied();
        let last = spec.keyframes.keys().next_back().copied();
        for k in keys {
            let (Some(kf), Some(akf)) = (spec.keyframes.get(&k.time), auto.keyframes.get(&k.time))
            else {
                continue;
            };
            let mut e = KeyEdit::new(k.clone());
            match TEMPORAL[d.temporal] {
                "Linear" | "Hold" => {
                    let i = if TEMPORAL[d.temporal] == "Hold" {
                        Interpolation::Hold
                    } else {
                        Interpolation::Linear
                    };
                    e.interp_in = Some(i);
                    e.interp_out = Some(i);
                    e.auto_bezier = Some(false);
                    e.continuous = Some(false);
                }
                "Bezier" | "Continuous Bezier" => {
                    e.interp_in = Some(Interpolation::Bezier);
                    e.interp_out = Some(Interpolation::Bezier);
                    e.auto_bezier = Some(false);
                    e.continuous = Some(TEMPORAL[d.temporal] == "Continuous Bezier");
                    if kf.interp_in != Interpolation::Bezier {
                        e.ease_in = Some(akf.ease_in.clone());
                    }
                    if kf.interp_out != Interpolation::Bezier {
                        e.ease_out = Some(akf.ease_out.clone());
                    }
                }
                "Auto Bezier" => e.auto_bezier = Some(true),
                _ => {}
            }
            if spatial {
                let zero = |t: Option<[f64; 3]>| t.is_none_or(|t| t == [0.0; 3]);
                match SPATIAL[d.spatial] {
                    "Linear" => {
                        e.spatial_in = Some([0.0; 3]);
                        e.spatial_out = Some([0.0; 3]);
                        e.spatial_auto_bezier = Some(false);
                        e.spatial_continuous = Some(false);
                    }
                    "Bezier" | "Continuous Bezier" => {
                        if zero(kf.spatial_in) && zero(kf.spatial_out) {
                            e.spatial_in = akf.spatial_in.or(Some([0.0; 3]));
                            e.spatial_out = akf.spatial_out.or(Some([0.0; 3]));
                        }
                        e.spatial_auto_bezier = Some(false);
                        e.spatial_continuous = Some(SPATIAL[d.spatial] == "Continuous Bezier");
                    }
                    "Auto Bezier" => e.spatial_auto_bezier = Some(true),
                    _ => {}
                }
                let ends = Some(k.time) == first || Some(k.time) == last;
                match ROVING[d.roving] {
                    "Rove Across Time" if !ends => e.roving = Some(true),
                    "Lock To Time" => e.roving = Some(false),
                    _ => {}
                }
            }
            edits.push(e);
        }
    }
    edits
}

/// Whether any of `keys` is on a spatial property (the Spatial and Roving menus
/// are enabled).
fn any_spatial(stage: &Stage, keys: &[KeyRef]) -> bool {
    by_property(stage, keys)
        .iter()
        .any(|(_, attr, spec, _)| is_spatial(attr, spec))
}

/// The line under a menu of the Keyframe Interpolation dialog, for its choice.
fn interpolation_note(choice: &str, roving: bool) -> &'static str {
    match (choice, roving) {
        ("Current Settings", false) => "Keeps the interpolation each selected keyframe has.",
        ("Current Settings", true) => "Keeps whether each selected keyframe roves.",
        ("Linear", _) => "Changes at a constant rate between keyframes.",
        ("Bezier", _) => "Curves you shape with separate incoming and outgoing handles.",
        ("Continuous Bezier", _) => "Smooth change through the keyframe; the handles stay joined.",
        ("Auto Bezier", _) => "Smooth change, recomputed when neighbouring keyframes change.",
        ("Hold", _) => "Keeps the value until the next keyframe.",
        ("Rove Across Time", _) => "Times the keyframe from its neighbours for an even speed.",
        ("Lock To Time", _) => "Keeps the keyframe at its current time.",
        _ => "",
    }
}

fn interpolation_body(ui: &mut Ui, stage: &Stage, d: &mut InterpolationDialog) {
    let spatial = any_spatial(stage, &d.keys);
    let row = |ui: &mut Ui, label: &str, sel: &mut usize, items: &[&str], enabled: bool| {
        ui.horizontal(|ui| {
            ui.add_sized(
                vec2(150.0, 20.0),
                egui::Label::new(egui::RichText::new(tr(label)).color(color::TEXT)),
            );
            let names: Vec<&str> = items.iter().map(|s| tr(s)).collect();
            ui.add_enabled_ui(enabled, |ui| {
                widgets::dropdown(ui, label, sel, &names, 180.0).labeled_value(
                    egui::WidgetType::ComboBox,
                    tr(label),
                    names[*sel],
                );
            });
        });
        let note = interpolation_note(items[*sel], items.len() == ROVING.len());
        ui.horizontal(|ui| {
            ui.add_space(154.0);
            ui.add_enabled(
                enabled,
                egui::Label::new(egui::RichText::new(tr(note)).color(color::TEXT_DIM).small()),
            );
        });
        ui.add_space(4.0);
    };
    row(
        ui,
        "Temporal Interpolation:",
        &mut d.temporal,
        &TEMPORAL,
        true,
    );
    row(
        ui,
        "Spatial Interpolation:",
        &mut d.spatial,
        &SPATIAL,
        spatial,
    );
    row(ui, "Roving:", &mut d.roving, &ROVING, spatial);
}

// ---------------------------------------------------------------------------
// Keyframe Velocity.

#[derive(Clone, Debug)]
pub struct VelocityDialog {
    pub keys: Vec<KeyRef>,
    /// Property the dialog names (the first key's).
    pub attr: String,
    /// Per dimension: incoming and outgoing ease (the first key's values).
    pub ease_in: Vec<Ease>,
    pub ease_out: Vec<Ease>,
    /// Continuous (Lock Outgoing to Incoming).
    pub continuous: bool,
    /// Lock Dimensions (several dimensions, not spatial): one edit sets them all.
    pub lock_dims: bool,
}

impl VelocityDialog {
    /// The dialog for `keys` (None when the first key is gone). A Bezier side shows
    /// its ease; a Linear or Hold side what AE shows for it: the segment's average
    /// speed and 16.667 % influence (R1 on AE 26).
    pub fn new(stage: &Stage, keys: Vec<KeyRef>) -> Option<Self> {
        let k = keys.first()?;
        let spec = query::attr_spec(stage, &k.path, &k.attr)?;
        let kf = spec.keyframes.get(&k.time)?;
        let n = query::ease_dimensions(&k.attr, spec, &kf.value);
        let spatial = is_spatial(&k.attr, spec);
        let a = crate::graph::comps(&kf.value).unwrap_or_default();
        let side = |bezier: bool, e: &[Ease], neighbour: Option<(&Time, &bae_model::Keyframe)>| {
            (0..n)
                .map(|i| {
                    if bezier && let Some(e) = e.get(i).or_else(|| e.first()) {
                        return *e;
                    }
                    let speed = neighbour.map_or(0.0, |(t, nk)| {
                        let dt = (*t - k.time).as_seconds();
                        let b = crate::graph::comps(&nk.value).unwrap_or_default();
                        let d = |j: usize| {
                            b.get(j).copied().unwrap_or(0.0) - a.get(j).copied().unwrap_or(0.0)
                        };
                        if dt.abs() < 1e-9 {
                            0.0
                        } else if spatial {
                            (0..a.len()).map(|j| d(j).powi(2)).sum::<f64>().sqrt() / dt.abs()
                        } else {
                            d(i) / dt
                        }
                    });
                    Ease {
                        speed,
                        influence: 100.0 / 6.0,
                    }
                })
                .collect::<Vec<_>>()
        };
        let next = spec.keyframes.range(k.time + Time(1)..).next();
        let prev = spec.keyframes.range(..k.time).next_back();
        Some(Self {
            attr: k.attr.clone(),
            ease_in: side(kf.interp_in == Interpolation::Bezier, &kf.ease_in, prev),
            ease_out: side(kf.interp_out == Interpolation::Bezier, &kf.ease_out, next),
            continuous: kf.flags.temporal_continuous,
            lock_dims: true,
            keys,
        })
    }
}

/// OK in Keyframe Velocity: every selected key of the same kind gets the entered
/// speeds and influences; both sides become Bezier and lose Auto Bezier.
pub(crate) fn velocity_edits(stage: &Stage, d: &VelocityDialog) -> Vec<KeyEdit> {
    let mut edits = Vec::new();
    for (_, attr, spec, keys) in by_property(stage, &d.keys) {
        for k in keys {
            let Some(kf) = spec.keyframes.get(&k.time) else {
                continue;
            };
            let n = query::ease_dimensions(attr, spec, &kf.value);
            let fit = |e: &[Ease]| -> Vec<Ease> {
                (0..n)
                    .map(|i| e.get(i).or_else(|| e.last()).copied().unwrap_or_default())
                    .collect()
            };
            let mut e = KeyEdit::new(k.clone());
            e.interp_in = Some(Interpolation::Bezier);
            e.interp_out = Some(Interpolation::Bezier);
            e.ease_in = Some(fit(&d.ease_in));
            e.ease_out = Some(fit(&d.ease_out));
            e.auto_bezier = Some(false);
            e.continuous = Some(d.continuous);
            edits.push(e);
        }
    }
    edits
}

/// AE's speed unit for a property.
fn speed_unit(attr: &str) -> &'static str {
    match attr {
        "transform:scale" | "transform:opacity" | "opacity" => "percent/sec",
        a if a.contains("rotation") || a.contains("orientation") || a.contains("skew") => {
            "degrees/sec"
        }
        a if a.contains("position") || a.contains("anchor") => "pixels/sec",
        _ => "units/sec",
    }
}

fn velocity_body(ui: &mut Ui, d: &mut VelocityDialog) {
    let unit = tr(speed_unit(&d.attr));
    let n = d.ease_in.len();
    if n > 1 {
        widgets::checkbox(ui, &mut d.lock_dims, tr("Lock Dimensions"));
    }
    let lock = n > 1 && d.lock_dims;
    // (incoming side, dimension, new speed / influence)
    let mut speed_edit: Option<(bool, usize, f64)> = None;
    let mut influence_edit: Option<(bool, f64)> = None;
    for (incoming, title) in [(true, "Incoming Velocity"), (false, "Outgoing Velocity")] {
        let eases = if incoming {
            &mut d.ease_in
        } else {
            &mut d.ease_out
        };
        ui.group(|ui| {
            ui.label(egui::RichText::new(tr(title)).color(color::TEXT_STRONG));
            for (i, e) in eases.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let label = if n > 1 {
                        format!(
                            "{} {}",
                            tr("Dimension:"),
                            ["X", "Y", "Z"].get(i).copied().unwrap_or("")
                        )
                    } else {
                        tr("Speed:").to_owned()
                    };
                    ui.add_sized(vec2(90.0, 18.0), egui::Label::new(label));
                    if ui
                        .add(
                            egui::DragValue::new(&mut e.speed)
                                .speed(0.1)
                                .max_decimals(2),
                        )
                        .labeled(
                            egui::WidgetType::DragValue,
                            &format!("{} {}", tr(title), tr("Speed:")),
                        )
                        .changed()
                    {
                        speed_edit = Some((incoming, i, e.speed));
                    }
                    ui.label(unit);
                });
                ui.horizontal(|ui| {
                    ui.add_sized(vec2(90.0, 18.0), egui::Label::new(tr("Influence:")));
                    if ui
                        .add(
                            egui::DragValue::new(&mut e.influence)
                                .range(0.1..=100.0)
                                .speed(0.1)
                                .max_decimals(2),
                        )
                        .labeled(
                            egui::WidgetType::DragValue,
                            &format!("{} {}", tr(title), tr("Influence:")),
                        )
                        .changed()
                    {
                        influence_edit = Some((incoming, e.influence));
                    }
                    ui.label("%");
                });
            }
        });
    }
    // Lock Dimensions: one dimension's edit sets them all; Continuous: a speed
    // edit sets the other side too.
    if let Some((incoming, i, v)) = speed_edit {
        let dims: Vec<usize> = if lock { (0..n).collect() } else { vec![i] };
        for side in [incoming, !incoming] {
            if side != incoming && !d.continuous {
                continue;
            }
            let eases = if side {
                &mut d.ease_in
            } else {
                &mut d.ease_out
            };
            for j in &dims {
                eases[*j].speed = v;
            }
        }
    }
    if lock && let Some((incoming, v)) = influence_edit {
        let eases = if incoming {
            &mut d.ease_in
        } else {
            &mut d.ease_out
        };
        eases.iter_mut().for_each(|e| e.influence = v);
    }
    widgets::checkbox(
        ui,
        &mut d.continuous,
        tr("Continuous (Lock Outgoing to Incoming)"),
    );
}

// ---------------------------------------------------------------------------
// Sequence Layers (Animation > Keyframe Assistant).

/// Transition menu, in AE's order, with the `layer.sequence` names.
pub(crate) const TRANSITIONS: [(&str, &str); 3] = [
    ("Off", "off"),
    ("Dissolve Front Layer", "dissolve_front"),
    ("Cross Dissolve Front and Back Layers", "cross_dissolve"),
];

#[derive(Clone, Debug)]
pub struct SequenceDialog {
    /// In playing order (the selection order).
    pub layers: Vec<PrimPath>,
    pub info: CompInfo,
    pub overlap: bool,
    /// The Duration field's text (timecode).
    pub duration: String,
    /// Index into [`TRANSITIONS`].
    pub transition: usize,
}

impl SequenceDialog {
    pub fn new(layers: Vec<PrimPath>, info: CompInfo) -> Self {
        let duration = common::duration_code(&info, Time::from_seconds(1.0));
        Self {
            layers,
            info,
            overlap: false,
            duration,
            transition: 0,
        }
    }

    fn command(&self) -> (&'static str, serde_json::Value) {
        let overlap = if self.overlap {
            super::markers::parse_timecode(&self.info, &self.duration)
                .map_or(Time::ZERO, |f| self.info.frame_rate.frame_to_time(f.max(0)))
        } else {
            Time::ZERO
        };
        let transition = if self.overlap {
            TRANSITIONS[self.transition].1
        } else {
            "off"
        };
        (
            "layer.sequence",
            json!({"layers": self.layers, "overlap": overlap, "transition": transition}),
        )
    }
}

fn sequence_body(ui: &mut Ui, d: &mut SequenceDialog) {
    widgets::checkbox(ui, &mut d.overlap, tr("Overlap"));
    ui.add_enabled_ui(d.overlap, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.add_sized(vec2(80.0, 18.0), egui::Label::new(tr("Duration:")));
            ui.add(egui::TextEdit::singleline(&mut d.duration).desired_width(90.0))
                .labeled(egui::WidgetType::TextEdit, tr("Duration:"));
        });
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.add_sized(vec2(80.0, 18.0), egui::Label::new(tr("Transition:")));
            let names: Vec<&str> = TRANSITIONS.iter().map(|(t, _)| tr(t)).collect();
            widgets::dropdown(ui, "sequence_transition", &mut d.transition, &names, 240.0)
                .labeled_value(
                    egui::WidgetType::ComboBox,
                    tr("Transition:"),
                    names[d.transition],
                );
        });
    });
}

// ---------------------------------------------------------------------------
// Host (temporary: a modal window over the Timeline).

impl KeyDialog {
    fn title(&self) -> &'static str {
        match self {
            KeyDialog::Interpolation(_) => "Keyframe Interpolation",
            KeyDialog::Velocity(_) => "Keyframe Velocity",
            KeyDialog::Marker(d) => d.title(),
            KeyDialog::Sequence(_) => "Sequence Layers",
        }
    }

    /// The command OK runs, if any.
    fn command(&self, stage: &Stage) -> Option<(&'static str, serde_json::Value)> {
        let edits = match self {
            KeyDialog::Interpolation(d) => interpolation_edits(stage, d),
            KeyDialog::Velocity(d) => velocity_edits(stage, d),
            KeyDialog::Marker(d) => return Some(d.command(stage)),
            KeyDialog::Sequence(d) => return Some(d.command()),
        };
        (!edits.is_empty()).then(|| ("key.edit", json!({ "edits": edits })))
    }
}

/// OK / Cancel in the platform's order (macOS: Cancel, OK; Windows: OK, Cancel);
/// Enter is OK and Esc is Cancel.
fn buttons(ui: &mut Ui) -> Option<Close> {
    let mut out = None;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let order: [(&str, Close); 2] = if cfg!(target_os = "macos") {
            [("OK", Close::Ok), ("Cancel", Close::Cancel)]
        } else {
            [("Cancel", Close::Cancel), ("OK", Close::Ok)]
        };
        for (label, close) in order {
            if ui
                .add_sized(vec2(72.0, 20.0), egui::Button::new(tr(label)))
                .clicked()
            {
                out = Some(close);
            }
        }
    });
    let (enter, esc) = ui.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
        )
    });
    if esc {
        out = Some(Close::Cancel);
    } else if enter && out.is_none() {
        out = Some(Close::Ok);
    }
    out
}

/// Draw the session's keyframe dialog, if any; OK runs its `key.edit`.
pub(super) fn show(ctx: &egui::Context, cx: &mut PanelContext<'_>, session: &SharedSession) {
    let Some(mut dialog) = session.lock().key_dialog.clone() else {
        return;
    };
    let stage = cx.stage;
    let mut close = None;
    egui::Modal::new(Id::new("bae-motion-key-dialog"))
        .frame(
            egui::Frame::new()
                .fill(color::PANEL)
                .stroke(Stroke::new(1.0, color::SEPARATOR))
                .inner_margin(12.0)
                .corner_radius(4.0),
        )
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.label(
                egui::RichText::new(tr(dialog.title()))
                    .color(color::TEXT_STRONG)
                    .strong(),
            );
            ui.add_space(8.0);
            match &mut dialog {
                KeyDialog::Interpolation(d) => interpolation_body(ui, stage, d),
                KeyDialog::Velocity(d) => velocity_body(ui, d),
                KeyDialog::Marker(d) => {
                    super::markers::marker_body(ui, d, &crate::prefs::current(cx.resources))
                }
                KeyDialog::Sequence(d) => sequence_body(ui, d),
            }
            ui.add_space(10.0);
            close = buttons(ui);
        });
    match close {
        Some(Close::Ok) => {
            if let Some((id, params)) = dialog.command(stage) {
                cx.invoke_with(id, params);
            }
            session.lock().key_dialog = None;
        }
        Some(Close::Cancel) => session.lock().key_dialog = None,
        None => session.lock().key_dialog = Some(dialog),
    }
}
