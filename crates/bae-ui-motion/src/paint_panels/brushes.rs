//! The Brushes panel gallery and its panel menu.
use super::*;
use egui::pos2;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Tip {
    name: Option<String>,
    diameter: f64,
    angle: f64,
    roundness: f64,
    hardness: f64,
    spacing: f64,
    dynamics: [u8; 5],
    minimum_size: f64,
}

impl Tip {
    fn capture(p: &PaintOptions) -> Self {
        Self {
            name: None,
            diameter: p.diameter,
            angle: p.angle,
            roundness: p.roundness,
            hardness: p.hardness,
            spacing: p.spacing,
            dynamics: p.dynamics,
            minimum_size: p.minimum_size,
        }
    }

    fn apply(&self, p: &mut PaintOptions) {
        p.diameter = self.diameter;
        p.angle = self.angle;
        p.roundness = self.roundness;
        p.hardness = self.hardness;
        p.spacing = self.spacing;
        p.dynamics = self.dynamics;
        p.minimum_size = self.minimum_size;
    }

    fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            let shape = match (self.hardness >= 100.0, self.roundness >= 100.0) {
                (true, true) => "Hard Round",
                (false, true) => "Soft Round",
                (true, false) => "Hard Elliptical",
                (false, false) => "Soft Elliptical",
            };
            format!("{} {} {}", tr(shape), self.diameter, tr("pixels"))
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Gallery {
    Text,
    SmallThumb,
    LargeThumb,
    SmallList,
    LargeList,
}

const MODES: [(Gallery, &str); 5] = [
    (Gallery::Text, "Text Only"),
    (Gallery::SmallThumb, "Small Thumbnail"),
    (Gallery::LargeThumb, "Large Thumbnail"),
    (Gallery::SmallList, "Small List"),
    (Gallery::LargeList, "Large List"),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct BrushLibrary {
    tips: Vec<Tip>,
    selected: Option<usize>,
    gallery: Gallery,
}

impl Default for BrushLibrary {
    fn default() -> Self {
        let tips = TIPS
            .iter()
            .map(|&(diameter, hard)| {
                Tip::capture(&PaintOptions {
                    diameter,
                    hardness: if hard { 100.0 } else { 0.0 },
                    ..Default::default()
                })
            })
            .collect();
        Self {
            tips,
            selected: None,
            gallery: Gallery::SmallThumb,
        }
    }
}

#[derive(Default)]
pub(crate) struct BrushesPanel {
    dialog: Option<Dialog>,
}

enum Dialog {
    New(Tip, String),
    Rename(usize, String),
    Delete(usize),
    Reset,
}

impl Dialog {
    fn show(&mut self, ui: &Ui, lib: &mut BrushLibrary) -> bool {
        let mut close = false;
        let modal = egui::Modal::new(ui.id().with("brush_tip_dialog")).show(ui.ctx(), |ui| {
            let (title, prompt) = match self {
                Self::New(..) => ("New Brush...", "New Brush Tip Name:"),
                Self::Rename(..) => ("Rename Brush...", "Rename Brush Tip:"),
                Self::Delete(_) => ("Delete Brush", "Delete the brush?"),
                Self::Reset => (
                    "Reset Brush Tips...",
                    "Replace current brush tips with the default brush tips?",
                ),
            };
            ui.heading(tr(title));
            ui.label(tr(prompt));
            let valid = match self {
                Self::New(_, name) | Self::Rename(_, name) => {
                    ui.text_edit_singleline(name);
                    !name.trim().is_empty()
                }
                _ => true,
            };
            ui.horizontal(|ui| {
                if ui.button(tr("Cancel")).clicked() {
                    close = true;
                }
                if matches!(self, Self::Reset) && ui.button(tr("Append")).clicked() {
                    lib.tips.extend(BrushLibrary::default().tips);
                    close = true;
                }
                if valid && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    || ui.add_enabled(valid, egui::Button::new(tr("OK"))).clicked()
                {
                    match self {
                        Self::New(tip, name) => {
                            tip.name = Some(name.trim().into());
                            lib.selected = Some(lib.tips.len());
                            lib.tips.push(tip.clone());
                        }
                        Self::Rename(i, name) => {
                            if let Some(tip) = lib.tips.get_mut(*i) {
                                tip.name = Some(name.trim().into());
                            }
                        }
                        Self::Delete(i) => {
                            if *i < lib.tips.len() {
                                lib.tips.remove(*i);
                                lib.selected = None;
                            }
                        }
                        Self::Reset => {
                            lib.tips = BrushLibrary::default().tips;
                            lib.selected = None;
                        }
                    }
                    close = true;
                }
            });
        });
        close || modal.should_close()
    }
}

impl Panel for BrushesPanel {
    fn tab(&self, _: &PanelContext<'_>) -> TabInfo {
        TabInfo::new(tr("Brushes"))
    }

    fn menu(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let tools = tools::shared(cx.resources);
        let mut state = tools.lock();
        let selected = state
            .brushes
            .selected
            .filter(|i| *i < state.brushes.tips.len());
        if ui.button(tr("New Brush...")).clicked() {
            let tip = Tip::capture(&state.options.paint);
            let name = tip.label();
            self.dialog = Some(Dialog::New(tip, name));
            ui.close();
        }
        if ui
            .add_enabled(selected.is_some(), egui::Button::new(tr("Rename Brush...")))
            .clicked()
            && let Some(i) = selected
        {
            self.dialog = Some(Dialog::Rename(i, state.brushes.tips[i].label()));
            ui.close();
        }
        if ui
            .add_enabled(selected.is_some(), egui::Button::new(tr("Delete Brush")))
            .clicked()
            && let Some(i) = selected
        {
            self.dialog = Some(Dialog::Delete(i));
            ui.close();
        }
        ui.separator();
        for (mode, label) in MODES {
            if ui
                .radio_value(&mut state.brushes.gallery, mode, tr(label))
                .clicked()
            {
                ui.close();
            }
        }
        ui.separator();
        if ui.button(tr("Reset Brush Tips...")).clicked() {
            self.dialog = Some(Dialog::Reset);
            ui.close();
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let tools = tools::shared(cx.resources);
        let mut state = tools.lock();
        let state = &mut *state;
        let lib = &mut state.brushes;
        if self
            .dialog
            .as_mut()
            .is_some_and(|dialog| dialog.show(ui, lib))
        {
            self.dialog = None;
        }
        let paint = &mut state.options.paint;
        let with_name = matches!(
            lib.gallery,
            Gallery::Text | Gallery::SmallList | Gallery::LargeList
        );
        let thumb = match lib.gallery {
            Gallery::Text => 0.0,
            Gallery::LargeList | Gallery::LargeThumb => 48.0,
            _ => 30.0,
        };
        egui::ScrollArea::vertical()
            .id_salt("brush_gallery")
            .max_height(180.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                    for (i, tip) in lib.tips.iter().enumerate() {
                        let width = if with_name {
                            ui.max_rect().width()
                        } else {
                            thumb
                        };
                        let (r, resp) = ui.allocate_exact_size(
                            vec2(width, f32::max(thumb, 18.0)),
                            Sense::click(),
                        );
                        let name = tip.label();
                        ui.painter().rect_filled(
                            r,
                            2.0,
                            if lib.selected == Some(i) {
                                color::ACCENT
                            } else {
                                color::FIELD
                            },
                        );
                        if thumb > 0.0 {
                            let center = pos2(r.left() + thumb * 0.5, r.center().y - 3.0);
                            let radius = (tip.diameter as f32 * 0.5).clamp(1.0, thumb * 0.33);
                            let rings = if tip.hardness >= 100.0 { 1 } else { 4 };
                            for ring in (1..=rings).rev() {
                                ui.painter().circle_filled(
                                    center,
                                    radius * ring as f32 / rings as f32,
                                    Color32::from_white_alpha(if rings == 1 {
                                        220
                                    } else {
                                        (48 * (5 - ring)) as u8
                                    }),
                                );
                            }
                        }
                        if with_name {
                            ui.painter().text(
                                pos2(r.left() + thumb + 3.0, r.center().y),
                                egui::Align2::LEFT_CENTER,
                                &name,
                                theme::small(),
                                color::TEXT,
                            );
                        } else {
                            ui.painter().text(
                                r.center_bottom(),
                                egui::Align2::CENTER_BOTTOM,
                                tip.diameter.to_string(),
                                egui::FontId::proportional(8.0),
                                color::TEXT_DIM,
                            );
                        }
                        if resp.on_hover_text(name).clicked() {
                            lib.selected = Some(i);
                            tip.apply(paint);
                        }
                    }
                });
            });
        ui.add_space(6.0);
        brush_controls(ui, paint);
    }
}
