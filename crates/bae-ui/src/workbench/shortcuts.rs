//! The Keyboard Shortcuts window (AE: Edit > Keyboard Shortcuts): every command
//! with its shortcuts in the active workspace's keymap; click + to record a new
//! chord, × to remove one. A chord already in use asks before replacing.

use egui::{Align, Event, FontId, Key, Layout, RichText, Shape, TextStyle, Ui, vec2};

use super::Workbench;
use crate::i18n::tr;
use crate::keymap::{Binding, Chord};
use crate::theme::color;

/// A command row in the editor (one command id + parameters).
#[derive(Clone, Debug, PartialEq)]
pub struct CommandEntry {
    pub id: String,
    pub params: serde_json::Value,
    /// English name of the row (variant title, or command title and argument).
    pub title: String,
    /// Panel kind for panel-scoped actions.
    pub panel: Option<String>,
}

#[derive(Default)]
pub(super) struct ShortcutEditor {
    pub open: bool,
    /// Focus the search field on the next frame (just opened).
    pub focus_search: bool,
    pub search: String,
    /// The row waiting for a key press.
    pub recording: Option<CommandEntry>,
    /// A recorded chord that collides with another binding, awaiting confirmation.
    pub confirm: Option<(Binding, String)>,
}

impl Workbench {
    /// Commands listed in the shortcut editor, one row per command and parameters:
    /// every bound command, the named variants of UI actions ("Back 10 Frames"), the
    /// other registered UI actions, and the model commands the shell supplied. Every
    /// row has its own name (#140): a UI action with variants is listed by them (not
    /// also bare), and a model command named like a row already listed (`comp.create`
    /// next to the New Composition action that runs it) is left out.
    pub fn shortcut_commands(&self) -> Vec<CommandEntry> {
        let mut out: Vec<CommandEntry> = Vec::new();
        let english = |id: &str, params: &serde_json::Value| {
            let (title, arg) = self.command_title_parts(id, params);
            match arg {
                Some(arg) => format!("{title}: {arg}"),
                None => title,
            }
        };
        fn push(out: &mut Vec<CommandEntry>, e: CommandEntry) {
            if !out.iter().any(|o| o.id == e.id && o.params == e.params) {
                out.push(e);
            }
        }
        let actions = self.actions.snapshot();
        let variants = self.variants.snapshot();
        for b in &self.keymap.bindings {
            push(
                &mut out,
                CommandEntry {
                    id: b.action.clone(),
                    params: b.params.clone(),
                    title: english(&b.action, &b.params),
                    panel: b.when.clone(),
                },
            );
        }
        for v in variants.iter().map(|e| e.item()) {
            if let Some(action) = actions.get(v.command.as_str()) {
                push(
                    &mut out,
                    CommandEntry {
                        id: v.command.clone(),
                        params: v.params.clone(),
                        title: v.title.clone(),
                        panel: action.panel.clone(),
                    },
                );
            }
        }
        for e in actions.iter() {
            if variants
                .iter()
                .any(|v| v.item().command == *e.id().as_ref())
            {
                continue;
            }
            push(
                &mut out,
                CommandEntry {
                    id: e.id().to_string(),
                    params: serde_json::Value::Null,
                    title: e.item().title.clone(),
                    panel: e.item().panel.clone(),
                },
            );
        }
        for (id, _) in &self.extra_commands {
            let title = english(id, &serde_json::Value::Null);
            if out.iter().any(|o| o.title == title) {
                continue;
            }
            push(
                &mut out,
                CommandEntry {
                    id: id.clone(),
                    params: serde_json::Value::Null,
                    title,
                    panel: None,
                },
            );
        }
        out
    }

    /// Display title of a command id with parameters, translated.
    pub fn command_title(&self, id: &str, params: &serde_json::Value) -> String {
        let (base, arg) = self.command_title_parts(id, params);
        match arg {
            Some(arg) => format!("{}: {}", tr(&base), tr(&arg)),
            None => tr(&base).to_owned(),
        }
    }

    /// The English parts of [`Self::command_title`]: the command's title and, for
    /// commands with a panel, workspace or preset argument, that argument's title
    /// (unregistered AE panels by their AE name, never a raw id when known).
    pub fn command_title_parts(
        &self,
        id: &str,
        params: &serde_json::Value,
    ) -> (String, Option<String>) {
        // A named variant ("Back 10 Frames", "New Solid") says it all (#140).
        let variants = self.variants.snapshot();
        if let Some(title) =
            crate::action::variant_title(variants.iter().map(|e| &**e.item()), id, params)
        {
            return (title.to_owned(), None);
        }
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
        let arg = params
            .get("id")
            .and_then(|v| v.as_str())
            .map(|arg| {
                self.kinds
                    .get(arg)
                    .map(|k| k.title.clone())
                    .or_else(|| self.preset(arg).map(|p| p.title.clone()))
                    .or_else(|| crate::known_panel_title(arg).map(str::to_owned))
                    .unwrap_or_else(|| arg.to_owned())
            })
            .or_else(|| {
                params.get("index").and_then(|v| v.as_u64()).map(|i| {
                    self.workspaces
                        .get(i as usize)
                        .map_or_else(|| format!("#{}", i + 1), |(_, p)| p.title.clone())
                })
            });
        (base, arg)
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
        // Escape closes it, as AE's dialogs (while recording, it cancels the
        // recording instead: `record_shortcut` has taken the key).
        if self.editor.recording.is_none() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            open = false;
        }
        let screen = ctx.content_rect().size();
        egui::Window::new(tr("Keyboard Shortcuts"))
            .open(&mut open)
            .default_size([
                640.0_f32.min(screen.x - 40.0),
                (screen.y * 0.7).clamp(320.0, 720.0),
            ])
            .min_size([360.0, 240.0])
            .resizable(true)
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
                    "{} {} {}",
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
        let mut entries = self.shortcut_commands();
        entries.sort_by_key(|e| self.command_title(&e.id, &e.params).to_lowercase());
        let conflicts = self.keymap.conflicts();
        let rows: Vec<Row> = entries
            .into_iter()
            .filter_map(|entry| {
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
                // The panel a shortcut works in, by its (translated) title.
                let scope = entry
                    .panel
                    .as_deref()
                    .map(|p| {
                        let title = self
                            .kinds
                            .get(p)
                            .map_or_else(|| tr(p).to_owned(), |k| tr(&k.title).to_owned());
                        format!(" · {title}")
                    })
                    .unwrap_or_default();
                let detail = format!("{}{scope}", entry.id);
                matches.then_some(Row {
                    entry,
                    title,
                    detail,
                    chords,
                })
            })
            .collect();
        // The command column is as wide as its longest name (names never
        // wrap), within 30–65 % of the dialog; the shortcuts get the rest.
        const GAP: f32 = 16.0;
        let body = TextStyle::Body.resolve(ui.style());
        let small = FontId::proportional(DETAIL_SIZE);
        let widest = ui.fonts_mut(|f| {
            let mut w = |text: &str, font: &FontId| {
                f.layout_no_wrap(text.to_owned(), font.clone(), color::TEXT)
                    .size()
                    .x
            };
            let mut widest = 0.0_f32;
            for r in &rows {
                widest = widest.max(w(&r.title, &body));
                widest = widest.max(w(&r.detail, &small));
            }
            widest
        });
        let width = ui.available_width();
        let column = widest.ceil().clamp(width * 0.3, width * 0.65);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for (i, row) in rows.into_iter().enumerate() {
                    let Row {
                        entry,
                        title,
                        detail,
                        chords,
                    } = row;
                    let stripe = ui.painter().add(Shape::Noop);
                    let resp = ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(
                            vec2(column, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(column);
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&title).color(color::TEXT_STRONG),
                                    )
                                    .truncate(),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(detail)
                                            .size(DETAIL_SIZE)
                                            .color(color::TEXT_DIM),
                                    )
                                    .truncate(),
                                );
                            },
                        );
                        ui.add_space(GAP - ui.spacing().item_spacing.x);
                        ui.horizontal_wrapped(|ui| {
                            for b in &chords {
                                let conflicted = conflicts
                                    .iter()
                                    .any(|c| c.chord == b.chord && c.when == b.when);
                                let text = RichText::new(b.chord.display()).color(if conflicted {
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
                    });
                    if i % 2 == 1 {
                        let r = egui::Rect::from_x_y_ranges(
                            ui.max_rect().x_range(),
                            resp.response.rect.y_range(),
                        )
                        .expand2(vec2(0.0, 2.0));
                        ui.painter().set(
                            stripe,
                            Shape::rect_filled(r, 0.0, ui.visuals().faint_bg_color),
                        );
                    }
                }
            });
    }
}

/// Size of the command id line under each title.
const DETAIL_SIZE: f32 = 10.0;

/// A listed command with what the editor shows for it.
struct Row {
    entry: CommandEntry,
    title: String,
    detail: String,
    chords: Vec<Binding>,
}
