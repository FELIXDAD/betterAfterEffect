//! Interpret Footage: AE's File ▸ Interpret Footage ▸ Main (Cmd/Ctrl+Alt+G,
//! the Project panel's Interpret Footage button) for the selected footage.
//! Main Options: Alpha, Frame Rate, Start Timecode, Fields and Pulldown, Other
//! Options; Color. Alpha, frame rate, pixel aspect ratio and loop are editable
//! and go to `footage.interpret` (one undo step); AE's other settings are
//! shown at their defaults, greyed.

use bae_model::{PrimPath, Stage, Value};
use bae_ui::dialog::{self, Outcome};
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui};
use serde_json::json;

use super::fields::{self, PIXEL_ASPECTS};

const ALPHA: [(&str, &str); 3] = [
    ("ignore", "Ignore"),
    ("straight", "Straight - Unmatted"),
    ("premultiplied", "Premultiplied - Matted With Color:"),
];

/// The open Interpret Footage dialog.
#[derive(Clone, Debug)]
pub struct InterpretFootage {
    items: Vec<PrimPath>,
    name: String,
    tab: usize,
    alpha: String,
    matte: [f64; 4],
    /// The file's rate, and whether an assumed one is in use.
    file_fps: f64,
    assume: bool,
    fps: f64,
    pixel_aspect: f64,
    loops: f64,
}

impl InterpretFootage {
    /// The dialog for the selected footage items (`None` when none is footage).
    pub(crate) fn new(stage: &Stage, selected: &[PrimPath]) -> Option<Self> {
        let items: Vec<PrimPath> = selected
            .iter()
            .filter(|p| {
                stage.prim(p).is_some_and(|v| {
                    matches!(
                        v.value("kind").and_then(Value::as_str),
                        Some("footage" | "sequence") | None
                    ) && v.is_a(&bae_model::PrimType::ASSET)
                })
            })
            .cloned()
            .collect();
        let first = items.first()?;
        let view = stage.prim(first)?;
        let get = |attr: &str| bae_motion::query::static_value(&view, attr);
        let fps_of = |v: Option<Value>| match v {
            Some(Value::FrameRate(r)) => Some(r.as_f64()),
            Some(v) => v.as_f64(),
            None => None,
        };
        let fps = fps_of(get("frame_rate")).unwrap_or(30.0);
        let file_fps = fps_of(get(bae_motion::interpret::FILE_FRAME_RATE));
        Some(InterpretFootage {
            name: bae_motion::query::display_name_of(stage, first),
            items,
            tab: 0,
            alpha: match get("alpha_mode") {
                Some(Value::Enum(a) | Value::String(a)) => a,
                _ => "straight".into(),
            },
            matte: match get("premultiply_color") {
                Some(Value::Color(c)) => c,
                _ => [0.0, 0.0, 0.0, 1.0],
            },
            assume: file_fps.is_some_and(|f| (f - fps).abs() > 1e-6),
            file_fps: file_fps.unwrap_or(fps),
            fps,
            pixel_aspect: get("pixel_aspect").and_then(|v| v.as_f64()).unwrap_or(1.0),
            loops: get("loop").and_then(|v| v.as_f64()).unwrap_or(1.0),
        })
    }

    /// Draw the dialog; returns whether it stays open.
    pub(crate) fn ui(&mut self, ctx: &egui::Context, cx: &mut PanelContext<'_>) -> bool {
        let id = Id::new("bae-ui-motion.interpret-footage");
        let title = format!("{}: {}", tr("Interpret Footage"), self.name);
        let shown = dialog::window(ctx, id, &title, |ui| self.body(ui, id));
        let outcome = match shown {
            Some(o) => o,
            None if dialog::closed_by_title_bar(ctx, id) => Outcome::Cancel,
            None => Outcome::None,
        };
        match outcome {
            Outcome::None => true,
            Outcome::Cancel => false,
            Outcome::Ok => {
                let mut params = json!({
                    "items": self.items,
                    "alpha_mode": self.alpha,
                    "premultiply_color": self.matte,
                    "pixel_aspect": self.pixel_aspect,
                    "loop_count": self.loops.round().max(1.0) as i64,
                });
                if self.assume {
                    params["frame_rate"] = json!(self.fps);
                } else {
                    params["use_file_frame_rate"] = json!(true);
                }
                cx.invoke_with("footage.interpret", params);
                false
            }
        }
    }

    fn body(&mut self, ui: &mut Ui, id: Id) -> Outcome {
        ui.set_width(420.0);
        ui.horizontal(|ui| {
            for (i, t) in ["Main Options", "Color"].into_iter().enumerate() {
                if ui.selectable_label(self.tab == i, tr(t)).clicked() {
                    self.tab = i;
                }
            }
        });
        ui.separator();
        if self.tab == 0 {
            self.main_options(ui, id);
        } else {
            ui.add_enabled_ui(false, |ui| {
                let mut off = false;
                bae_ui::widgets::checkbox(
                    ui,
                    &mut off,
                    tr("Preserve RGB (Disable Color Management Conversions for This Item)"),
                );
                dialog::row(ui, 190.0, tr("Interpret As Linear Light:"), |ui| {
                    let mut i = 0;
                    bae_ui::widgets::dropdown(ui, "linear", &mut i, &[tr("On for 32 bpc")], 140.0);
                });
            });
        }
        ui.add_space(6.0);
        dialog::buttons(ui, true)
    }

    fn main_options(&mut self, ui: &mut Ui, id: Id) {
        let group = |ui: &mut Ui, title: &str| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tr(title)).strong());
        };
        group(ui, "Alpha");
        ui.indent(id.with("alpha"), |ui| {
            for (value, label) in ALPHA {
                ui.horizontal(|ui| {
                    if ui.radio(self.alpha == value, tr(label)).clicked() {
                        self.alpha = value.to_owned();
                    }
                    if value == "premultiplied" {
                        let [r, g, b, _] = self.matte.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
                        bae_ui::widgets::color_swatch(
                            ui,
                            egui::Color32::from_rgb(r, g, b),
                            egui::vec2(28.0, 14.0),
                        );
                    }
                });
            }
            ui.add_enabled_ui(false, |ui| {
                ui.horizontal(|ui| {
                    let mut invert = false;
                    bae_ui::widgets::checkbox(ui, &mut invert, tr("Invert Alpha"));
                    let _ = ui.button(tr("Guess"));
                });
            });
        });
        group(ui, "Frame Rate");
        ui.indent(id.with("rate"), |ui| {
            let file = tr("Use frame rate from file:").to_owned()
                + &format!(" ({:.3} fps)", self.file_fps);
            if ui.radio(!self.assume, file).clicked() {
                self.assume = false;
            }
            ui.horizontal(|ui| {
                if ui
                    .radio(self.assume, tr("Assume this frame rate:"))
                    .clicked()
                {
                    self.assume = true;
                }
                ui.add_enabled_ui(self.assume, |ui| {
                    fields::number(ui, &mut self.fps, 0.001..=999.0, 3, "", tr("Frame Rate"));
                });
            });
        });
        ui.add_enabled_ui(false, |ui| {
            group(ui, "Start Timecode");
            ui.indent(id.with("tc"), |ui| {
                let _ = ui.radio(true, tr("Use Source Timecode from File:"));
                let _ = ui.radio(false, tr("Override Start Timecode:"));
            });
            group(ui, "Fields and Pulldown");
            ui.indent(id.with("fields"), |ui| {
                dialog::row(ui, 120.0, tr("Separate Fields:"), |ui| {
                    let mut i = 0;
                    bae_ui::widgets::dropdown(ui, "fields", &mut i, &[tr("Off")], 120.0);
                });
                let mut edges = false;
                bae_ui::widgets::checkbox(ui, &mut edges, tr("Preserve Edges (Best Quality Only)"));
                dialog::row(ui, 120.0, tr("Remove Pulldown:"), |ui| {
                    let mut i = 0;
                    bae_ui::widgets::dropdown(ui, "pulldown", &mut i, &[tr("Off")], 120.0);
                });
            });
        });
        group(ui, "Other Options");
        ui.indent(id.with("other"), |ui| {
            dialog::row(ui, 130.0, tr("Pixel Aspect Ratio:"), |ui| {
                let names: Vec<&str> = PIXEL_ASPECTS.iter().map(|(n, _)| tr(n)).collect();
                let mut i = PIXEL_ASPECTS
                    .iter()
                    .position(|(_, a)| (a - self.pixel_aspect).abs() < 1e-3)
                    .unwrap_or(0);
                if bae_ui::widgets::dropdown(ui, "par", &mut i, &names, 220.0).changed() {
                    self.pixel_aspect = PIXEL_ASPECTS[i].1;
                }
            });
            dialog::row(ui, 130.0, tr("Loop:"), |ui| {
                fields::number(
                    ui,
                    &mut self.loops,
                    1.0..=9999.0,
                    0,
                    tr("Times"),
                    tr("Loop"),
                );
            });
        });
    }
}

/// English strings of Interpret Footage (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "Interpret Footage",
    "Main Options",
    "Color",
    "Alpha",
    "Ignore",
    "Straight - Unmatted",
    "Premultiplied - Matted With Color:",
    "Invert Alpha",
    "Guess",
    "Frame Rate",
    "Use frame rate from file:",
    "Assume this frame rate:",
    "Start Timecode",
    "Use Source Timecode from File:",
    "Override Start Timecode:",
    "Fields and Pulldown",
    "Separate Fields:",
    "Preserve Edges (Best Quality Only)",
    "Remove Pulldown:",
    "Other Options",
    "Pixel Aspect Ratio:",
    "Loop:",
    "Times",
    "Loop",
    "Preserve RGB (Disable Color Management Conversions for This Item)",
    "Interpret As Linear Light:",
    "On for 32 bpc",
];
