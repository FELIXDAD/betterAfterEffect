//! Workspace management: the bar order, hidden workspaces, user-created
//! workspaces (AE: Save as New Workspace / Edit Workspaces), and the first-launch
//! role picker that chooses the starting workspace.

use std::sync::Arc;

use egui::{RichText, Ui};

use super::{CustomWorkspace, Workbench};
use crate::i18n::tr;
use crate::theme::color;
use crate::workspace::WorkspacePreset;

/// Prefix of user-created workspace ids.
const CUSTOM_PREFIX: &str = "user:";

#[derive(Default)]
pub(super) struct Dialogs {
    pub edit_open: bool,
    pub role_picker: bool,
    new_title: String,
}

impl Workbench {
    /// Merge registered presets and user workspaces, in the user's bar order.
    pub(super) fn rebuild_workspaces(&mut self) {
        let snapshot = self.presets.snapshot();
        let mut list: Vec<(String, Arc<WorkspacePreset>)> = snapshot
            .iter()
            .map(|e| (e.id().to_string(), e.item().clone()))
            .collect();
        list.sort_by(|a, b| a.1.order.cmp(&b.1.order).then_with(|| a.0.cmp(&b.0)));
        for (id, custom) in &self.state.custom {
            let mut preset = WorkspacePreset::new(
                custom.title.clone(),
                custom.keymap.clone(),
                custom.layout.root.clone(),
            );
            preset.layout = custom.layout.clone();
            preset.order = i32::MAX;
            list.push((id.clone(), Arc::new(preset)));
        }
        let rank = |id: &str| {
            self.state
                .order
                .iter()
                .position(|o| o == id)
                .unwrap_or(usize::MAX)
        };
        // Stable: ids in `order` first, the rest keep preset order.
        list.sort_by_key(|(id, _)| rank(id));
        self.workspaces = list;
    }

    /// Open the first-launch role picker (the shell calls this when there is no
    /// saved workspace state yet).
    pub fn show_role_picker(&mut self) {
        self.dialogs.role_picker = true;
    }

    /// Whether the first-launch role picker is showing.
    pub fn role_picker_open(&self) -> bool {
        self.dialogs.role_picker
    }

    /// "Save as New Workspace": the current layout under a new name; switches to it.
    /// Returns the new workspace id.
    pub fn save_as_new_workspace(&mut self, title: &str) -> Option<String> {
        let title = title.trim();
        if title.is_empty() {
            return None;
        }
        let mut n = self.state.custom.len() + 1;
        let id = loop {
            let id = format!("{CUSTOM_PREFIX}{n}");
            if !self.state.custom.contains_key(&id) {
                break id;
            }
            n += 1;
        };
        self.state.custom.insert(
            id.clone(),
            CustomWorkspace {
                title: title.to_owned(),
                keymap: self.keymap_id(),
                layout: self.dock.layout.clone(),
            },
        );
        self.rebuild_workspaces();
        self.switch_workspace(&id);
        Some(id)
    }

    /// Delete a user-created workspace (not the active one).
    pub fn delete_workspace(&mut self, id: &str) -> bool {
        if id == self.active || self.state.custom.remove(id).is_none() {
            return false;
        }
        for map in [&mut self.state.current, &mut self.state.saved] {
            map.remove(id);
        }
        self.state.order.retain(|o| o != id);
        self.state.hidden.retain(|o| o != id);
        self.rebuild_workspaces();
        true
    }

    /// Move a workspace one place left (`-1`) or right (`+1`) in the bar.
    pub fn move_workspace(&mut self, id: &str, delta: i32) {
        let mut order: Vec<String> = self.workspaces.iter().map(|(w, _)| w.clone()).collect();
        let Some(i) = order.iter().position(|w| w == id) else {
            return;
        };
        let j = (i as i32 + delta).clamp(0, order.len() as i32 - 1) as usize;
        let item = order.remove(i);
        order.insert(j, item);
        self.state.order = order;
        self.rebuild_workspaces();
    }

    /// Show or hide a workspace in the bar (it stays in the Window menu).
    pub fn set_workspace_hidden(&mut self, id: &str, hidden: bool) {
        self.state.hidden.retain(|h| h != id);
        if hidden {
            self.state.hidden.push(id.to_owned());
        }
    }

    pub(super) fn workspace_dialogs(&mut self, ctx: &egui::Context) {
        if self.dialogs.role_picker {
            self.role_picker(ctx);
        }
        if self.dialogs.edit_open {
            let mut open = true;
            egui::Window::new(tr("Edit Workspaces"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(380.0)
                .show(ctx, |ui| self.edit_workspaces_ui(ui));
            self.dialogs.edit_open &= open;
        }
    }

    fn role_picker(&mut self, ctx: &egui::Context) {
        let roles: Vec<(String, String, String)> = self
            .workspaces
            .iter()
            .filter(|(_, p)| !p.description.is_empty())
            .map(|(id, p)| (id.clone(), p.title.clone(), p.description.clone()))
            .collect();
        let mut chosen = None;
        let mut close = false;
        // Modal: Esc, a click outside or the close button dismiss it and keep the
        // current workspace (the picker is a suggestion, not a required step).
        let modal = egui::Modal::new(egui::Id::new("bae-role-picker"))
            .frame(egui::Frame::window(&ctx.global_style()))
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tr("Welcome to BAE"))
                            .strong()
                            .color(color::TEXT_STRONG),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::icons::button(ui, crate::icons::Icon::Close, 16.0, false)
                            .on_hover_text(tr("Close"))
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.label(tr("What do you do? BAE opens the matching workspace; you can switch any time in the workspace bar."));
                ui.add_space(8.0);
                for (id, title, description) in &roles {
                    let text = format!("{}\n{}", tr(title), tr(description));
                    let button = egui::Button::new(RichText::new(text).color(color::TEXT_STRONG))
                        .min_size(egui::vec2(ui.available_width(), 40.0));
                    if ui.add(button).clicked() {
                        chosen = Some(id.clone());
                    }
                }
                for upcoming in ["Storyboard", "Editing"] {
                    ui.add_enabled(
                        false,
                        egui::Button::new(format!("{} · {}", tr(upcoming), tr("coming later")))
                            .min_size(egui::vec2(ui.available_width(), 24.0)),
                    );
                }
            });
        if let Some(id) = chosen {
            self.switch_workspace(&id);
            self.dialogs.role_picker = false;
        } else if close || modal.should_close() {
            self.dialogs.role_picker = false;
        }
    }

    fn edit_workspaces_ui(&mut self, ui: &mut Ui) {
        let rows: Vec<(String, String)> = self
            .workspaces
            .iter()
            .map(|(id, p)| (id.clone(), p.title.clone()))
            .collect();
        let count = rows.len();
        egui::Grid::new("bae-edit-workspaces")
            .num_columns(4)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                for (i, (id, title)) in rows.iter().enumerate() {
                    let active = *id == self.active;
                    let label = RichText::new(tr(title)).color(if active {
                        color::TEXT_STRONG
                    } else {
                        color::TEXT
                    });
                    ui.label(label);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(i > 0, egui::Button::new("↑").small())
                            .clicked()
                        {
                            self.move_workspace(id, -1);
                        }
                        if ui
                            .add_enabled(i + 1 < count, egui::Button::new("↓").small())
                            .clicked()
                        {
                            self.move_workspace(id, 1);
                        }
                    });
                    let mut shown = !self.state.hidden.contains(id);
                    if crate::widgets::checkbox(ui, &mut shown, tr("Show in Bar")).changed() {
                        self.set_workspace_hidden(id, !shown);
                    }
                    let deletable = id.starts_with(CUSTOM_PREFIX) && !active;
                    if deletable && ui.small_button(tr("Delete")).clicked() {
                        self.delete_workspace(id);
                    } else if !deletable {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(tr("New Workspace"));
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.dialogs.new_title)
                    .desired_width(160.0)
                    .hint_text(tr("Name")),
            );
            let submit = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let can_save = !self.dialogs.new_title.trim().is_empty();
            if (ui
                .add_enabled(can_save, egui::Button::new(tr("Save")))
                .clicked()
                || submit)
                && can_save
            {
                let title = std::mem::take(&mut self.dialogs.new_title);
                self.save_as_new_workspace(&title);
            }
        });
        ui.label(
            RichText::new(tr(
                "Saves the current layout and shortcuts as a new workspace.",
            ))
            .size(10.5)
            .color(color::TEXT_DIM),
        );
    }
}
