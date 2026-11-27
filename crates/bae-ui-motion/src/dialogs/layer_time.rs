//! Two Layer-menu dialogs: Layer ▸ Time ▸ Time Stretch (`layer.time_stretch`)
//! and Layer ▸ Transform ▸ Auto-Orient (Cmd/Ctrl+Alt+O, `layer.auto_orient`),
//! laid out as AE 26's (docs/reference/ae/ui/dialogs/time-stretch.json,
//! auto-orient.json). Each OK is one undo step for all selected layers.

use bae_model::{PrimPath, Stage, Time, Value};
use bae_motion::query::{self, CompInfo};
use bae_ui::dialog::{self, Outcome};
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui};
use serde_json::json;

use super::fields;
use crate::session::SharedSession;

/// What Time Stretch holds in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hold {
    InPoint,
    CurrentFrame,
    OutPoint,
}

/// The open Time Stretch dialog.
#[derive(Clone, Debug)]
pub struct TimeStretch {
    layers: Vec<PrimPath>,
    comp: CompInfo,
    /// The layer's duration at 100 % (comp time).
    original: Time,
    pub stretch: f64,
    duration_text: String,
    hold: Hold,
    now: Time,
}

impl TimeStretch {
    pub(crate) fn new(stage: &Stage, session: &SharedSession) -> Option<Self> {
        let s = session.lock();
        let layers: Vec<PrimPath> = s
            .selected_layers
            .iter()
            .filter(|l| !query::is_locked(stage, l))
            .cloned()
            .collect();
        let first = layers.first()?;
        let comp = query::comp_info(stage, &query::owning_comp(stage, first)?)?;
        let t = query::layer_timing(stage, first);
        let stretch = if t.stretch.abs() < 1.0 {
            100.0
        } else {
            t.stretch
        };
        let duration = t.out_point - t.in_point;
        let original = Time((duration.0 as f64 * 100.0 / stretch.abs()).round() as i64);
        let now = s.time(&comp.path);
        drop(s);
        Some(TimeStretch {
            duration_text: code(&comp, duration),
            layers,
            comp,
            original,
            stretch,
            hold: Hold::InPoint,
            now,
        })
    }

    /// The new duration the stretch gives.
    fn duration(&self) -> Time {
        Time((self.original.0 as f64 * self.stretch.abs() / 100.0).round() as i64)
    }

    pub(crate) fn ui(&mut self, ctx: &egui::Context, cx: &mut PanelContext<'_>) -> bool {
        let id = Id::new("bae-ui-motion.time-stretch");
        let shown = dialog::window(ctx, id, tr("Time Stretch"), |ui| self.body(ui, id));
        match shown {
            Some(Outcome::Ok) => {
                let hold = match self.hold {
                    Hold::InPoint => "in_point",
                    Hold::CurrentFrame => "current_frame",
                    Hold::OutPoint => "out_point",
                };
                cx.invoke_with(
                    "layer.time_stretch",
                    json!({"layers": self.layers, "stretch": self.stretch, "hold": hold, "time": self.now}),
                );
                false
            }
            Some(Outcome::Cancel) => false,
            Some(Outcome::None) => true,
            None => !dialog::closed_by_title_bar(ctx, id),
        }
    }

    fn body(&mut self, ui: &mut Ui, id: Id) -> Outcome {
        ui.set_width(360.0);
        ui.label(egui::RichText::new(tr("Stretch")).strong());
        ui.indent(id.with("stretch"), |ui| {
            dialog::row(ui, 120.0, tr("Original Duration:"), |ui| {
                ui.label(code(&self.comp, self.original));
            });
            dialog::row(ui, 120.0, tr("Stretch Factor:"), |ui| {
                if fields::number(
                    ui,
                    &mut self.stretch,
                    -9900.0..=9900.0,
                    4,
                    "%",
                    tr("Stretch Factor"),
                )
                .changed()
                {
                    // AE: (−1, 1) becomes ±1 %.
                    if self.stretch.abs() < 1.0 {
                        self.stretch = if self.stretch < 0.0 { -1.0 } else { 1.0 };
                    }
                    self.duration_text = code(&self.comp, self.duration());
                }
            });
            dialog::row(ui, 120.0, tr("New Duration:"), |ui| {
                let r =
                    ui.add(egui::TextEdit::singleline(&mut self.duration_text).desired_width(90.0));
                if r.lost_focus()
                    && let Some(f) = fields::parse_timecode(
                        &self.duration_text,
                        self.comp.frame_rate.as_f64(),
                        false,
                    )
                    && f > 0
                    && self.original > Time::ZERO
                {
                    let d = self.comp.frame_rate.frame_to_time(f);
                    let sign = if self.stretch < 0.0 { -1.0 } else { 1.0 };
                    self.stretch = sign * d.0 as f64 / self.original.0 as f64 * 100.0;
                }
                ui.label(
                    tr("is {time}  Base {base}")
                        .replace("{time}", &code(&self.comp, self.duration()))
                        .replace(
                            "{base}",
                            &fields::timecode_base(self.comp.frame_rate.as_f64()).to_string(),
                        ),
                );
            });
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new(tr("Hold in Place")).strong());
        ui.indent(id.with("hold"), |ui| {
            for (hold, label) in [
                (Hold::InPoint, "Layer In-point"),
                (Hold::CurrentFrame, "Current Frame"),
                (Hold::OutPoint, "Layer Out-point"),
            ] {
                if ui.radio(self.hold == hold, tr(label)).clicked() {
                    self.hold = hold;
                }
            }
        });
        ui.add_space(6.0);
        dialog::buttons(ui, self.stretch != 0.0)
    }
}

/// `d` as timecode from 0 in `comp`'s rate.
fn code(comp: &CompInfo, d: Time) -> String {
    bae_ui::widgets::timecode(comp.frame_rate.time_to_frame(d), comp.frame_rate.as_f64())
}

/// The open Auto-Orientation dialog.
#[derive(Clone, Debug)]
pub struct AutoOrient {
    layers: Vec<PrimPath>,
    /// Cameras and lights orient towards their point of interest.
    camera: bool,
    three_d: bool,
    pub mode: String,
}

impl AutoOrient {
    pub(crate) fn new(stage: &Stage, session: &SharedSession) -> Option<Self> {
        let layers: Vec<PrimPath> = session
            .lock()
            .selected_layers
            .iter()
            .filter(|l| !query::is_locked(stage, l))
            .cloned()
            .collect();
        let first = layers.first()?;
        let view = stage.prim(first)?;
        let kind = query::layer_kind(stage, first).unwrap_or_default();
        let mode = match query::static_value(&view, "auto_orient") {
            Some(Value::Enum(m) | Value::String(m)) => m,
            _ => "off".into(),
        };
        Some(AutoOrient {
            camera: matches!(kind.as_str(), "camera" | "light"),
            three_d: crate::common::is_3d(stage, first),
            layers,
            mode,
        })
    }

    pub(crate) fn ui(&mut self, ctx: &egui::Context, cx: &mut PanelContext<'_>) -> bool {
        let id = Id::new("bae-ui-motion.auto-orient");
        let shown = dialog::window(ctx, id, tr("Auto-Orientation"), |ui| self.body(ui));
        match shown {
            Some(Outcome::Ok) => {
                cx.invoke_with(
                    "layer.auto_orient",
                    json!({"layers": self.layers, "mode": self.mode}),
                );
                false
            }
            Some(Outcome::Cancel) => false,
            Some(Outcome::None) => true,
            None => !dialog::closed_by_title_bar(ctx, id),
        }
    }

    fn body(&mut self, ui: &mut Ui) -> Outcome {
        ui.set_width(300.0);
        let mut radio = |ui: &mut Ui, value: &str, label: &str| {
            if ui.radio(self.mode == value, tr(label)).clicked() {
                self.mode = value.to_owned();
            }
        };
        radio(ui, "off", "Off");
        radio(ui, "along_path", "Orient Along Path");
        if self.camera {
            radio(
                ui,
                "towards_point_of_interest",
                "Orient Towards Point of Interest",
            );
        } else if self.three_d {
            // AE's 3D layers can face the camera (not in BAE yet).
            ui.add_enabled_ui(false, |ui| {
                let _ = ui.radio(false, tr("Orient Towards Camera"));
                let mut each = false;
                bae_ui::widgets::checkbox(
                    ui,
                    &mut each,
                    tr("Orient Each Character Independently (requires Per-character 3D)"),
                );
            });
        }
        ui.add_space(6.0);
        dialog::buttons(ui, true)
    }
}

/// English strings of these dialogs (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "Time Stretch",
    "Stretch",
    "Original Duration:",
    "Stretch Factor:",
    "Stretch Factor",
    "New Duration:",
    "is {time}  Base {base}",
    "Hold in Place",
    "Layer In-point",
    "Current Frame",
    "Layer Out-point",
    "Auto-Orientation",
    "Off",
    "Orient Along Path",
    "Orient Towards Point of Interest",
    "Orient Towards Camera",
    "Orient Each Character Independently (requires Per-character 3D)",
];
