//! The Character panel (AE: Window > Character, Cmd/Ctrl+6), laid out like AE
//! 26's: font family and style, eyedropper, fill and stroke swatches (swap, set
//! to black / white), size, leading, kerning, tracking, stroke width and order,
//! vertical / horizontal scale, baseline shift, tsume, the six style buttons,
//! Ligatures and Hindi Digits. Changes go to the selected text layers
//! (`text.set_style`, one undo step per change; drags commit on release); with
//! no text layer selected they are the settings new text gets, as in AE.

use std::cell::RefCell;

use bae_model::{PrimPath, Stage, TextDocument, Value};
use bae_motion::query;
use bae_motion::text::{DEFAULT_FONT, DEFAULT_SIZE, SOURCE};
use bae_ui::color_picker::{self, ColorEdit};
use bae_ui::icons::{self, Icon};
use bae_ui::widgets::HotNumber;
use bae_ui::{Panel, PanelContext, tr};
use egui::{Align2, Color32, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value as Json, json};

use crate::common;

thread_local! {
    /// The installed fonts (scanned once, on the UI thread).
    static FONTS: RefCell<Option<bae_text::FontSystem>> = const { RefCell::new(None) };
}

fn with_fonts<R>(f: impl FnOnce(&mut bae_text::FontSystem) -> R) -> R {
    FONTS.with(|cell| {
        let mut slot = cell.borrow_mut();
        f(slot.get_or_insert_with(bae_text::FontSystem::new))
    })
}

/// AE's size and leading presets (px).
const SIZES: [f64; 14] = [
    6.0, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 18.0, 24.0, 30.0, 36.0, 48.0, 60.0, 72.0,
];
/// AE's tracking presets (1/1000 em).
const TRACKING: [f64; 14] = [
    -100.0, -75.0, -50.0, -25.0, -10.0, -5.0, 0.0, 5.0, 10.0, 25.0, 50.0, 75.0, 100.0, 200.0,
];

/// Which swatch the open Color Picker edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Swatch {
    Fill,
    Stroke,
}

/// The Character panel.
#[derive(Default)]
pub struct CharacterPanel {
    families: Option<Vec<String>>,
    picking: Option<Swatch>,
}

/// The style the panel shows: the first selected text layer's at the current
/// time, else the settings for new text.
fn current_doc(
    stage: &Stage,
    layers: &[PrimPath],
    comp_time: bae_model::Time,
) -> Option<TextDocument> {
    let layer = layers.first()?;
    let t = query::to_key_time(stage, layer, comp_time);
    match query::sample_attr(stage, layer, SOURCE, t) {
        Some(Value::TextDocument(d)) => Some(*d),
        _ => None,
    }
}

/// The style name of a face for the style menu (`Helvetica-BoldOblique` → "Bold Oblique").
fn style_name(face: &bae_text::FontFace) -> String {
    let suffix = face
        .postscript_name
        .split_once('-')
        .map(|(_, s)| s.to_owned())
        .unwrap_or_default();
    if suffix.is_empty() {
        return if face.italic {
            "Italic".into()
        } else {
            "Regular".into()
        };
    }
    let mut out = String::new();
    for (i, c) in suffix.chars().enumerate() {
        if i > 0 && c.is_uppercase() && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

impl Panel for CharacterPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        let (layers, time, defaults) = {
            let s = session.lock();
            let layers: Vec<PrimPath> = s
                .selected_layers
                .iter()
                .filter(|l| query::layer_kind(cx.stage, l).as_deref() == Some("text"))
                .cloned()
                .collect();
            let time = s
                .active_comp
                .as_ref()
                .map_or(bae_model::Time::ZERO, |c| s.time(c));
            (layers, time, s.text_defaults.clone())
        };
        let doc = current_doc(cx.stage, &layers, time)
            .or(defaults)
            .unwrap_or_else(|| TextDocument::new("", DEFAULT_FONT, DEFAULT_SIZE));
        let mut change = json!({});
        let id = ui.id().with("character");
        let (family, face_style) = with_fonts(|f| match f.resolve(&doc.font) {
            bae_text::FontMatch::Found(face) => (face.family.clone(), style_name(&face)),
            bae_text::FontMatch::Missing => (doc.font.clone(), String::new()),
        });
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        ui.add_space(4.0);

        // Font family / style, eyedropper, swatches.
        let top = ui.cursor().min;
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            let resp = combo(ui, &family, 122.0, tr("Set the font family"));
            let families = self
                .families
                .get_or_insert_with(|| with_fonts(|f| f.families()))
                .clone();
            egui::Popup::menu(&resp).id(id.with("family")).show(|ui| {
                egui::ScrollArea::vertical().max_height(320.0).show_rows(
                    ui,
                    20.0,
                    families.len(),
                    |ui, range| {
                        for name in &families[range] {
                            if ui.selectable_label(*name == family, name).clicked() {
                                let ps = with_fonts(|f| match f.resolve(name) {
                                    bae_text::FontMatch::Found(face) => face.postscript_name,
                                    bae_text::FontMatch::Missing => name.clone(),
                                });
                                change["font"] = json!(ps);
                            }
                        }
                    },
                );
            });
            if let Some(rgb) = color_picker::eyedropper_button(ui, id.with("eyedropper")) {
                change["fill_color"] = json!([rgb[0], rgb[1], rgb[2], 1.0]);
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            let resp = combo(ui, &face_style, 122.0, tr("Set the font style"));
            let faces = with_fonts(|f| f.faces(&family));
            egui::Popup::menu(&resp).id(id.with("style")).show(|ui| {
                for face in &faces {
                    let name = style_name(face);
                    if ui.selectable_label(name == face_style, &name).clicked() {
                        change["font"] = json!(face.postscript_name);
                    }
                }
            });
            // Set to black / white (AE's two small swatches under the eyedropper).
            let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            let black = Rect::from_min_size(r.min, vec2(18.0, 8.0));
            let white = Rect::from_min_size(r.min + vec2(0.0, 9.0), vec2(18.0, 8.0));
            let p = ui.painter();
            p.rect_filled(black, 0.0, Color32::BLACK);
            p.rect_filled(white, 0.0, Color32::WHITE);
            p.rect_stroke(
                r,
                0.0,
                Stroke::new(1.0, bae_ui::theme::color::DIVIDER),
                egui::StrokeKind::Inside,
            );
            use bae_ui::a11y::Accessible as _;
            if ui
                .interact(black, id.with("black"), Sense::click())
                .labeled(egui::WidgetType::Button, tr("Set to Black"))
                .on_hover_text(tr("Set to Black"))
                .clicked()
            {
                change["fill_color"] = json!([0.0, 0.0, 0.0, 1.0]);
            }
            if ui
                .interact(white, id.with("white"), Sense::click())
                .labeled(egui::WidgetType::Button, tr("Set to White"))
                .on_hover_text(tr("Set to White"))
                .clicked()
            {
                change["fill_color"] = json!([1.0, 1.0, 1.0, 1.0]);
            }
        });
        // Fill (front, top left) and stroke (back, bottom right) swatches.
        let swatches = Rect::from_min_size(top + vec2(170.0, 0.0), vec2(44.0, 44.0));
        if let Some(edit) = self.swatches_ui(ui, id, swatches, &doc) {
            for (k, v) in edit.as_object().into_iter().flatten() {
                change[k] = v.clone();
            }
        }

        ui.add_space(6.0);
        divider(ui);
        // Size | leading; kerning | tracking.
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            field_icon(ui, Icon::TextSize, tr("Set the font size"));
            if let Some(v) = number(
                ui,
                id.with("size"),
                doc.font_size,
                tr("px"),
                0.1..=1296.0,
                1,
                tr("Set the font size"),
            ) {
                change["font_size"] = json!(v);
            }
            if let Some(v) = presets(ui, id.with("size_presets"), &SIZES, tr("px"), None) {
                change["font_size"] = json!(v);
            }
            ui.add_space(8.0);
            glyph_icon(ui, "A\u{2195}", tr("Set the leading"));
            match doc.leading {
                Some(l) => {
                    if let Some(v) = number(
                        ui,
                        id.with("leading"),
                        l,
                        tr("px"),
                        0.0..=5000.0,
                        1,
                        tr("Set the leading"),
                    ) {
                        change["leading"] = json!(v);
                    }
                }
                None => {
                    ui.label(egui::RichText::new(tr("Auto")).color(bae_ui::theme::color::ACCENT));
                }
            }
            if let Some(v) = presets(
                ui,
                id.with("leading_presets"),
                &SIZES,
                tr("px"),
                Some(tr("Auto")),
            ) {
                if v.is_nan() {
                    change["auto_leading"] = json!(true);
                } else {
                    change["leading"] = json!(v);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            field_icon(
                ui,
                Icon::TextKerning,
                tr("Set the kerning between two characters"),
            );
            ui.add_enabled_ui(false, |ui| {
                ui.label(
                    egui::RichText::new(tr("Metrics")).color(bae_ui::theme::color::TEXT_DISABLED),
                );
            });
            ui.add_space(30.0);
            field_icon(
                ui,
                Icon::TextTracking,
                tr("Set the tracking for the selected characters"),
            );
            if let Some(v) = number(
                ui,
                id.with("tracking"),
                doc.tracking,
                "",
                -1000.0..=10000.0,
                0,
                tr("Set the tracking for the selected characters"),
            ) {
                change["tracking"] = json!(v);
            }
            if let Some(v) = presets(ui, id.with("tracking_presets"), &TRACKING, "", None) {
                change["tracking"] = json!(v);
            }
        });
        // Stroke width and order.
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            field_icon(ui, Icon::TextStroke, tr("Set the stroke width"));
            if let Some(v) = number(
                ui,
                id.with("stroke_width"),
                doc.stroke_width,
                tr("px"),
                0.0..=1000.0,
                1,
                tr("Set the stroke width"),
            ) {
                change["stroke_width"] = json!(v);
            }
            ui.add_space(8.0);
            let order = if doc.stroke_over_fill {
                tr("Stroke Over Fill")
            } else {
                tr("Fill Over Stroke")
            };
            let resp = combo(ui, order, 118.0, tr("Stroke"));
            egui::Popup::menu(&resp)
                .id(id.with("stroke_order"))
                .show(|ui| {
                    if ui
                        .selectable_label(!doc.stroke_over_fill, tr("Fill Over Stroke"))
                        .clicked()
                    {
                        change["stroke_over_fill"] = json!(false);
                    }
                    if ui
                        .selectable_label(doc.stroke_over_fill, tr("Stroke Over Fill"))
                        .clicked()
                    {
                        change["stroke_over_fill"] = json!(true);
                    }
                    ui.add_enabled_ui(false, |ui| {
                        let _ = ui.selectable_label(false, tr("All Fills Over All Strokes"));
                        let _ = ui.selectable_label(false, tr("All Strokes Over All Fills"));
                    });
                });
        });
        ui.add_space(4.0);
        divider(ui);
        // Scales, baseline shift, tsume (scales and tsume: not drawn by the text
        // engine yet, shown like AE greyed).
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            glyph_icon(ui, "\u{2195}T", tr("Vertical Scale"));
            disabled_value(ui, "100 %");
            ui.add_space(30.0);
            glyph_icon(ui, "\u{2194}T", tr("Horizontal Scale"));
            disabled_value(ui, "100 %");
        });
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            field_icon(ui, Icon::TextBaselineShift, tr("Set the baseline shift"));
            if let Some(v) = number(
                ui,
                id.with("baseline"),
                doc.baseline_shift,
                tr("px"),
                -5000.0..=5000.0,
                1,
                tr("Set the baseline shift"),
            ) {
                change["baseline_shift"] = json!(v);
            }
            ui.add_space(30.0);
            glyph_icon(
                ui,
                "\u{2192}\u{2190}",
                tr("Set the tsume for the selected characters"),
            );
            disabled_value(ui, "0 %");
        });
        ui.add_space(4.0);
        divider(ui);
        // Faux Bold, Faux Italic, All Caps, Small Caps, Superscript, Subscript.
        ui.horizontal(|ui| {
            ui.add_space(22.0);
            for (key, on, icon, text, tip, enabled) in [
                (
                    "faux_bold",
                    doc.faux_bold,
                    Some(Icon::TextBold),
                    "",
                    "Faux Bold",
                    true,
                ),
                (
                    "faux_italic",
                    doc.faux_italic,
                    Some(Icon::TextItalic),
                    "",
                    "Faux Italic",
                    true,
                ),
                ("all_caps", doc.all_caps, None, "TT", "All Caps", true),
                (
                    "small_caps",
                    doc.small_caps,
                    Some(Icon::SmallCaps),
                    "",
                    "Small Caps",
                    true,
                ),
                (
                    "superscript",
                    false,
                    Some(Icon::TextSuperscript),
                    "",
                    "Superscript",
                    false,
                ),
                (
                    "subscript",
                    false,
                    Some(Icon::TextSubscript),
                    "",
                    "Subscript",
                    false,
                ),
            ] {
                if style_button(ui, on, icon, text, tr(tip), enabled).clicked() {
                    change[key] = json!(!on);
                    // AE: All Caps and Small Caps exclude each other.
                    if key == "all_caps" && !on {
                        change["small_caps"] = json!(false);
                    } else if key == "small_caps" && !on {
                        change["all_caps"] = json!(false);
                    }
                }
            }
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.add_enabled_ui(false, |ui| {
                let mut off = false;
                bae_ui::widgets::checkbox(ui, &mut off, tr("Ligatures"));
                ui.add_space(24.0);
                bae_ui::widgets::checkbox(ui, &mut off, tr("Hindi Digits"));
            });
        });

        if change.as_object().is_some_and(|o| !o.is_empty()) {
            if layers.is_empty() {
                // No text selected: the settings for new text (AE).
                let mut d = doc.clone();
                if let Ok(c) = serde_json::from_value::<bae_motion::TextStyleChange>(change.clone())
                {
                    c.apply(&mut d);
                }
                session.lock().text_defaults = Some(d);
            } else {
                cx.invoke_with(
                    "text.set_style",
                    json!({ "layers": layers, "time": time, "style": change }),
                );
            }
        }
    }

    fn menu(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let _ = cx;
        // AE's panel menu items; the ones the text engine does not do yet are greyed.
        ui.add_enabled_ui(false, |ui| {
            for item in [
                "Faux Bold",
                "Faux Italic",
                "Standard Vertical Roman Alignment",
                "Tate-Chuu-Yoko",
                "All Caps",
                "Small Caps",
                "Superscript",
                "Subscript",
                "Ligatures",
                "Hindi Digits",
                "Use Smart Quotes",
                "Show Font Names in English",
                "No Break",
                "Reset Character",
            ] {
                let _ = ui.button(tr(item));
            }
        });
    }
}

impl CharacterPanel {
    /// AE's fill and stroke swatches with the swap arrow; the Color Picker opens
    /// on a click. Returns the change it made.
    fn swatches_ui(&mut self, ui: &mut Ui, id: Id, r: Rect, doc: &TextDocument) -> Option<Json> {
        let fill_r = Rect::from_min_size(r.min, vec2(28.0, 28.0));
        let stroke_r = Rect::from_min_size(r.min + vec2(14.0, 14.0), vec2(28.0, 28.0));
        let swap_r = Rect::from_min_size(r.min + vec2(32.0, 0.0), vec2(12.0, 12.0));
        let p = ui.painter();
        let c = |c: [f64; 4]| {
            Color32::from_rgb(
                (c[0] * 255.0) as u8,
                (c[1] * 255.0) as u8,
                (c[2] * 255.0) as u8,
            )
        };
        // The stroke is behind (a hollow square in AE), the fill in front.
        match doc.stroke_color {
            Some(s) => {
                p.rect_filled(stroke_r, 0.0, c(s));
                p.rect_filled(stroke_r.shrink(7.0), 0.0, bae_ui::theme::color::PANEL);
            }
            None => {
                p.rect_filled(stroke_r, 0.0, Color32::WHITE);
                p.line_segment(
                    [stroke_r.left_bottom(), stroke_r.right_top()],
                    Stroke::new(2.0, Color32::from_rgb(0xe0, 0x30, 0x30)),
                );
            }
        }
        p.rect_filled(fill_r, 0.0, c(doc.fill_color));
        p.rect_stroke(
            fill_r,
            0.0,
            Stroke::new(1.0, bae_ui::theme::color::DIVIDER),
            egui::StrokeKind::Inside,
        );
        icons::paint(p, swap_r, Icon::Rotate, bae_ui::theme::color::ICON);
        use bae_ui::a11y::Accessible as _;
        // The fill is on top: registered last, it gets the clicks where they overlap.
        let stroke = ui
            .interact(stroke_r, id.with("stroke"), Sense::click())
            .labeled(
                egui::WidgetType::ColorButton,
                tr("Stroke Color (Click to change)"),
            )
            .on_hover_text(tr("Stroke Color (Click to change)"));
        let fill = ui
            .interact(fill_r, id.with("fill"), Sense::click())
            .labeled(
                egui::WidgetType::ColorButton,
                tr("Fill Color (Click to change)"),
            )
            .on_hover_text(tr("Fill Color (Click to change)"));
        let swap = ui
            .interact(swap_r, id.with("swap"), Sense::click())
            .labeled(egui::WidgetType::Button, tr("Swap Fill and Stroke"))
            .on_hover_text(tr("Swap Fill and Stroke"));
        if fill.clicked() {
            self.picking = Some(Swatch::Fill);
        } else if stroke.clicked() {
            self.picking = Some(Swatch::Stroke);
        }
        if swap.clicked() {
            return Some(match doc.stroke_color {
                Some(s) => json!({ "fill_color": s, "stroke_color": doc.fill_color }),
                None => json!({ "stroke_color": doc.fill_color }),
            });
        }
        let which = self.picking?;
        let start = match which {
            Swatch::Fill => doc.fill_color,
            Swatch::Stroke => doc.stroke_color.unwrap_or([1.0; 4]),
        };
        match color_picker::picker(
            ui.ctx(),
            id.with(("picker", which == Swatch::Fill)),
            tr("Text Color"),
            start,
        ) {
            ColorEdit::Commit(c) => {
                self.picking = None;
                Some(match which {
                    Swatch::Fill => json!({ "fill_color": c }),
                    Swatch::Stroke => json!({ "stroke_color": c, "stroke": true }),
                })
            }
            ColorEdit::Cancel => {
                self.picking = None;
                None
            }
            _ => None,
        }
    }
}

/// A hot number whose dragged value commits on release; a typed one at once.
fn number(
    ui: &mut Ui,
    id: Id,
    model: f64,
    suffix: &str,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    label: &str,
) -> Option<f64> {
    let key = id.with("scrub");
    let mut v = ui.data(|d| d.get_temp::<f64>(key)).unwrap_or(model);
    let resp = ui
        .push_id(id, |ui| {
            ui.add(
                HotNumber::new(&mut v)
                    .decimals(decimals)
                    .speed(0.5)
                    .suffix(suffix)
                    .range(range)
                    .label(label),
            )
        })
        .inner;
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<f64>(key));
        return (v != model).then_some(v);
    }
    if resp.dragged() {
        ui.data_mut(|d| d.insert_temp(key, v));
        return None;
    }
    (resp.changed() && v != model).then_some(v)
}

/// The ▾ after a value with AE's presets; `auto` adds "Auto" first (NaN).
fn presets(ui: &mut Ui, id: Id, values: &[f64], suffix: &str, auto: Option<&str>) -> Option<f64> {
    let (r, resp) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::click());
    icons::paint(
        ui.painter(),
        r,
        Icon::ChevronDown,
        bae_ui::theme::color::ICON,
    );
    let mut picked = None;
    egui::Popup::menu(&resp).id(id).show(|ui| {
        if let Some(a) = auto
            && ui.button(a).clicked()
        {
            picked = Some(f64::NAN);
        }
        for v in values {
            let text = if suffix.is_empty() {
                format!("{v}")
            } else {
                format!("{v} {suffix}")
            };
            if ui.button(text).clicked() {
                picked = Some(*v);
            }
        }
    });
    picked
}

/// A Spectrum icon before a field, with the field's AE tooltip.
fn field_icon(ui: &mut Ui, icon: Icon, tip: &str) {
    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    icons::paint(ui.painter(), r, icon, bae_ui::theme::color::ICON);
    resp.on_hover_text(tip);
}

/// A field marker AE draws as letters (leading, scales, tsume).
fn glyph_icon(ui: &mut Ui, text: &str, tip: &str) {
    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(10.0),
        bae_ui::theme::color::ICON,
    );
    resp.on_hover_text(tip);
}

fn disabled_value(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(bae_ui::theme::color::TEXT_DISABLED));
}

fn divider(ui: &mut Ui) {
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(
        r.left() + 4.0..=r.left() + 220.0,
        r.top(),
        Stroke::new(1.0, bae_ui::theme::color::DIVIDER),
    );
    ui.add_space(5.0);
}

/// One of the six style buttons (on: highlighted).
fn style_button(
    ui: &mut Ui,
    on: bool,
    icon: Option<Icon>,
    text: &str,
    tip: &str,
    enabled: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(24.0, 22.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    use bae_ui::a11y::Accessible as _;
    let resp = resp
        .labeled_toggle(egui::WidgetType::Button, tip, on)
        .on_hover_text(tip);
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 3.0, bae_ui::theme::color::BUTTON_ON);
    } else if enabled && resp.hovered() {
        p.rect_filled(rect, 3.0, bae_ui::theme::color::HOVER);
    }
    let c = if enabled {
        bae_ui::theme::color::ICON
    } else {
        bae_ui::theme::color::TEXT_DISABLED
    };
    match icon {
        Some(i) => icons::paint(
            p,
            Rect::from_center_size(rect.center(), vec2(16.0, 16.0)),
            i,
            c,
        ),
        None => {
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(12.0),
                c,
            );
        }
    }
    resp
}

/// A dark dropdown field.
fn combo(ui: &mut Ui, current: &str, width: f32, label: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click());
    use bae_ui::a11y::Accessible as _;
    let resp = resp
        .labeled_value(egui::WidgetType::ComboBox, label, current)
        .on_hover_text(label);
    let p = ui.painter();
    let bg = if resp.hovered() {
        bae_ui::theme::color::FIELD_HOVER
    } else {
        bae_ui::theme::color::DROPDOWN
    };
    p.rect_filled(rect, 3.0, bg);
    bae_ui::widgets::clipped_text(
        p,
        pos2(rect.left() + 7.0, rect.center().y),
        current,
        bae_ui::theme::body(),
        bae_ui::theme::color::TEXT_STRONG,
        width - 28.0,
    );
    icons::paint(
        p,
        Rect::from_center_size(pos2(rect.right() - 11.0, rect.center().y), vec2(14.0, 14.0)),
        Icon::ChevronDown,
        bae_ui::theme::color::TEXT_STRONG,
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names_come_from_postscript_names() {
        let face = |ps: &str, italic| bae_text::FontFace {
            family: "Helvetica".into(),
            postscript_name: ps.into(),
            weight: 400.0,
            italic,
            width: 1.0,
        };
        assert_eq!(
            style_name(&face("Helvetica-BoldOblique", true)),
            "Bold Oblique"
        );
        assert_eq!(style_name(&face("Helvetica", false)), "Regular");
        assert_eq!(style_name(&face("ArialMT", false)), "Regular");
    }
}
