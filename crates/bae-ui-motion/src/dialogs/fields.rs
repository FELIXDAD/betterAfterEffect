//! Controls shared by the motion dialogs: AE's blue timecode hot text, typed
//! timecode parsing (drop-frame aware) and a labeled hot number.

use bae_ui::Accessible as _;
use bae_ui::theme::{self, color};
use bae_ui::widgets::HotNumber;
use egui::{CursorIcon, Id, Key, Response, Sense, TextEdit, Ui, vec2};

/// Frames per second used for timecode digits (30 for 29.97).
pub(crate) fn timecode_base(fps: f64) -> i64 {
    (fps.round() as i64).max(1)
}

/// Whether a rate can use drop-frame timecode (29.97, 59.94).
pub(crate) fn supports_drop_frame(fps: f64) -> bool {
    let base = fps.round();
    (base == 30.0 || base == 60.0)
        && (fps - base).abs() > 1e-3
        && (fps - base * 1000.0 / 1001.0).abs() < 1e-2
}

/// Frames dropped per minute (2 at 29.97, 4 at 59.94).
fn dropped_per_minute(fps: f64) -> i64 {
    timecode_base(fps) / 15
}

/// `0:00:10:00`, or `0;00;10;00` for drop-frame (AE's display).
pub(crate) fn format_timecode(frames: i64, fps: f64, drop_frame: bool) -> String {
    let base = timecode_base(fps);
    let neg = frames < 0;
    let mut f = frames.abs();
    let drop = drop_frame && supports_drop_frame(fps);
    if drop {
        let d = dropped_per_minute(fps);
        let per_min = base * 60 - d;
        let per_10 = per_min * 10 + d;
        let tens = f / per_10;
        let rem = f % per_10;
        f += 9 * d * tens;
        if rem > d {
            f += d * ((rem - d) / per_min);
        }
    }
    let ff = f % base;
    let total_s = f / base;
    let sep = if drop { ';' } else { ':' };
    format!(
        "{}{}{sep}{:02}{sep}{:02}{sep}{:02}",
        if neg { "-" } else { "" },
        total_s / 3600,
        (total_s / 60) % 60,
        total_s % 60,
        ff
    )
}

/// Parse a typed timecode into frames: `0:00:10:00`, `10:00`, `10.00`, or digits
/// only (`1000` = 10 seconds 00 frames: pairs of digits from the right, like
/// AE). Drop-frame timecodes skip the dropped frame numbers.
pub(crate) fn parse_timecode(text: &str, fps: f64, drop_frame: bool) -> Option<i64> {
    let t = text.trim();
    let (neg, t) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    if t.is_empty() {
        return None;
    }
    let mut parts: Vec<i64> = Vec::new();
    if t.chars().all(|c| c.is_ascii_digit()) {
        // Digits only: pairs from the right are frames, seconds, minutes, hours.
        let digits: Vec<char> = t.chars().collect();
        let mut end = digits.len();
        while end > 0 {
            let start = end.saturating_sub(2);
            let s: String = digits[start..end].iter().collect();
            parts.push(s.parse().ok()?);
            end = start;
        }
    } else {
        for p in t.split([':', ';', '.']).rev() {
            if p.is_empty() {
                parts.push(0);
            } else {
                parts.push(p.parse().ok()?);
            }
        }
    }
    if parts.len() > 4 {
        return None;
    }
    let get = |i: usize| parts.get(i).copied().unwrap_or(0);
    let base = timecode_base(fps);
    let (ff, ss, mm, hh) = (get(0), get(1), get(2), get(3));
    let minutes = hh * 60 + mm;
    let mut frames = (minutes * 60 + ss) * base + ff;
    if drop_frame && supports_drop_frame(fps) {
        let d = dropped_per_minute(fps);
        frames -= d * (minutes - minutes / 10);
    }
    Some(if neg { -frames } else { frames })
}

#[derive(Clone, Default)]
struct TimecodeState {
    editing: Option<String>,
    /// Focus the text field in the frame it first appears.
    focus: bool,
    carry: f64,
}

/// Blue timecode hot text in frames: drag to scrub (Shift ×10), click to type.
/// `min` / `max` clamp the value.
pub(crate) fn timecode_field(
    ui: &mut Ui,
    id: Id,
    frames: &mut i64,
    fps: f64,
    drop_frame: bool,
    range: (i64, i64),
    label: &str,
) -> Response {
    let mut state: TimecodeState = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let text = format_timecode(*frames, fps, drop_frame);
    if let Some(mut buf) = state.editing.take() {
        let mut resp = ui.add(
            TextEdit::singleline(&mut buf)
                .id(id.with("edit"))
                .desired_width(84.0)
                .text_color(color::TEXT_STRONG),
        );
        resp = resp.labeled(egui::WidgetType::TextEdit, label);
        if std::mem::take(&mut state.focus) {
            resp.request_focus();
        }
        let (enter, escape) = ui.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        if escape {
            ui.data_mut(|d| d.insert_temp(id, TimecodeState::default()));
        } else if enter || resp.lost_focus() {
            if let Some(v) = parse_timecode(&buf, fps, drop_frame) {
                let v = v.clamp(range.0, range.1);
                if v != *frames {
                    *frames = v;
                    resp.mark_changed();
                }
            }
            ui.data_mut(|d| d.insert_temp(id, TimecodeState::default()));
        } else {
            state.editing = Some(buf);
            ui.data_mut(|d| d.insert_temp(id, state));
        }
        return resp;
    }
    let galley = ui
        .painter()
        .layout_no_wrap(text.clone(), theme::body(), color::ACCENT);
    let size = vec2(galley.size().x + 3.0, ui.spacing().interact_size.y);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    resp = resp.labeled_value(egui::WidgetType::DragValue, label, &text);
    let hot = resp.hovered() || resp.dragged();
    if hot {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }
    if resp.dragged() {
        let factor = if ui.input(|i| i.modifiers.shift) {
            10.0
        } else {
            1.0
        };
        let raw = state.carry + f64::from(resp.drag_delta().x) * 0.5 * factor;
        let step = raw.trunc();
        state.carry = raw - step;
        if step != 0.0 {
            let v = (*frames + step as i64).clamp(range.0, range.1);
            if v != *frames {
                *frames = v;
                resp.mark_changed();
            }
        }
    }
    if resp.clicked() {
        state.editing = Some(text);
        state.focus = true;
        ui.ctx().request_repaint();
    }
    ui.data_mut(|d| d.insert_temp(id, state));
    let c = if hot {
        color::ACCENT_HOVER
    } else {
        color::ACCENT
    };
    ui.painter().galley_with_override_text_color(
        egui::pos2(rect.left() + 1.5, rect.center().y - galley.size().y / 2.0),
        galley,
        c,
    );
    resp
}

/// A hot number with a unit label after it ("1920 px").
pub(crate) fn number(
    ui: &mut Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    unit: &str,
    label: &str,
) -> Response {
    let r = ui.add(
        HotNumber::new(value)
            .range(range)
            .decimals(decimals)
            .field_width(40.0)
            .label(label),
    );
    if !unit.is_empty() {
        ui.label(egui::RichText::new(unit).color(color::TEXT));
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timecodes_format_and_parse() {
        assert_eq!(format_timecode(300, 30.0, false), "0:00:10:00");
        assert_eq!(format_timecode(30 * 3600 + 15, 30.0, false), "1:00:00:15");
        assert_eq!(parse_timecode("0:00:10:00", 30.0, false), Some(300));
        assert_eq!(parse_timecode("10:00", 30.0, false), Some(300));
        assert_eq!(parse_timecode("1000", 30.0, false), Some(300));
        assert_eq!(parse_timecode("5", 25.0, false), Some(5));
        assert_eq!(parse_timecode("1.12", 24.0, false), Some(36));
        assert_eq!(parse_timecode("abc", 30.0, false), None);
        assert_eq!(parse_timecode("1:2:3:4:5", 30.0, false), None);
    }

    #[test]
    fn drop_frame_timecode_skips_frame_numbers() {
        let fps = 30000.0 / 1001.0;
        assert!(supports_drop_frame(fps));
        assert!(!supports_drop_frame(30.0));
        assert!(!supports_drop_frame(25.0));
        // Minute 1 starts at ;02 (frames 00 and 01 are dropped).
        assert_eq!(format_timecode(1800, fps, true), "0;01;00;02");
        assert_eq!(format_timecode(1799, fps, true), "0;00;59;29");
        // Every tenth minute keeps them.
        assert_eq!(format_timecode(17982, fps, true), "0;10;00;00");
        for f in [0, 1799, 1800, 17982, 107892, 123456] {
            let tc = format_timecode(f, fps, true);
            assert_eq!(parse_timecode(&tc, fps, true), Some(f), "{tc}");
        }
        // Non-drop at 29.97 counts 30 per second.
        assert_eq!(format_timecode(1800, fps, false), "0:01:00:00");
    }
}
