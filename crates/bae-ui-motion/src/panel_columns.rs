//! Column selection shared by the Project and Render Queue panels.
use bae_ui::{tr, widgets};
use egui::Ui;

pub(crate) struct Column {
    pub title: &'static str,
    pub width: f32,
    pub visible: bool,
    pub fixed: bool,
}

impl Column {
    pub const fn new(title: &'static str, width: f32, visible: bool, fixed: bool) -> Self {
        Self {
            title,
            width,
            visible,
            fixed,
        }
    }
}

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
pub(crate) struct Columns(std::collections::BTreeSet<String>);

impl Columns {
    pub fn visible(&self, col: &Column) -> bool {
        col.fixed || col.visible != self.0.contains(col.title)
    }

    pub(crate) fn toggle(&mut self, col: &Column) {
        if !col.fixed && !self.0.remove(col.title) {
            self.0.insert(col.title.to_owned());
        }
    }

    pub fn menu(&mut self, ui: &mut Ui, columns: &[Column]) {
        widgets::menu_button(ui, tr("Columns"), |ui| {
            for col in columns {
                let mut shown = self.visible(col);
                if ui
                    .add_enabled(!col.fixed, egui::Checkbox::new(&mut shown, tr(col.title)))
                    .changed()
                {
                    self.toggle(col);
                    ui.close();
                }
            }
        });
    }

    pub fn header_menu(&mut self, ui: &mut Ui, columns: &[Column], col: &Column) {
        if ui
            .add_enabled(!col.fixed, egui::Button::new(tr("Hide This")))
            .clicked()
        {
            self.toggle(col);
            ui.close();
        }
        ui.add_enabled(false, egui::Button::new(tr("Rename This...")));
        ui.separator();
        self.menu(ui, columns);
    }
}
