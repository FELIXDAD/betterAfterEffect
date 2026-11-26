//! The Paragraph panel (AE: Window > Paragraph, Cmd/Ctrl+7), laid out like AE
//! 26's: seven alignment buttons, then indent left / right margin, space before
//! / after, indent first line and the text direction buttons. Alignment goes to
//! the selected text layers (`text.set_style`, one undo step) or, with none
//! selected, to the settings for new text.

use bae_model::{PrimPath, TextDocument};
use bae_motion::query;
use bae_motion::text::{DEFAULT_FONT, DEFAULT_SIZE, SOURCE};
use bae_ui::icons::{self, Icon};
use bae_ui::{Panel, PanelContext, tr};
use egui::{Align2, Rect, Sense, Stroke, Ui, vec2};
use serde_json::json;

use crate::common;

/// AE's alignment buttons: (token, icon or line glyph, tooltip).
const ALIGNMENTS: [(&str, Option<Icon>, &str); 7] = [
    ("left", Some(Icon::TextAlignLeft), "Left align text"),
    ("center", Some(Icon::TextAlignCenter), "Center text"),
    ("right", Some(Icon::TextAlignRight), "Right align text"),
    ("justify_last_left", None, "Justify last line left"),
    ("justify_last_center", None, "Justify last line center"),
    ("justify_last_right", None, "Justify last line right"),
    (
        "justify_all",
        Some(Icon::TextAlignJustify),
        "Justify all lines",
    ),
];

/// The Paragraph panel.
#[derive(Default)]
pub struct ParagraphPanel;

impl Panel for ParagraphPanel {
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
        let doc = layers
            .first()
            .and_then(|l| {
                let t = query::to_key_time(cx.stage, l, time);
                match query::sample_attr(cx.stage, l, SOURCE, t) {
                    Some(bae_model::Value::TextDocument(d)) => Some(*d),
                    _ => None,
                }
            })
            .or(defaults)
            .unwrap_or_else(|| TextDocument::new("", DEFAULT_FONT, DEFAULT_SIZE));
        let range = session.lock().text_range(layers.first());
        let selected = common::SelectedStyles {
            doc: &doc,
            range: range.clone().unwrap_or(0..usize::MAX),
        };
        let token = selected
            .paragraph(|p| p.justification.clone())
            .unwrap_or_default();
        let current = if token == "justify_last_full" {
            "justify_all"
        } else {
            token.as_str()
        };
        let mut change = json!({});
        let mut gesture = super::character::Gesture::None;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(6.0);
            for (i, (token, icon, tip)) in ALIGNMENTS.iter().enumerate() {
                // AE groups them 3 + 3 + 1.
                if i == 3 || i == 6 {
                    ui.add_space(8.0);
                }
                if align_button(ui, *token == current, *icon, token, tr(tip)).clicked() {
                    change["justification"] = json!(token);
                }
            }
        });
        ui.add_space(6.0);
        // AE groups paragraph indentation and spacing in two rows.
        let fields = [
            [
                (
                    "left_indent",
                    selected.paragraph(|p| p.left_indent),
                    "Indent left margin",
                    "\u{2192}\u{2261}",
                ),
                (
                    "space_before",
                    selected.paragraph(|p| p.space_before),
                    "Add space before paragraph",
                    "\u{2261}\u{2191}",
                ),
                (
                    "first_line_indent",
                    selected.paragraph(|p| p.first_line_indent),
                    "Indent first line",
                    "\u{21b3}\u{2261}",
                ),
            ],
            [
                (
                    "right_indent",
                    selected.paragraph(|p| p.right_indent),
                    "Indent right margin",
                    "\u{2261}\u{2190}",
                ),
                (
                    "space_after",
                    selected.paragraph(|p| p.space_after),
                    "Add space after paragraph",
                    "\u{2261}\u{2193}",
                ),
                ("", None, "", ""),
            ],
        ];
        for row in fields {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                for (field, value, tip, glyph) in row {
                    if tip.is_empty() {
                        // AE: the text direction buttons.
                        ui.add_enabled_ui(false, |ui| {
                            for (dir, tip) in [
                                ("\u{25b8}\u{00b6}", "Left to right text direction"),
                                ("\u{00b6}\u{25c2}", "Right to left text direction"),
                            ] {
                                let (r, resp) =
                                    ui.allocate_exact_size(vec2(22.0, 18.0), Sense::hover());
                                ui.painter().text(
                                    r.center(),
                                    Align2::CENTER_CENTER,
                                    dir,
                                    egui::FontId::proportional(11.0),
                                    bae_ui::theme::color::TEXT_DISABLED,
                                );
                                resp.on_hover_text(tr(tip));
                            }
                        });
                        continue;
                    }
                    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                    ui.painter().text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        glyph,
                        egui::FontId::proportional(10.0),
                        bae_ui::theme::color::ICON,
                    );
                    resp.on_hover_text(tr(tip));
                    if let Some(v) = super::character::number(
                        ui,
                        &mut gesture,
                        ui.id().with(field),
                        value,
                        tr("px"),
                        -10000.0..=10000.0,
                        1,
                        tr(tip),
                    ) {
                        change[field] = json!(v);
                    }
                    ui.add_space(14.0);
                }
            });
        }
        super::text_menu::Selection {
            layers,
            time,
            range,
            doc,
        }
        .send(cx, change, gesture);
    }

    fn menu(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        super::text_menu::show(ui, cx, false);
    }
}

/// One alignment button: a Spectrum icon, or for the "justify last line"
/// ones AE's four lines with the last one short on its side.
fn align_button(
    ui: &mut Ui,
    on: bool,
    icon: Option<Icon>,
    token: &str,
    tip: &str,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::click());
    use bae_ui::a11y::Accessible as _;
    let resp = resp
        .labeled_toggle(egui::WidgetType::Button, tip, on)
        .on_hover_text(tip);
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 3.0, bae_ui::theme::color::BUTTON_ON);
    } else if resp.hovered() {
        p.rect_filled(rect, 3.0, bae_ui::theme::color::HOVER);
    }
    let c = bae_ui::theme::color::ICON;
    match icon {
        Some(i) => icons::paint(
            p,
            Rect::from_center_size(rect.center(), vec2(16.0, 16.0)),
            i,
            c,
        ),
        None => {
            let g = Rect::from_center_size(rect.center(), vec2(14.0, 11.0));
            let stroke = Stroke::new(1.5, c);
            for k in 0..3 {
                let y = g.top() + k as f32 * 3.5;
                p.hline(g.left()..=g.right(), y, stroke);
            }
            let (x0, x1) = match token {
                "justify_last_center" => (g.left() + 3.5, g.right() - 3.5),
                "justify_last_right" => (g.left() + 6.0, g.right()),
                _ => (g.left(), g.right() - 6.0),
            };
            p.hline(x0..=x1, g.bottom(), stroke);
        }
    }
    resp
}
