//! Theme tokens for the AE-style dark UI.
//!
//! Colors were measured from After Effects 2026 (default brightness) on macOS and are
//! stored as the raw display values, so the shell looks the same when shown side by side.
//! Metrics are in egui points (1 pt = 2 px on a Retina display).

use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub mod color {
    use super::rgb;
    use egui::Color32;

    /// Gaps between panels and the window background.
    pub const APP_BG: Color32 = rgb(0x00, 0x00, 0x00);
    pub const TITLE_BAR: Color32 = rgb(0x32, 0x35, 0x36);
    pub const TITLE_TEXT: Color32 = rgb(0xaf, 0xb2, 0xb2);
    pub const PANEL: Color32 = rgb(0x1d, 0x1d, 0x1d);
    /// Darker wells: search fields, dropdowns, list separators.
    pub const FIELD: Color32 = rgb(0x0e, 0x0e, 0x0e);
    pub const FIELD_BORDER: Color32 = rgb(0x30, 0x30, 0x30);
    pub const SEPARATOR: Color32 = rgb(0x0e, 0x0e, 0x0e);
    pub const DIVIDER: Color32 = rgb(0x3a, 0x3a, 0x3a);
    pub const TEXT: Color32 = rgb(0xb1, 0xb1, 0xb1);
    pub const TEXT_STRONG: Color32 = rgb(0xd1, 0xd1, 0xd1);
    pub const TEXT_DIM: Color32 = rgb(0x80, 0x80, 0x80);
    pub const TEXT_DISABLED: Color32 = rgb(0x5a, 0x5a, 0x5a);
    pub const ICON: Color32 = rgb(0xb1, 0xb1, 0xb1);
    pub const ICON_DIM: Color32 = rgb(0x7a, 0x7a, 0x7a);
    /// Focus border, hot text, current-time indicator, links.
    pub const ACCENT: Color32 = rgb(0x58, 0x95, 0xed);
    pub const ACCENT_DIM: Color32 = rgb(0x28, 0x59, 0xa8);
    /// Background of the selected tool in the toolbar.
    pub const TOOL_SELECTED: Color32 = rgb(0x2f, 0x6a, 0xdf);
    pub const BUTTON_ON: Color32 = rgb(0x3d, 0x3d, 0x3d);
    pub const HOVER: Color32 = rgb(0x2a, 0x2a, 0x2a);
    pub const TIMELINE_BG: Color32 = rgb(0x26, 0x26, 0x26);
    pub const ROW: Color32 = rgb(0x30, 0x30, 0x30);
    pub const ROW_SELECTED: Color32 = rgb(0x4b, 0x4b, 0x4b);
    pub const NAVIGATOR: Color32 = rgb(0x65, 0x66, 0x66);
    pub const WORK_AREA: Color32 = rgb(0x5e, 0x5f, 0x5f);
    pub const CACHE_GREEN: Color32 = rgb(0x10, 0xbf, 0x20);
    pub const KEYFRAME: Color32 = rgb(0xb1, 0xb1, 0xb1);
    pub const MOTION_PATH: Color32 = rgb(0x6c, 0x7d, 0xda);
    pub const HANDLE: Color32 = rgb(0x6c, 0x7d, 0xda);
}

/// AE's default label colors (Edit > Preferences > Labels), as shown on screen.
pub const LABELS: [(&str, Color32); 17] = [
    ("None", rgb(0x3a, 0x3a, 0x3a)),
    ("Red", rgb(0xa7, 0x41, 0x3d)),
    ("Yellow", rgb(0xd6, 0xcd, 0x5a)),
    ("Aqua", rgb(0xa5, 0xc6, 0xc2)),
    ("Pink", rgb(0xdc, 0xb9, 0xc5)),
    ("Lavender", rgb(0xa5, 0xa6, 0xc7)),
    ("Peach", rgb(0xde, 0xbf, 0x9f)),
    ("Sea Foam", rgb(0xb0, 0xc3, 0xb0)),
    ("Blue", rgb(0x6b, 0x7c, 0xd9)),
    ("Green", rgb(0x52, 0x9c, 0x4e)),
    ("Purple", rgb(0x86, 0x37, 0x93)),
    ("Orange", rgb(0xde, 0x8f, 0x2b)),
    ("Brown", rgb(0x77, 0x47, 0x30)),
    ("Fuchsia", rgb(0xe5, 0x71, 0xcf)),
    ("Cyan", rgb(0x45, 0x9d, 0xa0)),
    ("Sandstone", rgb(0xa1, 0x92, 0x76)),
    ("Dark Green", rgb(0x2c, 0x43, 0x2c)),
];

pub fn label_color(index: usize) -> Color32 {
    LABELS.get(index).map_or(LABELS[0].1, |l| l.1)
}

/// Layer bar color in the timeline: the label color pulled toward the timeline background.
pub fn layer_bar_color(label: usize, selected: bool) -> Color32 {
    let c = label_color(label);
    let t = if selected { 0.86 } else { 0.55 };
    lerp(color::TIMELINE_BG, c, t)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Brightness offset like AE's "User Interface Colors > Brightness" slider.
pub fn brighten(c: Color32, amount: i16) -> Color32 {
    let f = |x: u8| (x as i16 + amount).clamp(0, 255) as u8;
    Color32::from_rgba_unmultiplied(f(c.r()), f(c.g()), f(c.b()), c.a())
}

pub mod metrics {
    pub const TITLE_BAR_H: f32 = 19.0;
    pub const TOOLBAR_H: f32 = 40.0;
    pub const PANEL_GAP: f32 = 4.0;
    pub const TOOLBAR_GAP: f32 = 2.0;
    pub const TAB_BAR_H: f32 = 25.0;
    pub const STACK_HEADER_H: f32 = 30.0;
    pub const ROW_H: f32 = 17.0;
    pub const FONT_SIZE: f32 = 12.0;
    pub const SMALL_FONT_SIZE: f32 = 11.0;
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn body() -> FontId {
    font(metrics::FONT_SIZE)
}

pub fn small() -> FontId {
    font(metrics::SMALL_FONT_SIZE)
}

/// System UI fonts. Nothing is bundled: Latin from SF / Helvetica Neue, CJK from PingFang.
const LATIN_FONTS: &[(&str, u32)] = &[
    ("/System/Library/Fonts/SFNS.ttf", 0),
    ("/System/Library/Fonts/HelveticaNeue.ttc", 0),
    ("C:\\Windows\\Fonts\\segoeui.ttf", 0),
];
const CJK_FONTS: &[(&str, u32)] = &[
    ("/System/Library/Fonts/PingFang.ttc", 0),
    ("/System/Library/AssetsV2/com_apple_MobileAsset_Font8/86ba2c91f017a3749571a82f2c6d890ac7ffb2fb.asset/AssetData/PingFang.ttc", 0),
    ("/System/Library/Fonts/STHeiti Light.ttc", 0),
    ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
    ("C:\\Windows\\Fonts\\msyh.ttc", 0),
];

fn load_first(defs: &mut FontDefinitions, name: &str, candidates: &[(&str, u32)]) -> bool {
    for (path, index) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut data = FontData::from_owned(bytes);
            data.index = *index;
            defs.font_data.insert(name.to_owned(), data.into());
            return true;
        }
    }
    false
}

pub fn install_fonts(ctx: &Context) {
    let mut defs = FontDefinitions::default();
    let mut front = Vec::new();
    if load_first(&mut defs, "ui-latin", LATIN_FONTS) {
        front.push("ui-latin".to_owned());
    }
    if load_first(&mut defs, "ui-cjk", CJK_FONTS) {
        front.push("ui-cjk".to_owned());
    }
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let list = defs.families.entry(family).or_default();
        for (i, name) in front.iter().enumerate() {
            list.insert(i, name.clone());
        }
    }
    ctx.set_fonts(defs);
}

/// Apply the AE dark look to egui's built-in widgets.
pub fn apply(ctx: &Context) {
    install_fonts(ctx);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, small()),
            (TextStyle::Body, body()),
            (TextStyle::Button, body()),
            (TextStyle::Heading, font(13.0)),
            (TextStyle::Monospace, FontId::monospace(11.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(6.0, 2.0);
        style.spacing.interact_size.y = 18.0;
        style.spacing.menu_margin = egui::Margin::same(4);
        style.interaction.selectable_labels = false;
        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = color::PANEL;
        v.window_fill = color::PANEL;
        v.extreme_bg_color = color::FIELD;
        v.faint_bg_color = color::ROW;
        v.override_text_color = Some(color::TEXT);
        v.selection.bg_fill = color::ACCENT;
        v.selection.stroke.color = color::TEXT_STRONG;
        v.hyperlink_color = color::ACCENT;
        v.window_stroke = egui::Stroke::new(1.0, color::DIVIDER);
        v.window_corner_radius = egui::CornerRadius::same(4);
        v.menu_corner_radius = egui::CornerRadius::same(4);
        v.popup_shadow = egui::Shadow::NONE;
        v.window_shadow = egui::Shadow::NONE;
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = color::PANEL;
        w.noninteractive.fg_stroke.color = color::TEXT;
        w.noninteractive.bg_stroke.color = color::DIVIDER;
        w.inactive.bg_fill = color::FIELD;
        w.inactive.weak_bg_fill = color::FIELD;
        w.inactive.fg_stroke.color = color::TEXT;
        w.inactive.bg_stroke = egui::Stroke::new(1.0, color::FIELD_BORDER);
        w.hovered.bg_fill = color::HOVER;
        w.hovered.weak_bg_fill = color::HOVER;
        w.hovered.fg_stroke.color = color::TEXT_STRONG;
        w.hovered.bg_stroke = egui::Stroke::new(1.0, color::DIVIDER);
        w.active.bg_fill = color::BUTTON_ON;
        w.active.weak_bg_fill = color::BUTTON_ON;
        w.active.fg_stroke.color = color::TEXT_STRONG;
        for s in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            s.corner_radius = egui::CornerRadius::same(3);
        }
    });
}
