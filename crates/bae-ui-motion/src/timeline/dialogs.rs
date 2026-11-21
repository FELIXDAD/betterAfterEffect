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
}

impl VelocityDialog {
    /// The dialog for `keys` (None when the first key is gone).
    pub fn new(stage: &Stage, keys: Vec<KeyRef>) -> Option<Self> {
        let k = keys.first()?;
        let spec = query::attr_spec(stage, &k.path, &k.attr)?;
        let kf = spec.keyframes.get(&k.time)?;
        let n = query::ease_dimensions(&k.attr, spec, &kf.value);
        let fill = |e: &[Ease]| -> Vec<Ease> {
            (0..n)
                .map(|i| {
                    e.get(i).or_else(|| e.first()).copied().unwrap_or(Ease {
                        speed: 0.0,
                        influence: Ease::DEFAULT_INFLUENCE,
                    })
                })
                .collect()
        };
        Some(Self {
            attr: k.attr.clone(),
            ease_in: fill(&kf.ease_in),
            ease_out: fill(&kf.ease_out),
            continuous: kf.flags.temporal_continuous,
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
    let continuous = d.continuous;
    let mut changed_in: Option<(usize, f64)> = None;
    let mut changed_out: Option<(usize, f64)> = None;
    for (title, eases, changed) in [
        ("Incoming Velocity", &mut d.ease_in, &mut changed_in),
        ("Outgoing Velocity", &mut d.ease_out, &mut changed_out),
    ] {
        ui.group(|ui| {
            ui.label(egui::RichText::new(tr(title)).color(color::TEXT_STRONG));
            for (i, e) in eases.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let label = if n > 1 {
                        format!("{} {}:", tr("Dimension"), i + 1)
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
                        *changed = Some((i, e.speed));
                    }
                    ui.label(unit);
                });
                ui.horizontal(|ui| {
                    ui.add_sized(vec2(90.0, 18.0), egui::Label::new(tr("Influence:")));
                    ui.add(
                        egui::DragValue::new(&mut e.influence)
                            .range(0.1..=100.0)
                            .speed(0.1)
                            .max_decimals(2),
                    )
                    .labeled(
                        egui::WidgetType::DragValue,
                        &format!("{} {}", tr(title), tr("Influence:")),
                    );
                    ui.label("%");
                });
            }
        });
    }
    // Continuous: editing one speed copies it to the other side.
    if continuous {
        if let Some((i, v)) = changed_in {
            d.ease_out[i].speed = v;
        }
        if let Some((i, v)) = changed_out {
            d.ease_in[i].speed = v;
        }
    }
    widgets::checkbox(
        ui,
        &mut d.continuous,
        tr("Continuous (Lock Outgoing to Incoming)"),
    );
}

// ---------------------------------------------------------------------------
// Host (temporary: a modal window over the Timeline).

impl KeyDialog {
    fn title(&self) -> &'static str {
        match self {
            KeyDialog::Interpolation(_) => "Keyframe Interpolation",
            KeyDialog::Velocity(_) => "Keyframe Velocity",
        }
    }

    fn edits(&self, stage: &Stage) -> Vec<KeyEdit> {
        match self {
            KeyDialog::Interpolation(d) => interpolation_edits(stage, d),
            KeyDialog::Velocity(d) => velocity_edits(stage, d),
        }
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
            }
            ui.add_space(10.0);
            close = buttons(ui);
        });
    match close {
        Some(Close::Ok) => {
            let edits = dialog.edits(stage);
            if !edits.is_empty() {
                cx.invoke_with("key.edit", json!({ "edits": edits }));
            }
            session.lock().key_dialog = None;
        }
        Some(Close::Cancel) => session.lock().key_dialog = None,
        None => session.lock().key_dialog = Some(dialog),
    }
}
