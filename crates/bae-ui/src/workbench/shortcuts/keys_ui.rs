//! The visual keyboard, its legend and the Key table of the Keyboard Shortcuts
//! dialog (AE: clicking a modifier cap holds it, clicking a key lists all its
//! combinations, dropping a command on a key binds it with the active modifiers).

use std::collections::HashMap;

use egui::text::{LayoutJob, TextWrapping};
use egui::{Align2, Color32, FontId, Key, Rect, Sense, Stroke, TextFormat, Ui, pos2, vec2};

use super::keyboard::{self, Cap, KeyCap, KeyLook, Modifier, colors};
use super::text::{self, Mods};
use super::tree::{self, RowKey};
use super::{Session, Workbench, palette};
use crate::a11y::Accessible as _;
use crate::i18n::tr;
use crate::keymap::{Binding, Chord};
use crate::theme::{self, color};

/// What the keyboard and the Key table show for the active modifiers and scope,
/// rebuilt when they or the draft change.
#[derive(Default)]
pub(super) struct KeyCache {
    stamp: Option<(Mods, Option<usize>, u64)>,
    pub(super) looks: HashMap<Key, KeyLook>,
}

/// What a click on the keyboard or a drop on it asks for.
pub(super) enum KeyboardAction {
    ToggleModifier(Modifier),
    SelectKey(Key),
    Drop(RowKey, Chord),
}

impl Session {
    /// The modifiers the keyboard shows: the caps clicked plus the keys held.
    pub(super) fn shown_mods(&self, held: egui::Modifiers) -> Mods {
        let held = Mods {
            command: held.command,
            alt: held.alt,
            shift: held.shift,
            ctrl: cfg!(target_os = "macos") && held.ctrl,
        };
        Mods {
            command: self.latched.command || held.command,
            alt: self.latched.alt || held.alt,
            shift: self.latched.shift || held.shift,
            ctrl: self.latched.ctrl || held.ctrl,
        }
    }

    /// Whether a binding is shown in the active scope (all of them in
    /// Application; only the scope's own in a panel scope).
    fn in_scope(&self, b: &Binding) -> bool {
        match self.scope {
            None => true,
            Some(i) => b
                .when
                .as_deref()
                .is_some_and(|w| self.tree.scopes[i].kinds.iter().any(|k| k == w)),
        }
    }

    /// The bindings of `chord` the active scope shows: application ones first.
    pub(super) fn bindings_on(&self, chord: &Chord) -> Vec<&Binding> {
        let mut v: Vec<&Binding> = self
            .keymap
            .bindings
            .iter()
            .filter(|b| b.chord == *chord && self.in_scope(b))
            .collect();
        v.sort_by_key(|b| b.when.is_some());
        v.dedup_by(|a, b| a.action == b.action && a.params == b.params && a.when == b.when);
        v
    }

    /// The name of a binding's command as the tree shows it.
    pub(super) fn binding_title(&self, b: &Binding) -> String {
        let key = row_key(b);
        self.row_index
            .get(&key)
            .map(|&i| self.tree.nodes[i].title.clone())
            .unwrap_or_else(|| b.action.clone())
    }

    pub(super) fn refresh_looks(&mut self, mods: Mods) {
        let stamp = (mods, self.scope, self.generation);
        if self.keys.stamp == Some(stamp) {
            return;
        }
        let mut looks: HashMap<Key, KeyLook> = HashMap::new();
        for b in &self.keymap.bindings {
            if Mods::of(&b.chord) != mods || !self.in_scope(b) {
                continue;
            }
            let look = looks.entry(b.chord.key).or_default();
            if b.when.is_none() {
                if look.application.is_none() {
                    look.application = Some(self.binding_title(b));
                }
            } else if look.panel.is_none() {
                look.panel = Some(self.binding_title(b));
            }
        }
        self.keys = KeyCache {
            stamp: Some(stamp),
            looks,
        };
    }

    /// The tooltip of a key: one line per command, "Scope: Command (Path → Command)".
    pub(super) fn key_tooltip(&self, chord: &Chord) -> Vec<String> {
        self.bindings_on(chord)
            .into_iter()
            .map(|b| {
                let scope = match &b.when {
                    None => tr("Application").to_owned(),
                    Some(w) => self
                        .tree
                        .scopes
                        .iter()
                        .find(|s| s.kinds.iter().any(|k| k == w))
                        .map_or_else(|| w.clone(), |s| tr(&s.title).to_owned()),
                };
                let title = self.binding_title(b);
                let path = self
                    .row_index
                    .get(&row_key(b))
                    .map(|&i| {
                        let mut parts: Vec<String> = self
                            .tree
                            .ancestor_nodes(i)
                            .into_iter()
                            .rev()
                            .map(|j| self.tree.nodes[j].title.clone())
                            .collect();
                        parts.push(title.clone());
                        parts.join(" \u{2192} ")
                    })
                    .unwrap_or_else(|| title.clone());
                format!("{scope}: {title} ({path})")
            })
            .collect()
    }
}

pub(super) fn row_key(b: &Binding) -> RowKey {
    RowKey {
        action: b.action.clone(),
        params: tree::params_key(&b.params),
        when: b.when.clone(),
    }
}

impl Workbench {
    /// The keyboard, scaled to `width` (and at most `max_h` high).
    pub(super) fn keyboard_ui(
        &self,
        ui: &mut Ui,
        s: &mut Session,
        width: f32,
        max_h: f32,
    ) -> Option<KeyboardAction> {
        let held = ui.input(|i| i.modifiers);
        let mods = s.shown_mods(held);
        s.refresh_looks(mods);
        let scale = (width / keyboard::SIZE.x).min(max_h / keyboard::SIZE.y);
        let (area, _) = ui.allocate_exact_size(keyboard::SIZE * scale, Sense::hover());
        let name_font = FontId::proportional((11.5 * scale).clamp(8.0, 11.5));
        let panel_scope = s.scope.is_some();
        let mut action = None;
        for (i, kc) in s.caps.iter().enumerate() {
            let r = Rect::from_min_size(
                area.min + vec2(kc.x, kc.y) * scale,
                vec2(kc.w, kc.h) * scale,
            );
            let key = kc.key();
            let sense = match kc.cap {
                Cap::Modifier(_) => Sense::click(),
                _ if key.is_some() => Sense::click(),
                _ => Sense::hover(),
            };
            let id = ui.id().with(("key", i));
            let resp = ui.interact(r, id, sense);
            let name = kc.name();
            let look = key
                .and_then(|k| s.keys.looks.get(&k))
                .cloned()
                .unwrap_or_default();
            let resp = match kc.cap {
                Cap::Modifier(m) => {
                    resp.labeled_toggle(egui::WidgetType::Button, &name, m.is_on(mods))
                }
                _ => resp.labeled_value(
                    egui::WidgetType::Button,
                    &name,
                    look.label(panel_scope).unwrap_or(""),
                ),
            };
            if ui.is_rect_visible(r) {
                paint_cap(
                    ui,
                    r,
                    kc,
                    &look,
                    mods,
                    panel_scope,
                    s.key == key && key.is_some(),
                );
                let p = ui.painter();
                let pad = 5.0 * scale;
                let inner = r.width() - 2.0 * pad;
                let name_galley = p.layout_job(cap_label(&name, name_font.clone(), inner, 2));
                if let Some(label) = look.label(panel_scope) {
                    // Two lines, one when the key's name needs two (Page Down).
                    let rows = if name_galley.rows.len() > 1 { 1 } else { 2 };
                    let galley = p.layout_job(cap_label(label, name_font.clone(), inner, rows));
                    p.galley(r.min + vec2(pad, pad * 0.8), galley, Color32::WHITE);
                }
                let pos = pos2(
                    r.left() + pad,
                    r.bottom() - pad * 0.6 - name_galley.size().y,
                );
                p.galley(pos, name_galley, Color32::from_rgb(0x1a, 0x1a, 0x1a));
                if resp.dnd_hover_payload::<RowKey>().is_some() && key.is_some() {
                    p.rect_stroke(
                        r,
                        3.0,
                        Stroke::new(2.0, colors::SELECTED),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            let Some(k) = key else {
                if let Cap::Modifier(m) = kc.cap
                    && resp.clicked()
                {
                    action = Some(KeyboardAction::ToggleModifier(m));
                }
                continue;
            };
            let chord = mods.chord(k);
            let resp = resp.on_hover_ui(|ui| {
                for line in s.key_tooltip(&chord) {
                    ui.label(line);
                }
            });
            if let Some(row) = resp.dnd_release_payload::<RowKey>() {
                action = Some(KeyboardAction::Drop((*row).clone(), chord));
            } else if resp.clicked() {
                action = Some(KeyboardAction::SelectKey(k));
            }
        }
        action
    }

    /// AE's legend under the keyboard.
    pub(super) fn legend_ui(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for (c, text) in [
                (
                    colors::APPLICATION,
                    "Application shortcuts are active regardless of panel focus",
                ),
                (
                    colors::PANEL,
                    "Panel shortcuts override application shortcuts when the panel has focus",
                ),
                (colors::PRESSED, "Modifier key pressed"),
            ] {
                let (r, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                ui.painter().rect_filled(r, 1.0, c);
                ui.label(egui::RichText::new(tr(text)).color(color::TEXT));
                ui.add_space(14.0);
            }
        });
    }

    /// "Key: K (multiple commands assigned)" and the table of the selected key's
    /// combinations. Returns a binding to remove (× on a pill).
    pub(super) fn key_table_ui(&self, ui: &mut Ui, s: &mut Session) -> Option<(RowKey, Chord)> {
        ui.spacing_mut().item_spacing.y = 0.0;
        let width = ui.available_width();
        let mods_w = (width * 0.22).clamp(70.0, 130.0);
        let (head, _) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
        let p = ui.painter();
        p.text(
            pos2(head.left() + 16.0, head.center().y),
            Align2::LEFT_CENTER,
            tr("Modifiers"),
            theme::body(),
            color::TEXT,
        );
        p.text(
            pos2(head.left() + mods_w + 8.0, head.center().y),
            Align2::LEFT_CENTER,
            tr("Command"),
            theme::body(),
            color::TEXT,
        );
        let list = Rect::from_min_size(ui.cursor().min, vec2(width, ui.available_height()));
        ui.painter().rect_filled(list, 2.0, palette::LIST_BG);
        let rows = Mods::table();
        let mut remove = None;
        let mut select = None;
        egui::ScrollArea::vertical()
            .id_salt("shortcut_key_table")
            .auto_shrink([false, false])
            .show_rows(ui, super::ROW_H, rows.len(), |ui, range| {
                for m in &rows[range] {
                    let (rect, resp) = ui.allocate_exact_size(
                        vec2(ui.available_width(), super::ROW_H),
                        Sense::click(),
                    );
                    let bindings: Vec<Binding> = s
                        .key
                        .map(|k| s.bindings_on(&m.chord(k)).into_iter().cloned().collect())
                        .unwrap_or_default();
                    let titles: Vec<String> = bindings.iter().map(|b| s.binding_title(b)).collect();
                    let selected = s.key.is_some() && s.key_row == Some(*m);
                    let resp = resp.labeled_value(
                        egui::WidgetType::SelectableLabel,
                        &m.text(),
                        &titles.join(", "),
                    );
                    if resp.clicked() {
                        select = Some(*m);
                    }
                    let p = ui.painter();
                    if selected {
                        p.rect_filled(rect, 0.0, palette::SELECTED_ROW);
                    }
                    p.hline(
                        rect.x_range(),
                        rect.bottom() - 0.5,
                        Stroke::new(1.0, palette::ROW_LINE),
                    );
                    p.text(
                        pos2(rect.left() + 16.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        m.text(),
                        theme::body(),
                        color::TEXT,
                    );
                    let cell = Rect::from_min_max(pos2(rect.left() + mods_w, rect.top()), rect.max);
                    if selected {
                        let mut x = cell.left() + 2.0;
                        for (j, (b, title)) in bindings.iter().zip(&titles).enumerate() {
                            let (pill, close) = super::pill_rects(ui, title, x, rect.center().y);
                            let id = ui.id().with(("key_pill", j));
                            let c = ui
                                .interact(close, id, Sense::click())
                                .labeled(egui::WidgetType::Button, tr("Clear"));
                            super::paint_pill(ui, pill, close, title, false, c.hovered());
                            if c.clicked() {
                                remove = Some((row_key(b), b.chord));
                            }
                            x = pill.right() + 6.0;
                        }
                    } else {
                        super::clipped(
                            ui.painter(),
                            cell.shrink2(vec2(2.0, 0.0)),
                            &titles.join(", "),
                            theme::body(),
                            color::TEXT_STRONG,
                        );
                    }
                }
            });
        if let Some(m) = select {
            s.key_row = Some(m);
        }
        remove
    }

    /// "Key: <name>" (with AE's note when more than one command uses the key).
    pub(super) fn key_label(&self, s: &Session) -> String {
        let Some(k) = s.key else {
            return tr("Key: ").trim_end().to_owned();
        };
        let name = s
            .caps
            .iter()
            .find(|c| c.key() == Some(k))
            .map_or_else(|| text::key_name(k), KeyCap::name);
        let count: usize = Mods::table()
            .into_iter()
            .map(|m| s.bindings_on(&m.chord(k)).len())
            .sum();
        let mut out = format!("{}{name}", tr("Key: "));
        if count > 1 {
            out.push_str(tr(" (multiple commands assigned)"));
        }
        out
    }

    /// A keyboard click or drop.
    pub(super) fn keyboard_action(
        &mut self,
        s: &mut Session,
        action: KeyboardAction,
        held: egui::Modifiers,
    ) {
        match action {
            KeyboardAction::ToggleModifier(m) => {
                m.toggle(&mut s.latched);
            }
            KeyboardAction::SelectKey(k) => {
                s.key = Some(k);
                s.key_row = Some(s.shown_mods(held));
            }
            KeyboardAction::Drop(row, chord) => {
                s.select(row.clone());
                self.assign_to(s, &row, None, chord);
                // Nothing waits for a key after a drop.
                s.focus = None;
                s.key = Some(chord.key);
                s.key_row = Some(Mods::of(&chord));
            }
        }
    }
}

/// The key cap: AE's colours, split along the diagonal when a key has an
/// application and a panel command (or only a panel one), blue for a pressed
/// modifier, an outline on the selected key.
fn paint_cap(
    ui: &Ui,
    r: Rect,
    kc: &KeyCap,
    look: &KeyLook,
    mods: Mods,
    panel_scope: bool,
    selected: bool,
) {
    let p = ui.painter();
    let rounding = 2.0;
    let (upper, lower) = match kc.cap {
        Cap::Modifier(m) if m.is_on(mods) => (colors::PRESSED, colors::PRESSED),
        Cap::Modifier(_) | Cap::Inert => (colors::MODIFIER, colors::MODIFIER),
        _ if panel_scope => {
            let c = if look.panel.is_some() {
                colors::PANEL
            } else {
                colors::UNASSIGNED
            };
            (c, c)
        }
        _ => look.fills(),
    };
    p.rect_filled(r, rounding, upper);
    if lower != upper {
        let pts = vec![r.left_bottom(), r.right_bottom(), r.right_top()];
        p.add(egui::Shape::convex_polygon(pts, lower, Stroke::NONE));
    }
    if selected {
        p.rect_stroke(
            r.expand(1.0),
            rounding,
            Stroke::new(1.5, colors::SELECTED),
            egui::StrokeKind::Inside,
        );
    }
}

/// A key cap's text: wrapped to `rows` lines, cut with "…".
fn cap_label(text: &str, font: FontId, width: f32, rows: usize) -> LayoutJob {
    let mut job =
        LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, Color32::WHITE));
    job.wrap = TextWrapping {
        max_width: width.max(8.0),
        max_rows: rows,
        break_anywhere: false,
        overflow_character: Some('\u{2026}'),
    };
    job
}
