//! Custom Resolution: the Composition viewer's Resolution popup ▸ Custom…
//! (AE). "Render every [x] pixels horizontally, [y] pixels vertically": OK
//! sets the comp's resolution like Composition Settings (`comp.set_settings`,
//! one undo step) and turns Auto off.

use bae_model::PrimPath;
use bae_ui::dialog::{self, Outcome};
use bae_ui::{PanelContext, tr};
use egui::{Id, Ui};
use serde_json::json;

use super::fields;
use crate::session::SharedSession;

/// The open Custom Resolution dialog.
#[derive(Clone, Debug)]
pub struct CustomResolution {
    comp: PrimPath,
    /// Pixel steps: horizontal, vertical.
    pub steps: [f64; 2],
}

impl CustomResolution {
    pub(crate) fn new(stage: &bae_model::Stage, session: &SharedSession) -> Option<Self> {
        let comp = session.lock().active_comp.clone()?;
        let steps = match stage
            .prim(&comp)
            .and_then(|v| bae_motion::query::static_value(&v, "resolution"))
        {
            Some(bae_model::Value::Vec2([h, v])) => [h.max(1.0), v.max(1.0)],
            _ => [1.0, 1.0],
        };
        Some(CustomResolution { comp, steps })
    }

    /// Draw the dialog; returns whether it stays open.
    pub(crate) fn ui(
        &mut self,
        ctx: &egui::Context,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
    ) -> bool {
        let id = Id::new("bae-ui-motion.custom-resolution");
        let shown = dialog::window(ctx, id, tr("Custom Resolution"), |ui| self.body(ui));
        let outcome = match shown {
            Some(o) => o,
            None if dialog::closed_by_title_bar(ctx, id) => Outcome::Cancel,
            None => Outcome::None,
        };
        match outcome {
            Outcome::None => true,
            Outcome::Cancel => false,
            Outcome::Ok => {
                session.lock().auto_resolution = false;
                let [h, v] = self.steps.map(|s| s.round().max(1.0) as u32);
                cx.invoke_with(
                    "comp.set_settings",
                    json!({"comp": self.comp, "resolution": [h, v]}),
                );
                false
            }
        }
    }

    fn body(&mut self, ui: &mut Ui) -> Outcome {
        ui.set_width(300.0);
        let [h, v] = &mut self.steps;
        ui.horizontal(|ui| {
            ui.label(tr("Render every"));
            fields::number(
                ui,
                h,
                1.0..=99.0,
                0,
                tr("pixels horizontally."),
                tr("pixels horizontally."),
            );
        });
        ui.horizontal(|ui| {
            // The second field sits under the first.
            let indent = ui
                .painter()
                .layout_no_wrap(
                    tr("Render every").to_owned(),
                    bae_ui::theme::body(),
                    egui::Color32::WHITE,
                )
                .size()
                .x;
            ui.add_space(indent + ui.spacing().item_spacing.x);
            fields::number(
                ui,
                v,
                1.0..=99.0,
                0,
                tr("pixels vertically."),
                tr("pixels vertically."),
            );
        });
        ui.add_space(6.0);
        dialog::buttons(ui, true)
    }
}

/// English strings of Custom Resolution (i18n completeness test).
#[cfg(test)]
pub(crate) const STRINGS: &[&str] = &[
    "Custom Resolution",
    "Render every",
    "pixels horizontally.",
    "pixels vertically.",
];
