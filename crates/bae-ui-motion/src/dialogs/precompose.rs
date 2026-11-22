//! Pre-compose: AE's Layer > Pre-compose (Cmd/Ctrl+Shift+C) dialog. OK runs
//! one `layer.precompose` (one undo step); "Open New Composition" opens the
//! new comp once it exists, otherwise the pre-comp layer is selected.

use bae_model::{PrimPath, Stage};
use bae_motion::precompose::{can_leave_attributes, default_precomp_name};
use bae_motion::query;
use bae_ui::dialog::{self, Outcome};
use bae_ui::theme::color;
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui};
use serde_json::json;

use super::SharedDialogs;
use super::fields;
use crate::session::{Pending, SharedSession};

/// What Pre-compose remembers between uses.
#[derive(Clone, Debug, Default)]
pub struct PrecomposeMemory {
    pub open_new: bool,
    pub adjust_duration: bool,
}

/// The open Pre-compose dialog.
#[derive(Clone, Debug)]
pub struct PrecomposeDialog {
    layers: Vec<PrimPath>,
    comp: PrimPath,
    comp_name: String,
    /// The (first) layer's name, for the Leave option's text.
    layer_name: Option<String>,
    can_leave: bool,
    pub name: String,
    /// Leave all attributes (else move them into the new comp).
    pub leave: bool,
    pub adjust_duration: bool,
    pub open_new: bool,
    name_focused: bool,
}

impl PrecomposeDialog {
    /// Pre-compose the selected layers (unlocked, one comp).
    pub(crate) fn new(
        stage: &Stage,
        layers: Vec<PrimPath>,
        memory: &PrecomposeMemory,
    ) -> Option<Self> {
        let comp = query::owning_comp(stage, layers.first()?)?;
        let can_leave = matches!(layers.as_slice(), [one] if can_leave_attributes(stage, one));
        // The Leave option's text names the topmost selected layer (AE).
        let stack = query::comp_layers(stage, &comp);
        let layer_name = stack
            .iter()
            .find(|l| layers.contains(l))
            .map(|l| query::display_name_of(stage, l));
        Some(PrecomposeDialog {
            name: default_precomp_name(stage, &layers),
            comp_name: query::display_name_of(stage, &comp),
            comp,
            layer_name,
            can_leave,
            leave: can_leave,
            adjust_duration: memory.adjust_duration,
            open_new: memory.open_new,
            layers,
            name_focused: false,
        })
    }

    /// Draw the dialog; returns whether it stays open.
    pub(crate) fn ui(
        &mut self,
        ctx: &egui::Context,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        dialogs: &SharedDialogs,
    ) -> bool {
        let id = Id::new("bae-ui-motion.precompose");
        let shown = dialog::window(ctx, id, tr("Pre-compose"), |ui| self.body(ui, id));
        let outcome = match shown {
            Some(o) => o,
            None if dialog::closed_by_title_bar(ctx, id) => Outcome::Cancel,
            None => Outcome::None,
        };
        match outcome {
            Outcome::None => true,
            Outcome::Cancel => false,
            Outcome::Ok => {
                self.commit(cx, session, dialogs);
                false
            }
        }
    }

    fn commit(&self, cx: &mut PanelContext<'_>, session: &SharedSession, dialogs: &SharedDialogs) {
        let name = self.name.trim().to_owned();
        let leave = self.leave && self.can_leave;
        cx.invoke_with(
            "layer.precompose",
            json!({
                "layers": self.layers,
                "name": name,
                "mode": if leave { "leave" } else { "move" },
                "adjust_duration": !leave && self.adjust_duration,
                "time": session.lock().time(&self.comp),
            }),
        );
        // The paths the command will use: the new comp next to this one, and
        // (Move) the pre-comp layer in this comp.
        let stage = cx.stage;
        let comp_path = self.comp.parent().and_then(|folder| {
            folder
                .child(&query::unique_child_name(stage, &folder, &name))
                .ok()
        });
        let mut s = session.lock();
        if self.open_new
            && let Some(p) = &comp_path
        {
            s.expect(Pending::Comp(p.clone()));
        } else if !leave {
            let folder = query::layers_folder(&self.comp);
            if let Ok(p) = folder.child(&query::unique_child_name(stage, &folder, &name)) {
                s.expect(Pending::Layer(p));
            }
        }
        drop(s);
        let mut d = dialogs.lock();
        d.precompose.open_new = self.open_new;
        d.precompose.adjust_duration = self.adjust_duration;
    }

    fn body(&mut self, ui: &mut Ui, id: Id) -> Outcome {
        ui.set_width(440.0);
        ui.horizontal(|ui| {
            ui.label(tr("New composition name:"));
            fields::name_field(
                ui,
                id.with("name"),
                &mut self.name,
                220.0,
                tr("New composition name"),
                &mut self.name_focused,
            );
        });
        ui.add_space(6.0);
        let dim = |s: String| egui::RichText::new(s).color(color::TEXT_DIM);
        ui.add_enabled_ui(self.can_leave, |ui| {
            let label = tr("Leave all attributes in \"{comp}\"").replace("{comp}", &self.comp_name);
            if ui.radio(self.leave && self.can_leave, label).clicked() {
                self.leave = true;
            }
            let layer = self.layer_name.clone().unwrap_or_default();
            ui.indent(id.with("leave-help"), |ui| {
                ui.label(dim(tr(
                    "Use this option to create a new intermediate composition with only \"{layer}\" in it. The new composition will become the source to the current layer.",
                )
                .replace("{layer}", &layer)));
            });
        });
        ui.add_space(4.0);
        if ui
            .radio(
                !(self.leave && self.can_leave),
                tr("Move all attributes into the new composition"),
            )
            .clicked()
        {
            self.leave = false;
        }
        ui.indent(id.with("move-help"), |ui| {
            ui.label(dim(tr(
                "Use this option to place the currently selected layers together into a new intermediate composition.",
            )
            .to_owned()));
        });
        ui.add_space(6.0);
        let moving = !(self.leave && self.can_leave);
        ui.add_enabled_ui(moving, |ui| {
            bae_ui::widgets::checkbox(
                ui,
                &mut self.adjust_duration,
                tr("Adjust composition duration to the time span of the selected layers"),
            );
        });
        bae_ui::widgets::checkbox(ui, &mut self.open_new, tr("Open New Composition"));
        ui.add_space(4.0);
        dialog::buttons(ui, !self.name.trim().is_empty())
    }
}

/// English strings of Pre-compose (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "Pre-compose",
    "Pre-compose...",
    "New composition name:",
    "New composition name",
    "Leave all attributes in \"{comp}\"",
    "Use this option to create a new intermediate composition with only \"{layer}\" in it. The new composition will become the source to the current layer.",
    "Move all attributes into the new composition",
    "Use this option to place the currently selected layers together into a new intermediate composition.",
    "Adjust composition duration to the time span of the selected layers",
    "Open New Composition",
];
