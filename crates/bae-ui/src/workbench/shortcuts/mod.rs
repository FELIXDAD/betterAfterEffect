//! The Keyboard Shortcuts dialog (AE: Edit > Keyboard Shortcuts, ⌘⌥' / Ctrl+Alt+'),
//! laid out and working like AE 26's (docs/reference/ae/mac/ui/keyboard-shortcuts.json):
//!
//! - Preset, Keyboard Layout and Commands (scope) menus at the top;
//! - a search field and the command tree (Application: tools, menus, the rest;
//!   Panels: one group per panel scope) with each command's shortcuts;
//! - the Key table, Undo and Clear on the right; the message bar, Cancel and OK
//!   at the bottom.
//!
//! Changes are made to a draft of the active keymap's customization: OK applies
//! and saves them, Cancel (or Esc, or the close button) drops them. Changing
//! the built-in preset makes it the Custom preset, like AE.
//!
//! In a selected row each shortcut is a pill with ×; clicking the Shortcut
//! column focuses the first pill, clicking it again adds an empty pill; a key
//! pressed while a pill has the focus is assigned to the command (a chord another
//! command had in the same scope moves, and the message bar says so, with a link
//! to that command). Undo takes back the last change.

mod keyboard;
mod keys_ui;
mod text;
mod tree;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use egui::{
    Align, Align2, Color32, CornerRadius, Event, FontId, Id, Key, Layout, Rect, Sense, Stroke, Ui,
    pos2, vec2,
};

use super::Workbench;
use crate::a11y::Accessible as _;
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::keymap::{Binding, Chord, Keymap, KeymapOverrides};
use crate::theme::{self, color};

pub use text::ZH;
use tree::{RowKey, Tree};

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

/// Name of the built-in preset (AE: "After Effects Default.txt"; BAE's own
/// name is neutral) and of the preset a change to it becomes.
const DEFAULT_PRESET: &str = "Default.txt";
const CUSTOM_PRESET: &str = "Custom.txt";

/// Height of a row of the command tree and the Key table.
const ROW_H: f32 = 22.0;
/// Height of a shortcut pill.
const PILL_H: f32 = 17.0;
/// Size AE opens the dialog at (points).
const AE_SIZE: egui::Vec2 = egui::vec2(1520.0, 887.0);

/// Colours of the dialog (AE 26, sampled; reference `colors`).
mod palette {
    use egui::Color32;
    pub const LIST_BG: Color32 = Color32::from_rgb(0x10, 0x10, 0x10);
    pub const HEADER: Color32 = Color32::from_rgb(0x2d, 0x2d, 0x2d);
    pub const SELECTED_ROW: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
    pub const ROW_LINE: Color32 = Color32::from_rgb(0x22, 0x22, 0x22);
    pub const FOCUS: Color32 = Color32::from_rgb(0x4e, 0xa9, 0xf6);
    pub const WARNING: Color32 = Color32::from_rgb(0xf2, 0xc2, 0x2e);
}

/// What the bottom bar says.
#[derive(Clone, Debug, Default, PartialEq)]
enum Message {
    /// ⓘ How to edit (AE's default message).
    #[default]
    Hint,
    /// Nothing (after Undo).
    Empty,
    /// ⚠ The shortcut just assigned was another command's (or is an
    /// application shortcut a panel shortcut now overrides).
    Conflict {
        kind: ConflictKind,
        chord: String,
        command: String,
        target: RowKey,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConflictKind {
    /// Another application command lost the shortcut.
    Application,
    /// Another command of the same panel lost it.
    Panel,
    /// An application command keeps it; this panel command wins in its panel.
    Override,
}

/// Which pill of the selected row has the keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PillFocus {
    /// An existing shortcut: a key pressed replaces it.
    Pill(usize),
    /// The empty pill after the others: a key pressed adds a shortcut.
    New,
}

/// The draft state before the last change (Undo: one level, like AE).
#[derive(Clone, Debug)]
struct Undo {
    overrides: KeymapOverrides,
    preset: String,
    custom: Option<KeymapOverrides>,
}

/// The dialog while it is open: the draft and the view state.
pub(super) struct Session {
    keymap_id: String,
    /// The keymap without the user's customization.
    base: Keymap,
    /// The draft customization and the keymap it makes.
    overrides: KeymapOverrides,
    keymap: Keymap,
    /// Shortcuts of every row of `keymap`.
    chords: HashMap<RowKey, Vec<Chord>>,
    /// Selected preset (file name as shown, English) and the Custom preset's
    /// content (kept while the built-in one is selected).
    preset: String,
    custom: Option<KeymapOverrides>,
    undo: Option<Undo>,
    tree: Tree,
    expanded: HashSet<String>,
    /// `None` = Application, else an index into `tree.scopes`.
    scope: Option<usize>,
    selected: Option<RowKey>,
    focus: Option<PillFocus>,
    message: Message,
    /// Scroll the tree so this node is at the top (next frame).
    reveal: Option<usize>,
    /// Search text the visible rows were computed for.
    visible: Option<(String, u64, Vec<usize>)>,
    /// Bumped on every change of the rows' expansion or shortcuts.
    generation: u64,
    /// First frame: the window goes above everything.
    just_opened: bool,
    /// The keyboard's keys, the modifier caps clicked, the key clicked and the
    /// Key table row selected; what the keys show (cached).
    caps: Vec<keyboard::KeyCap>,
    latched: text::Mods,
    key: Option<Key>,
    key_row: Option<text::Mods>,
    keys: keys_ui::KeyCache,
    /// The tree node of every command row.
    row_index: HashMap<RowKey, usize>,
}

impl Session {
    fn rebuild(&mut self) {
        self.keymap = self.base.with_overrides(&self.overrides);
        self.chords = tree::chords_by_row(&self.keymap);
        self.generation += 1;
    }

    fn chords_of(&self, key: &RowKey) -> &[Chord] {
        self.chords.get(key).map_or(&[], Vec::as_slice)
    }

    fn snapshot(&self) -> Undo {
        Undo {
            overrides: self.overrides.clone(),
            preset: self.preset.clone(),
            custom: self.custom.clone(),
        }
    }

    /// After a change: the built-in preset becomes Custom (AE), and the Custom
    /// preset follows the draft.
    fn changed(&mut self, before: Undo) {
        self.undo = Some(before);
        if self.preset == DEFAULT_PRESET {
            self.preset = CUSTOM_PRESET.into();
        }
        self.custom = Some(self.overrides.clone());
        self.rebuild();
    }

    /// Open the groups above node `i` and scroll to it.
    fn reveal_node(&mut self, i: usize) {
        for k in self.tree.ancestors(i) {
            self.expanded.insert(k);
        }
        self.reveal = Some(i);
        self.generation += 1;
    }

    fn select(&mut self, key: RowKey) {
        if self.selected.as_ref() != Some(&key) {
            self.focus = None;
        }
        self.selected = Some(key);
    }

    fn remove_chord(&mut self, row: &RowKey, chord: Chord) {
        let before = self.snapshot();
        self.overrides.unbind(chord, row.when.clone());
        self.changed(before);
        self.focus = (!self.chords_of(row).is_empty()).then_some(PillFocus::Pill(0));
        self.message = Message::Hint;
    }

    fn undo_change(&mut self) {
        if let Some(u) = self.undo.take() {
            self.overrides = u.overrides;
            self.preset = u.preset;
            self.custom = u.custom;
            self.rebuild();
            self.message = Message::Empty;
            if let Some(row) = &self.selected {
                self.focus = (!self.chords_of(row).is_empty()).then_some(PillFocus::Pill(0));
            }
        }
    }

    fn choose_preset(&mut self, name: &str) {
        if self.preset == name {
            return;
        }
        self.overrides = if name == DEFAULT_PRESET {
            KeymapOverrides::default()
        } else {
            self.custom.clone().unwrap_or_default()
        };
        self.preset = name.to_owned();
        self.undo = None;
        self.focus = None;
        self.message = Message::Hint;
        self.rebuild();
    }

    fn choose_scope(&mut self, scope: Option<usize>) {
        self.scope = scope;
        let group = match scope {
            Some(i) => self.tree.scope_group(i),
            None => Some(0),
        };
        if let Some(g) = group {
            let key = self.tree.nodes[g].key.clone();
            self.expanded.insert(key);
            self.reveal_node(g);
        }
    }
}

#[derive(Default)]
pub(super) struct ShortcutEditor {
    pub open: bool,
    /// Focus the search field on the next frame (just opened).
    pub focus_search: bool,
    pub search: String,
    pub(super) session: Option<Session>,
}

impl Workbench {
    /// Commands listed in the shortcut editor, one row per command and parameters:
    /// every bound command, the named variants of UI actions ("Back 10 Frames"), the
    /// other registered UI actions, and the model commands the shell supplied. Every
    /// row has its own name (#140): a UI action with variants is listed by them (not
    /// also bare), and a model command named like a row already listed (`comp.create`
    /// next to the New Composition action that runs it) is left out.
    pub fn shortcut_commands(&self) -> Vec<CommandEntry> {
        self.command_entries(&self.keymap, false)
    }

    /// [`Self::shortcut_commands`] for `keymap`; with `per_scope`, a command bound
    /// in several scopes is listed once per scope (the dialog's tree).
    fn command_entries(&self, keymap: &Keymap, per_scope: bool) -> Vec<CommandEntry> {
        let mut out: Vec<CommandEntry> = Vec::new();
        let english = |id: &str, params: &serde_json::Value| {
            let (title, arg) = self.command_title_parts(id, params);
            match arg {
                Some(arg) => format!("{title}: {arg}"),
                None => title,
            }
        };
        let push = |out: &mut Vec<CommandEntry>, e: CommandEntry| {
            if !out
                .iter()
                .any(|o| o.id == e.id && o.params == e.params && (!per_scope || o.panel == e.panel))
            {
                out.push(e);
            }
        };
        let actions = self.actions.snapshot();
        let variants = self.variants.snapshot();
        for b in &keymap.bindings {
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

    /// The rows of the dialog's tree: one per command, parameters and scope.
    pub(super) fn shortcut_commands_for(&self, keymap: &Keymap) -> Vec<CommandEntry> {
        self.command_entries(keymap, true)
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

    /// Opens the dialog on the active keymap (Edit > Keyboard Shortcuts).
    pub(super) fn open_shortcut_editor(&mut self) {
        if self.editor.open {
            return;
        }
        let keymap_id = self.keymap_id();
        let base = self.base_keymap(&keymap_id);
        let overrides = self.keymap_overrides(&keymap_id);
        let keymap = base.with_overrides(&overrides);
        let tree = self.shortcut_tree(&keymap);
        let custom = (!overrides.is_empty()).then(|| overrides.clone());
        let preset = if custom.is_some() {
            CUSTOM_PRESET
        } else {
            DEFAULT_PRESET
        };
        let chords = tree::chords_by_row(&keymap);
        let row_index = tree.row_index();
        self.editor = ShortcutEditor {
            open: true,
            focus_search: true,
            search: String::new(),
            session: Some(Session {
                keymap_id,
                base,
                overrides,
                keymap,
                chords,
                preset: preset.into(),
                custom,
                undo: None,
                tree,
                expanded: Tree::initially_expanded(),
                scope: None,
                selected: None,
                focus: None,
                message: Message::Hint,
                reveal: None,
                visible: None,
                generation: 0,
                just_opened: true,
                caps: keyboard::layout(),
                latched: text::Mods::NONE,
                key: None,
                key_row: None,
                keys: keys_ui::KeyCache::default(),
                row_index,
            }),
        };
    }

    /// OK: the draft becomes the keymap's customization (saved with the state).
    fn commit_shortcut_editor(&mut self) {
        if let Some(s) = self.editor.session.take() {
            let overrides = if s.preset == DEFAULT_PRESET {
                KeymapOverrides::default()
            } else {
                s.overrides
            };
            self.set_keymap_overrides(&s.keymap_id, overrides);
            self.save_requested = true;
        }
        self.editor = ShortcutEditor::default();
    }

    /// Cancel: everything changed in the dialog is dropped.
    fn cancel_shortcut_editor(&mut self) {
        self.editor = ShortcutEditor::default();
    }

    pub(super) fn shortcut_window(&mut self, ctx: &egui::Context) {
        if !self.editor.open {
            return;
        }
        let Some(mut s) = self.editor.session.take() else {
            self.editor.open = false;
            return;
        };
        let outcome = self.shortcut_dialog(ctx, &mut s);
        self.editor.session = Some(s);
        match outcome {
            Outcome::Ok => self.commit_shortcut_editor(),
            Outcome::Cancel => self.cancel_shortcut_editor(),
            Outcome::None => {}
        }
    }

    fn shortcut_dialog(&mut self, ctx: &egui::Context, s: &mut Session) -> Outcome {
        let id = Id::new("bae_ui.keyboard_shortcuts");
        // Keys first: a focused pill takes the key press; Esc and Enter are the
        // dialog's (Cancel / OK) otherwise.
        let mut outcome = self.shortcut_keys(ctx, s);
        block_background(ctx, id);
        let screen = ctx.content_rect();
        let size = vec2(
            AE_SIZE.x.min(screen.width() - 24.0),
            AE_SIZE.y.min(screen.height() - 24.0),
        );
        let mut open = true;
        let frame = egui::Frame::window(&ctx.global_style())
            .fill(crate::dialog::DIALOG_BG)
            .stroke(Stroke::new(1.0, color::DIVIDER))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(16, 12));
        egui::Window::new(egui::RichText::new(tr("Keyboard Shortcuts")).color(color::TEXT_STRONG))
            .id(id)
            .order(egui::Order::Foreground)
            .frame(frame)
            .open(&mut open)
            .collapsible(false)
            // AE's dialog appears and goes at once.
            .fade_in(false)
            .fade_out(false)
            .resizable(true)
            .default_size(size)
            .min_size([560.0, 360.0])
            .pivot(Align2::CENTER_CENTER)
            .default_pos(screen.center())
            .show(ctx, |ui| {
                // The window's own size, not its content's: the tree fills it.
                ui.set_min_size(ui.available_size());
                if let Some(o) = self.shortcut_editor_ui(ui, s) {
                    outcome = o;
                }
            });
        // The command being dragged follows the pointer.
        if let Some(row) = egui::DragAndDrop::payload::<RowKey>(ctx)
            && let Some(at) = ctx.pointer_latest_pos()
            && let Some(&i) = s.row_index.get(&*row)
        {
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, id.with("drag")));
            let galley = p.layout_no_wrap(
                s.tree.nodes[i].title.clone(),
                theme::body(),
                color::TEXT_STRONG,
            );
            let r = Rect::from_min_size(at + vec2(12.0, 8.0), galley.size() + vec2(12.0, 6.0));
            p.rect_filled(r, 3.0, palette::SELECTED_ROW);
            p.galley(r.min + vec2(6.0, 3.0), galley, color::TEXT_STRONG);
        }
        if std::mem::take(&mut s.just_opened) {
            ctx.move_to_top(egui::LayerId::new(egui::Order::Foreground, id));
        }
        if !open {
            outcome = Outcome::Cancel;
        }
        outcome
    }

    /// The frame's key presses: recorded into the focused pill, or Esc / Enter.
    fn shortcut_keys(&mut self, ctx: &egui::Context, s: &mut Session) -> Outcome {
        let typing = ctx
            .memory(|m| m.focused())
            .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
        if let Some(focus) = s.focus
            && !typing
        {
            let mut chord = None;
            let mut cancel = false;
            ctx.input_mut(|input| {
                let mods = input.modifiers;
                let chord_of = |key: Key, m: egui::Modifiers| Chord {
                    key,
                    command: m.command,
                    mac_ctrl: cfg!(target_os = "macos") && m.ctrl,
                    shift: m.shift,
                    alt: m.alt,
                };
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
                            chord = Some(chord_of(physical_key.unwrap_or(*key), *modifiers));
                        }
                        false
                    }
                    // egui-winit sends Cmd/Ctrl+C / X / V as these, without a key.
                    Event::Copy | Event::Cut | Event::Paste(_) => {
                        if chord.is_none() {
                            let key = match e {
                                Event::Copy => Key::C,
                                Event::Cut => Key::X,
                                _ => Key::V,
                            };
                            chord = Some(chord_of(key, mods));
                        }
                        false
                    }
                    Event::Key { .. } | Event::Text(_) => false,
                    _ => true,
                });
            });
            if cancel {
                // Esc gives up the pill (an empty one goes away).
                s.focus = None;
            } else if let Some(chord) = chord {
                self.assign_chord(s, focus, chord);
            }
            return Outcome::None;
        }
        let (escape, enter) = ctx.input(|i| {
            (
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Enter) && !i.modifiers.any(),
            )
        });
        if escape {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
            return Outcome::Cancel;
        }
        // Enter is OK unless the search field has the focus (it keeps typing).
        if enter && !typing {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
            return Outcome::Ok;
        }
        Outcome::None
    }

    /// A key pressed with a pill focused: replace that shortcut or add one.
    fn assign_chord(&mut self, s: &mut Session, focus: PillFocus, chord: Chord) {
        let Some(row) = s.selected.clone() else {
            s.focus = None;
            return;
        };
        let replacing = match focus {
            PillFocus::Pill(i) => s.chords_of(&row).get(i).copied(),
            PillFocus::New => None,
        };
        self.assign_to(s, &row, replacing, chord);
    }

    /// Give `row` the shortcut `chord` (instead of `replacing`): a chord another
    /// command has in the same scope moves, with AE's message.
    fn assign_to(&mut self, s: &mut Session, row: &RowKey, replacing: Option<Chord>, chord: Chord) {
        let row = row.clone();
        let own = s.chords_of(&row).to_vec();
        if own.contains(&chord) {
            // Already this command's: nothing changes.
            s.focus = (!own.is_empty()).then_some(PillFocus::Pill(0));
            return;
        }
        let when = row.when.clone();
        let params: serde_json::Value = if row.params.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(&row.params).unwrap_or_default()
        };
        // Who else has the chord: in the same scope it moves; an application
        // shortcut stays under a panel one.
        let taken = s
            .keymap
            .bindings
            .iter()
            .find(|b| b.chord == chord && b.when == when)
            .cloned();
        let shadowed = when.is_some().then(|| {
            s.keymap
                .bindings
                .iter()
                .find(|b| b.chord == chord && b.when.is_none())
                .cloned()
        });
        let before = s.snapshot();
        if let Some(old) = replacing {
            s.overrides.unbind(old, when.clone());
        }
        s.overrides.bind(Binding {
            chord,
            action: row.action.clone(),
            params,
            when: when.clone(),
        });
        s.changed(before);
        let text = text::chord_text(&chord);
        let row_of = |b: &Binding| RowKey {
            action: b.action.clone(),
            params: tree::params_key(&b.params),
            when: b.when.clone(),
        };
        s.message = if let Some(other) = taken {
            Message::Conflict {
                kind: if when.is_some() {
                    ConflictKind::Panel
                } else {
                    ConflictKind::Application
                },
                chord: text,
                command: self.row_title(s, &row_of(&other)),
                target: row_of(&other),
            }
        } else if let Some(Some(app)) = shadowed {
            Message::Conflict {
                kind: ConflictKind::Override,
                chord: text,
                command: self.row_title(s, &row_of(&app)),
                target: row_of(&app),
            }
        } else {
            Message::Hint
        };
        // AE: the search is cleared and the tree shows the changed command; the
        // focus goes back to its first shortcut.
        self.editor.search.clear();
        s.focus = Some(PillFocus::Pill(0));
        if let Some(i) = s.tree.find(&row) {
            s.reveal_node(i);
        }
    }

    /// The name a row has in the tree (or the command's title).
    fn row_title(&self, s: &Session, key: &RowKey) -> String {
        s.tree
            .find(key)
            .map(|i| s.tree.nodes[i].title.clone())
            .unwrap_or_else(|| {
                let params = serde_json::from_str(&key.params).unwrap_or_default();
                self.command_title(&key.action, &params)
            })
    }

    /// The dialog's contents; `Some` when OK or Cancel was clicked.
    fn shortcut_editor_ui(&mut self, ui: &mut Ui, s: &mut Session) -> Option<Outcome> {
        let mut outcome = None;
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        let label_w = 64.0;
        // Preset row.
        ui.horizontal(|ui| {
            right_label(ui, label_w, tr("Preset:"));
            let mut names = vec![DEFAULT_PRESET];
            if s.custom.is_some() || s.preset == CUSTOM_PRESET {
                names.push(CUSTOM_PRESET);
            }
            let current = tr(&s.preset).to_owned();
            let mut picked = None;
            combo(ui, "preset", tr("Preset:"), &current, 258.0, |ui| {
                for n in &names {
                    if check_item(ui, tr(n), *n == s.preset, true).clicked() {
                        picked = Some(*n);
                    }
                }
            });
            if let Some(n) = picked {
                s.choose_preset(n);
            }
            ui.add_space(14.0);
            // Preset files (Save As, Delete, Copy To Clipboard) come next.
            ui.add_enabled(false, button(tr("Save As...")));
            ui.add_enabled(false, button(tr("Delete")));
            ui.add_enabled(false, button(tr("Copy To Clipboard")));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(false, |ui| {
                    combo(ui, "layout", tr("Keyboard Layout:"), "en", 70.0, |_| {});
                });
                ui.label(egui::RichText::new(tr("Keyboard Layout:")).color(color::TEXT));
            });
        });
        // Commands (scope) row.
        ui.horizontal(|ui| {
            right_label(ui, label_w, tr("Commands:"));
            let current = match s.scope {
                Some(i) => tr(&s.tree.scopes[i].title).to_owned(),
                None => tr("Application").to_owned(),
            };
            let mut picked = None;
            combo(ui, "scope", tr("Commands:"), &current, 258.0, |ui| {
                if check_item(ui, tr("Application"), s.scope.is_none(), true).clicked() {
                    picked = Some(None);
                }
                ui.separator();
                for (i, scope) in s.tree.scopes.iter().enumerate() {
                    if check_item(ui, tr(&scope.title), s.scope == Some(i), true).clicked() {
                        picked = Some(Some(i));
                    }
                }
            });
            if let Some(scope) = picked {
                s.choose_scope(scope);
            }
        });
        // The keyboard (AE: 385 of the dialog's 887 points) and its legend.
        let width = ui.available_width();
        let keyboard_h = (ui.available_height() * 0.5).clamp(90.0, keyboard::SIZE.y);
        let held = ui.input(|i| i.modifiers);
        if let Some(action) = self.keyboard_ui(ui, s, width, keyboard_h) {
            self.keyboard_action(s, action, held);
        }
        self.legend_ui(ui);

        // Search, and "Key:" over the Key table.
        let buttons_w = 96.0;
        let tree_w = ((width - buttons_w - 24.0) * 0.575).max(240.0);
        ui.horizontal(|ui| {
            let before = self.editor.search.clone();
            let search = crate::widgets::search_field(ui, &mut self.editor.search, 220.0);
            if std::mem::take(&mut self.editor.focus_search) {
                search.request_focus();
            }
            if search.gained_focus() {
                s.focus = None;
            }
            if before != self.editor.search {
                s.visible = None;
            }
            let x = ui.min_rect().left() + tree_w + 12.0;
            let at = pos2(x, ui.min_rect().center().y);
            ui.painter().text(
                at,
                Align2::LEFT_CENTER,
                self.key_label(s),
                theme::body(),
                color::TEXT_STRONG,
            );
        });

        // The tree (left) and the Key table with Undo / Clear (right).
        let bottom_h = 30.0;
        let body_h = (ui.available_height() - bottom_h - 8.0).max(80.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(tree_w, body_h), Layout::top_down(Align::Min), |ui| {
                ui.set_min_size(vec2(tree_w, body_h));
                self.command_tree_ui(ui, s);
            });
            ui.add_space(4.0);
            let table_w = (ui.available_width() - buttons_w - 12.0).max(120.0);
            ui.allocate_ui_with_layout(vec2(table_w, body_h), Layout::top_down(Align::Min), |ui| {
                ui.set_min_size(vec2(table_w, body_h));
                if let Some((row, chord)) = self.key_table_ui(ui, s) {
                    s.remove_chord(&row, chord);
                }
            });
            ui.allocate_ui_with_layout(
                vec2(buttons_w, body_h),
                Layout::bottom_up(Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 10.0;
                    let pill = match (s.focus, &s.selected) {
                        (Some(PillFocus::Pill(i)), Some(row)) => {
                            s.chords_of(row).get(i).copied().map(|c| (row.clone(), c))
                        }
                        _ => None,
                    };
                    let clear = ui.add_enabled(
                        pill.is_some(),
                        button(tr("Clear")).min_size(vec2(buttons_w, 22.0)),
                    );
                    if clear.clicked()
                        && let Some((row, chord)) = pill
                    {
                        s.remove_chord(&row, chord);
                    }
                    let undo = ui.add_enabled(
                        s.undo.is_some(),
                        button(tr("Undo")).min_size(vec2(buttons_w, 22.0)),
                    );
                    if undo.clicked() {
                        s.undo_change();
                    }
                },
            );
        });

        // Message bar, Cancel, OK.
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let bar_w = (ui.available_width() - 180.0).max(100.0);
            ui.allocate_ui_with_layout(
                vec2(bar_w, 24.0),
                Layout::left_to_right(Align::Center),
                |ui| self.message_ui(ui, s),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mac = cfg!(target_os = "macos");
                let order: [bool; 2] = if mac { [true, false] } else { [false, true] };
                for is_ok in order {
                    if is_ok {
                        let b = egui::Button::new(
                            egui::RichText::new(tr("OK")).color(color::TEXT_STRONG),
                        )
                        .fill(color::ACCENT_DIM)
                        .min_size(vec2(64.0, 22.0));
                        if ui.add(b).clicked() {
                            outcome = Some(Outcome::Ok);
                        }
                    } else if ui
                        .add(button(tr("Cancel")).min_size(vec2(64.0, 22.0)))
                        .clicked()
                    {
                        outcome = Some(Outcome::Cancel);
                    }
                }
            });
        });
        outcome
    }

    fn message_ui(&mut self, ui: &mut Ui, s: &mut Session) {
        match s.message.clone() {
            Message::Empty => {}
            Message::Hint => {
                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                paint_info(ui.painter(), r);
                ui.label(
                    egui::RichText::new(tr(
                        "Create new or edit existing shortcut by clicking in the \"Shortcut\" column",
                    ))
                    .color(color::TEXT),
                );
            }
            Message::Conflict {
                kind,
                chord,
                command,
                target,
            } => {
                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                paint_warning(ui.painter(), r);
                let template = tr(match kind {
                    ConflictKind::Application => {
                        "The shortcut: {chord} is already in use by another Application command: {command}. That command will no longer have a shortcut."
                    }
                    ConflictKind::Panel => {
                        "The shortcut: {chord} is already in use by another command of this panel: {command}. That command will no longer have a shortcut."
                    }
                    ConflictKind::Override => {
                        "The shortcut: {chord} is already in use by the Application command: {command}. That command will be overridden by this panel command when the panel has focus."
                    }
                })
                .replace("{chord}", &chord);
                let (head, tail) = template.split_once("{command}").unwrap_or((&template, ""));
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(egui::RichText::new(head).color(color::TEXT));
                if crate::widgets::link(ui, &command).clicked() {
                    // Select the command that lost (or keeps) the shortcut.
                    self.editor.search.clear();
                    s.select(target.clone());
                    if let Some(i) = s.tree.find(&target) {
                        s.reveal_node(i);
                    }
                }
                ui.label(egui::RichText::new(tail).color(color::TEXT));
            }
        }
    }

    fn command_tree_ui(&self, ui: &mut Ui, s: &mut Session) {
        // Rows touch (show_rows adds the spacing to the row height).
        ui.spacing_mut().item_spacing.y = 0.0;
        let width = ui.available_width();
        let name_w = (width * 0.4).clamp(180.0, 360.0);
        // Header.
        let (head, _) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
        let p = ui.painter();
        p.text(
            pos2(head.left() + 16.0, head.center().y),
            Align2::LEFT_CENTER,
            tr("Command"),
            theme::body(),
            color::TEXT,
        );
        p.text(
            pos2(head.left() + name_w + 8.0, head.center().y),
            Align2::LEFT_CENTER,
            tr("Shortcut"),
            theme::body(),
            color::TEXT,
        );
        let query = self.editor.search.trim().to_owned();
        let stale = s
            .visible
            .as_ref()
            .is_none_or(|(q, g, _)| *q != query || *g != s.generation);
        if stale {
            let rows = s.tree.visible(&s.expanded, &query, &s.chords);
            s.visible = Some((query.clone(), s.generation, rows));
        }
        let rows = s
            .visible
            .as_ref()
            .map(|(_, _, r)| r.clone())
            .unwrap_or_default();
        let list = Rect::from_min_size(ui.cursor().min, vec2(width, ui.available_height()));
        ui.painter().rect_filled(list, 2.0, palette::LIST_BG);
        let mut area = egui::ScrollArea::vertical()
            .id_salt("shortcut_tree")
            .auto_shrink([false, false]);
        if let Some(target) = s.reveal.take()
            && let Some(at) = rows.iter().position(|&i| i == target)
        {
            area = area.vertical_scroll_offset(at as f32 * ROW_H);
        }
        let mut clicked: Option<TreeClick> = None;
        area.show_rows(ui, ROW_H, rows.len(), |ui, range| {
            for &i in &rows[range] {
                if let Some(c) = self.tree_row(ui, s, i, name_w) {
                    clicked = Some(c);
                }
            }
        });
        match clicked {
            Some(TreeClick::Toggle(key)) => {
                if !s.expanded.remove(&key) {
                    s.expanded.insert(key);
                }
                s.generation += 1;
            }
            Some(TreeClick::Select(key)) => {
                s.select(key);
            }
            Some(TreeClick::Cell(key)) => {
                // First click: select the row, focus its first shortcut (an
                // empty pill when it has none); again on the empty part: add one.
                let has = !s.chords_of(&key).is_empty();
                if s.selected.as_ref() == Some(&key) {
                    s.focus = Some(PillFocus::New);
                } else {
                    s.select(key);
                    s.focus = Some(if has {
                        PillFocus::Pill(0)
                    } else {
                        PillFocus::New
                    });
                }
                ui.memory_mut(|m| m.stop_text_input());
            }
            Some(TreeClick::Pill(key, i)) => {
                s.select(key);
                s.focus = Some(PillFocus::Pill(i));
                ui.memory_mut(|m| m.stop_text_input());
            }
            Some(TreeClick::Remove(key, chord)) => {
                s.select(key.clone());
                s.remove_chord(&key, chord);
            }
            None => {}
        }
    }

    /// One row of the tree; what was clicked in it.
    fn tree_row(&self, ui: &mut Ui, s: &Session, i: usize, name_w: f32) -> Option<TreeClick> {
        let node = &s.tree.nodes[i];
        let width = ui.available_width();
        // A command can be dragged onto a key of the keyboard.
        let sense = if node.is_group() {
            Sense::click()
        } else {
            Sense::click_and_drag()
        };
        let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), sense);
        let indent = 6.0 + node.depth as f32 * 16.0;
        let font = theme::body();
        let Some(entry) = &node.entry else {
            // A group: chevron and title; a click opens or closes it.
            let open = s.expanded.contains(&node.key) || !self.editor.search.trim().is_empty();
            let resp = resp.labeled_toggle(egui::WidgetType::CollapsingHeader, &node.title, open);
            if ui.is_rect_visible(rect) {
                let p = ui.painter();
                p.rect_filled(rect, 0.0, palette::HEADER);
                let chev = Rect::from_center_size(
                    pos2(rect.left() + indent + 6.0, rect.center().y),
                    vec2(12.0, 12.0),
                );
                let icon = if open {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                };
                icons::paint(p, chev, icon, color::ICON);
                p.text(
                    pos2(rect.left() + indent + 18.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    &node.title,
                    font,
                    color::TEXT_STRONG,
                );
            }
            return resp.clicked().then(|| TreeClick::Toggle(node.key.clone()));
        };
        let key = RowKey::of(entry);
        resp.dnd_set_drag_payload(key.clone());
        let selected = s.selected.as_ref() == Some(&key);
        let chords = s.chords_of(&key);
        let cell = Rect::from_min_max(pos2(rect.left() + name_w, rect.top()), rect.max);
        let shown = text::chords_text(chords);
        let resp = resp.labeled_value(egui::WidgetType::SelectableLabel, &node.title, &shown);
        let mut out = None;
        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            if selected {
                p.rect_filled(rect, 0.0, palette::SELECTED_ROW);
            }
            p.hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, palette::ROW_LINE),
            );
            let name = Rect::from_min_max(
                pos2(rect.left() + indent + 18.0, rect.top()),
                pos2(cell.left() - 8.0, rect.bottom()),
            );
            clipped(p, name, &node.title, font.clone(), color::TEXT_STRONG);
        }
        if selected {
            // Pills: each shortcut with ×, the focused one ringed; an empty
            // pill when a new shortcut is being added.
            let mut x = cell.left() + 2.0;
            for (j, chord) in chords.iter().enumerate() {
                let label = text::chord_text(chord);
                let focused = s.focus == Some(PillFocus::Pill(j));
                let (pill, close) = pill_rects(ui, &label, x, rect.center().y);
                let pid = ui.id().with(("pill", i, j));
                let presp = ui.interact(pill, pid, Sense::click());
                let cresp = ui
                    .interact(close, pid.with("x"), Sense::click())
                    .labeled(egui::WidgetType::Button, tr("Clear"));
                paint_pill(ui, pill, close, &label, focused, cresp.hovered());
                if cresp.clicked() {
                    out = Some(TreeClick::Remove(key.clone(), *chord));
                } else if presp.clicked() {
                    out = Some(TreeClick::Pill(key.clone(), j));
                }
                x = pill.right() + 6.0;
            }
            if s.focus == Some(PillFocus::New) {
                let (pill, close) = pill_rects(ui, "", x, rect.center().y);
                paint_pill(ui, pill, close, "", true, false);
            }
        } else if ui.is_rect_visible(rect) {
            let area = cell.shrink2(vec2(2.0, 0.0));
            clipped(ui.painter(), area, &shown, font, color::TEXT);
        }
        if out.is_none() && resp.clicked() {
            let in_cell = resp
                .interact_pointer_pos()
                .is_some_and(|p| p.x >= cell.left());
            out = Some(if in_cell {
                TreeClick::Cell(key)
            } else {
                TreeClick::Select(key)
            });
        }
        out
    }

    /// The keymap `id` is made of without the user's customization: the
    /// registered keymap plus the modules' extensions.
    fn base_keymap(&self, id: &str) -> Keymap {
        self.extended_keymap(id)
    }
}

/// What a click in the tree did.
enum TreeClick {
    Toggle(String),
    Select(RowKey),
    Cell(RowKey),
    Pill(RowKey, usize),
    Remove(RowKey, Chord),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    None,
    Ok,
    Cancel,
}

/// A label right-aligned in a column of `width` ("Preset:").
fn right_label(ui: &mut Ui, width: f32, text: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
    ui.painter().text(
        r.right_center(),
        Align2::RIGHT_CENTER,
        text,
        theme::body(),
        color::TEXT,
    );
}

fn button(text: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(text).color(color::TEXT_STRONG))
        .min_size(vec2(78.0, 22.0))
}

/// A dark menu field showing `current`; `items` fills its popup.
fn combo(
    ui: &mut Ui,
    id_salt: &str,
    label: &str,
    current: &str,
    width: f32,
    items: impl FnOnce(&mut Ui),
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click());
    let resp = resp.labeled_value(egui::WidgetType::ComboBox, label, current);
    if ui.is_rect_visible(rect) {
        let enabled = ui.is_enabled();
        let p = ui.painter();
        let bg = if resp.hovered() {
            color::FIELD_HOVER
        } else {
            color::DROPDOWN
        };
        p.rect_filled(rect, 3.0, bg);
        let fg = if enabled {
            color::TEXT_STRONG
        } else {
            color::TEXT_DISABLED
        };
        clipped(
            p,
            Rect::from_min_max(rect.min + vec2(8.0, 0.0), rect.max - vec2(24.0, 0.0)),
            current,
            theme::body(),
            fg,
        );
        let chev = Rect::from_center_size(
            pos2(rect.right() - 12.0, rect.center().y),
            egui::Vec2::splat(14.0),
        );
        icons::paint(p, chev, Icon::ChevronDown, fg);
    }
    egui::Popup::menu(&resp)
        .id(ui.id().with(id_salt))
        .width(width)
        .show(items);
    resp
}

/// A menu item with a check mark column (AE's popup menus); a disabled one is
/// greyed and does not react.
fn check_item(ui: &mut Ui, text: &str, checked: bool, enabled: bool) -> egui::Response {
    let width = ui.available_width().max(120.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 20.0), sense);
    let resp = resp.labeled_toggle(egui::WidgetType::SelectableLabel, text, checked);
    let p = ui.painter();
    if enabled && resp.hovered() {
        p.rect_filled(rect, 2.0, color::ACCENT_DIM);
    }
    if checked {
        let r = Rect::from_center_size(pos2(rect.left() + 10.0, rect.center().y), vec2(10.0, 10.0));
        crate::widgets::paint_check(p, r, color::TEXT_STRONG);
    }
    let fg = if enabled {
        color::TEXT_STRONG
    } else {
        color::TEXT_DISABLED
    };
    p.text(
        pos2(rect.left() + 24.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        theme::body(),
        fg,
    );
    resp
}

/// Text cut at the right edge of `rect` with "…".
fn clipped(p: &egui::Painter, rect: Rect, text: &str, font: FontId, color: Color32) {
    if rect.width() > 4.0 {
        crate::widgets::clipped_text(p, rect.left_center(), text, font, color, rect.width());
    }
}

/// The rectangles of a pill (whole, × button) starting at `x`.
fn pill_rects(ui: &Ui, label: &str, x: f32, cy: f32) -> (Rect, Rect) {
    let w = if label.is_empty() {
        0.0
    } else {
        ui.fonts_mut(|f| {
            f.layout_no_wrap(label.to_owned(), theme::body(), color::TEXT)
                .size()
                .x
        }) + 6.0
    };
    let pill = Rect::from_min_size(pos2(x, cy - PILL_H / 2.0), vec2(w + 24.0, PILL_H));
    let close = Rect::from_center_size(pos2(pill.right() - 9.0, cy), vec2(12.0, 12.0));
    (pill, close)
}

fn paint_pill(ui: &Ui, pill: Rect, close: Rect, label: &str, focused: bool, close_hover: bool) {
    let p = ui.painter();
    p.rect_filled(pill, CornerRadius::same(4), palette::LIST_BG);
    p.rect_stroke(
        pill,
        CornerRadius::same(4),
        Stroke::new(1.0, color::TEXT_DIM),
        egui::StrokeKind::Inside,
    );
    if focused {
        p.rect_stroke(
            pill.expand(2.0),
            CornerRadius::same(5),
            Stroke::new(1.5, palette::FOCUS),
            egui::StrokeKind::Inside,
        );
    }
    if !label.is_empty() {
        p.text(
            pos2(pill.left() + 6.0, pill.center().y),
            Align2::LEFT_CENTER,
            label,
            theme::body(),
            color::TEXT_STRONG,
        );
    }
    let c = if close_hover {
        color::TEXT_STRONG
    } else {
        color::TEXT
    };
    crate::widgets::paint_cross(p, close.shrink(1.0), c);
}

/// AE's ⓘ before the hint.
fn paint_info(p: &egui::Painter, r: Rect) {
    let c = r.center();
    p.circle_stroke(c, 7.5, Stroke::new(1.2, color::TEXT));
    p.circle_filled(c - vec2(0.0, 3.5), 1.1, color::TEXT);
    p.line_segment(
        [c - vec2(0.0, 1.0), c + vec2(0.0, 4.0)],
        Stroke::new(1.4, color::TEXT),
    );
}

/// AE's yellow ⚠ before a conflict message.
fn paint_warning(p: &egui::Painter, r: Rect) {
    let c = r.center();
    let pts = vec![
        pos2(c.x, r.top() + 1.0),
        pos2(r.right() - 0.5, r.bottom() - 1.5),
        pos2(r.left() + 0.5, r.bottom() - 1.5),
    ];
    p.add(egui::Shape::convex_polygon(
        pts,
        palette::WARNING,
        Stroke::NONE,
    ));
    let dark = Color32::from_rgb(0x1d, 0x1d, 0x1d);
    p.line_segment(
        [pos2(c.x, r.top() + 6.0), pos2(c.x, r.bottom() - 6.5)],
        Stroke::new(1.6, dark),
    );
    p.circle_filled(pos2(c.x, r.bottom() - 4.0), 1.0, dark);
}

/// An invisible layer over the window, under the dialog, that eats clicks meant
/// for the panels behind it (AE's dialog is modal).
fn block_background(ctx: &egui::Context, id: Id) {
    let rect = ctx.content_rect();
    egui::Area::new(id.with("blocker"))
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .interactable(true)
        .show(ctx, |ui| {
            ui.allocate_rect(rect, Sense::click_and_drag());
        });
}
