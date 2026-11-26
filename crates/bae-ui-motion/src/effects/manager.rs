//! Enabled effects are a user preference, applied before the first project opens.
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bae_app::App;
use bae_model::EffectDef;
use bae_platform::settings::{SettingsFile, VersionedSettings};
use bae_ui::dialog::{self, Outcome};
use bae_ui::{PanelContext, tr};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Disabled(BTreeSet<String>);
impl VersionedSettings for Disabled {
    const KIND: &'static str = "disabled-effects";
    const VERSION: u32 = 1;
}

#[derive(Debug)]
struct InstalledEffect {
    def: Arc<EffectDef>,
    location: Option<PathBuf>,
}

#[derive(Debug)]
pub(super) struct Installed {
    rows: Vec<InstalledEffect>,
    at_start: Disabled,
    next_start: Mutex<Disabled>,
    file: SettingsFile<Disabled>,
}

impl Installed {
    pub(super) fn location(&self, match_name: &str) -> Option<&std::path::Path> {
        self.rows
            .iter()
            .find(|row| row.def.match_name == match_name)?
            .location
            .as_deref()
    }

    pub(super) fn disabled(&self, name: &str) -> bool {
        self.at_start.0.contains(name)
    }
}

/// Snapshots installed effects, then withdraws disabled definitions, render kernels and menu entries.
pub fn configure(app: &mut App, packages: HashMap<String, PathBuf>) {
    let Some(dirs) = app.resource::<bae_platform::AppDirs>() else {
        return;
    };
    let file = SettingsFile::<Disabled>::new(dirs.config_file("disabled-effects.json"));
    let disabled = file.load().value;
    let defs = app.registry::<EffectDef>();
    let kernels = app.registry::<dyn bae_render::Effect>();
    let menu = app.registry::<bae_ui::menu::MenuItemSpec>();
    let binary = std::env::current_exe().ok();
    let mut rows = Vec::new();
    for entry in defs.snapshot().iter() {
        let def = entry.item().clone();
        let location = match &def.plugin {
            Some(p) => packages.get(&p.id).cloned(),
            None => binary.clone(),
        };
        if disabled.0.contains(&def.match_name) {
            defs.unregister(entry.id());
            kernels.unregister(entry.id());
        }
        rows.push(InstalledEffect { def, location });
    }
    if !disabled.0.is_empty() {
        for entry in menu.snapshot().iter() {
            let spec = entry.item();
            if spec.action == super::APPLY
                && spec.params["match_name"]
                    .as_str()
                    .is_some_and(|m| disabled.0.contains(m))
            {
                menu.unregister(entry.id());
            }
        }
    }
    rows.sort_by(|a, b| a.def.name.cmp(&b.def.name));
    app.insert_resource(Installed {
        rows,
        next_start: Mutex::new(disabled.clone()),
        at_start: disabled,
        file,
    });
}

#[derive(Clone, Debug)]
pub struct EffectManager {
    next: Disabled,
    search: String,
    error: String,
}

pub(crate) fn open(cx: &PanelContext<'_>) {
    let Some(installed) = cx.resources.get::<Installed>() else {
        return;
    };
    let dialogs = crate::dialogs::shared(cx.resources);
    dialogs.lock().open = Some(crate::dialogs::Dialog::EffectManager(Box::new(
        EffectManager {
            next: installed.next_start.lock().unwrap().clone(),
            search: String::new(),
            error: String::new(),
        },
    )));
}

impl EffectManager {
    pub(crate) fn ui(&mut self, ctx: &egui::Context, cx: &PanelContext<'_>) -> bool {
        let Some(installed) = cx.resources.get::<Installed>() else {
            return false;
        };
        let id = egui::Id::new("effect-manager");
        let out = dialog::window(ctx, id, tr("Effect Manager"), |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text(tr("Search")));
            let query = self.search.trim().to_lowercase();
            egui::ScrollArea::both().max_height(350.0).show(ui, |ui| {
                egui::Grid::new("effect-manager-table")
                    .striped(true)
                    .show(ui, |ui| {
                        for header in ["Enabled", "Name", "Category", "Version", "File Path"] {
                            ui.strong(tr(header));
                        }
                        ui.end_row();
                        for row in &installed.rows {
                            let def = &row.def;
                            let shown = tr(&def.name);
                            if !query.is_empty()
                                && ![shown, &def.name, &def.match_name]
                                    .iter()
                                    .any(|n| n.to_lowercase().contains(&query))
                            {
                                continue;
                            }
                            let mut enabled = !self.next.0.contains(&def.match_name);
                            if ui.checkbox(&mut enabled, "").changed() {
                                if enabled {
                                    self.next.0.remove(&def.match_name);
                                } else {
                                    self.next.0.insert(def.match_name.clone());
                                }
                            }
                            ui.label(shown);
                            ui.label(tr(&def.category));
                            ui.label(
                                def.plugin
                                    .as_ref()
                                    .map_or(bae_platform::VERSION, |p| &p.version),
                            );
                            ui.label(
                                row.location
                                    .as_ref()
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_default(),
                            );
                            ui.end_row();
                        }
                    });
            });
            ui.label(tr("Changes take effect when BAE restarts."));
            if !self.error.is_empty() {
                ui.label(&self.error);
            }
            dialog::buttons(ui, true)
        });
        match out {
            Some(Outcome::Ok) => match installed.file.save(&self.next) {
                Ok(()) => {
                    *installed.next_start.lock().unwrap() = self.next.clone();
                    false
                }
                Err(error) => {
                    self.error = error.to_string();
                    true
                }
            },
            Some(Outcome::Cancel) => false,
            _ => !dialog::closed_by_title_bar(ctx, id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabling_removes_kernel_and_definition_until_reenabled() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = bae_platform::AppDirs::under_root(dir.path());
        let file = SettingsFile::new(dirs.config_file("disabled-effects.json"));
        let name = "ADBE Gaussian Blur 2";
        let key = bae_render::effect::registry_id(name);
        for enabled in [false, true] {
            let mut app = bae_app::AppBuilder::new();
            app.add_module(bae_effects::EffectsModule);
            let mut app = app.build();
            app.insert_resource(dirs.clone());
            let disabled = if enabled {
                Disabled::default()
            } else {
                Disabled([name.to_owned()].into_iter().collect())
            };
            file.save(&disabled).unwrap();
            configure(&mut app, HashMap::new());
            assert_eq!(app.registry::<EffectDef>().get(&key).is_some(), enabled);
            assert_eq!(
                app.registry::<dyn bae_render::Effect>().get(&key).is_some(),
                enabled
            );
            let installed = app.resource::<Installed>().unwrap();
            assert!(installed.rows.iter().any(|row| row.def.match_name == name));
            assert_eq!(installed.disabled(name), !enabled);
        }
    }
}
