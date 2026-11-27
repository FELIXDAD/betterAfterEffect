//! The Character and Paragraph panel menus (AE) and how both panels apply a
//! change: to the selected text layers (their selected characters while the
//! Type tool edits one), else to the settings for new text.
use std::ops::Range;

use super::character::Gesture;
use crate::common;
use crate::session::Scrub;
use bae_model::{BaselineOption, PrimPath, TextDocument, TextJoin, Time, Value as ModelValue};
use bae_motion::{
    query,
    text::{DEFAULT_FONT, DEFAULT_SIZE, SOURCE},
};
use bae_ui::{PanelContext, tr};
use egui::Ui;
use serde_json::{Value, json};

/// What the Character and Paragraph panels edit.
pub(super) struct Selection {
    pub layers: Vec<PrimPath>,
    pub time: Time,
    /// The selected characters of the one layer being edited.
    pub range: Option<Range<usize>>,
    /// The first layer's text (or the settings for new text) the panels show.
    pub doc: TextDocument,
}

impl Selection {
    pub fn read(cx: &PanelContext<'_>) -> Self {
        let session = common::session(cx.resources);
        let s = session.lock();
        let layers: Vec<PrimPath> = s
            .selected_layers
            .iter()
            .filter(|l| query::layer_kind(cx.stage, l).as_deref() == Some("text"))
            .cloned()
            .collect();
        let time = s.active_comp.as_ref().map_or(Time::ZERO, |c| s.time(c));
        let doc = layers
            .first()
            .and_then(|l| source(cx, l, time))
            .or_else(|| s.text_defaults.clone())
            .unwrap_or_else(|| TextDocument::new("", DEFAULT_FONT, DEFAULT_SIZE));
        let range = s.text_range(layers.first());
        Self {
            layers,
            time,
            range,
            doc,
        }
    }

    pub fn styles(&self) -> common::SelectedStyles<'_> {
        common::SelectedStyles {
            doc: &self.doc,
            range: self.range.clone().unwrap_or(0..usize::MAX),
        }
    }

    /// One undo step (`text.set_style`); with no text layer selected, the
    /// settings for new text (AE).
    pub fn apply(&self, cx: &mut PanelContext<'_>, change: Value) {
        if self.layers.is_empty() {
            let mut d = self.doc.clone();
            if let Ok(c) = serde_json::from_value::<bae_motion::TextStyleChange>(change) {
                c.apply(&mut d, 0..usize::MAX);
            }
            common::session(cx.resources).lock().text_defaults = Some(d);
            return;
        }
        // Shown as the command will leave it until it runs (no flash of the
        // old value after a drag).
        self.scrub(
            cx,
            &change,
            Some((cx.stage.revision(), std::time::Instant::now())),
        );
        cx.invoke_with(
            "text.set_style",
            json!({ "layers": self.layers, "time": self.time, "style": change, "range": self.range }),
        );
    }

    /// What a panel changed this frame: previewed while a number is dragged,
    /// one command otherwise; a drag released on its starting value leaves
    /// nothing behind (no undo step, no preview).
    pub fn send(self, cx: &mut PanelContext<'_>, change: Value, gesture: Gesture) {
        let empty = change.as_object().is_none_or(|o| o.is_empty());
        match (empty, gesture) {
            (true, Gesture::Ended) => {
                let session = common::session(cx.resources);
                let mut s = session.lock();
                if s.scrub
                    .as_ref()
                    .is_some_and(|d| d.attr == SOURCE && d.committed.is_none())
                {
                    s.scrub = None;
                }
            }
            (true, _) => {}
            (false, Gesture::Live) if !self.layers.is_empty() => self.scrub(cx, &change, None),
            (false, _) => self.apply(cx, change),
        }
    }

    fn scrub(
        &self,
        cx: &PanelContext<'_>,
        change: &Value,
        committed: Option<(u64, std::time::Instant)>,
    ) {
        let Ok(style) = serde_json::from_value::<bae_motion::TextStyleChange>(change.clone())
        else {
            return;
        };
        let mut targets = self
            .layers
            .iter()
            .filter(|l| !query::is_locked(cx.stage, l))
            .filter_map(|l| {
                let mut doc = source(cx, l, self.time)?;
                let range = self.range.clone().unwrap_or(0..usize::MAX);
                style.apply(&mut doc, range);
                Some((l.clone(), ModelValue::TextDocument(Box::new(doc))))
            });
        let Some((path, value)) = targets.next() else {
            return;
        };
        common::session(cx.resources).lock().scrub = Some(Scrub {
            path,
            attr: SOURCE.into(),
            value,
            peers: targets.collect(),
            time: self.time,
            auto_key: false,
            committed,
        });
    }
}

/// A text layer's Source Text at comp time `time`.
fn source(cx: &PanelContext<'_>, layer: &PrimPath, time: Time) -> Option<TextDocument> {
    let t = query::to_key_time(cx.stage, layer, time);
    match query::sample_attr(cx.stage, layer, SOURCE, t) {
        Some(ModelValue::TextDocument(d)) => Some(*d),
        _ => None,
    }
}

/// The Character panel menu in AE's order ("" = separator).
const CHARACTER: &[(&str, &str)] = &[
    ("faux_bold", "Faux Bold"),
    ("faux_italic", "Faux Italic"),
    ("", ""),
    ("vertical", "Standard Vertical Roman Alignment"),
    ("vertical", "Tate-Chuu-Yoko"),
    ("", ""),
    ("all_caps", "All Caps"),
    ("small_caps", "Small Caps"),
    ("superscript", "Superscript"),
    ("subscript", "Subscript"),
    ("", ""),
    ("ligatures", "Ligatures"),
    ("hindi_digits", "Hindi Digits"),
    ("", ""),
    ("join", "Line Join"),
    ("", ""),
    ("quotes", "Use Smart Quotes"),
    ("font_names", "Show Font Names in English"),
    ("", ""),
    ("no_break", "No Break"),
    ("", ""),
    ("reset_character", "Reset Character"),
];

/// The Paragraph panel menu in AE's order.
const PARAGRAPH: &[(&str, &str)] = &[
    ("hanging_punctuation", "Roman Hanging Punctuation"),
    ("", ""),
    ("top_leading", "Top-to-Top Leading"),
    ("bottom_leading", "Bottom-to-Bottom Leading"),
    ("", ""),
    ("single_line", "Adobe Single-line Composer"),
    ("every_line", "Adobe Every-line Composer"),
    ("", ""),
    ("reset_paragraph", "Reset Paragraph"),
];

/// Whether a menu item works and its check mark (`None`: a plain item or
/// mixed over the selection).
fn state(key: &str, sel: &common::SelectedStyles<'_>, smart: bool) -> (bool, Option<bool>) {
    let checked = match key {
        // Vertical text and localized font names are not done yet.
        "vertical" | "font_names" => return (false, None),
        "faux_bold" => sel.character(|s| s.faux_bold),
        "faux_italic" => sel.character(|s| s.faux_italic),
        "all_caps" => sel.character(|s| s.all_caps),
        "small_caps" => sel.character(|s| s.small_caps),
        "superscript" => sel.character(|s| s.baseline_option == BaselineOption::Superscript),
        "subscript" => sel.character(|s| s.baseline_option == BaselineOption::Subscript),
        "ligatures" => sel.character(|s| s.ligatures),
        "hindi_digits" => sel.character(|s| s.hindi_digits),
        "no_break" => sel.character(|s| s.no_break),
        "quotes" => Some(smart),
        "hanging_punctuation" => sel.paragraph(|p| p.hanging_punctuation),
        "top_leading" => sel.paragraph(|p| p.top_leading),
        "bottom_leading" => sel.paragraph(|p| !p.top_leading),
        "single_line" => sel.paragraph(|p| !p.every_line),
        "every_line" => sel.paragraph(|p| p.every_line),
        _ => return (true, None),
    };
    (true, Some(checked.unwrap_or(false)))
}

/// The change a menu item makes, given its check mark.
fn change(key: &str, checked: bool) -> Value {
    match key {
        "superscript" | "subscript" => {
            let on = if key == "superscript" {
                BaselineOption::Superscript
            } else {
                BaselineOption::Subscript
            };
            json!({ "baseline_option": if checked { BaselineOption::Normal } else { on } })
        }
        "top_leading" | "bottom_leading" => json!({ "top_leading": key == "top_leading" }),
        "single_line" | "every_line" => json!({ "every_line": key == "every_line" }),
        "reset_character" | "reset_paragraph" => json!({ (key): true }),
        _ => json!({ (key): !checked }),
    }
}

/// The Character (`character`) or Paragraph panel menu.
pub(super) fn show(ui: &mut Ui, cx: &mut PanelContext<'_>, character: bool) {
    let selection = Selection::read(cx);
    let styles = selection.styles();
    let session = common::session(cx.resources);
    let smart = !session.lock().straight_quotes;
    for &(key, title) in if character { CHARACTER } else { PARAGRAPH } {
        if key.is_empty() {
            ui.separator();
            continue;
        }
        if key == "join" {
            let join = styles.character(|s| s.line_join);
            bae_ui::widgets::menu_button(ui, tr(title), |ui| {
                for (value, title) in [
                    (TextJoin::Miter, "Miter Join"),
                    (TextJoin::Round, "Round Join"),
                    (TextJoin::Bevel, "Bevel Join"),
                ] {
                    if ui.radio(join == Some(value), tr(title)).clicked() {
                        selection.apply(cx, json!({ "line_join": value }));
                        ui.close();
                    }
                }
            });
            continue;
        }
        let (enabled, checked) = state(key, &styles, smart);
        let clicked = match checked {
            Some(mut on) => ui
                .add_enabled(enabled, egui::Checkbox::new(&mut on, tr(title)))
                .clicked(),
            None => ui
                .add_enabled(enabled, egui::Button::new(tr(title)))
                .clicked(),
        };
        if clicked {
            if key == "quotes" {
                session.lock().straight_quotes = smart;
            } else {
                selection.apply(cx, change(key, checked.unwrap_or(false)));
            }
            ui.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menus_follow_ae_and_toggle_the_selection() {
        let mut doc = TextDocument::new("ab", DEFAULT_FONT, DEFAULT_SIZE);
        doc.style.baseline_option = BaselineOption::Superscript;
        let sel = common::SelectedStyles {
            doc: &doc,
            range: 0..2,
        };
        let rows: Vec<_> = CHARACTER
            .iter()
            .map(|&(k, _)| state(k, &sel, true))
            .collect();
        assert_eq!(CHARACTER.len(), 22);
        assert_eq!(rows[3], (false, None), "vertical text greyed");
        assert_eq!(rows[8], (true, Some(true)), "Superscript checked");
        assert_eq!(rows[16], (true, Some(true)), "smart quotes on");
        assert_eq!(PARAGRAPH.len(), 9);
        assert_eq!(state("bottom_leading", &sel, true), (true, Some(true)));
        // Unchecking Superscript returns to Normal; Subscript replaces it.
        assert_eq!(
            change("superscript", true),
            json!({"baseline_option": "normal"})
        );
        assert_eq!(
            change("subscript", false),
            json!({"baseline_option": "subscript"})
        );
        for (_, title) in CHARACTER
            .iter()
            .chain(PARAGRAPH)
            .filter(|(k, _)| !k.is_empty())
        {
            assert!(
                super::super::ZH.iter().any(|(e, _)| e == title),
                "zh: {title}"
            );
        }
    }
}
