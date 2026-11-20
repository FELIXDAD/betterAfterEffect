//! The Keyboard Shortcuts window (AE: Edit > Keyboard Shortcuts): every command
//! with its shortcuts in the active workspace's keymap; click + to record a new
//! chord, × to remove one. A chord already in use asks before replacing.

use egui::{Event, Key, RichText, Ui};

use super::Workbench;
use crate::i18n::tr;
use crate::keymap::{Binding, Chord};
use crate::theme::color;

/// A command row in the editor (one command id + parameters).
#[derive(Clone, Debug, PartialEq)]
pub struct CommandEntry {
    pub id: String,
    pub params: serde_json::Value,
    /// English title.
    pub title: String,
    /// Panel kind for panel-scoped actions.
    pub panel: Option<String>,
}

#[derive(Default)]
pub(super) struct ShortcutEditor {
    pub open: bool,
    /// Focus the search field on the next frame (just opened).
    pub focus_search: bool,
    search: String,
    /// The row waiting for a key press.
    pub recording: Option<CommandEntry>,
    /// A recorded chord that collides with another binding, awaiting confirmation.
    pub confirm: Option<(Binding, String)>,
}

impl Workbench {
    /// Commands listed in the shortcut editor: registered UI actions, the extra
    /// commands the shell supplied (model commands) and every bound command.
    pub fn shortcut_commands(&self) -> Vec<CommandEntry> {
        let mut out: Vec<CommandEntry> = Vec::new();
        let mut push = |e: CommandEntry| {
            if !out.iter().any(|o| o.id == e.id && o.params == e.params) {
                out.push(e);
            }
        };
        let actions = self.actions.snapshot();
        for b in &self.keymap.bindings {
            push(CommandEntry {
                id: b.action.clone(),
                params: b.params.clone(),
                title: self.command_title(&b.action, &b.params),
                panel: b.when.clone(),
            });
        }
        for e in actions.iter() {
            push(CommandEntry {
                id: e.id().to_string(),
                params: serde_json::Value::Null,
                title: e.item().title.clone(),
                panel: e.item().panel.clone(),
            });
        }
        for (id, title) in &self.extra_commands {
            push(CommandEntry {
                id: id.clone(),
                params: serde_json::Value::Null,
                title: title.clone(),
                panel: None,
            });
        }
        out
    }

    /// Display title of a command id with parameters, translated.
    pub fn command_title(&self, id: &str, params: &serde_json::Value) -> String {
        let base = self
            .actions
            .get(id)
            .map(|a| a.title.clone())
            .or_else(|| {
                self.extra_commands
                    .iter()
                    .find(|(c, _)| c == id)
                    .map(|(_, t)| t.clone())
            })
            .unwrap_or_else(|| id.to_owned());
        let base = tr(&base).to_owned();
        let arg = params
            .get("id")
            .and_then(|v| v.as_str())
            .map(|arg| {
                self.kinds
                    .get(arg)
                    .map(|k| tr(&k.title).to_owned())
                    .or_else(|| self.preset(arg).map(|p| tr(&p.title).to_owned()))
                    .unwrap_or_else(|| arg.to_owned())
            })
            .or_else(|| {
                params.get("index").and_then(|v| v.as_u64()).map(|i| {
                    self.workspaces
                        .get(i as usize)
                        .map_or_else(|| format!("#{}", i + 1), |(_, p)| tr(&p.title).to_owned())
                })
            });
        match arg {
            Some(arg) => format!("{base}: {arg}"),
            None => base,
        }
    }

    /// Capture the next key press while recording; returns `true` if it consumed
    /// the frame's key events.
    pub(super) fn record_shortcut(&mut self, ctx: &egui::Context) -> bool {
        let Some(entry) = self.editor.recording.clone() else {
            return false;
        };
        let mut chord = None;
        let mut cancel = false;
        ctx.input_mut(|input| {
            input.events.retain(|e| match e {
                Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if *key == Key::Escape && modifiers.is_none() {
                        cancel = true;
                    } else if chord.is_none() {
                        let key = physical_key.unwrap_or(*key);
                        chord = Some(Chord {
                            key,
                            command: modifiers.command,
                            mac_ctrl: cfg!(target_os = "macos") && modifiers.ctrl,
                            shift: modifiers.shift,
                            alt: modifiers.alt,
                        });
                    }
                    false
                }
                Event::Text(_) | Event::Copy | Event::Cut | Event::Paste(_) => false,
                _ => true,
            });
        });
        if cancel {
            self.editor.recording = None;
        } else if let Some(chord) = chord {
            self.editor.recording = None;
            let binding = Binding {
                chord,
                action: entry.id.clone(),
                params: entry.params.clone(),
                when: entry.panel.clone(),
            };
            match self.keymap.conflict_for(&chord, entry.panel.as_deref()) {
                Some(other) if other.action == binding.action && other.params == binding.params => {
                }
                Some(other) => {
                    let title = self.command_title(&other.action, &other.params);
                    self.editor.confirm = Some((binding, title));
                }
                None => self.bind(binding),
            }
        }
        true
    }

    pub(super) fn bind(&mut self, binding: Binding) {
        let id = self.keymap_id();
        let mut overrides = self.keymap_overrides(&id);
        overrides.bind(binding);
        self.set_keymap_overrides(&id, overrides);
    }

    fn unbind(&mut self, chord: Chord, when: Option<String>) {
        let id = self.keymap_id();
        let mut overrides = self.keymap_overrides(&id);
        overrides.unbind(chord, when);
        self.set_keymap_overrides(&id, overrides);
    }

    pub(super) fn shortcut_window(&mut self, ctx: &egui::Context) {
        if !self.editor.open {
            return;
        }
        let mut open = true;
        let keymap_title = self
            .keymaps
            .get(&self.keymap_id())
            .map(|k| tr(&k.title).to_owned())
            .unwrap_or_default();
        egui::Window::new(tr("Keyboard Shortcuts"))
            .open(&mut open)
            .default_size([560.0, 520.0])
            .collapsible(false)
            .show(ctx, |ui| self.shortcut_editor_ui(ui, &keymap_title));
        if !open {
            self.editor.open = false;
            self.editor.recording = None;
            self.editor.confirm = None;
        }
    }

    fn shortcut_editor_ui(&mut self, ui: &mut Ui, keymap_title: &str) {
        ui.horizontal(|ui| {
            ui.label(format!("{}: {keymap_title}", tr("Keyboard Layout")));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let id = self.keymap_id();
                let custom = !self.keymap_overrides(&id).is_empty();
                if ui
                    .add_enabled(custom, egui::Button::new(tr("Reset All")))
                    .clicked()
                {
                    self.set_keymap_overrides(&id, Default::default());
                }
            });
        });
        let search =
            crate::widgets::search_field(ui, &mut self.editor.search, ui.available_width());
        if std::mem::take(&mut self.editor.focus_search) {
            search.request_focus();
        }
        if let Some((binding, other)) = self.editor.confirm.clone() {
            ui.add_space(4.0);
            ui.colored_label(
                color::ACCENT_HOVER,
                format!(
                    "{} {} → {}",
                    binding.chord.display(),
                    tr("is already used by"),
                    other
                ),
            );
            ui.horizontal(|ui| {
                if ui.button(tr("Replace")).clicked() {
                    self.editor.confirm = None;
                    self.bind(binding);
                }
                if ui.button(tr("Cancel")).clicked() {
                    self.editor.confirm = None;
                }
            });
        }
        ui.separator();

        let query = self.editor.search.to_lowercase();
        let mut rows = self.shortcut_commands();
        rows.sort_by_key(|e| self.command_title(&e.id, &e.params).to_lowercase());
        let conflicts = self.keymap.conflicts();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("bae-shortcuts")
                    .num_columns(2)
                    .striped(true)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for entry in rows {
                            let title = self.command_title(&entry.id, &entry.params);
                            let chords: Vec<Binding> = self
                                .keymap
                                .bindings
                                .iter()
                                .filter(|b| b.action == entry.id && b.params == entry.params)
                                .cloned()
                                .collect();
                            let matches = query.is_empty()
                                || title.to_lowercase().contains(&query)
                                || entry.id.contains(&query)
                                || chords
                                    .iter()
                                    .any(|b| b.chord.display().to_lowercase().contains(&query));
                            if !matches {
                                continue;
                            }
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&title).color(color::TEXT_STRONG));
                                let scope = entry
                                    .panel
                                    .as_deref()
                                    .map(|p| format!(" · {}", tr(p)))
                                    .unwrap_or_default();
                                ui.label(
                                    RichText::new(format!("{}{scope}", entry.id))
                                        .size(10.0)
                                        .color(color::TEXT_DIM),
                                );
                            });
                            ui.horizontal_wrapped(|ui| {
                                for b in &chords {
                                    let conflicted = conflicts
                                        .iter()
                                        .any(|c| c.chord == b.chord && c.when == b.when);
                                    let text =
                                        RichText::new(b.chord.display()).color(if conflicted {
                                            color::TRAFFIC_CLOSE
                                        } else {
                                            color::TEXT
                                        });
                                    ui.label(text);
                                    if ui.small_button("×").on_hover_text(tr("Remove")).clicked() {
                                        self.unbind(b.chord, b.when.clone());
                                    }
                                }
                                let recording = self.editor.recording.as_ref() == Some(&entry);
                                if recording {
                                    ui.label(
                                        RichText::new(tr("Press a key… (Esc to cancel)"))
                                            .color(color::ACCENT),
                                    );
                                } else if ui
                                    .small_button("+")
                                    .on_hover_text(tr("Add Shortcut"))
                                    .clicked()
                                {
                                    self.editor.recording = Some(entry.clone());
                                    self.editor.confirm = None;
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
    }
}
