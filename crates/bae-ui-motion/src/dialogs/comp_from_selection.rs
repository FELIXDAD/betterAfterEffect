//! File ▸ New Comp from Selection… (Alt+\): AE's New Composition from
//! Selection dialog for the items selected in the Project panel. OK runs
//! `comp.create_from_items` (one undo step), opens the new comp and, with Add
//! to Render Queue, queues it.

use bae_model::{PrimPath, Stage, Time};
use bae_motion::query;
use bae_ui::dialog::{self, Outcome};
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui};
use serde_json::json;

use super::comp_settings::menu_field;
use super::fields;
use crate::session::{Pending, SharedSession};

/// Sequence Layers ▸ Transition, in AE's menu order.
const TRANSITIONS: [(&str, &str); 3] = [
    ("Dissolve Front Layer", "dissolve_front"),
    ("Cross Dissolve Front and Back Layers", "cross_dissolve"),
    ("Off", "off"),
];

/// The open New Composition from Selection dialog.
#[derive(Clone, Debug)]
pub struct CompFromSelection {
    items: Vec<PrimPath>,
    names: Vec<String>,
    multiple: bool,
    /// Use Dimensions From: index into `items`.
    dims: usize,
    /// Frame rate the durations are typed in.
    fps: f64,
    still: i64,
    queue: bool,
    sequence: bool,
    overlap: bool,
    overlap_frames: i64,
    transition: usize,
}

impl CompFromSelection {
    pub(crate) fn new(stage: &Stage, items: Vec<PrimPath>, still: Option<Time>) -> Option<Self> {
        if items.is_empty() {
            return None;
        }
        let names = items
            .iter()
            .map(|i| query::display_name_of(stage, i))
            .collect();
        let fps = 30.0;
        let frames = |t: Time| (t.as_seconds() * fps).round() as i64;
        Some(CompFromSelection {
            items,
            names,
            multiple: false,
            dims: 0,
            fps,
            still: frames(still.unwrap_or(Time::from_seconds(5.0))).max(1),
            queue: false,
            sequence: false,
            overlap: false,
            overlap_frames: frames(Time::from_seconds(1.0)),
            transition: 0,
        })
    }

    pub(crate) fn ui(
        &mut self,
        ctx: &egui::Context,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
    ) -> bool {
        let id = Id::new("bae-ui-motion.comp-from-selection");
        let shown = dialog::window(ctx, id, tr("New Composition from Selection"), |ui| {
            self.body(ui, id)
        });
        match shown {
            Some(Outcome::Ok) => {
                self.commit(cx, session);
                false
            }
            Some(Outcome::Cancel) => false,
            Some(Outcome::None) => true,
            None => !dialog::closed_by_title_bar(ctx, id),
        }
    }

    fn time(&self, frames: i64) -> Time {
        Time::from_seconds(frames as f64 / self.fps)
    }

    fn commit(&self, cx: &mut PanelContext<'_>, session: &SharedSession) {
        let single = !self.multiple;
        let prefs = crate::prefs::current(cx.resources);
        let mut params = json!({
            "items": self.items,
            "multiple": self.multiple,
            "still_duration": self.time(self.still),
            "label": crate::prefs::comp_label(&prefs),
        });
        if single {
            params["dimensions_from"] = json!(self.items[self.dims]);
            if self.sequence {
                params["sequence"] = json!(true);
                if self.overlap {
                    params["overlap"] = json!(self.time(self.overlap_frames));
                    params["transition"] = json!(TRANSITIONS[self.transition].1);
                }
            }
        }
        session.lock().expect(Pending::NewComps {
            before: query::comps(cx.stage),
            queue: self.queue,
        });
        cx.invoke_with("comp.create_from_items", params);
    }

    fn body(&mut self, ui: &mut Ui, id: Id) -> Outcome {
        const LW: f32 = 140.0;
        ui.set_min_width(380.0);
        dialog::group(ui, tr("Create"), |ui| {
            ui.radio_value(&mut self.multiple, false, tr("Single Composition"));
            ui.radio_value(&mut self.multiple, true, tr("Multiple Compositions"));
        });
        dialog::group(ui, tr("Options"), |ui| {
            ui.add_enabled_ui(!self.multiple, |ui| {
                dialog::row(ui, LW, tr("Use Dimensions From:"), |ui| {
                    let items: Vec<Option<String>> =
                        self.names.iter().map(|n| Some(n.clone())).collect();
                    if let Some(i) = menu_field(
                        ui,
                        id.with("dims"),
                        &self.names[self.dims],
                        &items,
                        200.0,
                        tr("Use Dimensions From"),
                    ) {
                        self.dims = i;
                    }
                });
            });
            dialog::row(ui, LW, tr("Still Duration:"), |ui| {
                let max = (3.0 * 3600.0 * self.fps) as i64;
                fields::timecode_field(
                    ui,
                    id.with("still"),
                    &mut self.still,
                    self.fps,
                    false,
                    (1, max),
                    tr("Still Duration"),
                );
            });
            bae_ui::widgets::checkbox(ui, &mut self.queue, tr("Add to Render Queue"));
        });
        ui.add_enabled_ui(!self.multiple, |ui| {
            bae_ui::widgets::checkbox(ui, &mut self.sequence, tr("Sequence Layers"));
            ui.add_enabled_ui(self.sequence, |ui| {
                ui.indent(id.with("seq"), |ui| {
                    bae_ui::widgets::checkbox(ui, &mut self.overlap, tr("Overlap"));
                    ui.add_enabled_ui(self.overlap, |ui| {
                        dialog::row(ui, LW - 20.0, tr("Duration:"), |ui| {
                            let max = (3600.0 * self.fps) as i64;
                            fields::timecode_field(
                                ui,
                                id.with("overlap"),
                                &mut self.overlap_frames,
                                self.fps,
                                false,
                                (1, max),
                                tr("Duration"),
                            );
                        });
                        dialog::row(ui, LW - 20.0, tr("Transition:"), |ui| {
                            let items: Vec<Option<String>> = TRANSITIONS
                                .iter()
                                .map(|(n, _)| Some(tr(n).to_owned()))
                                .collect();
                            if let Some(i) = menu_field(
                                ui,
                                id.with("transition"),
                                tr(TRANSITIONS[self.transition].0),
                                &items,
                                240.0,
                                tr("Transition"),
                            ) {
                                self.transition = i;
                            }
                        });
                    });
                });
            });
        });
        ui.add_space(6.0);
        dialog::buttons(ui, true)
    }
}

/// English strings of the dialog (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "New Composition from Selection",
    "Create",
    "Single Composition",
    "Multiple Compositions",
    "Options",
    "Use Dimensions From:",
    "Use Dimensions From",
    "Still Duration:",
    "Still Duration",
    "Add to Render Queue",
    "Sequence Layers",
    "Overlap",
    "Duration:",
    "Duration",
    "Transition:",
    "Transition",
    "Dissolve Front Layer",
    "Cross Dissolve Front and Back Layers",
    "Off",
];
