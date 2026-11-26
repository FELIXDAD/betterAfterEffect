//! The Preferences dialog (AE 26: Edit > Preferences on Windows, After Effects >
//! Settings… on macOS; the window is titled Preferences on both), built from
//! [`bae_prefs::schema`] (generated from R1's reading of AE 26.0): a fixed-size
//! modal window with the 16 pages listed on the left, the current page in the
//! middle and OK, Cancel, Previous, Next in a column at the top right. Edits go to
//! a draft; OK hands it back to the host to store, Cancel drops it. Previous and
//! Next wrap around.
//!
//! The host opens it for the `app.settings` action (parameter `page`, e.g.
//! `startup_repair`) and runs the buttons it handles (Reveal Preferences,
//! Choose Folder…) from [`SettingsDialog::take_requests`]; every other button is
//! drawn disabled.

use bae_prefs::schema::{self, Item, Kind, PAGES, Pref, Style};
use bae_prefs::{Preferences, parse_color, parse_timecode};
use egui::{Color32, Key, RichText, Ui, vec2};
use serde_json::Value;

use crate::a11y::Accessible as _;
use crate::theme::color;
use crate::widgets;

/// Action id that opens the dialog; parameter `{"page": "<page id>"}`.
pub const ACTION: &str = "app.settings";

/// Dialog size in points: AE 26's window at 96 DPI (948 × 801 px).
pub const SIZE: egui::Vec2 = vec2(948.0, 801.0);
/// Page list width and row height (R1: 210 px, 24 px rows).
const LIST_W: f32 = 210.0;
const LIST_ROW: f32 = 24.0;
/// Content column (593 px) and button column (60 × 24 px buttons, 8 px apart).
const CONTENT_W: f32 = 593.0;
const BUTTON: egui::Vec2 = vec2(60.0, 24.0);
const BUTTON_GAP: f32 = 8.0;

/// Several AE 26 group titles stay in English in the Chinese Preferences dialog.
fn tr(en: &str) -> &str {
    if crate::i18n::lang() == crate::Lang::En {
        return en;
    }
    if let Some(text) = bae_prefs::i18n::zh(en) {
        return text;
    }
    if bae_prefs::pages::SAME_IN_ZH.binary_search(&en).is_ok() {
        return en;
    }
    crate::i18n::tr(en)
}

/// Window title: "Preferences" on both platforms. On macOS the app menu item reads
/// "Settings…" (the system renames AE's Preferences item since macOS 13), but the
/// window keeps its title (`docs/reference/ae/mac/menus.md` §4).
pub fn title() -> &'static str {
    "Preferences"
}

/// Registers the `app.settings` action, one named variant per page (Keyboard
/// Shortcuts window, menus) and AE's shortcut for Startup & Repair (Cmd+Opt+; /
/// Ctrl+Alt+;).
pub fn register(app: &mut bae_app::AppBuilder) {
    // The app's own terms win: only labels it does not translate yet are added.
    let new: Vec<(&'static str, &'static str)> = bae_prefs::i18n::ZH
        .iter()
        .chain(bae_prefs::i18n::DEFAULT_VALUES_ZH)
        .copied()
        .filter(|(en, _)| !crate::i18n::has_translation(en))
        .collect();
    crate::i18n::register(&new);
    let zh = |en: &'static str| bae_prefs::i18n::zh(en).unwrap_or(en);
    let owner = app.current_owner();
    let actions = app.registry::<crate::ActionInfo>();
    if !actions.snapshot().contains(ACTION) {
        let _ = actions.register(owner.clone(), ACTION, crate::ActionInfo::new(title()));
    }
    // One named variant per page ("Preferences: Project"), as the Keyboard
    // Shortcuts window and the menus list them.
    let zh_title = zh(title());
    let mut names: Vec<(&'static str, &'static str)> = Vec::new();
    let variants = app.registry::<crate::CommandVariant>();
    variants.batch(|tx| {
        for page in PAGES {
            let en: &'static str =
                Box::leak(format!("{}: {}", title(), page.title).into_boxed_str());
            let zh_page = zh(page.title);
            names.push((
                en,
                Box::leak(format!("{zh_title}：{zh_page}").into_boxed_str()),
            ));
            let id = format!("{ACTION}.{}", page.id);
            if !tx.contains(id.as_str()) {
                let v =
                    crate::CommandVariant::new(ACTION, serde_json::json!({ "page": page.id }), en);
                let _ = tx.register(owner.clone(), id.as_str(), v);
            }
        }
    });
    crate::i18n::register(&names);
    let bindings = crate::KeymapBindings::new(
        crate::presets::AE_KEYMAP,
        vec![
            #[cfg(target_os = "macos")]
            crate::Binding::new("Cmd+,", ACTION),
            crate::Binding::new("Cmd+Alt+;", ACTION)
                .params(serde_json::json!({"page": "startup_repair"})),
        ],
    );
    let _ = app.register(ACTION, bindings);
}

/// Applies the preferences the UI shell draws with: label colors and names
/// (Labels page) and the color theme (Appearance). Call at startup and after OK.
/// A label color left at AE's value is drawn as measured on screen in AE
/// (`theme::LABELS`); a changed one is drawn as chosen.
pub fn apply(prefs: &Preferences) {
    use bae_prefs::keys;
    crate::theme::set_label_colors(std::array::from_fn(|i| {
        if i == 0 || prefs.label_color_is_default(i) {
            crate::theme::LABELS[i].1
        } else {
            let [r, g, b] = prefs.label_color(i);
            Color32::from_rgb(r, g, b)
        }
    }));
    crate::theme::set_label_names(std::array::from_fn(|i| {
        let name = prefs.label_name(i);
        (i > 0 && !name.is_empty() && name != schema::LABEL_NAMES[i]).then(|| name.to_owned())
    }));
    crate::theme::set_appearance(crate::theme::Appearance::from_id(
        prefs.str(keys::COLOR_THEME),
    ));
}

/// Preferences > Grids & Guides, as the Composition viewer draws them.
#[derive(Clone, Debug, PartialEq)]
pub struct GridGuides {
    pub grid_color: Color32,
    /// `lines`, `dashed_lines` or `dots`.
    pub grid_style: &'static str,
    /// Gridline spacing in composition pixels.
    pub spacing: f32,
    pub subdivisions: u32,
    pub guide_color: Color32,
    pub guide_style: &'static str,
    /// Action-safe and title-safe margins (fractions of the frame).
    pub action_safe: f32,
    pub title_safe: f32,
    /// Center-cut action-safe and title-safe margins (fractions of the width).
    pub cc_action_safe: f32,
    pub cc_title_safe: f32,
    /// Proportional grid cells.
    pub proportional: [u32; 2],
}

impl GridGuides {
    pub fn from_prefs(p: &Preferences) -> Self {
        use bae_prefs::keys;
        let c = |k| {
            let [r, g, b] = p.color(k);
            Color32::from_rgb(r, g, b)
        };
        let pct = |k| (p.float(k) / 100.0) as f32;
        // No allocation per frame: the style is one of the schema's options.
        let style = |k| match p.str(k) {
            "dashed_lines" => "dashed_lines",
            "dots" => "dots",
            _ => "lines",
        };
        GridGuides {
            grid_color: c(keys::GRID_COLOR),
            grid_style: style(keys::GRID_STYLE),
            spacing: p.float(keys::GRID_SPACING) as f32,
            subdivisions: p.int(keys::GRID_SUBDIVISIONS).max(1) as u32,
            guide_color: c(keys::GUIDE_COLOR),
            guide_style: style(keys::GUIDE_STYLE),
            action_safe: pct(keys::ACTION_SAFE),
            title_safe: pct(keys::TITLE_SAFE),
            cc_action_safe: pct(keys::CC_ACTION_SAFE),
            cc_title_safe: pct(keys::CC_TITLE_SAFE),
            proportional: [
                p.int(keys::PROP_GRID_H).max(1) as u32,
                p.int(keys::PROP_GRID_V).max(1) as u32,
            ],
        }
    }

    /// The current values from the app's `Arc<PrefsStore>` (defaults without one).
    pub fn current(resources: &bae_app::Resources) -> Self {
        match resources.get::<std::sync::Arc<bae_prefs::PrefsStore>>() {
            Some(store) => Self::from_prefs(&store.display()),
            None => Self::from_prefs(&Preferences::default()),
        }
    }

    /// Draws the grid over `comp` (screen rect of the frame) at `scale` screen
    /// points per composition pixel: gridlines every `spacing` pixels, fainter
    /// subdivisions in between, in the grid style.
    pub fn paint_grid(&self, painter: &egui::Painter, comp: egui::Rect, scale: f32) {
        let major = self.spacing * scale;
        let minor = major / self.subdivisions as f32;
        if minor < 3.0 {
            return;
        }
        let strong = egui::Stroke::new(1.0, self.grid_color);
        let faint = egui::Stroke::new(1.0, self.grid_color.gamma_multiply(0.45));
        let mut i = 0u32;
        let mut x = comp.left();
        while x <= comp.right() + 0.5 {
            let s = if i.is_multiple_of(self.subdivisions) {
                strong
            } else {
                faint
            };
            self.line(
                painter,
                [egui::pos2(x, comp.top()), egui::pos2(x, comp.bottom())],
                s,
            );
            x += minor;
            i += 1;
        }
        let mut i = 0u32;
        let mut y = comp.top();
        while y <= comp.bottom() + 0.5 {
            let s = if i.is_multiple_of(self.subdivisions) {
                strong
            } else {
                faint
            };
            self.line(
                painter,
                [egui::pos2(comp.left(), y), egui::pos2(comp.right(), y)],
                s,
            );
            y += minor;
            i += 1;
        }
    }

    fn line(&self, painter: &egui::Painter, [a, b]: [egui::Pos2; 2], stroke: egui::Stroke) {
        match self.grid_style {
            "dashed_lines" => {
                painter.extend(egui::Shape::dashed_line(&[a, b], stroke, 4.0, 3.0));
            }
            "dots" => {
                painter.extend(egui::Shape::dotted_line(&[a, b], stroke.color, 4.0, 0.75));
            }
            _ => {
                painter.line_segment([a, b], stroke);
            }
        }
    }

    /// The action-safe and title-safe rectangles of `comp`, and the center-cut
    /// x positions (left/right pairs, action then title).
    pub fn safe_rects(&self, comp: egui::Rect) -> ([egui::Rect; 2], [[f32; 2]; 2]) {
        let shrink = |f: f32| egui::Rect::from_center_size(comp.center(), comp.size() * (1.0 - f));
        let cc = |f: f32| {
            let half = comp.width() * (1.0 - f) / 2.0;
            [comp.center().x - half, comp.center().x + half]
        };
        (
            [shrink(self.action_safe), shrink(self.title_safe)],
            [cc(self.cc_action_safe), cc(self.cc_title_safe)],
        )
    }

    /// Draws the safe margins (Title/Action Safe) in `stroke`.
    pub fn paint_safe(&self, painter: &egui::Painter, comp: egui::Rect, stroke: egui::Stroke) {
        let ([action, title], cc) = self.safe_rects(comp);
        for r in [action, title] {
            painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
        }
        // Center-cut: short vertical marks inside the title-safe band.
        for (xs, band) in cc.iter().zip([action, title]) {
            for x in xs {
                painter.line_segment(
                    [
                        egui::pos2(*x, band.top()),
                        egui::pos2(*x, band.top() + 12.0),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        egui::pos2(*x, band.bottom() - 12.0),
                        egui::pos2(*x, band.bottom()),
                    ],
                    stroke,
                );
            }
        }
    }
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

/// A button the host runs (`startup_repair.reveal_preferences_in_explorer`,
/// `choose:<key>` for Choose…).
pub type Request = String;

/// The Preferences window.
#[derive(Default)]
pub struct SettingsDialog {
    open: bool,
    confirm_reset: bool,
    display_changed: bool,
    page: usize,
    draft: Preferences,
    requests: Vec<Request>,
    /// Buttons the host implements (others are drawn disabled); `choose` stands
    /// for every Choose… button.
    host_buttons: Vec<String>,
    /// The UI language when the window opened and the Language value shown: a
    /// new choice switches the UI at once, Cancel switches it back.
    lang_at_open: Option<(crate::i18n::Lang, String)>,
}

impl SettingsDialog {
    /// Buttons the host handles from [`Self::take_requests`]; every other button
    /// is drawn disabled.
    pub fn with_host_buttons(mut self, ids: &[&str]) -> Self {
        self.host_buttons = ids.iter().map(|s| (*s).to_owned()).collect();
        self
    }

    /// Opens on `page` (an id like `grids_guides`; unknown or `None`: the first
    /// page, Startup & Repair) with a copy of `current`.
    pub fn open(&mut self, page: Option<&str>, current: &Preferences) {
        self.page = page
            .and_then(|id| PAGES.iter().position(|p| p.id == id))
            .unwrap_or(0);
        self.draft = current.clone();
        self.open = true;
        self.confirm_reset = false;
        self.display_changed = false;
        self.lang_at_open = Some((
            crate::i18n::lang(),
            current.str(bae_prefs::keys::LANGUAGE).to_owned(),
        ));
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

    /// Sets a draft value (host answers to Choose…, tests).
    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.display_changed |= self.draft.set(key, value) && visual_setting(key);
    }

    /// New display values from this frame, including an in-progress number drag.
    pub fn take_display_preview(&mut self) -> Option<&Preferences> {
        std::mem::take(&mut self.display_changed).then_some(&self.draft)
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
        // Enter is OK even on a focused button or menu (AE); Space clicks it.
        let enter = crate::dialog::take_enter(ctx);
        egui::Window::new(tr(title()))
            .id(egui::Id::new("bae-settings"))
            .order(egui::Order::Foreground)
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .fixed_size(SIZE)
            .default_pos(screen.center() - SIZE / 2.0)
            .show(ctx, |ui| outcome = self.contents(ui));
        if self.confirm_reset {
            egui::Modal::new(egui::Id::new("reset-all-preferences")).show(ctx, |ui| {
                ui.label(tr("Reset All Preferences to Default..."));
                ui.horizontal(|ui| {
                    if ui.button(tr("OK")).clicked() || enter {
                        self.draft = Preferences::default();
                        self.display_changed = true;
                        self.confirm_reset = false;
                    }
                    if ui.button(tr("Cancel")).clicked()
                        || ctx.input(|i| i.key_pressed(Key::Escape))
                    {
                        self.confirm_reset = false;
                    }
                });
            });
            self.preview_language(false);
            return Outcome::None;
        }
        let typing = ctx.egui_wants_keyboard_input();
        if !window_open || (!typing && ctx.input(|i| i.key_pressed(Key::Escape))) {
            outcome = Outcome::Cancel;
        } else if enter || (!typing && ctx.input(|i| i.key_pressed(Key::Enter))) {
            outcome = Outcome::Ok(self.draft.clone());
        }
        self.preview_language(outcome == Outcome::Cancel);
        if outcome != Outcome::None {
            self.open = false;
        }
        outcome
    }

    /// Shows the UI in the Language chosen in the draft right away (no restart);
    /// `cancel` goes back to the language the window opened with.
    fn preview_language(&mut self, cancel: bool) {
        use crate::i18n;
        let Some((before, shown)) = &mut self.lang_at_open else {
            return;
        };
        let value = self.draft.str(bae_prefs::keys::LANGUAGE);
        if cancel {
            i18n::set_lang(*before);
        } else if value != shown.as_str() {
            *shown = value.to_owned();
            i18n::set_lang(i18n::lang_for_setting(value));
        }
    }

    /// Page list | current page | OK, Cancel, Previous, Next at the top right.
    fn contents(&mut self, ui: &mut Ui) -> Outcome {
        let mut outcome = Outcome::None;
        let body_h = SIZE.y - 24.0;
        ui.horizontal_top(|ui| {
            egui::Frame::new()
                .fill(color::FIELD)
                .inner_margin(2)
                .show(ui, |ui| {
                    ui.set_width(LIST_W);
                    ui.set_height(body_h);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.vertical(|ui| self.page_list(ui));
                });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_width(CONTENT_W);
                ui.set_height(body_h);
                egui::ScrollArea::vertical()
                    .id_salt(PAGES[self.page].id)
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.items(ui, PAGES[self.page].items));
            });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = BUTTON_GAP;
                let button = |text: &str| egui::Button::new(tr(text)).min_size(BUTTON);
                if ui.add(button("OK")).clicked() {
                    outcome = Outcome::Ok(self.draft.clone());
                }
                if ui.add(button("Cancel")).clicked() {
                    outcome = Outcome::Cancel;
                }
                // Previous / Next wrap around and are never disabled (AE 26).
                let n = PAGES.len();
                if ui.add(button("Previous")).clicked() {
                    self.page = (self.page + n - 1) % n;
                }
                if ui.add(button("Next")).clicked() {
                    self.page = (self.page + 1) % n;
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
            if ui.add_sized(vec2(LIST_W, LIST_ROW), button).clicked() {
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
            Item::P(p) => self.pref(ui, p),
            Item::Row(list) => {
                ui.horizontal(|ui| {
                    for item in *list {
                        self.item(ui, item);
                        ui.add_space(10.0);
                    }
                });
            }
            Item::Cols(cols) => {
                ui.columns(cols.len(), |uis| {
                    for (ui, col) in uis.iter_mut().zip(cols.iter()) {
                        self.items(ui, col);
                    }
                });
            }
            Item::Group(title, list) => {
                ui.add_space(4.0);
                ui.add_enabled_ui(*title != "Mercury Transmit", |ui| {
                    ui.label(RichText::new(tr(title)).color(color::TEXT_STRONG));
                    egui::Frame::new()
                        .stroke(egui::Stroke::new(1.0, color::DIVIDER))
                        .corner_radius(3)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width() - 4.0);
                            self.items(ui, list);
                        });
                });
                ui.add_space(4.0);
            }
            Item::Button(id, label) => {
                if *id == "startup_repair.reset_all_preferences" {
                    if ui.button(tr(label)).clicked() {
                        self.confirm_reset = true;
                    }
                    return;
                }
                if ui
                    .add_enabled(self.handles(id), egui::Button::new(tr(label)))
                    .clicked()
                {
                    self.requests.push((*id).to_owned());
                }
            }
            Item::Info(label, value) => {
                ui.horizontal(|ui| {
                    if !label.is_empty() {
                        ui.label(tr(label));
                    }
                    // `<…>` values come from the machine (installed RAM, …).
                    let shown = if value.contains('<') { "—" } else { value };
                    if !shown.is_empty() {
                        ui.label(RichText::new(shown).color(color::TEXT_DIM));
                    }
                });
            }
            Item::Table(label, columns) => {
                ui.label(tr(label));
                egui::Frame::new()
                    .fill(color::FIELD)
                    .inner_margin(4)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width() - 4.0);
                        ui.horizontal(|ui| {
                            for c in *columns {
                                ui.label(RichText::new(tr(c)).color(color::TEXT_DIM));
                                ui.add_space(24.0);
                            }
                        });
                        ui.add_space(40.0);
                    });
            }
            Item::Preview(_) => self.expression_preview(ui),
            Item::When(w, item) => {
                let on = self.draft.holds(w);
                ui.add_enabled_ui(on, |ui| self.item(ui, item));
            }
            Item::Only(os, item) => {
                if os.current() {
                    self.item(ui, item);
                }
            }
        }
    }

    /// Whether the host runs a button (others are drawn disabled).
    fn handles(&self, id: &str) -> bool {
        let id = if id.starts_with("choose:") {
            "choose"
        } else {
            id
        };
        self.host_buttons.iter().any(|h| h == id)
    }

    fn pref(&mut self, ui: &mut Ui, p: &'static Pref) {
        let enabled = !matches!(
            p.key,
            "project.dynamic_link_highest_number" | "export.preview_output_during_render_queue"
        ) && self.draft.enabled(p);
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                if p.indent {
                    ui.add_space(18.0);
                }
                self.control(ui, p)
            });
        });
    }

    fn control(&mut self, ui: &mut Ui, p: &'static Pref) {
        let label = tr(p.label);
        let shown = if p.quiet { "" } else { label };
        let unit = tr(p.unit);
        let draft = &self.draft;
        match p.kind {
            Kind::Bool => {
                let mut v = p.key != "video.enable_mercury_transmit" && draft.bool(p.key);
                if widgets::checkbox(ui, &mut v, label).changed() {
                    self.set(p.key, v);
                }
            }
            Kind::Int { min, max } => {
                field_label(ui, shown);
                let mut v = draft.int(p.key) as f64;
                let r = ui.add(
                    widgets::HotNumber::new(&mut v)
                        .range(min as f64..=max as f64)
                        .label(a11y_name(p))
                        .speed(if max - min > 1000 { 10.0 } else { 1.0 }),
                );
                if r.changed() {
                    self.set(p.key, v.round() as i64);
                }
                field_label(ui, unit);
            }
            Kind::Float { min, max, decimals } => {
                field_label(ui, shown);
                let mut v = draft.float(p.key);
                let r = ui.add(
                    widgets::HotNumber::new(&mut v)
                        .range(min..=max)
                        .decimals(decimals as usize)
                        .label(a11y_name(p))
                        .speed(if decimals > 0 { 0.1 } else { 1.0 }),
                );
                if r.changed() {
                    self.set(p.key, v);
                }
                field_label(ui, unit);
            }
            Kind::Slider {
                min,
                max,
                left,
                right,
            } => {
                field_label(ui, shown);
                ui.label(RichText::new(tr(left)).color(color::TEXT_DIM));
                let mut v = draft.float(p.key);
                let r = ui.add(
                    egui::Slider::new(&mut v, min..=max)
                        .step_by(1.0)
                        .show_value(false),
                );
                if r.on_hover_text(a11y_name(p)).changed() {
                    self.set(p.key, v);
                }
                ui.label(RichText::new(tr(right)).color(color::TEXT_DIM));
            }
            Kind::Choice { options, style } => {
                let current = draft.str(p.key).to_owned();
                match style {
                    Style::Dropdown => {
                        field_label(ui, shown);
                        let names: Vec<&str> = options.iter().map(|o| tr(o.label)).collect();
                        let mut i = options.iter().position(|o| o.value == current).unwrap_or(0);
                        if widgets::dropdown(ui, p.key, &mut i, &names, 220.0).changed() {
                            self.set(p.key, options[i].value);
                        }
                    }
                    Style::Radio => {
                        ui.vertical(|ui| {
                            field_label(ui, shown);
                            for o in options {
                                if ui.radio(current == o.value, tr(o.label)).clicked() {
                                    self.set(p.key, o.value);
                                }
                            }
                        });
                    }
                    Style::Theme => {
                        ui.vertical(|ui| {
                            field_label(ui, shown);
                            ui.horizontal(|ui| {
                                for o in options {
                                    if theme_card(ui, o.value, tr(o.label), current == o.value) {
                                        self.set(p.key, o.value);
                                    }
                                }
                            });
                        });
                    }
                }
            }
            Kind::Label => {
                field_label(ui, shown);
                let index = draft.int(p.key).clamp(0, 16) as usize;
                swatch(ui, crate::theme::label_color(index));
                let names: Vec<String> = (0..17).map(|i| label_name(draft, i)).collect();
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                let mut i = index;
                if widgets::dropdown(ui, p.key, &mut i, &names, 140.0).changed() {
                    self.set(p.key, i as i64);
                }
            }
            Kind::Color => {
                field_label(ui, shown);
                let mut rgb = draft.color(p.key);
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    self.set(p.key, bae_prefs::format_color(rgb));
                }
            }
            Kind::Text => {
                field_label(ui, shown);
                let mut s = draft.str(p.key).to_owned();
                if p.key.starts_with("labels.label_name_") {
                    s = label_name(draft, label_index(p.key));
                } else if let bae_prefs::DefaultValue::Str(d) = p.default
                    && s == d
                {
                    // An unchanged default name shows in the UI language
                    // ("Solids" → "纯色"), as BAE names the folder (#206).
                    s = tr(d).to_owned();
                }
                let r = ui.add(
                    egui::TextEdit::singleline(&mut s)
                        .desired_width(160.0)
                        .hint_text(a11y_name(p)),
                );
                if r.changed() {
                    self.set(p.key, s);
                }
            }
            Kind::Timecode => {
                field_label(ui, shown);
                // Edited as text; a well-formed timecode is stored when typed.
                let id = ui.id().with(p.key);
                let mut s = ui
                    .data(|d| d.get_temp::<String>(id))
                    .unwrap_or_else(|| draft.str(p.key).to_owned());
                let r = ui.add(egui::TextEdit::singleline(&mut s).desired_width(90.0));
                if r.changed() && parse_timecode(&s).is_some() {
                    self.set(p.key, s.clone());
                }
                if r.has_focus() {
                    ui.data_mut(|d| d.insert_temp(id, s));
                } else {
                    ui.data_mut(|d| d.remove::<String>(id));
                }
            }
            Kind::Path => {
                let path = draft.str(p.key);
                let path = if path.is_empty() { "—" } else { path };
                ui.label(RichText::new(path).color(color::TEXT_DIM))
                    .on_hover_text(label);
            }
        }
    }

    /// Expression editor preview (Scripting & Expressions): a few lines of code
    /// in the editor's font size and colors.
    fn expression_preview(&self, ui: &mut Ui) {
        let p = &self.draft;
        let c = |k: &str| {
            let [r, g, b] = p.color(&format!("scripting_expressions.expr_color_{k}"));
            Color32::from_rgb(r, g, b)
        };
        let size = p.int(bae_prefs::keys::EXPR_FONT_SIZE) as f32;
        let font = egui::FontId::monospace(size);
        let mut job = egui::text::LayoutJob::default();
        let mut add = |text: &str, color: Color32| {
            job.append(text, 0.0, egui::TextFormat::simple(font.clone(), color));
        };
        add("// wiggle the position\n", c("comments"));
        add("var ", c("keywords"));
        add("amp", c("identifiers"));
        add(" = ", c("operators"));
        add("20", c("numbers"));
        add(";\n", c("default"));
        add("wiggle", c("identifiers"));
        add("(", c("default"));
        add("2", c("numbers"));
        add(", amp); ", c("default"));
        add("\"done\"", c("strings"));
        egui::Frame::new()
            .fill(c("background"))
            .inner_margin(6)
            .show(ui, |ui| {
                ui.set_width(ui.available_width() - 4.0);
                ui.label(job);
            });
    }
}

fn visual_setting(key: &str) -> bool {
    key.starts_with("grids_guides.")
        || key.starts_with("appearance.")
        || key.starts_with("labels.")
        || key.starts_with("scripting_expressions.expr_")
}

fn field_label(ui: &mut Ui, label: &str) {
    if !label.is_empty() {
        ui.label(label);
    }
}

fn swatch(ui: &mut Ui, c: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(14.0, 12.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 1.0, c);
}

/// A Color Theme card: a small picture of the theme above its name.
fn theme_card(ui: &mut Ui, id: &str, name: &str, selected: bool) -> bool {
    let (bg, fg) = match id {
        "light" => (Color32::from_gray(0xe1), Color32::from_gray(0x50)),
        "dark" => (Color32::from_gray(0x32), Color32::from_gray(0xb8)),
        _ => (Color32::from_gray(0x1d), Color32::from_gray(0xb1)),
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(96.0, 72.0), egui::Sense::click());
    let resp = resp.labeled_toggle(egui::WidgetType::RadioButton, name, selected);
    let p = ui.painter();
    let pic = egui::Rect::from_min_size(rect.min, vec2(rect.width(), 52.0));
    p.rect_filled(pic, 3.0, bg);
    p.rect_filled(
        egui::Rect::from_min_size(pic.min + vec2(8.0, 10.0), vec2(48.0, 4.0)),
        1.0,
        fg,
    );
    p.rect_filled(
        egui::Rect::from_min_size(pic.min + vec2(8.0, 20.0), vec2(64.0, 4.0)),
        1.0,
        fg,
    );
    let stroke = if selected {
        egui::Stroke::new(2.0, color::ACCENT)
    } else {
        egui::Stroke::new(1.0, color::DIVIDER)
    };
    p.rect_stroke(pic, 3.0, stroke, egui::StrokeKind::Inside);
    p.text(
        egui::pos2(rect.center().x, rect.bottom() - 8.0),
        egui::Align2::CENTER_CENTER,
        name,
        crate::theme::body(),
        color::TEXT,
    );
    resp.clicked()
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
