//! The Effects & Presets panel: a search field and AE's effect categories as
//! a tree (sorted by their localized names like AE). Double-click an effect to
//! apply it to the selected layers, or drag it onto a layer in the timeline or
//! the composition viewer. Effects BAE does not implement are greyed.

use std::collections::{BTreeMap, HashSet};

use bae_ui::theme::color;
use bae_ui::widgets;
use bae_ui::{Invocation, Panel, PanelContext, TabInfo, tr};
use egui::{Sense, Ui};

use super::EffectDrag;
use crate::actions;
use crate::common;

/// The Effects & Presets panel.
pub struct EffectsPanel {
    search: String,
    /// Expanded categories (English names); a search expands every match.
    open: HashSet<String>,
    /// Panel menu: Show Effects, Show Animation Presets (AE: both on).
    show_effects: bool,
    show_presets: bool,
    /// The user presets and when they were read (re-read every few seconds
    /// and on Refresh List).
    presets: Option<(std::time::Instant, Vec<(String, std::path::PathBuf)>)>,
}

impl Default for EffectsPanel {
    fn default() -> Self {
        EffectsPanel {
            search: String::new(),
            open: HashSet::new(),
            show_effects: true,
            show_presets: true,
            presets: None,
        }
    }
}

/// Category key of the user presets (AE lists it first).
const PRESETS: &str = "* Animation Presets";

/// One effect row: an AE effect or a registered effect AE does not have.
struct Row {
    match_name: String,
    /// Shown name.
    name: String,
    /// Category (English key).
    category: String,
    /// The other language's name (search matches both).
    extra: String,
}

impl Row {
    fn matches(&self, query: &str) -> bool {
        query.is_empty()
            || self.name.to_lowercase().contains(query)
            || self.extra.to_lowercase().contains(query)
            || self.match_name.to_lowercase().contains(query)
    }
}

impl Panel for EffectsPanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        TabInfo::new(tr("Effects & Presets"))
    }

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        actions::handle(action, cx, &session)
    }

    /// AE's panel menu: what the list shows, and Refresh List.
    fn menu(&mut self, ui: &mut Ui, _cx: &mut PanelContext<'_>) {
        ui.checkbox(&mut self.show_effects, tr("Show Effects"));
        ui.checkbox(&mut self.show_presets, tr("Show Animation Presets"));
        ui.separator();
        if ui.button(tr("Refresh List")).clicked() {
            self.presets = None;
            ui.close();
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text(tr("Search"))
                    .desired_width(f32::INFINITY),
            );
        });
        ui.add_space(4.0);
        let registries = super::registries(cx.resources);
        let implemented = |m: &str| registries.as_ref().is_some_and(|r| r.def(m).is_some());
        // AE's effects, plus registered effects AE does not have (plugins)
        // under their own category; availability is read from the registry
        // every frame, so effects loaded or unloaded later show at once.
        let mut rows: Vec<Row> = super::directory()
            .iter()
            .map(|e| Row {
                match_name: e.match_name.to_owned(),
                name: tr(e.name).to_owned(),
                category: e.category.to_owned(),
                extra: format!("{} {}", e.name, e.name_zh),
            })
            .collect();
        if let Some(r) = &registries {
            for entry in r.defs.snapshot().iter() {
                let def = entry.item();
                if rows.iter().any(|row| row.match_name == def.match_name) {
                    continue;
                }
                rows.push(Row {
                    match_name: def.match_name.clone(),
                    name: tr(&def.name).to_owned(),
                    category: def.category.clone(),
                    extra: def.name.clone(),
                });
            }
        }
        let query = self.search.trim().to_lowercase();
        rows.retain(|r| r.matches(&query));
        if !self.show_effects {
            rows.clear();
        }
        // The user presets, re-read every few seconds (a preset just saved shows).
        let stale = self
            .presets
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed() > std::time::Duration::from_secs(2));
        if stale {
            let list = crate::presets::folder(cx.resources)
                .map(|f| crate::presets::list(&f))
                .unwrap_or_default();
            self.presets = Some((std::time::Instant::now(), list));
        }
        let presets: Vec<(String, std::path::PathBuf)> = if self.show_presets {
            self.presets
                .as_ref()
                .map(|(_, l)| l.clone())
                .unwrap_or_default()
                .into_iter()
                .filter(|(name, _)| query.is_empty() || name.to_lowercase().contains(&query))
                .collect()
        } else {
            Vec::new()
        };
        // Categories sorted by their shown names (AE sorts by localized name).
        let mut by_category: BTreeMap<String, (String, Vec<Row>)> = BTreeMap::new();
        for row in rows {
            by_category
                .entry(tr(&row.category).to_owned())
                .or_insert_with(|| (row.category.clone(), Vec::new()))
                .1
                .push(row);
        }
        let searching = !query.is_empty();
        let mut apply: Option<String> = None;
        let mut apply_preset: Option<std::path::PathBuf> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // * Animation Presets first, as in AE.
                if self.show_presets {
                    let mut open = searching || self.open.contains(PRESETS);
                    ui.horizontal(|ui| {
                        if widgets::disclosure(ui, &mut open).clicked() && !searching {
                            if open {
                                self.open.insert(PRESETS.to_owned());
                            } else {
                                self.open.remove(PRESETS);
                            }
                        }
                        ui.label(tr(PRESETS));
                    });
                    if open {
                        for (name, path) in &presets {
                            ui.horizontal(|ui| {
                                ui.add_space(28.0);
                                let resp = ui.add(egui::Label::new(name).sense(Sense::click()));
                                if resp.double_clicked() {
                                    apply_preset = Some(path.clone());
                                }
                            });
                        }
                    }
                }
                for (shown, (category, mut effects)) in by_category {
                    let mut open = searching || self.open.contains(&category);
                    ui.horizontal(|ui| {
                        if widgets::disclosure(ui, &mut open).clicked() && !searching {
                            if open {
                                self.open.insert(category.clone());
                            } else {
                                self.open.remove(&category);
                            }
                        }
                        ui.label(&shown);
                    });
                    if !open {
                        continue;
                    }
                    effects.sort_by(|a, b| a.name.cmp(&b.name));
                    for e in effects {
                        let can = implemented(&e.match_name);
                        ui.horizontal(|ui| {
                            ui.add_space(28.0);
                            let text = egui::RichText::new(&e.name).color(if can {
                                color::TEXT
                            } else {
                                color::TEXT_DISABLED
                            });
                            let sense = if can {
                                Sense::click_and_drag()
                            } else {
                                Sense::hover()
                            };
                            let resp = ui.add(egui::Label::new(text).sense(sense));
                            if can {
                                resp.dnd_set_drag_payload(EffectDrag(e.match_name.clone()));
                                if resp.double_clicked() {
                                    apply = Some(e.match_name.clone());
                                }
                            } else {
                                resp.on_hover_text(tr("Not available in BAE yet"));
                            }
                        });
                    }
                }
            });
        if let Some(m) = apply {
            let session = common::session(cx.resources);
            let layers = session.lock().selected_layers.clone();
            super::apply(cx, &layers, &m);
        }
        if let Some(path) = apply_preset {
            let session = common::session(cx.resources);
            crate::presets::apply_file(cx, &session, &path);
        }
    }
}
