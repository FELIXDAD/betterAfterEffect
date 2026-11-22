//! AE's Color Picker, shared by every workspace: a modal dialog with a color
//! field and a slider for one channel (H, S, B, R, G or B, chosen with the radio
//! buttons; Hue by default), numeric H S B / R G B fields, a hex field, the
//! new / original swatches and the eyedropper.
//!
//! - [`color_button`]: the swatch that opens the picker (Solid Settings, color
//!   properties in the timeline and Effect Controls, Character panel …).
//! - [`picker`]: the dialog itself, for callers that open it some other way.
//! - [`eyedropper_button`]: the stand-alone eyedropper next to a swatch.
//!
//! Colors are straight RGB(A) 0–1, as stored in the project. The numeric
//! fields show 8-bit values (0–255), hue in degrees, saturation and brightness
//! in percent, like AE in an 8 bpc project.
//!
//! The eyedropper samples the window under the pointer: it asks the backend
//! for a screenshot (`ViewportCommand::Screenshot`) and reads the pixel from
//! the `Event::Screenshot` reply. Backends without screenshots leave the
//! color unchanged.

use egui::{
    Color32, CursorIcon, Id, Key, Mesh, Order, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2,
};

use crate::a11y::Accessible as _;
use crate::dialog::{self, Outcome};
use crate::i18n::tr;
use crate::theme::color;
use crate::widgets::HotNumber;

// ---- conversions -------------------------------------------------------------------

/// RGB 0–1 → HSB: hue in degrees 0–360 (0 for grays), saturation and
/// brightness 0–1.
pub fn rgb_to_hsb(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(|c| c.clamp(0.0, 1.0));
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let s = if max > 0.0 { d / max } else { 0.0 };
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    [h, s, max]
}

/// HSB (hue in degrees, saturation and brightness 0–1) → RGB 0–1.
pub fn hsb_to_rgb(hsb: [f64; 3]) -> [f64; 3] {
    let h = hsb[0].rem_euclid(360.0) / 60.0;
    let s = hsb[1].clamp(0.0, 1.0);
    let v = hsb[2].clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

/// 8-bit channel values of an RGB 0–1 color (rounded, clamped).
pub fn to_8bit(rgb: [f64; 3]) -> [u8; 3] {
    rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// `"FF8000"`: the hex field of AE's picker (upper case, no `#`).
pub fn to_hex(rgb: [f64; 3]) -> String {
    let [r, g, b] = to_8bit(rgb);
    format!("{r:02X}{g:02X}{b:02X}")
}

/// Parse a hex color: `FF8000`, `#ff8000` or the short `F80`.
pub fn parse_hex(s: &str) -> Option<[f64; 3]> {
    let s = s.trim().trim_start_matches('#');
    let digits: Vec<u8> = s
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<_>>()?;
    let bytes: [u8; 3] = match digits.len() {
        3 => [digits[0] * 17, digits[1] * 17, digits[2] * 17],
        6 => [
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
        ],
        _ => return None,
    };
    Some(bytes.map(|b| f64::from(b) / 255.0))
}

/// The nearest web-safe color (each channel a multiple of 0x33).
pub fn web_safe(rgb: [f64; 3]) -> [f64; 3] {
    rgb.map(|c| (c.clamp(0.0, 1.0) * 5.0).round() / 5.0)
}

fn color32(rgb: [f64; 3]) -> Color32 {
    let [r, g, b] = to_8bit(rgb);
    Color32::from_rgb(r, g, b)
}

// ---- picker state ------------------------------------------------------------------

/// The channel the slider edits (the field shows the other two of its model).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Channel {
    #[default]
    Hue,
    Saturation,
    Brightness,
    Red,
    Green,
    Blue,
}

impl Channel {
    const ALL: [Channel; 6] = [
        Channel::Hue,
        Channel::Saturation,
        Channel::Brightness,
        Channel::Red,
        Channel::Green,
        Channel::Blue,
    ];

    /// Letter shown next to the radio button.
    fn letter(self) -> &'static str {
        match self {
            Channel::Hue => "H:",
            Channel::Saturation => "S:",
            Channel::Brightness | Channel::Blue => "B:",
            Channel::Red => "R:",
            Channel::Green => "G:",
        }
    }

    /// Name for screen readers (English, translated with `tr`).
    fn name(self) -> &'static str {
        match self {
            Channel::Hue => "Hue",
            Channel::Saturation => "Saturation",
            Channel::Brightness => "Brightness",
            Channel::Red => "Red",
            Channel::Green => "Green",
            Channel::Blue => "Blue",
        }
    }
}

/// The color being edited, in both models (HSB keeps the hue of grays).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickerColor {
    /// Hue degrees, saturation 0–1, brightness 0–1.
    pub hsb: [f64; 3],
    /// RGB 0–1.
    pub rgb: [f64; 3],
}

impl PickerColor {
    pub fn from_rgb(rgb: [f64; 3]) -> Self {
        PickerColor {
            hsb: rgb_to_hsb(rgb),
            rgb: rgb.map(|c| c.clamp(0.0, 1.0)),
        }
    }

    fn set_hsb(&mut self, hsb: [f64; 3]) {
        self.hsb = [
            hsb[0].clamp(0.0, 360.0),
            hsb[1].clamp(0.0, 1.0),
            hsb[2].clamp(0.0, 1.0),
        ];
        self.rgb = hsb_to_rgb(self.hsb);
    }

    fn set_rgb(&mut self, rgb: [f64; 3]) {
        let hue = self.hsb[0];
        *self = PickerColor::from_rgb(rgb);
        // Grays and black have no hue: keep the one the user had.
        if self.hsb[1] == 0.0 || self.hsb[2] == 0.0 {
            self.hsb[0] = hue;
        }
    }

    /// Field position (x right, y up, 0–1) of this color for `channel`.
    pub fn field_pos(&self, channel: Channel) -> [f64; 2] {
        let [h, s, v] = self.hsb;
        let [r, g, b] = self.rgb;
        match channel {
            Channel::Hue => [s, v],
            Channel::Saturation => [h / 360.0, v],
            Channel::Brightness => [h / 360.0, s],
            Channel::Red => [b, g],
            Channel::Green => [b, r],
            Channel::Blue => [r, g],
        }
    }

    /// Slider position (0 bottom – 1 top) of this color for `channel`.
    pub fn slider_pos(&self, channel: Channel) -> f64 {
        let [h, s, v] = self.hsb;
        let [r, g, b] = self.rgb;
        match channel {
            Channel::Hue => h / 360.0,
            Channel::Saturation => s,
            Channel::Brightness => v,
            Channel::Red => r,
            Channel::Green => g,
            Channel::Blue => b,
        }
    }

    /// The color at field position `p` (the slider channel unchanged).
    pub fn at_field(&self, channel: Channel, p: [f64; 2]) -> PickerColor {
        let [x, y] = p.map(|c| c.clamp(0.0, 1.0));
        let mut out = *self;
        let [h, s, v] = self.hsb;
        let [r, g, b] = self.rgb;
        match channel {
            Channel::Hue => out.set_hsb([h, x, y]),
            Channel::Saturation => out.set_hsb([x * 360.0, s, y]),
            Channel::Brightness => out.set_hsb([x * 360.0, y, v]),
            Channel::Red => out.set_rgb([r, y, x]),
            Channel::Green => out.set_rgb([y, g, x]),
            Channel::Blue => out.set_rgb([x, y, b]),
        }
        out
    }

    /// The color with the slider channel at `t` (0 bottom – 1 top).
    pub fn at_slider(&self, channel: Channel, t: f64) -> PickerColor {
        let t = t.clamp(0.0, 1.0);
        let mut out = *self;
        let [h, s, v] = self.hsb;
        let [r, g, b] = self.rgb;
        match channel {
            Channel::Hue => out.set_hsb([t * 360.0, s, v]),
            Channel::Saturation => out.set_hsb([h, t, v]),
            Channel::Brightness => out.set_hsb([h, s, t]),
            Channel::Red => out.set_rgb([t, g, b]),
            Channel::Green => out.set_rgb([r, t, b]),
            Channel::Blue => out.set_rgb([r, g, t]),
        }
        out
    }

    /// What the slider shows at `t`: for Hue the pure hues, otherwise the
    /// current color with that channel at `t`.
    fn slider_color(&self, channel: Channel, t: f64) -> [f64; 3] {
        match channel {
            Channel::Hue => hsb_to_rgb([t * 360.0, 1.0, 1.0]),
            _ => self.at_slider(channel, t).rgb,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PickerState {
    original: [f64; 4],
    color: PickerColor,
    hex: Option<String>,
    web_only: bool,
}

/// What a picker or color button did this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColorEdit {
    None,
    /// The color changed in the open picker (preview it).
    Live([f64; 4]),
    /// OK: the new color.
    Commit([f64; 4]),
    /// Cancel: drop any preview.
    Cancel,
}

fn channel_id() -> Id {
    Id::new("bae-ui.color-picker.channel")
}

fn open_flag_id() -> Id {
    Id::new("bae-ui.color-picker.open")
}

/// A color picker was shown in the previous pass (it is modal: the workbench
/// routes no shortcuts to the project while one is open).
pub fn open_last_pass(ctx: &egui::Context) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<u64>(open_flag_id()))
        .is_some_and(|p| p + 1 >= pass)
}

/// The Color Picker dialog titled `title` (translated by the caller), opened on
/// `color`. Call it every frame while it is open; the state lives under `id`
/// and is dropped when it returns `Commit` or `Cancel`.
pub fn picker(ctx: &egui::Context, id: Id, title: &str, color: [f64; 4]) -> ColorEdit {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(open_flag_id(), pass));
    let mut st: PickerState = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| PickerState {
        original: color,
        color: PickerColor::from_rgb([color[0], color[1], color[2]]),
        hex: None,
        web_only: false,
    });
    let mut channel: Channel = ctx.data(|d| d.get_temp(channel_id())).unwrap_or_default();
    let before = st.color;
    let shown = dialog::window(ctx, id.with("window"), title, |ui| {
        let out = picker_body(ui, id, &mut st, &mut channel);
        let keys = dialog::keys(ui.ctx(), true);
        if out == Outcome::None { keys } else { out }
    });
    let closed = dialog::closed_by_title_bar(ctx, id.with("window"));
    ctx.data_mut(|d| d.insert_temp(channel_id(), channel));
    let rgba = |c: &PickerColor| [c.rgb[0], c.rgb[1], c.rgb[2], st.original[3]];
    match shown {
        Some(Outcome::Ok) => {
            ctx.data_mut(|d| d.remove::<PickerState>(id));
            ColorEdit::Commit(rgba(&st.color))
        }
        Some(Outcome::Cancel) => {
            ctx.data_mut(|d| d.remove::<PickerState>(id));
            ColorEdit::Cancel
        }
        None if closed => {
            ctx.data_mut(|d| d.remove::<PickerState>(id));
            ColorEdit::Cancel
        }
        _ => {
            let changed = st.color != before;
            let live = rgba(&st.color);
            ctx.data_mut(|d| d.insert_temp(id, st));
            if changed {
                ColorEdit::Live(live)
            } else {
                ColorEdit::None
            }
        }
    }
}

const FIELD: f32 = 200.0;
const SLIDER_W: f32 = 18.0;

fn picker_body(ui: &mut Ui, id: Id, st: &mut PickerState, channel: &mut Channel) -> Outcome {
    let mut out = Outcome::None;
    ui.horizontal_top(|ui| {
        // Field and slider.
        let (field, field_resp) =
            ui.allocate_exact_size(Vec2::splat(FIELD), Sense::click_and_drag());
        let field_resp = field_resp.labeled(egui::WidgetType::Other, tr("Color Field"));
        ui.add_space(6.0);
        let (slider, slider_resp) =
            ui.allocate_exact_size(vec2(SLIDER_W + 10.0, FIELD), Sense::click_and_drag());
        let slider_resp = slider_resp.labeled(egui::WidgetType::Slider, tr("Color Slider"));
        let bar = Rect::from_min_size(slider.min + vec2(5.0, 0.0), vec2(SLIDER_W, FIELD));
        if let Some(p) = field_resp.interact_pointer_pos()
            && (field_resp.dragged() || field_resp.clicked() || field_resp.drag_started())
        {
            let x = ((p.x - field.left()) / field.width()) as f64;
            let y = ((field.bottom() - p.y) / field.height()) as f64;
            st.color = st.color.at_field(*channel, [x, y]);
            st.hex = None;
        }
        if let Some(p) = slider_resp.interact_pointer_pos()
            && (slider_resp.dragged() || slider_resp.clicked() || slider_resp.drag_started())
        {
            let t = ((bar.bottom() - p.y) / bar.height()) as f64;
            st.color = st.color.at_slider(*channel, t);
            st.hex = None;
        }
        if st.web_only {
            st.color.set_rgb(web_safe(st.color.rgb));
        }
        paint_field(ui, field, &st.color, *channel);
        paint_slider(ui, bar, &st.color, *channel);

        ui.add_space(10.0);
        ui.vertical(|ui| {
            // New / original swatches; clicking the original restores it.
            let (sw, sw_resp) = ui.allocate_exact_size(vec2(64.0, 56.0), Sense::click());
            let sw_resp = sw_resp.labeled(egui::WidgetType::Button, tr("Original Color"));
            let half = sw.height() / 2.0;
            let top = Rect::from_min_size(sw.min, vec2(sw.width(), half));
            let bottom = Rect::from_min_size(sw.min + vec2(0.0, half), vec2(sw.width(), half));
            let p = ui.painter();
            p.rect_filled(top, 0.0, color32(st.color.rgb));
            p.rect_filled(
                bottom,
                0.0,
                color32([st.original[0], st.original[1], st.original[2]]),
            );
            p.rect_stroke(
                sw,
                0.0,
                Stroke::new(1.0, color::DIVIDER),
                egui::StrokeKind::Outside,
            );
            if sw_resp.clicked()
                && sw_resp
                    .interact_pointer_pos()
                    .is_some_and(|p| bottom.contains(p))
            {
                st.color
                    .set_rgb([st.original[0], st.original[1], st.original[2]]);
                st.hex = None;
            }
            ui.add_space(8.0);
            numeric_fields(ui, st, channel);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("#");
                let mut hex = st.hex.clone().unwrap_or_else(|| to_hex(st.color.rgb));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut hex)
                        .desired_width(64.0)
                        .char_limit(7)
                        .text_color(color::TEXT_STRONG),
                );
                let resp = resp.labeled(egui::WidgetType::TextEdit, tr("Hex Color"));
                if resp.changed() {
                    if let Some(rgb) = parse_hex(&hex) {
                        st.color.set_rgb(rgb);
                    }
                    st.hex = Some(hex);
                } else if resp.lost_focus() {
                    st.hex = None;
                }
                if let Some(rgb) = eyedropper_button(ui, id.with("dropper")) {
                    st.color.set_rgb(rgb);
                    st.hex = None;
                }
            });
            ui.add_space(4.0);
            crate::widgets::checkbox(ui, &mut st.web_only, tr("Only Web Colors"));
        });

        ui.add_space(10.0);
        // OK / Cancel stacked at the top right (Adobe picker layout).
        ui.vertical(|ui| {
            let ok = egui::Button::new(egui::RichText::new(tr("OK")).color(color::TEXT_STRONG))
                .fill(color::ACCENT_DIM)
                .min_size(vec2(72.0, 22.0));
            if ui.add(ok).clicked() {
                out = Outcome::Ok;
            }
            let cancel =
                egui::Button::new(egui::RichText::new(tr("Cancel")).color(color::TEXT_STRONG))
                    .min_size(vec2(72.0, 22.0));
            if ui.add(cancel).clicked() {
                out = Outcome::Cancel;
            }
        });
    });
    out
}

fn numeric_fields(ui: &mut Ui, st: &mut PickerState, channel: &mut Channel) {
    egui::Grid::new("bae-ui.color-picker.fields")
        .num_columns(2)
        .spacing(vec2(14.0, 4.0))
        .show(ui, |ui| {
            for row in 0..3 {
                for ch in [Channel::ALL[row], Channel::ALL[row + 3]] {
                    ui.horizontal(|ui| {
                        let selected = *channel == ch;
                        let r = ui.radio(selected, ch.letter()).labeled_toggle(
                            egui::WidgetType::RadioButton,
                            tr(ch.name()),
                            selected,
                        );
                        if r.clicked() {
                            *channel = ch;
                        }
                        let (mut v, max, suffix) = match ch {
                            Channel::Hue => (st.color.hsb[0].round(), 360.0, "°"),
                            Channel::Saturation => ((st.color.hsb[1] * 100.0).round(), 100.0, "%"),
                            Channel::Brightness => ((st.color.hsb[2] * 100.0).round(), 100.0, "%"),
                            Channel::Red => (f64::from(to_8bit(st.color.rgb)[0]), 255.0, ""),
                            Channel::Green => (f64::from(to_8bit(st.color.rgb)[1]), 255.0, ""),
                            Channel::Blue => (f64::from(to_8bit(st.color.rgb)[2]), 255.0, ""),
                        };
                        let name = format!("{} {}", tr(ch.name()), tr("Value"));
                        let resp = ui.add(
                            HotNumber::new(&mut v)
                                .range(0.0..=max)
                                .suffix(suffix)
                                .field_width(30.0)
                                .label(&name),
                        );
                        if resp.changed() {
                            let mut hsb = st.color.hsb;
                            let mut rgb = st.color.rgb;
                            match ch {
                                Channel::Hue => hsb[0] = v,
                                Channel::Saturation => hsb[1] = v / 100.0,
                                Channel::Brightness => hsb[2] = v / 100.0,
                                Channel::Red => rgb[0] = v / 255.0,
                                Channel::Green => rgb[1] = v / 255.0,
                                Channel::Blue => rgb[2] = v / 255.0,
                            }
                            if matches!(
                                ch,
                                Channel::Hue | Channel::Saturation | Channel::Brightness
                            ) {
                                st.color.set_hsb(hsb);
                            } else {
                                st.color.set_rgb(rgb);
                            }
                            st.hex = None;
                        }
                    });
                }
                ui.end_row();
            }
        });
}

/// Grid resolution of the field mesh (hue is not linear across the field).
const GRID: usize = 24;

fn paint_field(ui: &Ui, rect: Rect, c: &PickerColor, channel: Channel) {
    let mut mesh = Mesh::default();
    for j in 0..=GRID {
        for i in 0..=GRID {
            let x = i as f64 / GRID as f64;
            let y = j as f64 / GRID as f64;
            let rgb = c.at_field(channel, [x, y]).rgb;
            let pos = pos2(
                rect.left() + x as f32 * rect.width(),
                rect.bottom() - y as f32 * rect.height(),
            );
            mesh.colored_vertex(pos, color32(rgb));
        }
    }
    let row = (GRID + 1) as u32;
    for j in 0..GRID as u32 {
        for i in 0..GRID as u32 {
            let a = j * row + i;
            mesh.add_triangle(a, a + 1, a + row);
            mesh.add_triangle(a + 1, a + row + 1, a + row);
        }
    }
    let p = ui.painter_at(rect.expand(6.0));
    p.add(mesh);
    p.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, color::DIVIDER),
        egui::StrokeKind::Outside,
    );
    let [x, y] = c.field_pos(channel);
    let center = pos2(
        rect.left() + x as f32 * rect.width(),
        rect.bottom() - y as f32 * rect.height(),
    );
    let ring = if c.hsb[2] > 0.6 && c.hsb[1] < 0.5 {
        Color32::BLACK
    } else {
        Color32::WHITE
    };
    p.circle_stroke(center, 5.0, Stroke::new(1.5, ring));
}

fn paint_slider(ui: &Ui, bar: Rect, c: &PickerColor, channel: Channel) {
    let mut mesh = Mesh::default();
    let steps = if channel == Channel::Hue { 36 } else { 12 };
    for k in 0..=steps {
        let t = k as f64 / steps as f64;
        let y = bar.bottom() - t as f32 * bar.height();
        let col = color32(c.slider_color(channel, t));
        mesh.colored_vertex(pos2(bar.left(), y), col);
        mesh.colored_vertex(pos2(bar.right(), y), col);
    }
    for k in 0..steps as u32 {
        let a = k * 2;
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 3, a + 2);
    }
    let p = ui.painter_at(bar.expand2(vec2(6.0, 6.0)));
    p.add(mesh);
    p.rect_stroke(
        bar,
        0.0,
        Stroke::new(1.0, color::DIVIDER),
        egui::StrokeKind::Outside,
    );
    let y = bar.bottom() - c.slider_pos(channel) as f32 * bar.height();
    for (x, dir) in [(bar.left() - 1.0, 1.0), (bar.right() + 1.0, -1.0)] {
        let tip = pos2(x, y);
        p.add(egui::Shape::convex_polygon(
            vec![
                tip,
                pos2(x - 5.0 * dir, y - 4.0),
                pos2(x - 5.0 * dir, y + 4.0),
            ],
            color::TEXT_STRONG,
            Stroke::NONE,
        ));
    }
}

// ---- swatch button -------------------------------------------------------------------

/// A color swatch that opens the Color Picker titled `title` when clicked.
/// While the picker is open it returns [`ColorEdit::Live`] on every change;
/// OK writes the color into `rgba` and returns [`ColorEdit::Commit`].
/// `label` names the swatch for screen readers.
pub fn color_button(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    rgba: &mut [f64; 4],
    size: Vec2,
    title: &str,
    label: &str,
) -> ColorEdit {
    let id = ui.id().with(("bae-ui.color-button", id_salt));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hex = format!("#{}", to_hex([rgba[0], rgba[1], rgba[2]]));
    let resp = resp.labeled_value(egui::WidgetType::ColorButton, label, &hex);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect_filled(rect, 1.0, color32([rgba[0], rgba[1], rgba[2]]));
        let stroke = if resp.hovered() {
            color::TEXT_STRONG
        } else {
            color::DIVIDER
        };
        p.rect_stroke(
            rect,
            1.0,
            Stroke::new(1.0, stroke),
            egui::StrokeKind::Outside,
        );
    }
    let open_id = id.with("open");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    if resp.clicked() && !open {
        open = true;
        // The field that had focus stays behind the modal picker.
        ui.memory_mut(|m| {
            if let Some(f) = m.focused() {
                m.surrender_focus(f);
            }
        });
    }
    let mut out = ColorEdit::None;
    if open {
        out = picker(ui.ctx(), id.with("picker"), title, *rgba);
        match out {
            ColorEdit::Commit(c) => {
                *rgba = c;
                open = false;
            }
            ColorEdit::Cancel => open = false,
            _ => {}
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    out
}

// ---- eyedropper ----------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Dropper {
    /// Waiting for the click (crosshair over the window).
    Aiming,
    /// Screenshot requested at this point (points) in this pass.
    Waiting(Pos2, u64),
}

/// Passes to wait for a screenshot before giving up (backend without support).
const SCREENSHOT_PASSES: u64 = 10;

/// The eyedropper button: click it, then click anywhere in the window to take
/// that pixel's color. Returns the color in the pass it is sampled.
pub fn eyedropper_button(ui: &mut Ui, id: Id) -> Option<[f64; 3]> {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
    let state: Option<Dropper> = ui.data(|d| d.get_temp(id));
    let resp = resp.labeled_toggle(egui::WidgetType::Button, tr("Eyedropper"), state.is_some());
    if ui.is_rect_visible(rect) {
        let active = state.is_some() || resp.hovered();
        let c = if active { color::ACCENT } else { color::ICON };
        crate::icons::paint(
            ui.painter(),
            rect.shrink(1.0),
            crate::icons::Icon::Eyedropper,
            c,
        );
    }
    if resp.clicked() && state.is_none() {
        ui.data_mut(|d| d.insert_temp(id, Dropper::Aiming));
        return None;
    }
    eyedropper(ui.ctx(), id)
}

/// Run the eyedropper whose state lives under `id` (see [`eyedropper_button`]).
pub fn eyedropper(ctx: &egui::Context, id: Id) -> Option<[f64; 3]> {
    let state: Option<Dropper> = ctx.data(|d| d.get_temp(id));
    match state? {
        Dropper::Aiming => {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
                ctx.data_mut(|d| d.remove::<Dropper>(id));
                return None;
            }
            let rect = ctx.content_rect();
            let clicked = egui::Area::new(id.with("aim"))
                .order(Order::Tooltip)
                .fixed_pos(rect.min)
                .show(ctx, |ui| {
                    let resp = ui.allocate_rect(rect, Sense::click());
                    ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
                    resp.clicked()
                        .then(|| resp.interact_pointer_pos())
                        .flatten()
                })
                .inner;
            if let Some(pos) = clicked {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    id.value(),
                )));
                let pass = ctx.cumulative_pass_nr();
                ctx.data_mut(|d| d.insert_temp(id, Dropper::Waiting(pos, pass)));
                ctx.request_repaint();
            }
            None
        }
        Dropper::Waiting(pos, since) => {
            let ppp = ctx.pixels_per_point();
            let sampled = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot {
                        user_data, image, ..
                    } if user_data
                        .data
                        .as_ref()
                        .and_then(|d| d.downcast_ref::<u64>())
                        == Some(&id.value()) =>
                    {
                        Some(sample(image, pos, ppp))
                    }
                    _ => None,
                })
            });
            if let Some(rgb) = sampled {
                ctx.data_mut(|d| d.remove::<Dropper>(id));
                return rgb;
            }
            if ctx.cumulative_pass_nr() > since + SCREENSHOT_PASSES {
                tracing::debug!("eyedropper: no screenshot from the backend");
                ctx.data_mut(|d| d.remove::<Dropper>(id));
            } else {
                ctx.request_repaint();
            }
            None
        }
    }
}

/// The pixel at `pos` (points) of a screenshot.
fn sample(image: &egui::ColorImage, pos: Pos2, pixels_per_point: f32) -> Option<[f64; 3]> {
    let [w, h] = image.size;
    let x = (pos.x * pixels_per_point).floor() as isize;
    let y = (pos.y * pixels_per_point).floor() as isize;
    if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
        return None;
    }
    let c = image.pixels[y as usize * w + x as usize];
    Some([c.r(), c.g(), c.b()].map(|v| f64::from(v) / 255.0))
}

/// The labels this module gives its widgets (for the i18n completeness test).
pub const LABELS: &[&str] = &[
    "Color Field",
    "Color Slider",
    "Original Color",
    "Hex Color",
    "Eyedropper",
    "Only Web Colors",
    "Hue",
    "Saturation",
    "Brightness",
    "Red",
    "Green",
    "Blue",
    "Value",
    "OK",
    "Cancel",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn hsb_round_trips() {
        for rgb in [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.5, 0.0],
            [0.2, 0.4, 0.6],
            [0.5, 0.5, 0.5],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.9, 0.1, 0.7],
        ] {
            let hsb = rgb_to_hsb(rgb);
            assert!(close(hsb_to_rgb(hsb), rgb), "{rgb:?} → {hsb:?}");
        }
    }

    #[test]
    fn hsb_values_match_ae() {
        // AE's picker: pure orange FF8000 is H 30°, S 100%, B 100%.
        let hsb = rgb_to_hsb([1.0, 128.0 / 255.0, 0.0]);
        assert!((hsb[0] - 30.1176).abs() < 1e-3);
        assert_eq!(hsb[1], 1.0);
        assert_eq!(hsb[2], 1.0);
        // Grays have no hue and no saturation.
        assert_eq!(rgb_to_hsb([0.5, 0.5, 0.5]), [0.0, 0.0, 0.5]);
        // Hue wraps: 360° is red.
        assert!(close(hsb_to_rgb([360.0, 1.0, 1.0]), [1.0, 0.0, 0.0]));
        assert!(close(hsb_to_rgb([240.0, 0.5, 0.8]), [0.4, 0.4, 0.8]));
    }

    #[test]
    fn hex_parses_and_formats() {
        assert_eq!(to_hex([1.0, 128.0 / 255.0, 0.0]), "FF8000");
        assert_eq!(parse_hex("FF8000"), Some([1.0, 128.0 / 255.0, 0.0]));
        assert_eq!(parse_hex("#ff8000"), parse_hex("FF8000"));
        assert_eq!(parse_hex("f80"), Some([1.0, 136.0 / 255.0, 0.0]));
        assert_eq!(parse_hex("12345"), None);
        assert_eq!(parse_hex("GG0000"), None);
        assert_eq!(to_8bit([1.2, -0.1, 0.5]), [255, 0, 128]);
        assert_eq!(web_safe([0.9, 0.31, 0.0]), [1.0, 0.4, 0.0]);
    }

    #[test]
    fn field_and_slider_edit_the_right_channels() {
        let c = PickerColor::from_rgb([1.0, 0.0, 0.0]);
        // Hue mode: the field is saturation × brightness.
        let d = c.at_field(Channel::Hue, [0.5, 0.5]);
        assert!(close(d.rgb, [0.5, 0.25, 0.25]));
        assert_eq!(d.field_pos(Channel::Hue), [0.5, 0.5]);
        // The slider changes only the hue.
        let e = d.at_slider(Channel::Hue, 1.0 / 3.0);
        assert!((e.hsb[0] - 120.0).abs() < 1e-9);
        assert_eq!(e.hsb[1], 0.5);
        // RGB modes edit channels directly: Red mode field is blue × green.
        let f = c.at_field(Channel::Red, [0.25, 0.75]);
        assert!(close(f.rgb, [1.0, 0.75, 0.25]));
        assert_eq!(f.slider_pos(Channel::Red), 1.0);
        // Dragging to black keeps the hue for the next edit.
        let g = e.at_field(Channel::Hue, [1.0, 0.0]);
        assert!((g.hsb[0] - 120.0).abs() < 1e-9);
        let back = g.at_field(Channel::Hue, [1.0, 1.0]);
        assert!(close(back.rgb, [0.0, 1.0, 0.0]));
    }

    #[test]
    fn every_label_has_chinese() {
        let missing: Vec<_> = LABELS
            .iter()
            .filter(|l| !crate::i18n::has_translation(l))
            .collect();
        assert!(missing.is_empty(), "no Chinese for {missing:?}");
    }

    #[test]
    fn screenshot_pixels_are_sampled_in_physical_pixels() {
        let mut img = egui::ColorImage::filled([4, 4], Color32::BLACK);
        img.pixels[2 * 4 + 3] = Color32::from_rgb(255, 128, 0);
        // Point (1.5, 1.0) at 2 px per point is pixel (3, 2).
        assert_eq!(
            sample(&img, pos2(1.5, 1.0), 2.0),
            Some([1.0, 128.0 / 255.0, 0.0])
        );
        assert_eq!(sample(&img, pos2(10.0, 1.0), 2.0), None);
    }
}
