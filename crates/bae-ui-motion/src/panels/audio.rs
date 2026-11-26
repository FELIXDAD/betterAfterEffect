//! Stereo output meters and the linked master fader.
use super::*;
use egui::Color32;

pub(super) struct AudioPanel {
    levels: [f32; 2],
}

impl Default for AudioPanel {
    fn default() -> Self {
        Self { levels: [-60.0; 2] }
    }
}

impl Panel for AudioPanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        TabInfo::new(tr("Audio")).min_size(vec2(144.0, 110.0))
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
        let decay = ui.input(|i| i.stable_dt) * 32.0;
        for (channel, level) in self.levels.iter_mut().enumerate() {
            *level = peaks.map_or(-60.0, |p| {
                (20.0 * p[channel].log10()).max(*level - decay).max(-60.0)
            });
        }
        let height = (ui.available_height() - 22.0).max(48.0);
        ui.horizontal_top(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(57.0, height), Sense::hover());
            let y = |db: f32| r.bottom() - ((db.clamp(-60.0, 12.0) + 60.0) / 72.0) * r.height();
            for (i, label) in ["Left output level", "Right output level"]
                .into_iter()
                .enumerate()
            {
                let bar =
                    Rect::from_min_size(r.min + vec2(13.0 * i as f32, 0.0), vec2(10.0, height));
                ui.painter().rect_filled(bar, 0.0, color::FIELD);
                for (low, high, color) in [
                    (-60.0, -6.0, Color32::from_rgb(75, 175, 86)),
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
                    .on_hover_text(format!("{}  {:.1} dBFS", tr(label), self.levels[i]));
            }
            for db in [12.0, 0.0, -12.0, -24.0, -36.0, -48.0, -60.0] {
                ui.painter().text(
                    pos2(
                        r.left() + 28.0,
                        y(db).clamp(r.top() + 6.0, r.bottom() - 6.0),
                    ),
                    Align2::LEFT_CENTER,
                    format!("{db:.0}"),
                    theme::small(),
                    color::TEXT_DIM,
                );
            }
            ui.vertical(|ui| {
                ui.spacing_mut().slider_width = height;
                if ui
                    .add(
                        egui::Slider::new(&mut master_db, -60.0..=12.0)
                            .vertical()
                            .step_by(0.1)
                            .suffix(" dB")
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
        if monitoring || self.levels.iter().any(|level| *level > -60.0) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(33));
        }
    }
}
