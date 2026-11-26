//! Display options for a single Layer or Footage viewer.
use bae_model::{PrimPath, Stage};
use bae_ui::{
    color_picker::{self, ColorEdit},
    tr, widgets,
};
use egui::{Color32, Ui};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Background {
    Black,
    DarkGray,
    #[default]
    MediumGray,
    LightGray,
    White,
    Custom,
}

const BACKGROUNDS: [(Background, &str); 6] = [
    (Background::Black, "Black"),
    (Background::DarkGray, "Dark Gray"),
    (Background::MediumGray, "Medium Gray (Default)"),
    (Background::LightGray, "Light Gray"),
    (Background::White, "White"),
    (Background::Custom, "Custom"),
];

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SourceView {
    pub grid: bool,
    correct_aspect: bool,
    background: Background,
    custom: [u8; 3],
    #[serde(skip)]
    picking: bool,
    #[serde(skip)]
    preview: Option<[u8; 3]>,
}

impl Default for SourceView {
    fn default() -> Self {
        Self {
            grid: false,
            correct_aspect: true,
            background: Background::MediumGray,
            custom: [35; 3],
            picking: false,
            preview: None,
        }
    }
}

impl SourceView {
    pub fn menu(&mut self, ui: &mut Ui) {
        if ui
            .checkbox(&mut self.grid, tr("Transparency Grid"))
            .changed()
        {
            ui.close();
        }
        widgets::menu_button(ui, tr("Panel Background Color"), |ui| {
            for (color, title) in BACKGROUNDS {
                if ui
                    .radio_value(&mut self.background, color, tr(title))
                    .clicked()
                {
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(tr("Select Custom Background Color...")).clicked() {
                self.picking = true;
                ui.close();
            }
        });
        if ui
            .checkbox(
                &mut self.correct_aspect,
                tr("Pixel Aspect Ratio Correction"),
            )
            .changed()
        {
            ui.close();
        }
    }

    pub fn background(&mut self, ui: &Ui) -> Color32 {
        if self.picking {
            let [r, g, b] = self.custom.map(|c| f64::from(c) / 255.0);
            let edit = color_picker::picker(
                ui.ctx(),
                ui.id().with("source_view_color"),
                tr("Select Custom Background Color..."),
                [r, g, b, 1.0],
            );
            let rgb =
                |v: [f64; 4]| [v[0], v[1], v[2]].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
            match edit {
                ColorEdit::None => {}
                ColorEdit::Live(v) => self.preview = Some(rgb(v)),
                ColorEdit::Commit(v) => {
                    self.custom = rgb(v);
                    self.background = Background::Custom;
                    self.preview = None;
                    self.picking = false;
                }
                ColorEdit::Cancel => {
                    self.preview = None;
                    self.picking = false;
                }
            }
        }
        let [r, g, b] = self.preview.unwrap_or(match self.background {
            Background::Black => [0; 3],
            Background::DarkGray => [20; 3],
            Background::MediumGray => [35; 3],
            Background::LightGray => [80; 3],
            Background::White => [255; 3],
            Background::Custom => self.custom,
        });
        Color32::from_rgb(r, g, b)
    }

    pub fn aspect(&self, stage: &Stage, source: Option<&PrimPath>) -> f32 {
        if !self.correct_aspect {
            return 1.0;
        }
        source
            .and_then(|p| stage.prim(p))
            .and_then(|v| v.value("pixel_aspect").and_then(|v| v.as_f64()))
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(1.0) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_menu_defaults_and_chinese() {
        let view = SourceView::default();
        assert!(!view.grid && view.correct_aspect);
        assert_eq!(view.background, Background::MediumGray);
        assert!(crate::viewer::ZH.contains(&("Transparency Grid", "透明网格")));
        assert!(crate::viewer::ZH.contains(&("Pixel Aspect Ratio Correction", "像素长宽比校正")));
        assert_eq!(BACKGROUNDS.len(), 6);
    }
}
