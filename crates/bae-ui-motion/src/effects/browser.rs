//! The Effects & Presets panel: a search field and AE's effect categories as
//! a tree (sorted by their localized names like AE). A click selects an effect
//! or preset, the arrow keys move the selection (Left / Right close and open
//! its category). Double-click an effect to apply it to the selected layers,
//! or drag it onto a layer in the timeline or the composition viewer.
//! Effects BAE does not implement are greyed.

use crate::presets::PresetIndex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::{path::PathBuf, sync::Arc};

use bae_ui::theme::color;
use bae_ui::widgets;
use bae_ui::{Invocation, Panel, PanelContext, TabInfo, tr};
use egui::{Sense, Ui};

use super::EffectDrag;
use crate::actions;
use crate::common;

/// The Effects & Presets panel.
#[derive(Default)]
pub struct EffectsPanel {
    search: String,
    /// Expanded categories (English names); a search expands every match.
    open: HashSet<String>,
    options: Options,
    selection: Option<Selection>,
    /// Selectable rows as last drawn, in order, with the category each is in.
    rows: Vec<(Selection, Option<String>)>,
    /// Arrow keys since the last draw: rows to move by, and close (-) or
    /// open (+) the selection's category.
    step: i32,
    fold: i32,
    /// Scroll the selection into view.
    reveal: bool,
}

/// A row of the list.
type Rows = Vec<(Selection, Option<String>)>;

#[derive(Clone, PartialEq, Eq)]
enum Selection {
    Effect(String),
    Preset(PathBuf),
    Builtin(&'static str),
}

impl Selection {
    fn location<'a>(
        &'a self,
        installed: Option<&'a super::manager::Installed>,
    ) -> Option<&'a std::path::Path> {
        match self {
            Self::Builtin(_) => None,
            Self::Preset(path) => Some(path),
            Self::Effect(name) => installed?.location(name),
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Grouping {
    #[default]
    Categories,
    Folders,
    Alphabetical,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Options {
    show_effects: bool,
    show_presets: bool,
    all_depths: bool,
    gpu_only: bool,
    grouping: Grouping,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            show_effects: true,
            show_presets: true,
            all_depths: true,
            gpu_only: false,
            grouping: Grouping::Categories,
        }
    }
}

const FOLDERS: &str = if cfg!(windows) {
    "Explorer Folders"
} else {
    "Finder Folders"
};
const REVEAL: &str = if cfg!(windows) {
    "Reveal in Explorer"
} else {
    "Reveal in Finder"
};

/// The Animation menu omits the ellipsis in Chinese. The panel menu includes it in both languages.
pub(super) fn save_preset_title() -> String {
    format!("{}...", tr("Save Animation Preset"))
}

impl Options {
    fn accepts(&self, capabilities: u8, depth: u8) -> bool {
        self.show_effects
            && (!self.gpu_only || capabilities & 8 != 0)
            && (depth == 1 || self.all_depths || capabilities & depth != 0)
    }
}

fn project_depth(stage: &bae_model::Stage) -> u8 {
    let path = bae_model::PrimPath::new(bae_motion::project_settings::PROJECT).unwrap();
    let depth = stage
        .prim(&path)
        .and_then(|v| bae_motion::query::static_value(&v, "bit_depth"));
    match depth.as_ref().and_then(|v| v.as_str()) {
        Some("8") => 1,
        Some("32") => 4,
        _ => 2,
    }
}

impl EffectsPanel {
    fn preset_folder(&mut self, ui: &mut Ui, key: &str, label: &str, searching: bool) -> bool {
        let mut open = searching || self.open.contains(key);
        ui.horizontal(|ui| {
            let toggle = widgets::disclosure(ui, &mut open).clicked();
            let label = ui.selectable_label(false, tr(label)).clicked();
            if label {
                open = !open;
            }
            if !searching && (toggle || label) {
                if open {
                    self.open.insert(key.to_owned());
                } else {
                    self.open.remove(key);
                }
            }
        });
        searching || open
    }

    #[allow(clippy::too_many_arguments)]
    fn builtin_folders(
        &mut self,
        ui: &mut Ui,
        recipes: &'static [crate::presets::builtins::Recipe],
        prefix: &str,
        query: &str,
        apply: &mut Option<Arc<[bae_motion::effect::PresetEffect]>>,
        rows: &mut Rows,
    ) {
        let segment = |r: &crate::presets::builtins::Recipe| {
            r.folder[prefix.len()..].split('/').next().unwrap_or("")
        };
        for group in recipes.chunk_by(|a, b| segment(a) == segment(b)) {
            let folder = segment(&group[0]);
            let path = format!("{prefix}{folder}");
            let matches = |name: &str| {
                name.to_lowercase().contains(query) || tr(name).to_lowercase().contains(query)
            };
            if !group
                .iter()
                .any(|r| matches(r.name) || r.folder.split('/').any(matches))
            {
                continue;
            }
            if self.preset_folder(ui, &format!("presets/{path}"), folder, !query.is_empty()) {
                ui.indent(&path, |ui| {
                    if group[0].folder != path {
                        self.builtin_folders(ui, group, &format!("{path}/"), query, apply, rows);
                    } else {
                        for r in group.iter().filter(|r| matches(r.name) || r.folder.split('/').any(matches)) {
                            let selected = matches!(self.selection, Some(Selection::Builtin(n)) if n == r.name);
                            let resp = ui
                                .selectable_label(selected, tr(r.name))
                                .interact(egui::Sense::drag());
                            self.reveal_if(selected, &resp);
                            rows.push((Selection::Builtin(r.name), None));
                            if resp.clicked() { self.selection = Some(Selection::Builtin(r.name)); }
                            crate::drag_feedback::source(
                                &resp,
                                || crate::drag_feedback::Ghost { icon: bae_ui::icons::Icon::Star, label: tr(r.name).to_owned() },
                                || EffectDrag::Preset(vec![r.build()].into()),
                            );
                            if resp.double_clicked() { *apply = Some(vec![r.build()].into()); }
                        }
                    }
                });
            }
        }
    }

    /// The arrow keys pressed since the last draw.
    fn navigate(&mut self) {
        let at = self
            .selection
            .as_ref()
            .and_then(|s| self.rows.iter().position(|(r, _)| r == s));
        if self.step != 0 && !self.rows.is_empty() {
            let last = self.rows.len() as i32 - 1;
            let next = match at {
                Some(i) => (i as i32 + self.step).clamp(0, last),
                None if self.step > 0 => 0,
                None => last,
            };
            self.selection = Some(self.rows[next as usize].0.clone());
            self.reveal = true;
        }
        if self.fold != 0
            && let Some(category) = at.and_then(|i| self.rows[i].1.clone())
        {
            if self.fold > 0 {
                self.open.insert(category);
            } else {
                self.open.remove(&category);
            }
        }
        (self.step, self.fold) = (0, 0);
    }

    fn reveal_if(&mut self, selected: bool, row: &egui::Response) {
        if selected && self.reveal {
            row.scroll_to_me(None);
            self.reveal = false;
        }
    }

    pub(super) fn restore(state: Option<&serde_json::Value>) -> Self {
        Self {
            options: state
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            ..Self::default()
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
    folder: String,
    capabilities: u8,
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
        // The arrow keys move the selection here, not the selected layers (AE).
        if action.id == crate::tools::ids::NUDGE {
            let step = |k: &str| {
                action
                    .params
                    .get(k)
                    .and_then(|v| v.as_f64())
                    .map_or(0, |v| v.signum() as i32)
            };
            self.step += step("dy");
            self.fold += step("dx");
            return true;
        }
        let session = common::session(cx.resources);
        actions::handle(action, cx, &session)
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.options).ok()
    }

    fn menu(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        let can_save = !crate::presets::effects_to_save(cx.stage, &session).is_empty();
        if ui
            .add_enabled(can_save, egui::Button::new(save_preset_title()))
            .clicked()
        {
            ui.close();
            crate::presets::save(cx, &session);
        }
        if ui.button(tr("Browse Presets...")).clicked() {
            ui.close();
            crate::presets::browse(cx);
        }
        ui.separator();
        for (value, label) in [
            (Grouping::Categories, "Categories"),
            (Grouping::Folders, FOLDERS),
            (Grouping::Alphabetical, "Alphabetical"),
        ] {
            if ui
                .radio_value(&mut self.options.grouping, value, tr(label))
                .clicked()
            {
                ui.close();
            }
        }
        ui.separator();
        let at_eight_bpc = project_depth(cx.stage) == 1;
        let mut all_depths = at_eight_bpc || self.options.all_depths;
        if ui
            .add_enabled(
                !at_eight_bpc,
                egui::Checkbox::new(&mut all_depths, tr("Show Effects for All Color Depths")),
            )
            .changed()
        {
            self.options.all_depths = all_depths;
        }
        ui.checkbox(
            &mut self.options.gpu_only,
            tr("Show GPU Capable Effects Only"),
        );
        ui.checkbox(&mut self.options.show_effects, tr("Show Effects"));
        ui.checkbox(&mut self.options.show_presets, tr("Show Animation Presets"));
        ui.separator();
        let path = self
            .selection
            .as_ref()
            .and_then(|s| s.location(cx.resources.get::<super::manager::Installed>()));
        if ui
            .add_enabled(path.is_some(), egui::Button::new(tr(REVEAL)))
            .clicked()
            && let Some(path) = path
        {
            bae_platform::reveal(path);
            ui.close();
        }
        if ui.button(tr("Refresh List")).clicked() {
            if let Some(index) = cx.resources.get::<Arc<PresetIndex>>() {
                index.refresh(true);
            }
            ui.close();
        }
        ui.separator();
        if ui.button(tr("Manage Effects...")).clicked() {
            super::manager::open(cx);
            ui.close();
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        self.navigate();
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
        let manager = cx.resources.get::<super::manager::Installed>();
        let mut rows: Vec<Row> = super::directory()
            .iter()
            .filter(|e| !manager.is_some_and(|m| m.disabled(e.match_name)))
            .map(|e| Row {
                match_name: e.match_name.to_owned(),
                name: tr(e.name).to_owned(),
                category: e.category.to_owned(),
                folder: "Built-in Effects".into(),
                capabilities: e.capabilities,
                extra: format!("{} {}", e.name, e.name_zh),
            })
            .collect();
        if let Some(r) = &registries {
            let catalogued: HashSet<_> = super::directory().iter().map(|e| e.match_name).collect();
            for entry in r.defs.snapshot().iter() {
                let def = entry.item();
                if catalogued.contains(def.match_name.as_str()) {
                    continue;
                }
                rows.push(Row {
                    match_name: def.match_name.clone(),
                    name: tr(&def.name).to_owned(),
                    category: def.category.clone(),
                    folder: def
                        .plugin
                        .as_ref()
                        .map_or_else(|| "Built-in Effects".into(), |p| p.id.clone()),
                    capabilities: 7 | if def.gpu { 8 } else { 0 },
                    extra: def.name.clone(),
                });
            }
        }
        let query = self.search.trim().to_lowercase();
        let depth = project_depth(cx.stage);
        rows.retain(|r| r.matches(&query) && self.options.accepts(r.capabilities, depth));
        let presets = cx
            .resources
            .get::<Arc<PresetIndex>>()
            .map(|index| {
                if self.options.show_presets {
                    index.refresh(false);
                }
                index.entries()
            })
            .unwrap_or_default();
        if let Some(Selection::Preset(selected)) = &self.selection
            && !presets.iter().any(|entry| entry.path == *selected)
        {
            self.selection = None;
        }
        // Categories sorted by their shown names (AE sorts by localized name).
        let mut by_category: BTreeMap<String, (String, Vec<Row>)> = BTreeMap::new();
        for row in rows {
            let group = match self.options.grouping {
                Grouping::Categories => &row.category,
                Grouping::Folders => &row.folder,
                Grouping::Alphabetical => "",
            };
            by_category
                .entry(tr(group).to_owned())
                .or_insert_with(|| (group.to_owned(), Vec::new()))
                .1
                .push(row);
        }
        let searching = !query.is_empty();
        let mut apply: Option<String> = None;
        let mut apply_preset = None;
        let mut list: Rows = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // * Animation Presets first, as in AE.
                if self.options.show_presets && self.preset_folder(ui, PRESETS, PRESETS, searching) {
                    ui.indent(PRESETS, |ui| {
                        self.builtin_folders(ui, crate::presets::builtins::LIBRARY, "", &query, &mut apply_preset, &mut list);
                        if !self.preset_folder(ui, "presets/user", "User Presets", searching) { return; }
                        for entry in presets
                            .iter()
                            .filter(|e| query.is_empty() || e.name.to_lowercase().contains(&query))
                        {
                            let (name, path) = (&entry.name, &entry.path);
                            ui.horizontal(|ui| {
                                ui.add_space(28.0);
                                let selected = matches!(&self.selection,Some(Selection::Preset(p)) if p == path);
                                let resp = ui.selectable_label(selected, name).interact(egui::Sense::drag());
                                self.reveal_if(selected, &resp);
                                list.push((Selection::Preset(path.clone()), None));
                                crate::drag_feedback::source(
                                    &resp,
                                    || crate::drag_feedback::Ghost {
                                        icon: bae_ui::icons::Icon::Star,
                                        label: name.clone(),
                                    },
                                    || EffectDrag::Preset(entry.effects.clone()),
                                );
                                if resp.clicked() {
                                    self.selection = Some(Selection::Preset(path.clone()));
                                }
                                if resp.double_clicked() {
                                    apply_preset = Some(entry.effects.clone());
                                }
                            });
                        }
                    });
                }
                for (shown, (category, mut effects)) in by_category {
                    if self.options.grouping != Grouping::Alphabetical {
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
                    }
                    effects.sort_by(|a, b| a.name.cmp(&b.name));
                    for e in effects {
                        let can = implemented(&e.match_name);
                        ui.horizontal(|ui| {
                            if self.options.grouping != Grouping::Alphabetical {
                                ui.add_space(28.0);
                            }
                            let text = egui::RichText::new(&e.name).color(if can {
                                color::TEXT
                            } else {
                                color::TEXT_DISABLED
                            });
                            if !can {
                                ui.add(egui::Label::new(text).sense(Sense::hover()))
                                    .on_hover_text(tr("Not available in BAE yet"));
                                return;
                            }
                            let selected = matches!(&self.selection, Some(Selection::Effect(m)) if *m == e.match_name);
                            let resp = ui.add(
                                egui::Button::selectable(selected, text)
                                    .frame_when_inactive(false)
                                    .sense(Sense::click_and_drag()),
                            );
                            self.reveal_if(selected, &resp);
                            let folder = (self.options.grouping != Grouping::Alphabetical)
                                .then(|| category.clone());
                            list.push((Selection::Effect(e.match_name.clone()), folder));
                            if resp.clicked() {
                                self.selection = Some(Selection::Effect(e.match_name.clone()));
                            }
                            crate::drag_feedback::source(
                                &resp,
                                || crate::drag_feedback::Ghost {
                                    icon: bae_ui::icons::Icon::Fx,
                                    label: e.name.clone(),
                                },
                                || EffectDrag::Effect(e.match_name.clone()),
                            );
                            if resp.double_clicked() {
                                apply = Some(e.match_name.clone());
                            }
                        });
                    }
                }
            });
        self.rows = list;
        // Double-click: onto the active composition's selected layers (one
        // undo step); without one, nothing happens (AE).
        if let Some(m) = apply {
            let layers = super::selected_layers(cx);
            super::apply(cx, &layers, &m);
        }
        if let Some(effects) = apply_preset {
            let audio = effects.iter().all(|e| super::is_audio(cx, &e.match_name));
            let (layers, refused) = super::applicable(cx, &super::selected_layers(cx), audio);
            if layers.is_empty() {
                if refused {
                    super::refuse_lights_and_cameras(cx);
                }
            } else {
                crate::presets::apply_to_layers(cx, &layers, &effects);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Up / Down move through the rows as drawn (search results too), Left /
    /// Right close and open the selection's category.
    #[test]
    fn arrow_keys_move_the_selection() {
        let effect = |m: &str| {
            (
                Selection::Effect(m.into()),
                Some("Blur & Sharpen".to_owned()),
            )
        };
        let mut panel = EffectsPanel {
            rows: vec![
                (Selection::Builtin("Fade In"), None),
                effect("a"),
                effect("b"),
            ],
            ..Default::default()
        };
        panel.step = 1;
        panel.navigate();
        assert!(panel.selection == Some(Selection::Builtin("Fade In")) && panel.reveal);
        panel.step = 5;
        panel.navigate();
        assert!(panel.selection == Some(Selection::Effect("b".into())));
        panel.step = -1;
        panel.fold = 1;
        panel.navigate();
        assert!(panel.selection == Some(Selection::Effect("a".into())));
        assert!(panel.open.contains("Blur & Sharpen"));
        panel.fold = -1;
        panel.navigate();
        assert!(!panel.open.contains("Blur & Sharpen"));
    }
    #[test]
    fn depth_and_gpu_filters_combine_and_persist() {
        let options = Options {
            gpu_only: true,
            all_depths: false,
            ..Default::default()
        };
        let flags = |name| {
            super::super::directory()
                .iter()
                .find(|e| e.match_name == name)
                .unwrap()
                .capabilities
        };
        assert!(options.accepts(flags("ADBE Gaussian Blur 2"), 4));
        assert!(!options.accepts(flags("ADBE Gaussian Blur"), 4));
        assert!(!options.accepts(8 | 1, 4));
        let state = serde_json::to_value(options).unwrap();
        let restored = EffectsPanel::restore(Some(&state));
        assert_eq!(restored.save_state(), Some(state));
    }
}
