use crate::common;
use bae_model::{PrimPath, TextDocument, TextJoin, Time};
use bae_motion::{
    query,
    text::{DEFAULT_FONT, DEFAULT_SIZE},
};
use bae_ui::{PanelContext, tr};
use egui::Ui;
use serde_json::{Value, json};

pub(super) struct Selection {
    layers: Vec<PrimPath>,
    time: Time,
    pub doc: TextDocument,
}
impl Selection {
    pub fn read(cx: &PanelContext<'_>) -> Self {
        let session = common::session(cx.resources);
        let s = session.lock();
        let layers: Vec<_> = s
            .selected_layers
            .iter()
            .filter(|p| query::layer_kind(cx.stage, p).as_deref() == Some("text"))
            .cloned()
            .collect();
        let time = s.active_time();
        let doc = super::character::current_doc(cx.stage, &layers, time)
            .or_else(|| s.text_defaults.clone())
            .unwrap_or_else(|| TextDocument::new("", DEFAULT_FONT, DEFAULT_SIZE));
        Self { layers, time, doc }
    }
    pub fn scrub(&self, cx: &mut PanelContext<'_>, change: Value, dragging: bool) {
        if self.layers.is_empty() {
            self.apply(cx, change);
            return;
        }
        let Ok(style) = serde_json::from_value::<bae_motion::TextStyleChange>(change) else {
            return;
        };
        let mut values = self
            .layers
            .iter()
            .filter(|p| !query::is_locked(cx.stage, p))
            .filter_map(|p| {
                let mut doc =
                    super::character::current_doc(cx.stage, std::slice::from_ref(p), self.time)?;
                style.apply(&mut doc);
                Some((p.clone(), bae_model::Value::TextDocument(Box::new(doc))))
            });
        let Some((path, value)) = values.next() else {
            return;
        };
        let edit = crate::session::Scrub {
            path,
            value,
            attr: bae_motion::text::SOURCE.into(),
            peers: values.collect(),
            time: self.time,
            committed: None,
        };
        common::apply_selection_edit(cx, &common::session(cx.resources), edit, !dragging);
    }
    pub fn apply(&self, cx: &mut PanelContext<'_>, change: Value) {
        if self.layers.is_empty() {
            let mut doc = self.doc.clone();
            if let Ok(change) = serde_json::from_value::<bae_motion::TextStyleChange>(change) {
                change.apply(&mut doc);
                common::session(cx.resources).lock().text_defaults = Some(doc);
            }
        } else {
            cx.invoke_with(
                "text.set_style",
                json!({"layers":self.layers,"time":self.time,"style":change}),
            );
        }
    }
}
const CHAR: &[(&str, &str)] = &[
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
const PARA: &[(&str, &str)] = &[
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
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Faux Bold", "仿粗体"),
    ("Faux Italic", "仿斜体"),
    ("Standard Vertical Roman Alignment", "标准垂直罗马对齐方式"),
    ("Tate-Chuu-Yoko", "直排内横排"),
    ("All Caps", "全部大写字母"),
    ("Small Caps", "小型大写字母"),
    ("Superscript", "上标"),
    ("Subscript", "下标"),
    ("Ligatures", "连字"),
    ("Hindi Digits", "印地语数字"),
    ("Line Join", "线段连接"),
    ("Use Smart Quotes", "使用弯引号"),
    ("Show Font Names in English", "显示英文字体名称"),
    ("No Break", "无间断"),
    ("Reset Character", "重置字符"),
    ("Roman Hanging Punctuation", "罗马悬挂式标点"),
    ("Top-to-Top Leading", "顶到顶行距"),
    ("Bottom-to-Bottom Leading", "底到底行距"),
    ("Adobe Single-line Composer", "Adobe 单行书写器"),
    ("Adobe Every-line Composer", "Adobe 逐行书写器"),
    ("Reset Paragraph", "重置段落"),
    ("Miter Join", "尖角"),
    ("Round Join", "圆角"),
    ("Bevel Join", "斜角"),
];

fn state(key: &str, doc: &TextDocument, smart: bool, english: bool) -> (bool, Option<bool>) {
    let t = &doc.typography;
    let checked = match key {
        "vertical" => return (false, None),
        "faux_bold" => doc.faux_bold,
        "faux_italic" => doc.faux_italic,
        "all_caps" => doc.all_caps,
        "small_caps" => doc.small_caps,
        "superscript" => t.superscript,
        "subscript" => t.subscript,
        "ligatures" => t.ligatures,
        "hindi_digits" => t.hindi_digits,
        "no_break" => t.no_break,
        "quotes" => smart,
        "font_names" => english,
        "hanging_punctuation" => t.hanging_punctuation,
        "top_leading" => t.top_leading,
        "bottom_leading" => !t.top_leading,
        "single_line" => !t.every_line,
        "every_line" => t.every_line,
        _ => return (true, None),
    };
    (true, Some(checked))
}
pub(super) fn show(ui: &mut Ui, cx: &mut PanelContext<'_>, character: bool) {
    let selection = Selection::read(cx);
    let session = common::session(cx.resources);
    let (smart, english) = {
        let s = session.lock();
        (!s.straight_quotes, s.english_font_names)
    };
    for &(key, title) in if character { CHAR } else { PARA } {
        if key.is_empty() {
            ui.separator();
            continue;
        }
        if key == "join" {
            ui.menu_button(tr(title), |ui| {
                for (join, title) in [
                    (TextJoin::Miter, "Miter Join"),
                    (TextJoin::Round, "Round Join"),
                    (TextJoin::Bevel, "Bevel Join"),
                ] {
                    if ui
                        .radio(selection.doc.typography.line_join == join, tr(title))
                        .clicked()
                    {
                        selection.apply(cx, json!({"line_join":join}));
                        ui.close();
                    }
                }
            });
            continue;
        }
        let (enabled, checked) = state(key, &selection.doc, smart, english);
        let click = match checked {
            Some(mut on) => ui
                .add_enabled(enabled, egui::Checkbox::new(&mut on, tr(title)))
                .clicked(),
            None => ui
                .add_enabled(enabled, egui::Button::new(tr(title)))
                .clicked(),
        };
        if click {
            match key {
                "quotes" => session.lock().straight_quotes = smart,
                "font_names" => session.lock().english_font_names = !english,
                "top_leading" | "bottom_leading" => {
                    selection.apply(cx, json!({"top_leading":key=="top_leading"}))
                }
                "single_line" | "every_line" => {
                    selection.apply(cx, json!({"every_line":key=="every_line"}))
                }
                _ => selection.apply(cx, json!({(key):!checked.unwrap_or(false)})),
            }
            ui.close();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chinese_menu_states_match_horizontal_text() {
        let doc = TextDocument::new("", DEFAULT_FONT, DEFAULT_SIZE);
        let rows = |table: &[(&str, &str)]| {
            table
                .iter()
                .map(|&(key, en)| {
                    let zh = ZH.iter().find(|p| p.0 == en).map_or("", |p| p.1);
                    (zh, state(key, &doc, true, false))
                })
                .collect::<Vec<_>>()
        };
        let ch = rows(CHAR);
        let par = rows(PARA);
        assert_eq!(ch.len(), 22);
        assert_eq!(
            &ch[3..5],
            &[
                ("标准垂直罗马对齐方式", (false, None)),
                ("直排内横排", (false, None))
            ]
        );
        assert_eq!(ch[16], ("使用弯引号", (true, Some(true))));
        assert_eq!(ch[14], ("线段连接", (true, None)));
        assert_eq!(par.len(), 9);
        assert_eq!(par[3], ("底到底行距", (true, Some(true))));
        assert_eq!(par[5], ("Adobe 单行书写器", (true, Some(true))));
    }
}
