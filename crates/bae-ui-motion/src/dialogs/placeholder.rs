//! File ▸ Import ▸ Placeholder…: AE's New Placeholder dialog
//! (docs/reference/ae/ui/dialogs/import-file.json, relatedDialogs). OK adds a
//! placeholder item of that size, frame rate and duration to the Project panel
//! (`item.new`).

use bae_model::Time;
use bae_ui::dialog::{self, Outcome};
use bae_ui::theme::color;
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui};
use serde_json::json;

use super::fields;

/// The open New Placeholder dialog.
#[derive(Clone, Debug)]
pub struct NewPlaceholder {
    name: String,
    width: f64,
    height: f64,
    fps: f64,
    /// Duration in frames at `fps`.
    frames: i64,
    name_focused: bool,
}

impl Default for NewPlaceholder {
    /// AE's defaults: "Missing Name", 1920 × 1080, 30 fps, 0:01:00:00.
    fn default() -> Self {
        NewPlaceholder {
            name: tr("Missing Name").to_owned(),
            width: 1920.0,
            height: 1080.0,
            fps: 30.0,
            frames: 60 * 30,
            name_focused: false,
        }
    }
}

impl NewPlaceholder {
    pub(crate) fn ui(&mut self, ctx: &egui::Context, cx: &mut PanelContext<'_>) -> bool {
        let id = Id::new("bae-ui-motion.new-placeholder");
        let shown = dialog::window(ctx, id, tr("New Placeholder"), |ui| self.body(ui, id));
        match shown {
            Some(Outcome::Ok) => {
                cx.invoke_with(
                    "item.new",
                    json!({
                        "kind": "placeholder",
                        "name": self.name.trim(),
                        "width": self.width as u32,
                        "height": self.height as u32,
                        "frame_rate": self.fps,
                        "duration": Time::from_seconds(self.frames as f64 / self.fps),
                    }),
                );
                false
            }
            Some(Outcome::Cancel) => false,
            Some(Outcome::None) => true,
            None => !dialog::closed_by_title_bar(ctx, id),
        }
    }

    fn body(&mut self, ui: &mut Ui, id: Id) -> Outcome {
        const LW: f32 = 90.0;
        ui.set_min_width(340.0);
        dialog::row(ui, 50.0, tr("Name:"), |ui| {
            fields::name_field(
                ui,
                id.with("name"),
                &mut self.name,
                240.0,
                tr("Name"),
                &mut self.name_focused,
            );
        });
        dialog::group(ui, tr("Size"), |ui| {
            dialog::row(ui, LW, tr("Width:"), |ui| {
                fields::number(ui, &mut self.width, 4.0..=30000.0, 0, tr("px"), tr("Width"));
            });
            dialog::row(ui, LW, tr("Height:"), |ui| {
                fields::number(
                    ui,
                    &mut self.height,
                    4.0..=30000.0,
                    0,
                    tr("px"),
                    tr("Height"),
                );
            });
        });
        dialog::group(ui, tr("Time"), |ui| {
            dialog::row(ui, LW, tr("Frame Rate:"), |ui| {
                let seconds = self.frames as f64 / self.fps;
                if fields::number(
                    ui,
                    &mut self.fps,
                    1.0..=999.0,
                    2,
                    tr("fps"),
                    tr("Frame Rate"),
                )
                .changed()
                {
                    // The duration keeps its length in seconds.
                    self.frames = (seconds * self.fps).round().max(1.0) as i64;
                }
            });
            dialog::row(ui, LW, tr("Duration:"), |ui| {
                let max = (3.0 * 3600.0 * self.fps) as i64;
                fields::timecode_field(
                    ui,
                    id.with("duration"),
                    &mut self.frames,
                    self.fps,
                    false,
                    (1, max),
                    tr("Duration"),
                );
                let note = tr("is {time}  Base {base}")
                    .replace(
                        "{time}",
                        &fields::format_timecode(self.frames, self.fps, false),
                    )
                    .replace("{base}", &fields::timecode_base(self.fps).to_string());
                ui.label(egui::RichText::new(note).color(color::TEXT_DIM));
            });
        });
        ui.add_space(6.0);
        dialog::buttons(ui, !self.name.trim().is_empty())
    }
}

/// English strings of the dialog (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "New Placeholder",
    "Missing Name",
    "Name:",
    "Name",
    "Size",
    "Width:",
    "Width",
    "Height:",
    "Height",
    "px",
    "Time",
    "Frame Rate:",
    "Frame Rate",
    "fps",
    "Duration:",
    "Duration",
];
