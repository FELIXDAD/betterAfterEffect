//! The Preferences dialog (AE: Edit > Preferences on Windows, After Effects >
//! Settings on macOS): a movable modal window with the page list on the left and
//! the current page on the right, built from [`bae_prefs::schema`]. Edits go to a
//! draft; OK hands it back to the host to store, Cancel drops it.
//!
//! The host opens it for the `app.settings` action (parameter `page`, e.g.
//! `general`) and runs the buttons it cannot handle itself (Reveal Preferences,
//! Choose Folder…) from [`SettingsDialog::take_requests`].

use bae_prefs::schema::{self, Item, Kind, PAGES, Pref};
use bae_prefs::{Preferences, parse_color};
use egui::{Align, Color32, Key, Layout, RichText, Ui, vec2};
use serde_json::Value;

use crate::i18n::tr;
use crate::theme::color;
use crate::widgets;

/// Action id that opens the dialog; parameter `{"page": "<page id>"}`.
pub const ACTION: &str = "app.settings";

/// Dialog size in points (AE's Preferences window at 100%).
pub const SIZE: egui::Vec2 = vec2(760.0, 540.0);
const LIST_W: f32 = 170.0;

/// Window title: "Settings" on macOS (AE 2026), "Preferences" elsewhere.
pub fn title() -> &'static str {
    if cfg!(target_os = "macos") {
        "Settings"
    } else {
        "Preferences"
    }
}

/// Registers the `app.settings` action, one named variant per page (Keyboard
/// Shortcuts window, menus) and AE's shortcut for General (Cmd+Opt+; / Ctrl+Alt+;).
pub fn register(app: &mut bae_app::AppBuilder) {
    crate::i18n::register(bae_prefs::i18n::ZH);
    let owner = app.current_owner();
    let actions = app.registry::<crate::ActionInfo>();
    if !actions.snapshot().contains(ACTION) {
        let _ = actions.register(owner.clone(), ACTION, crate::ActionInfo::new(title()));
    }
    let variants = app.registry::<crate::CommandVariant>();
    variants.batch(|tx| {
        for page in PAGES {
            let id = format!("{ACTION}.{}", page.id);
            if !tx.contains(id.as_str()) {
                let v = crate::CommandVariant::new(
                    ACTION,
                    serde_json::json!({ "page": page.id }),
                    page.title,
                );
                let _ = tx.register(owner.clone(), id.as_str(), v);
            }
        }
    });
    let bindings = crate::KeymapBindings::new(
        crate::presets::AE_KEYMAP,
        vec![
            crate::Binding::new("Cmd+Alt+;", ACTION).params(serde_json::json!({"page": "general"})),
        ],
    );
    let _ = app.register(ACTION, bindings);
}

/// What the dialog produced this frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Still open (or closed).
    None,
    /// OK: store these values.
    Ok(Preferences),
    /// Cancel, Esc or the close button: nothing is stored.
    Cancel,
}

/// A button the host runs (`general.reveal`, `choose:<key>` for Choose Folder…).
pub type Request = String;

/// The Preferences window.
#[derive(Default)]
pub struct SettingsDialog {
    open: bool,
    page: usize,
    draft: Preferences,
    requests: Vec<Request>,
    /// Buttons the host implements (others are drawn disabled).
    host_buttons: Vec<String>,
}

impl SettingsDialog {
    /// Buttons the host handles from [`Self::take_requests`]; every other button
    /// that the dialog does not handle itself is drawn disabled.
    pub fn with_host_buttons(mut self, ids: &[&str]) -> Self {
        self.host_buttons = ids.iter().map(|s| (*s).to_owned()).collect();
        self
    }

    /// Opens on `page` (an id like `grids_guides`; unknown or `None`: General) with
    /// a copy of `current`.
    pub fn open(&mut self, page: Option<&str>, current: &Preferences) {
        self.page = page
            .and_then(|id| PAGES.iter().position(|p| p.id == id))
            .unwrap_or(0);
        self.draft = current.clone();
        self.open = true;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Id of the page shown.
    pub fn page(&self) -> &'static str {
        PAGES[self.page].id
    }

    /// The edited, not yet stored values.
    pub fn draft(&self) -> &Preferences {
        &self.draft
    }

    /// Sets a draft value (host answers to Choose Folder…, tests).
    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.draft.set(key, value);
    }

    /// Buttons pressed since the last call that the host has to run.
    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    /// Shows the window; returns OK / Cancel on the frame the user closes it.
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome {
        if !self.open {
            return Outcome::None;
        }
        let mut outcome = Outcome::None;
        // Clicks outside the window do nothing (modal), without dimming the app.
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("bae-settings-blocker"))
            .order(egui::Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                ui.allocate_response(screen.size(), egui::Sense::click_and_drag());
            });
        let mut window_open = true;
        egui::Window::new(tr(title()))
            .id(egui::Id::new("bae-settings"))
            .order(egui::Order::Foreground)
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .fixed_size(SIZE)
            .default_pos(screen.center() - SIZE / 2.0)
            .show(ctx, |ui| outcome = self.contents(ui));
        let typing = ctx.egui_wants_keyboard_input();
        if !window_open || (!typing && ctx.input(|i| i.key_pressed(Key::Escape))) {
            outcome = Outcome::Cancel;
        } else if !typing && ctx.input(|i| i.key_pressed(Key::Enter)) {
            outcome = Outcome::Ok(self.draft.clone());
        }
        if outcome != Outcome::None {
            self.open = false;
        }
        outcome
    }

    fn contents(&mut self, ui: &mut Ui) -> Outcome {
        let mut outcome = Outcome::None;
        let body_h = SIZE.y - 40.0;
        ui.horizontal_top(|ui| {
            // Page list.
            egui::Frame::new()
                .fill(color::FIELD)
                .inner_margin(4)
                .show(ui, |ui| {
                    ui.set_width(LIST_W);
                    ui.set_height(body_h);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.vertical(|ui| self.page_list(ui));
                });
            ui.separator();
            // Current page.
            ui.vertical(|ui| {
                ui.set_height(body_h);
                ui.label(
                    RichText::new(tr(PAGES[self.page].title))
                        .strong()
                        .size(13.0),
                );
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_salt(PAGES[self.page].id)
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.items(ui, PAGES[self.page].items));
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            let last = PAGES.len() - 1;
            if ui
                .add_enabled(self.page > 0, egui::Button::new(tr("Previous")))
                .clicked()
            {
                self.page -= 1;
            }
            if ui
                .add_enabled(self.page < last, egui::Button::new(tr("Next")))
                .clicked()
            {
                self.page += 1;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Right to left: on macOS the default button is rightmost
                // (Cancel, OK); on Windows OK comes first (OK, Cancel).
                let mac = cfg!(target_os = "macos");
                let (first, second) = if mac {
                    ("OK", "Cancel")
                } else {
                    ("Cancel", "OK")
                };
                for label in [first, second] {
                    let button = egui::Button::new(tr(label)).min_size(vec2(72.0, 20.0));
                    if ui.add(button).clicked() {
                        outcome = if label == "OK" {
                            Outcome::Ok(self.draft.clone())
                        } else {
                            Outcome::Cancel
                        };
                    }
                }
            });
        });
        outcome
    }

    fn page_list(&mut self, ui: &mut Ui) {
        for (i, page) in PAGES.iter().enumerate() {
            let selected = i == self.page;
            let text = RichText::new(tr(page.title)).color(if selected {
                color::TEXT_STRONG
            } else {
                color::TEXT
            });
            let button = egui::Button::selectable(selected, text).frame_when_inactive(false);
            if ui.add_sized(vec2(LIST_W, 20.0), button).clicked() {
                self.page = i;
            }
        }
    }

    fn items(&mut self, ui: &mut Ui, items: &'static [Item]) {
        for item in items {
            self.item(ui, item);
        }
    }

    fn item(&mut self, ui: &mut Ui, item: &'static Item) {
        match item {
            Item::P(p) => {
                if p.os.current() {
                    self.pref(ui, p);
                }
            }
            Item::Row(list) => {
                ui.horizontal(|ui| {
                    for item in *list {
                        self.item(ui, item);
                        ui.add_space(12.0);
                    }
                });
            }
            Item::Group(title, list) => {
                ui.add_space(4.0);
                ui.label(RichText::new(tr(title)).color(color::TEXT_STRONG));
                egui::Frame::new()
                    .stroke(egui::Stroke::new(1.0, color::DIVIDER))
                    .corner_radius(3)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width() - 4.0);
                        self.items(ui, list);
                    });
                ui.add_space(4.0);
            }
            Item::Button(id, label) => {
                let enabled = self.handles(id);
                if ui
                    .add_enabled(enabled, egui::Button::new(tr(label)))
                    .clicked()
                {
                    self.button(id);
                }
            }
            Item::Note(text) => {
                ui.label(tr(text).replace("{ram}", "—").replace("{shared}", "—"));
            }
            Item::Only(os, item) => {
                if os.current() {
                    self.item(ui, item);
                }
            }
            Item::Gap => ui.add_space(8.0),
        }
    }

    /// Whether a button does something (in the dialog or in the host).
    fn handles(&self, id: &str) -> bool {
        id.ends_with("_default")
            || id == "scripting.reset_editor"
            || self.host_buttons.iter().any(|h| h == id)
    }

    fn button(&mut self, id: &str) {
        if let Some(prefix) = id.strip_suffix("_default") {
            self.draft.reset(prefix);
        } else if id == "scripting.reset_editor" {
            for p in schema::all() {
                if p.key.starts_with("scripting.")
                    && !p.key.starts_with("scripting.allow")
                    && !p.key.starts_with("scripting.debugger")
                    && !p.key.starts_with("scripting.warn")
                    && !p.key.starts_with("scripting.error")
                    && !p.key.starts_with("scripting.compact")
                {
                    self.draft.reset(p.key);
                }
            }
        } else {
            self.requests.push(id.to_owned());
        }
    }

    fn pref(&mut self, ui: &mut Ui, p: &'static Pref) {
        let enabled = self.draft.enabled(p);
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| self.control(ui, p));
        });
    }

    fn control(&mut self, ui: &mut Ui, p: &'static Pref) {
        let label = tr(p.label);
        let unit = tr(p.unit);
        let draft = &mut self.draft;
        match p.kind {
            Kind::Bool => {
                let mut v = draft.bool(p.key);
                if widgets::checkbox(ui, &mut v, label).changed() {
                    draft.set(p.key, v);
                }
            }
            Kind::Int { min, max } => {
                field_label(ui, label);
                let mut v = draft.int(p.key) as f64;
                let r = ui.add(
                    widgets::HotNumber::new(&mut v)
                        .range(min as f64..=max as f64)
                        .label(a11y_name(p))
                        .speed(if max - min > 1000 { 10.0 } else { 1.0 }),
                );
                if r.changed() {
                    draft.set(p.key, v.round() as i64);
                }
                unit_label(ui, unit);
            }
            Kind::Float { min, max, decimals } => {
                field_label(ui, label);
                let mut v = draft.float(p.key);
                let r = ui.add(
                    widgets::HotNumber::new(&mut v)
                        .range(min..=max)
                        .decimals(decimals as usize)
                        .label(a11y_name(p))
                        .speed(if decimals > 0 { 0.1 } else { 1.0 }),
                );
                if r.changed() {
                    draft.set(p.key, v);
                }
                unit_label(ui, unit);
            }
            Kind::Duration { max } => {
                field_label(ui, label);
                let mut v = draft.float(p.key);
                let r = ui.add(
                    widgets::HotNumber::new(&mut v)
                        .range(0.0..=max)
                        .decimals(2)
                        .label(a11y_name(p))
                        .speed(0.1),
                );
                if r.changed() {
                    draft.set(p.key, v);
                }
                unit_label(ui, tr("seconds"));
            }
            Kind::Slider {
                min,
                max,
                left,
                right,
            } => {
                field_label(ui, label);
                ui.label(RichText::new(tr(left)).color(color::TEXT_DIM));
                let mut v = draft.float(p.key);
                let r = ui.add(
                    egui::Slider::new(&mut v, min..=max)
                        .show_value(false)
                        .text(""),
                );
                let r = r.on_hover_text(a11y_name(p));
                if r.changed() {
                    draft.set(p.key, v);
                }
                ui.label(RichText::new(tr(right)).color(color::TEXT_DIM));
            }
            Kind::Choice { options, radio } => {
                field_label(ui, label);
                let current = draft.str(p.key).to_owned();
                if radio {
                    ui.vertical(|ui| {
                        for o in options {
                            if ui.radio(current == *o, tr(o)).clicked() {
                                draft.set(p.key, *o);
                            }
                        }
                    });
                } else {
                    let names: Vec<&str> = options.iter().map(|o| tr(o)).collect();
                    let mut i = options.iter().position(|o| *o == current).unwrap_or(0);
                    if widgets::dropdown(ui, p.key, &mut i, &names, 220.0).changed() {
                        draft.set(p.key, options[i]);
                    }
                }
            }
            Kind::Label => {
                field_label(ui, label);
                let index = draft.int(p.key).clamp(0, 16) as usize;
                let [r, g, b] = draft.label_color(index);
                swatch(ui, Color32::from_rgb(r, g, b));
                let names: Vec<String> = (0..17).map(|i| label_name(draft, i)).collect();
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                let mut i = index;
                if widgets::dropdown(ui, p.key, &mut i, &names, 140.0).changed() {
                    draft.set(p.key, i as i64);
                }
            }
            Kind::Color => {
                field_label(ui, label);
                let mut rgb = draft.color(p.key);
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    draft.set(p.key, bae_prefs::format_color(rgb));
                }
            }
            Kind::Text => {
                field_label(ui, label);
                let mut s = draft.str(p.key).to_owned();
                if p.key.starts_with("labels.name_") {
                    s = label_name(draft, label_index(p.key));
                }
                let r = ui.add(egui::TextEdit::singleline(&mut s).desired_width(160.0));
                if r.changed() {
                    draft.set(p.key, s);
                }
            }
            Kind::Path { button } => {
                field_label(ui, label);
                let shown = draft.str(p.key);
                let shown = if shown.is_empty() { "—" } else { shown };
                ui.label(RichText::new(shown).color(color::TEXT_DIM));
                let choose = format!("choose:{}", p.key);
                if ui
                    .add_enabled(self.handles("choose"), egui::Button::new(tr(button)))
                    .clicked()
                {
                    self.requests.push(choose);
                }
            }
        }
    }
}

fn field_label(ui: &mut Ui, label: &str) {
    if !label.is_empty() {
        ui.label(label);
    }
}

fn unit_label(ui: &mut Ui, unit: &str) {
    if !unit.is_empty() {
        ui.label(unit);
    }
}

fn swatch(ui: &mut Ui, c: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(14.0, 12.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 1.0, c);
}

/// Accessible name of a field: its label, or the key for unlabeled ones.
fn a11y_name(p: &Pref) -> &'static str {
    if p.label.is_empty() {
        p.key
    } else {
        tr(p.label)
    }
}

fn label_index(key: &str) -> usize {
    key.rsplit('_')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// A label's name as shown: the user's name, or the translated default.
fn label_name(p: &Preferences, i: usize) -> String {
    let name = p.label_name(i);
    if i == 0 || name.is_empty() || name == schema::LABEL_NAMES[i] {
        tr(schema::LABEL_NAMES[i]).to_owned()
    } else {
        name.to_owned()
    }
}

/// `#rrggbb` → color (black when invalid).
pub fn color32(hex: &str) -> Color32 {
    let [r, g, b] = parse_color(hex).unwrap_or_default();
    Color32::from_rgb(r, g, b)
}

#[cfg(test)]
mod tests;
