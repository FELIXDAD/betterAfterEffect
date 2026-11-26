//! Stereo output meters and the linked master fader.
use super::*;
use egui::Color32;

use bae_ui::dialog::{self, Outcome};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

const MINIMUMS: [i8; 8] = [-12, -24, -36, -48, -60, -72, -84, -96];

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Options {
    percentage: bool,
    minimum_db: i8,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            percentage: false,
            minimum_db: -60,
        }
    }
}

impl Options {
    fn display(self, db: f64) -> String {
        if self.percentage {
            format!("{:.1}%", 100.0 * 10.0_f64.powf(db / 20.0))
        } else {
            format!("{db:.1} dB")
        }
    }
    fn parse(self, value: &str) -> Option<f64> {
        let value = value
            .trim()
            .trim_end_matches('%')
            .trim_end_matches("dB")
            .trim()
            .parse::<f64>()
            .ok()?;
        if self.percentage {
            (value >= 0.0).then(|| (20.0 * (value / 100.0).log10()).max(f64::from(self.minimum_db)))
        } else {
            Some(value)
        }
    }
}

#[derive(Clone, Debug)]
pub struct AudioOptionsDialog {
    current: Arc<Mutex<Options>>,
    draft: Options,
}

impl AudioOptionsDialog {
    pub(crate) fn ui(&mut self, ctx: &egui::Context) -> bool {
        let outcome = dialog::window(
            ctx,
            egui::Id::new("audio-options"),
            tr("Audio Options"),
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Units"));
                    egui::ComboBox::from_id_salt("units")
                        .selected_text(if self.draft.percentage {
                            tr("Percentage")
                        } else {
                            tr("Decibels (dB)")
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.draft.percentage,
                                false,
                                tr("Decibels (dB)"),
                            );
                            ui.selectable_value(&mut self.draft.percentage, true, tr("Percentage"));
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Slider Minimum"));
                    egui::ComboBox::from_id_salt("minimum")
                        .selected_text(format!("{} dB", self.draft.minimum_db))
                        .show_ui(ui, |ui| {
                            for db in MINIMUMS {
                                ui.selectable_value(
                                    &mut self.draft.minimum_db,
                                    db,
                                    format!("{db} dB"),
                                );
                            }
                        });
                });
                dialog::buttons(ui, true)
            },
        );
        if outcome == Some(Outcome::Ok) {
            *self.current.lock().unwrap_or_else(|e| e.into_inner()) = self.draft;
        }
        !matches!(outcome, Some(Outcome::Ok | Outcome::Cancel))
    }
}

pub(super) struct AudioPanel {
    levels: [f32; 2],
    options: Arc<Mutex<Options>>,
}

impl AudioPanel {
    pub(super) fn from_state(saved: Option<&serde_json::Value>) -> Self {
        let options: Options = saved
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .filter(|o: &Options| MINIMUMS.contains(&o.minimum_db))
            .unwrap_or_default();
        Self {
            levels: [f32::from(options.minimum_db); 2],
            options: Arc::new(Mutex::new(options)),
        }
    }

    fn options(&self) -> Options {
        *self.options.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Panel for AudioPanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        TabInfo::new(tr("Audio")).min_size(vec2(144.0, 110.0))
    }

    fn menu(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        if ui.button(tr("Options...")).clicked() {
            crate::dialogs::audio_options(
                cx,
                AudioOptionsDialog {
                    current: self.options.clone(),
                    draft: self.options(),
                },
            );
            ui.close();
        }
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        serde_json::to_value(self.options()).ok()
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        let (peaks, monitoring, mut master_db) = {
            let s = session.lock();
            (
                s.output.as_ref().map(|o| o.peaks()),
                s.monitoring_audio(),
                s.master_db,
            )
        };
        let options = self.options();
        let minimum = f32::from(options.minimum_db);
        let decay = ui.input(|i| i.stable_dt) * 32.0;
        for (channel, level) in self.levels.iter_mut().enumerate() {
            *level = peaks.map_or(minimum, |p| {
                (20.0 * p[channel].log10()).max(*level - decay).max(minimum)
            });
        }
        let height = (ui.available_height() - 22.0).max(48.0);
        ui.horizontal_top(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(57.0, height), Sense::hover());
            let y = |db: f32| {
                r.bottom() - ((db.clamp(minimum, 12.0) - minimum) / (12.0 - minimum)) * r.height()
            };
            for (i, label) in ["Left output level", "Right output level"]
                .into_iter()
                .enumerate()
            {
                let bar =
                    Rect::from_min_size(r.min + vec2(13.0 * i as f32, 0.0), vec2(10.0, height));
                ui.painter().rect_filled(bar, 0.0, color::FIELD);
                for (low, high, color) in [
                    (minimum, -6.0, Color32::from_rgb(75, 175, 86)),
                    (-6.0, 0.0, Color32::from_rgb(226, 187, 68)),
                    (0.0, 12.0, Color32::LIGHT_RED),
                ] {
                    let level = self.levels[i].min(high);
                    if level > low {
                        ui.painter().rect_filled(
                            Rect::from_min_max(
                                pos2(bar.left(), y(level)),
                                pos2(bar.right(), y(low)),
                            ),
                            0.0,
                            color,
                        );
                    }
                }
                ui.interact(bar, ui.id().with(i), Sense::hover())
                    .on_hover_text(format!(
                        "{}  {}",
                        tr(label),
                        options.display(self.levels[i].into())
                    ));
            }
            for db in
                std::iter::once(12).chain((i32::from(options.minimum_db)..=0).rev().step_by(12))
            {
                let db = db as f32;
                ui.painter().text(
                    pos2(
                        r.left() + 28.0,
                        y(db).clamp(r.top() + 6.0, r.bottom() - 6.0),
                    ),
                    Align2::LEFT_CENTER,
                    if options.percentage {
                        format!("{:.0}%", 100.0 * 10.0_f32.powf(db / 20.0))
                    } else {
                        format!("{db:.0}")
                    },
                    theme::small(),
                    color::TEXT_DIM,
                );
            }
            ui.vertical(|ui| {
                ui.spacing_mut().slider_width = height;
                if ui
                    .add(
                        egui::Slider::new(&mut master_db, minimum..=12.0)
                            .vertical()
                            .step_by(0.1)
                            .clamping(egui::SliderClamping::Edits)
                            .custom_formatter(move |db, _| options.display(db))
                            .custom_parser(move |text| options.parse(text))
                            .text(tr("Master Volume")),
                    )
                    .changed()
                {
                    let mut s = session.lock();
                    s.master_db = master_db;
                    if let Some(output) = &s.output {
                        output.set_gain(10.0_f32.powf(master_db / 20.0));
                    }
                }
            });
        });
        if monitoring || self.levels.iter().any(|level| *level > minimum) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changing_units_preserves_gain_and_clamps_silence_to_the_selected_floor() {
        let options = Options {
            percentage: true,
            minimum_db: -84,
        };
        assert_eq!(options.display(0.0), "100.0%");
        assert_eq!(options.parse("100%"), Some(0.0));
        assert_eq!(options.parse("0%"), Some(-84.0));
        assert_eq!(Options::default().parse("-12 dB"), Some(-12.0));
        assert!(MINIMUMS.contains(&Options::default().minimum_db));
    }
}
