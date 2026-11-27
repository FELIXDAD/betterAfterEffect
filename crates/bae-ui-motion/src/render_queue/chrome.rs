//! The Render Queue's chrome as AE 26 draws it (measured with Accessibility
//! Color Contrast off): the Current Render bar with the Render button, the
//! column header with its separator ticks, twirls, the template arrows' boxes
//! and the batch footer.

use super::*;
use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use egui::{Color32, Rect, Sense, Stroke, Vec2, pos2, vec2};

/// Height of a queue row (item, Render Settings, Output Module); rows are
/// separated by a 1 pt [`LINE`].
pub(super) const ROW_H: f32 = 16.0;
/// Column header height.
pub(super) const HEADER_H: f32 = 18.0;
const BAR_H: f32 = 40.0;
/// The Current Render bar and its progress track.
const BAR_BG: Color32 = Color32::from_rgb(0x17, 0x17, 0x17);
const TRACK: Color32 = Color32::from_rgb(0x23, 0x23, 0x23);
/// Background of a render item's row.
pub(super) const ITEM_ROW: Color32 = Color32::from_rgb(0x4b, 0x4b, 0x4b);
/// Rule between rows.
pub(super) const LINE: Color32 = color::SEPARATOR;
/// Tick before each column title.
const TICK: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
/// Idle text of the Current Render bar.
const IDLE: Color32 = Color32::from_rgb(0x6a, 0x6a, 0x6a);

/// What the Current Render bar shows.
#[derive(Default)]
pub(super) struct Progress {
    /// The queue is rendering (Stop instead of Render).
    pub running: bool,
    /// Rendering item: frames done and total, seconds since it started.
    pub current: Option<(usize, usize, f64)>,
    /// Queued items left to render (Render is enabled with one or more).
    pub queued: usize,
}

pub(super) enum BarAction {
    Render,
    Stop,
}

/// AE's Current Render bar: twirl, progress track, percent, frame count, Info,
/// Elapsed and Remaining, and the Render button (Stop while rendering) right of
/// it. `details`: the twirl / Info state.
pub(super) fn current_render(
    ui: &mut egui::Ui,
    p: &Progress,
    details: &mut bool,
) -> Option<BarAction> {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, BAR_H), Sense::hover());
    // Render at the right (dimmed while rendering), Stop left of it while
    // rendering. Interacted first: Tab reaches Render before the bar.
    let button = |n: f32| {
        Rect::from_min_size(
            pos2(rect.right() - 84.0 * n, rect.center().y - 12.0),
            vec2(80.0, 24.0),
        )
    };
    let mut buttons = vec![(
        button(1.0),
        "Render",
        BarAction::Render,
        !p.running && p.queued > 0,
    )];
    if p.running {
        buttons.push((button(2.0), "Stop", BarAction::Stop, true));
    }
    let band = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 2.0),
        pos2(buttons.last().unwrap().0.left() - 12.0, rect.bottom() - 2.0),
    );
    let mut chosen = None;
    let pressed: Vec<_> = buttons
        .into_iter()
        .map(|(r, text, action, enabled)| {
            let sense = if enabled {
                Sense::click()
            } else {
                Sense::hover()
            };
            let resp = ui.interact(r, ui.id().with(("rq-bar", text)), sense);
            resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tr(text))
            });
            if resp.clicked() {
                chosen = Some(action);
            }
            (r, text, enabled, resp.hovered())
        })
        .collect();
    let painter = ui.painter().clone();
    painter.rect_filled(band, 4.0, BAR_BG);
    let ink = if p.current.is_some() {
        color::TEXT
    } else {
        IDLE
    };
    let mid = band.center().y;
    let font = theme::body();

    let twirl = Rect::from_min_size(pos2(band.left() + 4.0, mid - 8.0), vec2(14.0, 16.0));
    let r = ui
        .interact(twirl, ui.id().with("rq-current-twirl"), Sense::click())
        .labeled_toggle(
            egui::WidgetType::CollapsingHeader,
            tr("Current Render"),
            *details,
        );
    if r.clicked() {
        *details = !*details;
    }
    let icon = if *details {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    icons::paint(&painter, twirl.shrink(1.0), icon, ink);
    let title = painter.text(
        pos2(twirl.right() + 4.0, mid),
        egui::Align2::LEFT_CENTER,
        tr("Current Render"),
        theme::font(13.0),
        ink,
    );

    // Right to left: Remaining, Elapsed, Info, frame count, percent; the track
    // takes the rest and the times go first when the panel is narrow.
    let (fraction, counts, elapsed, remaining) = match p.current {
        Some((done, total, secs)) => {
            let fraction = if total == 0 {
                0.0
            } else {
                done as f32 / total as f32
            };
            let remaining = (done > 0).then(|| secs * (total - done) as f64 / done as f64);
            (
                fraction,
                tr("({done} of {total})")
                    .replace("{done}", &done.to_string())
                    .replace("{total}", &total.to_string()),
                clock(Some(secs)),
                clock(remaining),
            )
        }
        None => (0.0, tr("(-- of --)").to_owned(), clock(None), clock(None)),
    };
    let mut x = band.right() - 12.0;
    let times_fit = band.width() > 760.0;
    if times_fit {
        for (label, value) in [("Remaining:", &remaining), ("Elapsed:", &elapsed)] {
            let v = painter.text(
                pos2(x, mid),
                egui::Align2::RIGHT_CENTER,
                value,
                font.clone(),
                ink,
            );
            x = v.left() - 24.0;
            let l = painter.text(
                pos2(x, mid),
                egui::Align2::RIGHT_CENTER,
                tr(label),
                font.clone(),
                ink,
            );
            x = l.left() - 24.0;
        }
    }
    let info = Rect::from_min_max(pos2(x - 64.0, mid - 11.0), pos2(x, mid + 11.0));
    let r = ui
        .interact(info, ui.id().with("rq-info"), Sense::click())
        .labeled_toggle(egui::WidgetType::Button, tr("Info"), *details);
    if r.clicked() {
        *details = !*details;
    }
    let info_ink = if r.hovered() { color::TEXT_STRONG } else { ink };
    painter.rect_stroke(
        info,
        11.0,
        Stroke::new(1.0, info_ink),
        egui::StrokeKind::Inside,
    );
    let dot = pos2(info.left() + 16.0, mid);
    painter.circle_stroke(dot, 6.0, Stroke::new(1.0, info_ink));
    painter.text(
        dot,
        egui::Align2::CENTER_CENTER,
        "i",
        theme::small(),
        info_ink,
    );
    painter.text(
        pos2(info.left() + 27.0, mid),
        egui::Align2::LEFT_CENTER,
        tr("Info"),
        font.clone(),
        info_ink,
    );
    x = info.left() - 24.0;
    let c = painter.text(
        pos2(x, mid),
        egui::Align2::RIGHT_CENTER,
        &counts,
        font.clone(),
        ink,
    );
    x = c.left() - 28.0;
    let pct = format!("{}%", (fraction * 100.0).round() as i32);
    let pc = painter.text(pos2(x, mid), egui::Align2::RIGHT_CENTER, &pct, font, ink);
    let track = Rect::from_min_max(
        pos2(title.right() + 16.0, mid - 8.5),
        pos2(pc.left() - 16.0, mid + 8.5),
    );
    if track.width() > 20.0 {
        painter.rect_filled(track, 8.5, TRACK);
        if fraction > 0.0 {
            let done = Rect::from_min_size(
                track.min,
                vec2(track.width() * fraction.min(1.0), track.height()),
            );
            painter.rect_filled(done, 8.5, color::ACCENT);
        }
    }

    for (r, text, enabled, hovered) in pressed {
        let ink = match (enabled, hovered) {
            (false, _) => IDLE,
            (true, true) => color::TEXT_STRONG,
            (true, false) => color::TEXT,
        };
        painter.rect_stroke(
            r,
            12.0,
            Stroke::new(1.0, if enabled { color::TEXT_DIM } else { IDLE }),
            egui::StrokeKind::Inside,
        );
        painter.text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            tr(text),
            theme::body(),
            ink,
        );
    }
    chosen
}

/// `--:--:--` or h:mm:ss.
fn clock(seconds: Option<f64>) -> String {
    match seconds {
        Some(s) => {
            let s = s.max(0.0).round() as u64;
            format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
        }
        None => "--:--:--".to_owned(),
    }
}

/// AE's column header: a 1 pt rule above and below, a tick before each title
/// (the Label column shows the tag icon). `menu` attaches the column menu.
pub(super) fn header(
    ui: &mut egui::Ui,
    columns: &[&Column],
    mut menu: impl FnMut(&egui::Response, &Column),
) {
    let width: f32 = columns
        .iter()
        .map(|c| c.width)
        .sum::<f32>()
        .max(ui.available_width());
    let (rect, _) = ui.allocate_exact_size(vec2(width, HEADER_H + 2.0), Sense::hover());
    let p = ui.painter().clone();
    p.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, LINE));
    p.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, LINE));
    let mut x = rect.left();
    for col in columns {
        let cell = Rect::from_min_size(pos2(x, rect.top() + 1.0), vec2(col.width, HEADER_H));
        p.rect_filled(
            Rect::from_center_size(pos2(x + 2.5, cell.center().y), vec2(3.0, 10.0)),
            0.0,
            TICK,
        );
        let resp = ui
            .interact(cell, ui.id().with(("rq-col", col.title)), Sense::click())
            .labeled(egui::WidgetType::Label, tr(col.title));
        if col.title == "Label" {
            icons::paint(
                &p,
                Rect::from_center_size(pos2(x + 13.0, cell.center().y), Vec2::splat(12.0)),
                Icon::Tag,
                color::ICON,
            );
        } else {
            p.text(
                pos2(x + 7.0, cell.center().y),
                egui::Align2::LEFT_CENTER,
                tr(col.title),
                theme::small(),
                color::TEXT,
            );
        }
        menu(&resp, col);
        x += col.width;
    }
}

/// A queue twirl named after its row ("Render Item 1 “Comp 1” · Render Settings").
pub(super) fn twirl(ui: &mut egui::Ui, open: &mut bool, name: &str) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(14.0, ROW_H), Sense::click());
    if resp.clicked() {
        *open = !*open;
        resp.mark_changed();
    }
    let resp = resp.labeled_toggle(egui::WidgetType::CollapsingHeader, name, *open);
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
        Rect::from_center_size(rect.center(), Vec2::splat(11.0)),
        icon,
        c,
    );
    resp
}

/// AE's template arrow: a small dark box with a chevron.
pub(super) fn chevron_box(ui: &mut egui::Ui, name: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(18.0, ROW_H - 1.0), Sense::click());
    let resp = resp.labeled(egui::WidgetType::Button, name);
    let ink = bae_ui::widgets::field_frame(ui.painter(), rect, resp.hovered(), ui.is_enabled());
    icons::paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(10.0)),
        Icon::ChevronDown,
        ink,
    );
    resp
}

/// The rule under a queue row.
pub(super) fn rule(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, LINE);
}

/// AE's batch footer: Message, RAM, Renders Started, Total Time Elapsed, at
/// AE's offsets.
pub(super) fn footer(ui: &mut egui::Ui, message: &str, started: Option<&str>, total: Option<f64>) {
    let total = total.map(|s| clock(Some(s))).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_H + 4.0);
        let left = ui.cursor().left();
        for (x, label, value) in [
            (0.0, tr("Message:"), message),
            (274.0, "RAM:", ""),
            (525.0, tr("Renders Started:"), started.unwrap_or("")),
            (842.0, tr("Total Time Elapsed:"), total.as_str()),
        ] {
            let at = ui.cursor().left();
            ui.add_space((left + x - at).max(if x == 0.0 { 0.0 } else { 12.0 }));
            ui.label(label);
            ui.label(value);
        }
    });
}

/// AE's dropdown field in a queue row (Log): dark well, thin border, the value
/// and a chevron; `menu` fills the popup.
pub(super) fn field_menu(
    ui: &mut egui::Ui,
    width: f32,
    value: &str,
    name: &str,
    menu: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H - 1.0), Sense::click());
    let resp = resp.labeled_value(egui::WidgetType::ComboBox, name, value);
    let p = ui.painter();
    let ink = bae_ui::widgets::field_frame(p, rect, resp.hovered(), ui.is_enabled());
    p.text(
        pos2(rect.left() + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        value,
        theme::body(),
        ink,
    );
    icons::paint(
        p,
        Rect::from_center_size(
            pos2(rect.right() - 12.0, rect.center().y),
            Vec2::splat(10.0),
        ),
        Icon::ChevronDown,
        ink,
    );
    egui::Popup::menu(&resp).width(width).show(menu);
    resp
}
