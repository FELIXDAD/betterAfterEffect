//! AE-style common controls.
//!
//! These widgets only edit the values they are given. Callers turn the edits
//! into `bae-model` commands; `drag_started` / `drag_stopped` on the returned
//! response mark the boundaries of one undo step while scrubbing.

use std::ops::RangeInclusive;

use egui::text::LayoutJob;
use egui::{
    Align2, Color32, CursorIcon, FontId, Key, Pos2, Rect, Response, Sense, Stroke, TextEdit,
    TextFormat, Ui, Vec2, Widget, pos2, vec2,
};

use crate::a11y::Accessible as _;
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::theme::{self, color};

/// Hot text: a blue number that changes when dragged and turns into a text
/// field when clicked, like AE's property values.
pub struct HotNumber<'a> {
    value: &'a mut f64,
    speed: f64,
    decimals: usize,
    suffix: &'a str,
    range: RangeInclusive<f64>,
    width: Option<f32>,
    font: FontId,
    /// Accessibility name (the property, e.g. "Position X").
    label: &'a str,
}

impl<'a> HotNumber<'a> {
    pub fn new(value: &'a mut f64) -> Self {
        Self {
            value,
            speed: 1.0,
            decimals: 0,
            suffix: "",
            range: f64::NEG_INFINITY..=f64::INFINITY,
            width: None,
            font: theme::body(),
            label: "",
        }
    }

    /// What the number is, for screen readers (the property name, translated).
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Value change per point dragged (before modifiers).
    pub fn speed(mut self, speed: f64) -> Self {
        self.speed = speed;
        self
    }

    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

    /// Unit drawn after the number in normal text color, e.g. `%` or `°`.
    pub fn suffix(mut self, suffix: &'a str) -> Self {
        self.suffix = suffix;
        self
    }

    pub fn range(mut self, range: RangeInclusive<f64>) -> Self {
        self.range = range;
        self
    }

    /// Fixed field width with the gray underline used in the Properties panel.
    pub fn field_width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn font(mut self, font: FontId) -> Self {
        self.font = font;
        self
    }

    fn format(&self, v: f64) -> String {
        format!("{:.*}", self.decimals, v)
    }
}

/// Persistent per-widget scrub/edit state.
#[derive(Clone, Default)]
struct HotState {
    editing: Option<String>,
    /// Sub-step drag remainder so slow drags still add up.
    carry: f64,
}

impl Widget for HotNumber<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let id = ui.next_auto_id();
        let mut state: HotState = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        let text = self.format(*self.value);
        let height = ui.spacing().interact_size.y;

        if let Some(mut buf) = state.editing.take() {
            let width = self.width.unwrap_or(48.0);
            let edit = TextEdit::singleline(&mut buf)
                .id(id.with("edit"))
                .font(self.font.clone())
                .desired_width(width)
                .margin(vec2(1.0, 0.0))
                .text_color(color::TEXT_STRONG);
            let mut resp = ui.add(edit);
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            let escape = ui.input(|i| i.key_pressed(Key::Escape));
            if escape {
                ui.data_mut(|d| d.insert_temp(id, HotState::default()));
            } else if enter || resp.lost_focus() {
                if let Some(v) = eval(&buf) {
                    let v = v.clamp(*self.range.start(), *self.range.end());
                    if v != *self.value {
                        *self.value = v;
                        resp.mark_changed();
                    }
                }
                ui.data_mut(|d| d.insert_temp(id, HotState::default()));
            } else {
                state.editing = Some(buf);
                ui.data_mut(|d| d.insert_temp(id, state));
            }
            return resp;
        }

        let painter = ui.painter().clone();
        let layout = |tint: Color32| {
            let mut job = LayoutJob::default();
            job.append(&text, 0.0, TextFormat::simple(self.font.clone(), tint));
            if !self.suffix.is_empty() {
                job.append(
                    self.suffix,
                    0.0,
                    TextFormat::simple(self.font.clone(), color::TEXT),
                );
            }
            painter.layout_job(job)
        };
        let galley = layout(color::ACCENT);
        let width = self
            .width
            .unwrap_or(0.0)
            .max(galley.size().x + 3.0)
            .max(8.0);
        let (rect, mut resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
        let spoken = format!("{text}{}", self.suffix);
        resp = resp.labeled_value(egui::WidgetType::DragValue, self.label, &spoken);
        let resp_hover = resp.hovered() || resp.dragged();
        if resp_hover {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }

        if resp.dragged() {
            let dx = resp.drag_delta().x as f64;
            let mods = ui.input(|i| i.modifiers);
            let factor = if mods.shift {
                10.0
            } else if mods.command {
                0.1
            } else {
                1.0
            };
            let step = 10f64.powi(-(self.decimals as i32));
            let raw = state.carry + dx * self.speed * factor;
            let applied = (raw / step).trunc() * step;
            state.carry = raw - applied;
            if applied != 0.0 {
                let v = (*self.value + applied).clamp(*self.range.start(), *self.range.end());
                if v != *self.value {
                    *self.value = v;
                    resp.mark_changed();
                }
            }
        }
        if resp.drag_stopped() {
            state.carry = 0.0;
        }
        if resp.clicked() {
            state.editing = Some(text.clone());
            ui.memory_mut(|m| m.request_focus(id.with("edit")));
        }
        ui.data_mut(|d| d.insert_temp(id, state));

        if ui.is_rect_visible(rect) {
            let galley = if resp_hover {
                layout(color::ACCENT_HOVER)
            } else {
                galley
            };
            let p = ui.painter();
            let galley_pos = pos2(rect.left() + 1.5, rect.center().y - galley.size().y / 2.0);
            p.galley(galley_pos, galley, color::ACCENT);
            if self.width.is_some() {
                let y = rect.bottom() - 0.5;
                p.hline(rect.x_range(), y, Stroke::new(1.0, color::UNDERLINE));
            }
        }
        resp
    }
}

/// Evaluate a typed value. Accepts a number or a left-to-right chain such as `950+10*2`.
pub fn eval(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Ok(v) = s.parse::<f64>() {
        return Some(v);
    }
    let mut acc: Option<f64> = None;
    let mut op = '+';
    let mut num = String::new();
    let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut i = 0;
    while i <= chars.len() {
        let c = chars.get(i).copied();
        let is_op = matches!(c, Some('+' | '-' | '*' | '/')) && !num.is_empty();
        if c.is_none() || is_op {
            let n: f64 = num.parse().ok()?;
            acc = Some(match (acc, op) {
                (None, _) => n,
                (Some(a), '+') => a + n,
                (Some(a), '-') => a - n,
                (Some(a), '*') => a * n,
                (Some(a), '/') => a / n,
                _ => return None,
            });
            num.clear();
            if let Some(c) = c {
                op = c;
            }
        } else {
            num.push(c?);
        }
        i += 1;
    }
    acc.filter(|v| v.is_finite())
}

/// Stopwatch toggle that enables keyframing for a property.
pub fn stopwatch(ui: &mut Ui, on: &mut bool) -> Response {
    stopwatch_for(ui, on, "")
}

/// [`stopwatch`] of the property `name` ("Layer “Solid 1” · Position"), so
/// screen readers tell the stopwatches apart.
pub fn stopwatch_for(ui: &mut Ui, on: &mut bool, name: &str) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let resp = resp.labeled_toggle(
        egui::WidgetType::Checkbox,
        &crate::a11y::context(&[name, tr("Time-Vary Stopwatch")]),
        *on,
    );
    let c = if *on {
        color::ACCENT
    } else if resp.hovered() {
        color::TEXT_STRONG
    } else {
        color::ICON
    };
    icons::paint(ui.painter(), rect, Icon::Stopwatch, c);
    resp
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyNav {
    Prev,
    Toggle,
    Next,
}

/// The ◀ ◆ ▶ keyframe navigator shown left of animated properties.
pub fn keyframe_nav(ui: &mut Ui, has_prev: bool, on_key: bool, has_next: bool) -> Option<KeyNav> {
    keyframe_nav_for(ui, has_prev, on_key, has_next, "")
}

/// [`keyframe_nav`] of the property `name` (screen-reader names include it).
pub fn keyframe_nav_for(
    ui: &mut Ui,
    has_prev: bool,
    on_key: bool,
    has_next: bool,
    name: &str,
) -> Option<KeyNav> {
    let mut out = None;
    let parts = [
        (Icon::TriangleLeft, KeyNav::Prev, has_prev),
        (Icon::Keyframe, KeyNav::Toggle, true),
        (Icon::TriangleRight, KeyNav::Next, has_next),
    ];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (icon, nav, enabled) in parts {
            let (rect, resp) = ui.allocate_exact_size(vec2(13.0, 16.0), Sense::click());
            let what = match nav {
                KeyNav::Prev => "Go to Previous Keyframe",
                KeyNav::Toggle => "Add or Remove Keyframe at Current Time",
                KeyNav::Next => "Go to Next Keyframe",
            };
            let resp = resp.labeled(
                egui::WidgetType::Button,
                &crate::a11y::context(&[name, tr(what)]),
            );
            if enabled && resp.clicked() {
                out = Some(nav);
            }
            let c = match nav {
                KeyNav::Toggle if on_key => color::ACCENT,
                KeyNav::Toggle => color::KEY_OFF,
                _ if !enabled => color::TEXT_DISABLED,
                _ if resp.hovered() => color::TEXT_STRONG,
                _ => color::ICON,
            };
            icons::paint(
                ui.painter(),
                Rect::from_center_size(rect.center(), Vec2::splat(15.0)),
                icon,
                c,
            );
        }
    });
    out
}

/// Twirl-down arrow for expanding layers and property groups.
pub fn disclosure(ui: &mut Ui, open: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::click());
    if resp.clicked() {
        *open = !*open;
        resp.mark_changed();
    }
    let resp = resp.labeled_toggle(
        egui::WidgetType::CollapsingHeader,
        tr(if *open { "Collapse" } else { "Expand" }),
        *open,
    );
    let icon = if *open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    let c = if resp.hovered() {
        color::TEXT_STRONG
    } else {
        color::ICON
    };
    icons::paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(12.0)),
        icon,
        c,
    );
    resp
}

/// AE-style checkbox: outlined when off, filled with a dark check when on.
pub fn checkbox(ui: &mut Ui, checked: &mut bool, label: &str) -> Response {
    let font = theme::body();
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, color::TEXT);
    let box_side = 11.0;
    let gap = if label.is_empty() { 0.0 } else { 7.0 };
    let size = vec2(
        box_side + gap + galley.size().x,
        ui.spacing().interact_size.y,
    );
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *checked = !*checked;
        resp.mark_changed();
    }
    let resp = resp.labeled_toggle(egui::WidgetType::Checkbox, label, *checked);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let b = Rect::from_min_size(
            pos2(rect.left(), rect.center().y - box_side / 2.0),
            Vec2::splat(box_side),
        );
        let fg = if resp.hovered() {
            color::TEXT_STRONG
        } else {
            color::TEXT
        };
        if *checked {
            p.rect_filled(b, 2.0, fg);
            let pts = vec![
                b.lerp_inside(vec2(0.22, 0.52)),
                b.lerp_inside(vec2(0.42, 0.72)),
                b.lerp_inside(vec2(0.78, 0.3)),
            ];
            icons::path(p, pts, false, Stroke::new(1.6, color::PANEL));
        } else {
            p.rect_stroke(
                b.shrink(0.75),
                2.0,
                Stroke::new(1.5, fg),
                egui::StrokeKind::Middle,
            );
        }
        p.galley_with_override_text_color(
            pos2(b.right() + gap, rect.center().y - galley.size().y / 2.0),
            galley,
            fg,
        );
    }
    resp
}

/// Dark rounded field with a chevron, opening a list of choices.
pub fn dropdown(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    selected: &mut usize,
    items: &[&str],
    width: f32,
) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 17.0), Sense::click());
    let current = items.get(*selected).copied().unwrap_or("");
    resp = resp.labeled_value(egui::WidgetType::ComboBox, current, current);
    let id = ui.id().with(id_salt);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let bg = if resp.hovered() {
            color::FIELD_HOVER
        } else {
            color::DROPDOWN
        };
        p.rect_filled(rect, 3.0, bg);
        let text = items.get(*selected).copied().unwrap_or("");
        p.text(
            pos2(rect.left() + 7.0, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            theme::body(),
            color::TEXT_STRONG,
        );
        let chev = Rect::from_center_size(
            pos2(rect.right() - 11.0, rect.center().y),
            Vec2::splat(14.0),
        );
        icons::paint(p, chev, Icon::ChevronDown, color::TEXT_STRONG);
    }
    let mut picked = None;
    egui::Popup::menu(&resp).id(id).width(width).show(|ui| {
        for (i, item) in items.iter().enumerate() {
            if ui.selectable_label(i == *selected, *item).clicked() {
                picked = Some(i);
            }
        }
    });
    if let Some(i) = picked
        && i != *selected
    {
        *selected = i;
        resp.mark_changed();
    }
    resp
}

/// Search field with the magnifier-and-chevron glyph on the left.
pub fn search_field(ui: &mut Ui, text: &mut String, width: f32) -> Response {
    let height = 20.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, color::FIELD);
    p.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, color::FIELD_BORDER),
        egui::StrokeKind::Inside,
    );
    let glass =
        Rect::from_center_size(pos2(rect.left() + 11.0, rect.center().y), Vec2::splat(13.0));
    icons::paint(p, glass, Icon::Search, color::ICON);
    let chev = Rect::from_center_size(
        pos2(rect.left() + 20.0, rect.center().y + 1.0),
        Vec2::splat(9.0),
    );
    icons::paint(p, chev, Icon::ChevronDown, color::ICON);
    let inner = Rect::from_min_max(pos2(rect.left() + 27.0, rect.top()), rect.max);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    let resp = child.add(
        TextEdit::singleline(text)
            .frame(egui::Frame::NONE)
            .desired_width(inner.width() - 6.0)
            .vertical_align(egui::Align::Center)
            .min_size(vec2(0.0, height))
            .text_color(color::TEXT_STRONG),
    );
    let value = text.clone();
    resp.labeled_value(egui::WidgetType::TextEdit, tr("Search"), &value)
}

/// Blue text button, like "Reset" in the Properties panel.
pub fn link(ui: &mut Ui, text: &str) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), theme::body(), color::ACCENT);
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::click());
    let resp = resp.labeled(egui::WidgetType::Link, text);
    let c = if resp.hovered() {
        color::ACCENT_HOVER
    } else {
        color::ACCENT
    };
    ui.painter()
        .galley_with_override_text_color(rect.min, galley, c);
    resp
}

/// Color swatch button (e.g. the Fill color in the toolbar).
pub fn color_swatch(ui: &mut Ui, fill: Color32, size: Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hex = format!("#{:02x}{:02x}{:02x}", fill.r(), fill.g(), fill.b());
    let resp = resp.labeled_value(egui::WidgetType::ColorButton, tr("Color"), &hex);
    let p = ui.painter();
    p.rect_filled(rect, 1.0, fill);
    if resp.hovered() {
        p.rect_stroke(
            rect,
            1.0,
            Stroke::new(1.0, color::TEXT_STRONG),
            egui::StrokeKind::Outside,
        );
    }
    resp
}

/// A switch cell in the timeline: dark box that shows `icon` while on.
pub fn switch_cell(ui: &mut Ui, on: &mut bool, icon: Icon, size: Vec2) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    paint_switch(ui.painter(), rect, *on, icon, resp.hovered());
    resp
}

pub fn paint_switch(p: &egui::Painter, rect: Rect, on: bool, icon: Icon, hovered: bool) {
    p.rect_filled(rect.shrink(0.5), 1.0, color::SWITCH_BG);
    if on {
        let c = if hovered {
            color::TEXT_STRONG
        } else {
            color::ICON
        };
        icons::paint(
            p,
            Rect::from_center_size(rect.center(), Vec2::splat(rect.height().min(14.0))),
            icon,
            c,
        );
    }
}

/// What happened in the workspace bar this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceEvent {
    Switch(usize),
    Reset,
    Save,
    Edit,
}

/// The workspace switcher at the right of the toolbar ("Default ☰  Review  …").
/// Names are translated with [`crate::i18n::tr`]. Items that do not fit go
/// into the » menu.
pub fn workspace_bar(
    ui: &mut Ui,
    rect: Rect,
    names: &[&str],
    active: usize,
) -> Option<WorkspaceEvent> {
    const LEFT: f32 = 23.5;
    const GAP: f32 = 13.0;
    const MENU_W: f32 = 8.0;
    const SPACING: f32 = 24.0;
    const BASELINE: f32 = 25.0;
    let mut event = None;
    let font = theme::body();
    let overflow_w = 30.0;
    let mut x = rect.left() + LEFT;
    let mut hidden = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let text = crate::i18n::tr(name);
        let galley = ui
            .painter()
            .layout_no_wrap(text.to_owned(), font.clone(), color::TEXT);
        let w = galley.size().x + GAP + MENU_W;
        if x + w > rect.right() - overflow_w {
            hidden.push(i);
            continue;
        }
        let item = Rect::from_min_max(pos2(x - 6.0, rect.top()), pos2(x + w + 6.0, rect.bottom()));
        let is_active = i == active;
        let resp = ui
            .interact(item, ui.id().with(("bae-workspace", i)), Sense::click())
            // One workspace is active at a time: radio buttons.
            .labeled_toggle(egui::WidgetType::RadioButton, text, is_active);
        let tc = if is_active || resp.hovered() {
            color::TAB_ACTIVE
        } else {
            color::TAB_INACTIVE
        };
        let top = rect.top() + BASELINE - theme::baseline_offset(&galley);
        let text_w = galley.size().x;
        ui.painter()
            .galley_with_override_text_color(pos2(x, top), galley, tc);
        if is_active {
            let mr = Rect::from_center_size(
                pos2(x + text_w + GAP + MENU_W / 2.0, rect.top() + BASELINE - 4.0),
                Vec2::splat(16.0),
            );
            icons::paint(ui.painter(), mr, Icon::Menu, tc);
            let ul = Rect::from_min_max(
                pos2(x - 1.0, rect.top() + 30.5),
                pos2(x + text_w + GAP + MENU_W, rect.top() + 32.5),
            );
            ui.painter().rect_filled(ul, 1.0, color::TAB_ACTIVE);
            egui::Popup::menu(&resp).show(|ui| {
                ui.set_min_width(220.0);
                if ui
                    .button(crate::i18n::tr("Reset to Saved Layout"))
                    .clicked()
                {
                    event = Some(WorkspaceEvent::Reset);
                }
                if ui
                    .button(crate::i18n::tr("Save Changes to this Workspace"))
                    .clicked()
                {
                    event = Some(WorkspaceEvent::Save);
                }
                ui.separator();
                if ui.button(crate::i18n::tr("Edit Workspaces...")).clicked() {
                    event = Some(WorkspaceEvent::Edit);
                }
            });
        } else if resp.clicked() {
            event = Some(WorkspaceEvent::Switch(i));
        }
        x += w + SPACING;
    }
    let r = Rect::from_center_size(
        pos2(rect.right() - 27.0, rect.top() + BASELINE - 4.5),
        Vec2::splat(14.0),
    );
    let resp = ui
        .interact(r, ui.id().with("bae-workspace-more"), Sense::click())
        .labeled(egui::WidgetType::Button, tr("Workspace Menu"));
    let c = if resp.hovered() {
        color::TEXT_STRONG
    } else {
        color::TAB_INACTIVE
    };
    icons::paint(ui.painter(), r, Icon::ChevronDoubleRight, c);
    egui::Popup::menu(&resp).show(|ui| {
        for i in hidden {
            if ui.button(crate::i18n::tr(names[i])).clicked() {
                event = Some(WorkspaceEvent::Switch(i));
            }
        }
    });
    event
}

/// `0:00:01:00` style timecode for a frame number.
pub fn timecode(frame: i64, fps: f64) -> String {
    let fps_i = fps.round().max(1.0) as i64;
    let sign = if frame < 0 { "-" } else { "" };
    let f = frame.abs();
    let (h, rem) = (f / (3600 * fps_i), f % (3600 * fps_i));
    let (m, rem) = (rem / (60 * fps_i), rem % (60 * fps_i));
    let (s, fr) = (rem / fps_i, rem % fps_i);
    format!("{sign}{h}:{m:02}:{s:02}:{fr:02}")
}

/// Paint text whose width is clipped with an ellipsis, e.g. column headers.
pub fn clipped_text(
    p: &egui::Painter,
    pos: Pos2,
    text: &str,
    font: FontId,
    color: Color32,
    max_w: f32,
) {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, color));
    job.wrap.max_width = max_w;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    let galley = p.layout_job(job);
    p.galley(pos - vec2(0.0, galley.size().y / 2.0), galley, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_numbers_and_chains() {
        assert_eq!(eval("950"), Some(950.0));
        assert_eq!(eval(" -12.5 "), Some(-12.5));
        assert_eq!(eval("950+10"), Some(960.0));
        assert_eq!(eval("10*3-5"), Some(25.0));
        assert_eq!(eval("1/0"), None);
        assert_eq!(eval("abc"), None);
    }

    #[test]
    fn timecode_format() {
        assert_eq!(timecode(30, 30.0), "0:00:01:00");
        assert_eq!(timecode(30 * 61 + 5, 30.0), "0:01:01:05");
        assert_eq!(timecode(-1, 24.0), "-0:00:00:01");
    }
}
