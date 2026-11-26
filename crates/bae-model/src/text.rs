//! Source Text, including the complete style state stored at each hold key.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BaselineOption {
    #[default]
    Normal,
    Superscript,
    Subscript,
}

/// How a text stroke's corners join (Character panel menu ▸ Line Join).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// Start of a new style in Unicode scalar indices. Its end is the next start.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StyleRun<T> {
    pub start: usize,
    pub style: T,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TextStyle {
    /// PostScript font name, e.g. `ArialMT`.
    pub font: String,
    pub font_size: f64,
    #[serde(default = "white")]
    pub fill_color: [f64; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_color: Option<[f64; 4]>,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub stroke_width: f64,
    #[serde(default)]
    pub stroke_over_fill: bool,
    /// Tracking in 1/1000 em.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub tracking: f64,
    /// Leading in pixels; `None` = auto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leading: Option<f64>,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub baseline_shift: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub faux_bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub faux_italic: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub all_caps: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub small_caps: bool,
    #[serde(default)]
    pub baseline_option: BaselineOption,
    /// Standard ligatures (Character panel menu ▸ Ligatures).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ligatures: bool,
    /// Digits drawn as Arabic-Indic ones (Hindi Digits).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hindi_digits: bool,
    /// Lines do not break inside this text (No Break).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_break: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub line_join: TextJoin,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ParagraphStyle {
    pub justification: String,
    pub left_indent: f64,
    pub right_indent: f64,
    pub first_line_indent: f64,
    pub space_before: f64,
    pub space_after: f64,
    /// Punctuation hangs outside the margins (Roman Hanging Punctuation).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hanging_punctuation: bool,
    /// Leading measured from the top of one line to the next (Top-to-Top
    /// Leading); `false`: baseline to baseline (AE's Bottom-to-Bottom).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub top_leading: bool,
    /// Lines balanced over the paragraph (Adobe Every-line Composer);
    /// `false`: Single-line Composer.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub every_line: bool,
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        Self {
            justification: "left".into(),
            left_indent: 0.,
            right_indent: 0.,
            first_line_indent: 0.,
            space_before: 0.,
            space_after: 0.,
            hanging_punctuation: false,
            top_leading: false,
            every_line: false,
        }
    }
}

/// The first character and paragraph styles start at zero, including for empty
/// text. Their flat encoding also reads single-style project documents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TextDocument {
    pub text: String,
    #[serde(flatten)]
    pub style: TextStyle,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<StyleRun<TextStyle>>,
    #[serde(flatten)]
    pub paragraph: ParagraphStyle,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paragraphs: Vec<StyleRun<ParagraphStyle>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub box_size: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub box_position: Option<[f64; 2]>,
}

fn white() -> [f64; 4] {
    [1.; 4]
}
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.
}

fn at<'a, T>(first: &'a T, runs: &'a [StyleRun<T>], i: usize) -> &'a T {
    runs[..runs.partition_point(|r| r.start <= i)]
        .last()
        .map_or(first, |r| &r.style)
}

fn split<T: Clone>(first: &T, runs: &mut Vec<StyleRun<T>>, index: usize) {
    if index != 0
        && let Err(k) = runs.binary_search_by_key(&index, |r| r.start)
    {
        let style = k.checked_sub(1).map_or(first, |n| &runs[n].style).clone();
        runs.insert(
            k,
            StyleRun {
                start: index,
                style,
            },
        );
    }
}

fn merge<T: PartialEq>(first: &T, runs: &mut Vec<StyleRun<T>>) {
    runs.dedup_by(|b, a| a.style == b.style);
    if runs.first().is_some_and(|r| &r.style == first) {
        runs.remove(0);
    }
}

fn edit<T: Clone + PartialEq>(
    first: &mut T,
    runs: &mut Vec<StyleRun<T>>,
    len: usize,
    range: Range<usize>,
    mut f: impl FnMut(&mut T),
) {
    let start = range.start.min(len);
    let end = range.end.min(len);
    if start >= end && len != 0 {
        return;
    }
    if end < len {
        split(first, runs, end);
    }
    split(first, runs, start);
    if start == 0 {
        f(first);
    }
    for r in runs
        .iter_mut()
        .skip_while(|r| r.start < start)
        .take_while(|r| r.start < end)
    {
        f(&mut r.style);
    }
    merge(first, runs);
}

impl TextDocument {
    pub fn new(text: impl Into<String>, font: impl Into<String>, font_size: f64) -> Self {
        Self {
            text: text.into(),
            style: TextStyle {
                font: font.into(),
                font_size,
                fill_color: white(),
                stroke_color: None,
                stroke_width: 0.,
                stroke_over_fill: false,
                tracking: 0.,
                leading: None,
                baseline_shift: 0.,
                faux_bold: false,
                faux_italic: false,
                all_caps: false,
                small_caps: false,
                baseline_option: BaselineOption::Normal,
                ligatures: false,
                hindi_digits: false,
                no_break: false,
                line_join: TextJoin::Miter,
            },
            runs: vec![],
            paragraph: ParagraphStyle::default(),
            paragraphs: vec![],
            box_size: None,
            box_position: None,
        }
    }

    pub fn style_at(&self, i: usize) -> &TextStyle {
        at(&self.style, &self.runs, i)
    }
    pub fn paragraph_at(&self, i: usize) -> &ParagraphStyle {
        at(&self.paragraph, &self.paragraphs, i)
    }

    pub fn edit_styles(&mut self, range: Range<usize>, f: impl FnMut(&mut TextStyle)) {
        edit(
            &mut self.style,
            &mut self.runs,
            self.text.chars().count(),
            range,
            f,
        );
    }

    /// Paragraph starts, including the empty paragraph after a trailing break.
    pub fn paragraph_starts(&self) -> Vec<usize> {
        let mut starts = vec![0];
        let mut chars = self.text.chars().enumerate().peekable();
        while let Some((i, c)) = chars.next() {
            if c == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
                continue;
            }
            if matches!(c, '\n' | '\r') {
                starts.push(i + 1);
            }
        }
        starts
    }

    pub fn edit_paragraphs(&mut self, range: Range<usize>, f: impl FnMut(&mut ParagraphStyle)) {
        let starts = self.paragraph_starts();
        let start = starts[starts.partition_point(|i| *i <= range.start) - 1];
        let len = self.text.chars().count() + 1;
        let end = starts
            .iter()
            .copied()
            .find(|i| *i > start && *i >= range.end)
            .unwrap_or(len);
        edit(
            &mut self.paragraph,
            &mut self.paragraphs,
            len,
            start..end,
            f,
        );
    }

    /// Inserted text takes the replaced character's style, or the preceding
    /// character's style at a caret. Suffixes retain their original styles.
    pub fn replace_range(&mut self, range: Range<usize>, replacement: &str) {
        let len = self.text.chars().count();
        let a = range.start.min(len);
        let b = range.end.clamp(a, len);
        let byte = |i| {
            self.text
                .char_indices()
                .nth(i)
                .map_or(self.text.len(), |(b, _)| b)
        };
        let bytes = byte(a)..byte(b);
        if &self.text[bytes.clone()] == replacement {
            return;
        }
        let n = replacement.chars().count();
        let inherited = self
            .style_at(if a == b { a.saturating_sub(1) } else { a })
            .clone();
        let suffix = self.style_at(b).clone();
        self.runs.retain(|r| r.start < a || r.start > b);
        for r in &mut self.runs {
            if r.start > b {
                r.start = r.start - b + a + n;
            }
        }
        for (i, style, keep) in [(a, inherited, n > 0 || len == b), (a + n, suffix, b < len)] {
            if keep {
                if i == 0 {
                    self.style = style;
                } else {
                    let k = self.runs.partition_point(|r| r.start < i);
                    self.runs.insert(k, StyleRun { start: i, style });
                }
            }
        }
        let new_len = len - b + a + n;
        self.runs.retain(|r| r.start < new_len);
        merge(&self.style, &mut self.runs);
        self.text.replace_range(bytes, replacement);
        let old = std::mem::take(&mut self.paragraphs);
        for i in self.paragraph_starts().into_iter().skip(1) {
            let source = if i <= a {
                i
            } else if i <= a + n {
                a
            } else {
                i - a - n + b
            };
            let style = at(&self.paragraph, &old, source);
            if style != self.paragraph_at(i) {
                self.paragraphs.push(StyleRun {
                    start: i,
                    style: style.clone(),
                });
            }
        }
    }

    /// For controls which supply a whole string, retain the common prefix and suffix.
    pub fn set_text(&mut self, value: &str) {
        if self.text == value {
            return;
        }
        let prefix = self
            .text
            .chars()
            .zip(value.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let old = self.text.chars().count();
        let new = value.chars().count();
        let suffix = self
            .text
            .chars()
            .rev()
            .zip(value.chars().rev())
            .take((old.min(new)) - prefix)
            .take_while(|(a, b)| a == b)
            .count();
        let byte = |i| value.char_indices().nth(i).map_or(value.len(), |(b, _)| b);
        self.replace_range(
            prefix..old - suffix,
            &value[byte(prefix)..byte(new - suffix)],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_split_shift_and_coalesce_unicode_styles() {
        let mut doc: TextDocument =
            serde_json::from_str(r#"{"text":"A中🙂B","font":"Arial","font_size":12}"#).unwrap();
        assert_eq!(doc.style_at(3), &doc.style);
        doc.edit_styles(1..3, |s| s.font_size = 24.);
        doc.replace_range(2..2, "xy");
        assert_eq!(doc.style_at(4).font_size, 24.);
        assert_eq!(doc.style_at(5).font_size, 12.);
        doc.replace_range(1..5, "");
        assert_eq!(doc.text, "AB");
        assert!(doc.runs.is_empty());
    }

    #[test]
    fn paragraph_splits_inherit_and_joins_keep_the_first_style() {
        let mut doc = TextDocument::new("one\r\ntwo\nthree", "Arial", 12.);
        doc.edit_paragraphs(5..7, |p| p.left_indent = 24.);
        doc.replace_range(6..6, "\n");
        assert_eq!(doc.paragraph_at(7).left_indent, 24.);
        doc.replace_range(3..5, "");
        assert_eq!(doc.paragraph_at(3).left_indent, 0.);
        assert_eq!(doc.paragraph_at(5).left_indent, 24.);
        assert_eq!(doc.paragraph_at(8).left_indent, 0.);
    }
}
