//! Theme tokens for the AE-style dark UI.
//!
//! Colors were measured from After Effects 2026 (default brightness) on macOS and are
//! stored as the raw display values, so the shell looks the same when shown side by side.
//! Metrics are in egui points (1 pt = 2 px on a Retina display).

use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Galley, TextStyle};

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
    /// 1px highlight along the top edge of the title bar.
    pub const TITLE_HIGHLIGHT: Color32 = rgb(0x4f, 0x52, 0x53);
    pub const TRAFFIC_CLOSE: Color32 = rgb(0xeb, 0x69, 0x5d);
    pub const TRAFFIC_MIN: Color32 = rgb(0xf4, 0xbf, 0x4f);
    pub const TRAFFIC_ZOOM: Color32 = rgb(0x60, 0xc4, 0x53);
    pub const TRAFFIC_IDLE: Color32 = rgb(0x5a, 0x5d, 0x5e);
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
    pub const ACCENT_HOVER: Color32 = rgb(0x7d, 0xae, 0xf3);
    /// 1pt border of the focused panel frame.
    pub const FOCUS: Color32 = rgb(0x57, 0x94, 0xec);
    /// Gray rule under hot text fields in the Properties panel.
    pub const UNDERLINE: Color32 = rgb(0x6a, 0x6a, 0x6a);
    /// Keyframe navigator diamond when the playhead is not on a key.
    pub const KEY_OFF: Color32 = rgb(0x8c, 0x8c, 0x8c);
    /// Dropdown fields (Parent & Link, viewer resolution).
    pub const DROPDOWN: Color32 = rgb(0x00, 0x00, 0x00);
    pub const FIELD_HOVER: Color32 = rgb(0x16, 0x16, 0x16);
    /// Empty switch cells in the timeline.
    pub const SWITCH_BG: Color32 = rgb(0x0e, 0x0e, 0x0e);
    /// Underline of the active tab and active tab text.
    pub const TAB_ACTIVE: Color32 = rgb(0xd0, 0xd0, 0xd0);
    pub const TAB_INACTIVE: Color32 = rgb(0xb0, 0xb0, 0xb0);
    pub const TAB_CLOSE: Color32 = rgb(0x96, 0x96, 0x96);
    /// Separator before the tab overflow chevron.
    pub const TAB_SEPARATOR: Color32 = rgb(0x4b, 0x4b, 0x4b);
    /// Drop-zone highlight while dragging a panel tab.
    pub const DROP_ZONE: Color32 = Color32::from_rgba_premultiplied(0x2c, 0x4a, 0x76, 0x80);
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

const fn default_label_colors() -> [Color32; 17] {
    let mut out = [Color32::BLACK; 17];
    let mut i = 0;
    while i < 17 {
        out[i] = LABELS[i].1;
        i += 1;
    }
    out
}

/// Label colors in use (Preferences > Labels; [`set_label_colors`]).
static LABEL_COLORS: std::sync::RwLock<[Color32; 17]> =
    std::sync::RwLock::new(default_label_colors());
/// Label names the user changed (Preferences > Labels); `None`: the default.
static LABEL_NAMES: std::sync::RwLock<[Option<String>; 17]> =
    std::sync::RwLock::new([const { None }; 17]);

/// Uses `colors` for labels 0..=16 from now on (index 0, None, stays fixed).
pub fn set_label_colors(colors: [Color32; 17]) {
    let mut table = LABEL_COLORS.write().unwrap_or_else(|e| e.into_inner());
    *table = colors;
    table[0] = LABELS[0].1;
}

/// Uses `names` for labels (`None` or the English default: the translated default).
pub fn set_label_names(names: [Option<String>; 17]) {
    *LABEL_NAMES.write().unwrap_or_else(|e| e.into_inner()) = names;
}

/// Color of label `index` (Preferences > Labels).
pub fn label_color(index: usize) -> Color32 {
    let table = LABEL_COLORS.read().unwrap_or_else(|e| e.into_inner());
    table.get(index).copied().unwrap_or(table[0])
}

/// Name of label `index` as shown (the user's name or the translated default).
pub fn label_name(index: usize) -> String {
    let names = LABEL_NAMES.read().unwrap_or_else(|e| e.into_inner());
    match names.get(index).and_then(Option::as_ref) {
        Some(name) => name.clone(),
        None => crate::i18n::tr(LABELS.get(index).map_or(LABELS[0].0, |l| l.0)).to_owned(),
    }
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

/// Preferences > Appearance > Color Theme (AE 26: Darkest, Dark, Light). The
/// theme tokens are measured from Darkest; every color the UI draws goes
/// through [`adjust`] (the shell applies it to the tessellated frame,
/// [`apply_appearance`]), so the tokens stay constants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    /// AE 26's default; the measured tokens as they are.
    #[default]
    Darkest,
    /// Grays lifted (panels about #323232).
    Dark,
    /// Grays inverted to a light UI with dark text.
    Light,
}

impl Appearance {
    /// From the stored option value (`darkest`, `dark`, `light`).
    pub fn from_id(id: &str) -> Self {
        match id {
            "dark" => Appearance::Dark,
            "light" => Appearance::Light,
            _ => Appearance::Darkest,
        }
    }
}

static GRADIENTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub(crate) fn set_gradients(enabled: bool) {
    GRADIENTS.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

/// Surface of a panel header or title bar. Uses the ordinary egui vertex-color pipeline.
pub(crate) fn bar_fill(painter: &egui::Painter, rect: egui::Rect, base: Color32) {
    if !GRADIENTS.load(std::sync::atomic::Ordering::Relaxed) {
        painter.rect_filled(rect, 0.0, base);
        return;
    }
    let top = brighten(base, 3);
    let bottom = brighten(base, -3);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(2, 1, 3);
    painter.add(mesh);
}

static APPEARANCE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

static ACCESSIBLE_CONTRAST: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

pub fn set_accessible_contrast(enabled: bool) {
    ACCESSIBLE_CONTRAST.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

/// Uses `a` from the next frame on.
pub fn set_appearance(a: Appearance) {
    APPEARANCE.store(a as u8, std::sync::atomic::Ordering::Relaxed);
}

/// The appearance in use.
pub fn appearance() -> Appearance {
    match APPEARANCE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => Appearance::Dark,
        2 => Appearance::Light,
        _ => Appearance::Darkest,
    }
}

/// `c` (premultiplied) as drawn with appearance `a`: only grays change; accent,
/// label and image colors keep their values.
pub fn adjust(c: Color32, a: Appearance) -> Color32 {
    adjust_ui_color(
        c,
        a,
        ACCESSIBLE_CONTRAST.load(std::sync::atomic::Ordering::Relaxed),
    )
}

fn adjust_ui_color(c: Color32, a: Appearance, accessible: bool) -> Color32 {
    let alpha = c.a();
    if alpha == 0 || (a == Appearance::Darkest && accessible) {
        return c;
    }
    let un = |v: u8| (f32::from(v) * 255.0 / f32::from(alpha)).min(255.0);
    let (r, g, b) = (un(c.r()), un(c.g()), un(c.b()));
    if r.max(g).max(b) - r.min(g).min(b) > 6.0 {
        return c;
    }
    let map = |v: f32| match a {
        Appearance::Dark => v + (255.0 - v) * 0.09,
        Appearance::Light => 240.0 - v * 0.8,
        Appearance::Darkest => v,
    };
    let q = |v: f32| {
        let shade = map(v);
        let shade = if accessible {
            shade
        } else {
            (shade - 128.0) * 0.85 + 128.0
        };
        (shade * f32::from(alpha) / 255.0).round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgba_premultiplied(q(r), q(g), q(b), alpha)
}

/// Applies the current [`Appearance`] to a tessellated frame. Only meshes drawn
/// with egui's own texture (shapes and text) change; images and the viewer are
/// left alone. The default Darkest palette already uses accessible contrast.
pub fn apply_appearance(prims: &mut [egui::ClippedPrimitive]) {
    let a = appearance();
    let accessible = ACCESSIBLE_CONTRAST.load(std::sync::atomic::Ordering::Relaxed);
    if a == Appearance::Darkest && accessible {
        return;
    }
    for prim in prims {
        if let egui::epaint::Primitive::Mesh(mesh) = &mut prim.primitive
            && mesh.texture_id == egui::TextureId::default()
        {
            for v in &mut mesh.vertices {
                v.color = adjust_ui_color(v.color, a, accessible);
            }
        }
    }
}

pub mod metrics {
    pub const TITLE_BAR_H: f32 = 19.0;
    pub const TOOLBAR_H: f32 = 40.0;
    pub const PANEL_GAP: f32 = 4.0;
    pub const TOOLBAR_GAP: f32 = 2.0;
    /// Tab row of a panel frame; also the header of the expanded panel in a stack.
    pub const TAB_BAR_H: f32 = 28.0;
    /// Header of a collapsed panel in a stacked frame.
    pub const STACK_HEADER_H: f32 = 24.0;
    pub const ROW_H: f32 = 17.0;
    /// Inner margin of panels laid out as forms (Render Queue, …), the
    /// inset of the Preview and Timeline controls.
    pub const PANEL_PADDING: f32 = 8.0;
    /// AE uses Adobe Clean; SF at 11.5pt matches its text widths within ~1%.
    pub const FONT_SIZE: f32 = 11.5;
    pub const SMALL_FONT_SIZE: f32 = 10.5;
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// Distance from the top of a single-line galley to its text baseline,
/// so text can be placed on baselines measured from AE.
pub fn baseline_offset(galley: &Galley) -> f32 {
    galley
        .rows
        .first()
        .and_then(|r| r.row.glyphs.first().map(|g| r.pos.y + g.pos.y))
        .unwrap_or(galley.size().y * 0.8)
}

pub fn body() -> FontId {
    font(metrics::FONT_SIZE)
}

pub fn small() -> FontId {
    font(metrics::SMALL_FONT_SIZE)
}

/// One UI font found on the system: the egui font name, the file's bytes (memory
/// mapped, alive for the process) and the face index in the file.
#[derive(Clone, Copy, Debug)]
pub struct SystemFont {
    pub name: &'static str,
    pub family: &'static str,
    pub data: &'static [u8],
    pub index: u32,
}

/// The UI's fonts, in fallback order. Nothing is bundled (egui's own fonts are off):
///
/// 1. `ui-text`: the system UI font (SF on macOS, Segoe UI on Windows);
/// 2. `ui-cjk`: the Simplified Chinese UI font (PingFang SC, Microsoft YaHei);
/// 3. `ui-ja`: the Japanese font (Hiragino Sans, Yu Gothic / Meiryo) for kana and
///    punctuation the Chinese font lacks on some systems (`・`);
/// 4. `ui-ko`: the Korean font (Apple SD Gothic Neo, Malgun Gothic) for Hangul;
/// 5. `ui-symbols`: the platform symbol font (Apple Symbols, Segoe UI Symbol) for
///    arrows, marks and the missing-glyph box `◻`.
///
/// Found through fontique, the font database bae-text and parley use, once per process.
pub fn system_ui_fonts() -> &'static [SystemFont] {
    static FONTS: std::sync::OnceLock<Vec<SystemFont>> = std::sync::OnceLock::new();
    FONTS.get_or_init(find_system_ui_fonts)
}

/// Starts [`system_ui_fonts`] on a background thread, so the font lookup (about 20 ms:
/// fontique scans the installed fonts) overlaps startup work such as creating the GPU
/// device instead of delaying the first frame.
pub fn prefetch_system_ui_fonts() {
    let _ = std::thread::Builder::new()
        .name("bae-ui-fonts".into())
        .spawn(|| {
            system_ui_fonts();
        });
}

fn find_system_ui_fonts() -> Vec<SystemFont> {
    let started = std::time::Instant::now();
    let fonts = find_system_ui_fonts_uncached();
    let families: Vec<&str> = fonts.iter().map(|f| f.family).collect();
    tracing::info!(
        "UI fonts: {} ({:.1} ms)",
        families.join(", "),
        started.elapsed().as_secs_f64() * 1e3
    );
    fonts
}

fn find_system_ui_fonts_uncached() -> Vec<SystemFont> {
    use fontique::{
        Collection, CollectionOptions, FallbackKey, FamilyId, GenericFamily, Language, Script,
        SourceCache,
    };
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    let mut cache = SourceCache::default();
    let ui: Vec<FamilyId> = collection
        .generic_families(GenericFamily::SystemUi)
        .collect();
    let zh = Language::parse("zh-Hans").ok();
    let cjk: Vec<FamilyId> = collection
        .fallback_families(FallbackKey::new(Script::from_bytes(*b"Hani"), zh.as_ref()))
        .collect();
    let named = |collection: &mut Collection, names: &[&str]| {
        names
            .iter()
            .find_map(|n| collection.family_by_name(n).map(|f| f.id()))
    };
    let ui = ui
        .first()
        .copied()
        .or_else(|| named(&mut collection, &["SF Pro", "Helvetica Neue", "Segoe UI"]));
    let cjk = cjk.first().copied().or_else(|| {
        named(
            &mut collection,
            &["PingFang SC", "Microsoft YaHei UI", "Microsoft YaHei"],
        )
    });
    let ja_locale = Language::parse("ja").ok();
    let japanese: Vec<FamilyId> = collection
        .fallback_families(FallbackKey::new(
            Script::from_bytes(*b"Hira"),
            ja_locale.as_ref(),
        ))
        .collect();
    // The first Japanese fallback that is not the Chinese font already chosen.
    let japanese = japanese
        .iter()
        .copied()
        .find(|id| Some(*id) != cjk)
        .or_else(|| {
            named(
                &mut collection,
                &["Hiragino Sans", "Yu Gothic UI", "Meiryo UI", "Meiryo"],
            )
            .filter(|id| Some(*id) != cjk)
        });
    let ko_locale = Language::parse("ko").ok();
    let korean: Vec<FamilyId> = collection
        .fallback_families(FallbackKey::new(
            Script::from_bytes(*b"Hang"),
            ko_locale.as_ref(),
        ))
        .collect();
    let korean = korean
        .iter()
        .copied()
        .find(|id| Some(*id) != cjk && Some(*id) != japanese)
        .or_else(|| named(&mut collection, &["Apple SD Gothic Neo", "Malgun Gothic"]))
        .filter(|id| Some(*id) != cjk && Some(*id) != japanese);
    let symbols = named(&mut collection, &["Apple Symbols", "Segoe UI Symbol"]);

    let mut out = Vec::new();
    for (name, id) in [
        ("ui-text", ui),
        ("ui-cjk", cjk),
        ("ui-ja", japanese),
        ("ui-ko", korean),
        ("ui-symbols", symbols),
    ] {
        let Some(family) = id.and_then(|id| collection.family(id)) else {
            tracing::warn!("no system font for {name}");
            continue;
        };
        let Some(font) = family.default_font() else {
            continue;
        };
        let Some(blob) = font.load(Some(&mut cache)) else {
            tracing::warn!("cannot load the system font {} for {name}", family.name());
            continue;
        };
        // The UI keeps its fonts for the life of the process.
        let blob: &'static fontique::Blob<u8> = Box::leak(Box::new(blob));
        out.push(SystemFont {
            name,
            family: Box::leak(family.name().to_owned().into_boxed_str()),
            data: blob.data(),
            index: font.index(),
        });
    }
    out
}

/// Installs [`ui_font_definitions`] in `ctx`.
pub fn install_fonts(ctx: &Context) {
    ctx.set_fonts(ui_font_definitions());
}

/// [`system_ui_fonts`] as egui's proportional and monospace families (the UI's
/// monospace text also uses the system UI font, as AE does).
pub fn ui_font_definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::empty();
    let mut names = Vec::new();
    for font in system_ui_fonts() {
        let mut data = FontData::from_static(font.data);
        data.index = font.index;
        defs.font_data.insert(font.name.to_owned(), data.into());
        names.push(font.name.to_owned());
    }
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        defs.families.insert(family, names.clone());
    }
    defs
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

#[cfg(test)]
#[test]
fn accessible_palette_has_higher_text_contrast_in_each_theme() {
    for theme in [Appearance::Darkest, Appearance::Dark, Appearance::Light] {
        let difference = |accessible| {
            let text = adjust_ui_color(Color32::from_gray(205), theme, accessible);
            let panel = adjust_ui_color(Color32::from_gray(35), theme, accessible);
            text.r().abs_diff(panel.r())
        };
        assert!(difference(true) > difference(false), "{theme:?}");
        assert_eq!(
            adjust_ui_color(Color32::RED, theme, true),
            adjust_ui_color(Color32::RED, theme, false)
        );
    }
}
