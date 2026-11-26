//! Text layout: a [`TextDocument`] → positioned glyph runs, lines and characters
//! in layer space (AE-TXT-003/004/040..045).
//!
//! Shaping, font fallback and line breaking (UAX #14, so Chinese breaks between
//! ideographs and never before closing punctuation) come from parley; the
//! AE-specific placement is done here:
//!
//! * point text: the first baseline is at y = 0; Left starts lines at x = 0,
//!   Center centres them on x = 0, Right ends them at x = 0;
//! * paragraph text: lines wrap at the box width; the first baseline is the box
//!   top plus the first line's ascent; lines that do not fit in the box height
//!   are overset (not drawn, not counted);
//! * baselines step down by the leading (auto = 120 % of the font size);
//! * tracking (1/1000 em) is added after every character and left out of the
//!   measured line width; baseline shift moves glyphs up.

use std::hash::{Hash, Hasher};
use std::ops::Range;

use bae_model::{BaselineOption, TextDocument, TextStyle};
use bae_vector::paint::{LineJoin, Rgba, paint_color};
use kurbo::{Affine, Point, Rect, Vec2};

mod vertical;
use parley::{
    Alignment, AlignmentOptions, FontData, FontFamily, FontFamilyName, FontStyle, FontWeight,
    FontWidth, GenericFamily, StyleProperty,
};
use rustc_hash::FxHasher;
use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::{GlyphId, MetadataProvider};

use crate::fonts::{FontMatch, FontSystem};

/// Auto leading: 120 % of the font size (AE-TXT-020).
pub const AUTO_LEADING: f64 = 1.2;
/// Faux bold: outline offset per side, in em (AE-TXT-023, BAE decision until measured).
pub const FAUX_BOLD_OFFSET_EM: f64 = 0.02;
/// Faux italic shear angle in degrees (AE-TXT-023, BAE decision until measured).
pub const FAUX_ITALIC_DEGREES: f64 = 12.0;

/// Paragraph justification (AE-TXT-028).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Justification {
    #[default]
    Left,
    Center,
    Right,
    JustifyLastLeft,
    JustifyLastCenter,
    JustifyLastRight,
    JustifyAll,
}

impl Justification {
    /// From the `TextDocument.justification` token.
    pub fn from_token(token: &str) -> Self {
        match token {
            "center" => Justification::Center,
            "right" => Justification::Right,
            "justify_last_left" => Justification::JustifyLastLeft,
            "justify_last_center" => Justification::JustifyLastCenter,
            "justify_last_right" => Justification::JustifyLastRight,
            "justify_all" | "justify_last_full" => Justification::JustifyAll,
            _ => Justification::Left,
        }
    }

    /// Alignment of lines that are not justified (point text, last lines).
    fn simple(self) -> Justification {
        match self {
            Justification::JustifyLastCenter => Justification::Center,
            Justification::JustifyLastRight => Justification::Right,
            Justification::JustifyLastLeft | Justification::JustifyAll => Justification::Left,
            other => other,
        }
    }

    fn is_justify(self) -> bool {
        matches!(
            self,
            Justification::JustifyLastLeft
                | Justification::JustifyLastCenter
                | Justification::JustifyLastRight
                | Justification::JustifyAll
        )
    }
}

/// Text stroke (AE-TXT-016). Glyph outlines are closed, so there are no caps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStroke {
    pub color: Rgba,
    /// Pixels, centred on the outline.
    pub width: f64,
    pub join: LineJoin,
    pub miter_limit: f64,
}

/// Fill and stroke of the text (one style per document until style runs land).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextPaint {
    /// `None` = no fill.
    pub fill: Option<Rgba>,
    /// `None` = no stroke.
    pub stroke: Option<TextStroke>,
    /// Per character: stroke drawn over the fill (AE-TXT-017).
    pub stroke_over_fill: bool,
}

/// A glyph placed in layer space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedGlyph {
    /// Glyph ID in the run's font.
    pub id: u32,
    /// Baseline origin, layer pixels (y down).
    pub x: f32,
    pub y: f32,
    /// Index into [`TextLayout::chars`].
    pub char_index: usize,
}

/// Glyphs sharing one font face, size and synthesis.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRun {
    /// Glyph-local transform in layer axes (before positioning).
    pub basis: Affine,
    pub paint: TextPaint,
    /// Font data (the same type as `vello::peniko::FontData`).
    pub font: FontData,
    /// Pixels per em.
    pub font_size: f32,
    /// Variable font instance: normalized design coordinates as F2Dot14 bits.
    pub normalized_coords: Vec<i16>,
    /// Outline expansion per side in pixels (faux bold or synthesized bold).
    pub embolden: f32,
    /// Shear angle in degrees (faux italic or synthesized oblique); positive leans right.
    pub skew_degrees: f32,
    pub glyphs: Vec<PositionedGlyph>,
}

/// What a character is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CharKind {
    /// Has glyphs.
    Glyph,
    /// Space or other whitespace: advance, no outline.
    Space,
    /// Paragraph break or soft return: no glyph.
    LineBreak,
}

/// One animatable "character" (a shaped cluster ≈ grapheme, AE-TXT-012).
#[derive(Clone, Debug, PartialEq)]
pub struct TextChar {
    pub right_to_left: bool,
    /// Height of a tate-chu-yoko cell containing this horizontal character.
    pub combined_em: Option<f64>,
    pub paint: TextPaint,
    /// Char indices (Unicode scalar values) into `TextDocument.text`.
    pub source: Range<usize>,
    /// Index into [`TextLayout::lines`].
    pub line: usize,
    /// Word index (words are separated by whitespace and line breaks).
    pub word: usize,
    /// The start of the advance on the baseline, in layer pixels.
    pub origin: Point,
    /// Advance to the next character, including tracking, in layer pixels.
    pub advance: f64,
    pub kind: CharKind,
}

/// One composed line.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    /// Baseline y for a row, or centre x for a column, in layer pixels.
    pub baseline: f64,
    /// Start along the line after alignment (x for a row, y for a column).
    pub x: f64,
    /// Inline extent, excluding trailing spaces and trailing tracking.
    pub width: f64,
    /// Ascent and descent of the line's fonts in pixels (positive).
    pub ascent: f64,
    pub descent: f64,
    /// Range in [`TextLayout::chars`].
    pub chars: Range<usize>,
}

/// The result of laying out a text document: everything needed to draw it, to
/// animate it per character and to answer `sourceRectAtTime`.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub orientation: bae_model::LineOrientation,
    pub runs: Vec<GlyphRun>,
    pub lines: Vec<TextLine>,
    pub chars: Vec<TextChar>,
    /// Per-character drawing state, empty for plain text.
    pub styles: Vec<crate::animator::CharacterStyle>,
    pub paint: TextPaint,
    /// Paragraph text box (layer pixels); `None` for point text.
    pub box_rect: Option<Rect>,
    /// How the document's font resolved; [`FontMatch::Missing`] means a
    /// substitute was used (AE-TXT-083).
    pub font: FontMatch,
    /// Paragraph text that did not fit in the box (AE-TXT-004).
    pub overset: bool,
}

impl TextLayout {
    /// An empty layout (no glyphs).
    pub fn empty(paint: TextPaint) -> Self {
        Self {
            orientation: bae_model::LineOrientation::Horizontal,
            runs: Vec::new(),
            lines: Vec::new(),
            chars: Vec::new(),
            styles: Vec::new(),
            paint,
            box_rect: None,
            font: FontMatch::Missing,
            overset: false,
        }
    }

    /// Whether nothing is drawn.
    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|r| r.glyphs.is_empty())
    }

    /// Nearest insertion point in source-character coordinates, including RTL runs.
    pub fn cursor_at(&self, x: f64, y: f64) -> Option<usize> {
        let distance = |l: &TextLine| {
            let r = self.line_rect(l);
            if self.vertical() {
                (r.x0 - x).max(x - r.x1).max(0.)
            } else {
                (r.y0 - y).max(y - r.y1).max(0.)
            }
        };
        let line = self
            .lines
            .iter()
            .min_by(|a, b| distance(a).total_cmp(&distance(b)))?;
        let chars = &self.chars[line.chars.clone()];
        chars
            .iter()
            .flat_map(|c| {
                let (left, right) = if c.right_to_left {
                    (c.source.end, c.source.start)
                } else {
                    (c.source.start, c.source.end)
                };
                let right = if c.kind == CharKind::LineBreak {
                    left
                } else {
                    right
                };
                [(false, left), (true, right)].map(|(end, index)| {
                    let [a, b] = self.caret_edge(c, end);
                    let dx = (a.x.min(b.x) - x).max(x - a.x.max(b.x)).max(0.);
                    let dy = (a.y.min(b.y) - y).max(y - a.y.max(b.y)).max(0.);
                    (dx * dx + dy * dy, index)
                })
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, i)| i)
            .or_else(|| {
                self.chars
                    .get(line.chars.start.checked_sub(1)?)
                    .map(|c| c.source.end)
            })
    }

    pub fn vertical(&self) -> bool {
        self.orientation == bae_model::LineOrientation::Vertical
    }

    /// Physical rectangle of a horizontal line or vertical column.
    pub fn line_rect(&self, line: &TextLine) -> Rect {
        if self.vertical() {
            Rect::new(
                line.baseline - line.descent,
                line.x,
                line.baseline + line.ascent,
                line.x + line.width,
            )
        } else {
            Rect::new(
                line.x,
                line.baseline - line.ascent,
                line.x + line.width,
                line.baseline + line.descent,
            )
        }
    }

    /// The two endpoints of a caret at the visual start or end of a cluster.
    pub fn caret_edge(&self, c: &TextChar, end: bool) -> [Point; 2] {
        let line = &self.lines[c.line];
        let offset = if end { c.advance } else { 0. };
        if let Some(em) = c.combined_em {
            let x = c.origin.x + offset;
            return [
                Point::new(x, c.origin.y - em / 2.),
                Point::new(x, c.origin.y + em / 2.),
            ];
        }
        if self.vertical() {
            let y = c.origin.y + offset;
            [
                Point::new(c.origin.x - line.descent, y),
                Point::new(c.origin.x + line.ascent, y),
            ]
        } else {
            let x = c.origin.x + offset;
            [
                Point::new(x, c.origin.y - line.ascent),
                Point::new(x, c.origin.y + line.descent),
            ]
        }
    }

    /// Ink bounds of all glyphs in layer space (`sourceRectAtTime` with
    /// `includeExtents = false`, AE-TXT-007); `with_stroke` adds half the stroke
    /// width (times the miter limit for miter joins). `None` when nothing is drawn.
    pub fn bounds(&self, with_stroke: bool) -> Option<Rect> {
        let mut out: Option<Rect> = None;
        for run in &self.runs {
            let stroke = match (with_stroke, run.paint.stroke) {
                (true, Some(s)) => {
                    let k = match s.join {
                        LineJoin::Miter => s.miter_limit.max(1.0),
                        _ => 1.0,
                    };
                    s.width.max(0.0) / 2.0 * k
                }
                _ => 0.0,
            };
            let Ok(font) = skrifa::FontRef::from_index(run.font.data.as_ref(), run.font.index)
            else {
                continue;
            };
            let coords: Vec<NormalizedCoord> = run
                .normalized_coords
                .iter()
                .map(|b| NormalizedCoord::from_bits(*b))
                .collect();
            let metrics = font.glyph_metrics(Size::new(run.font_size), LocationRef::new(&coords));
            let lean = (f64::from(run.skew_degrees).to_radians().tan()).abs();
            let grow = f64::from(run.embolden);
            for g in &run.glyphs {
                let Some(b) = metrics.bounds(GlyphId::new(g.id)) else {
                    continue;
                };
                if b.x_min >= b.x_max && b.y_min >= b.y_max {
                    continue;
                }
                let (x, y) = (f64::from(g.x), f64::from(g.y));
                // Font units are y up; layer space is y down.
                let top = f64::from(b.y_max);
                let bottom = f64::from(b.y_min);
                let r = Rect::new(
                    f64::from(b.x_min) - lean * bottom.abs().max(0.0) - grow,
                    -top - grow,
                    f64::from(b.x_max) + lean * top.max(0.0) + grow,
                    -bottom + grow,
                );
                let r = run.basis.transform_rect_bbox(r.inflate(stroke, stroke)) + Vec2::new(x, y);
                let r = self
                    .styles
                    .get(g.char_index)
                    .map_or(r, |s| s.transform.transform_rect_bbox(r));
                out = Some(out.map_or(r, |o| o.union(r)));
            }
        }
        out
    }

    /// Hash of everything that affects the pixels.
    pub fn content_hash(&self) -> u64 {
        let mut h = FxHasher::default();
        self.orientation.hash(&mut h);
        for run in &self.runs {
            run.basis.as_coeffs().map(f64::to_bits).hash(&mut h);
            hash_paint(&run.paint, &mut h);
            run.font.data.id().hash(&mut h);
            run.font.index.hash(&mut h);
            run.font_size.to_bits().hash(&mut h);
            run.embolden.to_bits().hash(&mut h);
            run.skew_degrees.to_bits().hash(&mut h);
            for c in &run.normalized_coords {
                c.hash(&mut h);
            }
            for g in &run.glyphs {
                g.id.hash(&mut h);
                g.x.to_bits().hash(&mut h);
                g.y.to_bits().hash(&mut h);
                g.char_index.hash(&mut h);
            }
        }
        for style in &self.styles {
            style.opacity.to_bits().hash(&mut h);
            style.transform.as_coeffs().map(f64::to_bits).hash(&mut h);
            hash_paint(&style.paint, &mut h);
        }
        h.finish()
    }
}

fn hash_paint(p: &TextPaint, h: &mut FxHasher) {
    p.fill.map(|c| c.map(f32::to_bits)).hash(h);
    if let Some(s) = p.stroke {
        s.color.map(f32::to_bits).hash(h);
        s.width.to_bits().hash(h);
        s.join.hash(h);
        s.miter_limit.to_bits().hash(h);
    }
    p.stroke_over_fill.hash(h);
}

/// The paint of a document (AE-TXT-015..017).
pub fn document_paint(doc: &TextStyle) -> TextPaint {
    let stroke = doc
        .stroke_color
        .filter(|_| doc.stroke_width > 0.0)
        .map(|c| TextStroke {
            color: paint_color(c),
            width: doc.stroke_width,
            join: LineJoin::Miter,
            miter_limit: 4.0,
        });
    TextPaint {
        fill: Some(paint_color(doc.fill_color)),
        stroke,
        stroke_over_fill: doc.stroke_over_fill,
    }
}

/// The text parley lays out, with a map back to the document's characters.
struct DisplayText {
    text: String,
    /// For every char of `text`: (byte offset in `text`, char index in the document).
    map: Vec<(usize, usize)>,
}

impl DisplayText {
    /// Normalises breaks (CR, LF, CRLF → paragraph break `\n`; ETX → line
    /// separator) and applies All Caps.
    fn new(doc: &TextDocument, source: &str, start: usize) -> Self {
        let mut text = String::with_capacity(source.len());
        let mut map = Vec::with_capacity(source.len());
        let chars: Vec<char> = source.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let mut push = |ch: char, text: &mut String| {
                map.push((text.len(), start + i));
                text.push(ch);
            };
            match c {
                '\r' if chars.get(i + 1) == Some(&'\n') => {
                    push('\n', &mut text);
                    i += 1;
                    // The LF is part of the same break.
                }
                '\r' | '\n' => push('\n', &mut text),
                '\u{3}' => push('\u{2028}', &mut text),
                _ if doc.style_at(start + i).all_caps => {
                    for u in c.to_uppercase() {
                        push(u, &mut text);
                    }
                }
                _ => push(c, &mut text),
            }
            i += 1;
        }
        Self { text, map }
    }

    /// Document char index of the display byte offset `byte`.
    fn source_at(&self, byte: usize) -> usize {
        match self.map.binary_search_by_key(&byte, |(b, _)| *b) {
            Ok(i) => self.map[i].1,
            Err(0) => 0,
            Err(i) => self.map[i - 1].1,
        }
    }

    /// Document char range of a display byte range.
    fn source_range(&self, bytes: Range<usize>, doc_len: usize) -> Range<usize> {
        if bytes.is_empty() {
            let s = self.source_at(bytes.start);
            return s..s;
        }
        let start = self.source_at(bytes.start);
        // The first display char at or after the end belongs to the next source char.
        let end = match self.map.binary_search_by_key(&bytes.end, |(b, _)| *b) {
            Ok(i) => self.map[i].1,
            Err(i) if i < self.map.len() => self.map[i].1,
            Err(_) => doc_len,
        };
        start..end.max(start + 1).min(doc_len.max(start + 1))
    }
}

/// Representative upright, sideways, punctuation and combined glyphs for the
/// background warm-up of the compositor and the editor's font collections.
pub fn vertical_warmup_document() -> TextDocument {
    let mut doc = TextDocument::new("文、A12", "Arial", 16.);
    doc.line_orientation = bae_model::LineOrientation::Vertical;
    doc.edit_styles(3..5, |s| {
        s.baseline_direction = bae_model::BaselineDirection::TateChuYoko
    });
    doc
}

/// Lays out `doc` in layer space.
pub fn layout_text(fonts: &mut FontSystem, doc: &TextDocument) -> TextLayout {
    shape_text(fonts, doc, false)
}

pub(crate) fn shape_text(
    fonts: &mut FontSystem,
    doc: &TextDocument,
    individual_chars: bool,
) -> TextLayout {
    let mut out = TextLayout::empty(document_paint(&doc.style));
    out.orientation = doc.line_orientation;
    out.box_rect = box_rect(doc);
    out.font = fonts.resolve(&doc.style.font);
    if doc.text.is_empty() {
        return out;
    }
    let mut from = 0;
    let mut start = 0;
    for end in doc
        .text
        .char_indices()
        .filter_map(|(i, c)| match c {
            '\n' => Some(i + 1),
            '\r' if !doc.text[i + 1..].starts_with('\n') => Some(i + 1),
            _ => None,
        })
        .chain(std::iter::once(doc.text.len()))
    {
        let source = &doc.text[from..end];
        let display = DisplayText::new(doc, source, start);
        let count = source.chars().count();
        shape_paragraph(
            fonts,
            doc,
            &display,
            start..start + count,
            individual_chars,
            &mut out,
        );
        if out.overset {
            break;
        }
        start += count;
        from = end;
    }
    if out.vertical() {
        vertical::compose(&mut out, doc);
    }
    out
}

fn size(style: &TextStyle) -> f64 {
    style.font_size.clamp(0.1, 1296.)
        * if style.baseline_option == BaselineOption::Normal {
            1.
        } else {
            0.6
        }
}

fn spacing(style: &TextStyle) -> f64 {
    size(style)
        * (style.tracking / 1000.
            + if style.faux_bold {
                2. * FAUX_BOLD_OFFSET_EM
            } else {
                0.
            })
}

fn shape_paragraph(
    fonts: &mut FontSystem,
    doc: &TextDocument,
    display: &DisplayText,
    source: Range<usize>,
    individual_chars: bool,
    out: &mut TextLayout,
) {
    let doc_len = source.end;
    let p = doc.paragraph_at(source.start);
    let justification = Justification::from_token(&p.justification);
    let paragraph = out.box_rect.filter(|_| !out.vertical());
    let base = doc.style_at(source.start);
    let relevant = doc.runs.partition_point(|r| r.start <= source.start)
        ..doc.runs.partition_point(|r| r.start < source.end);
    let faces: Vec<_> = std::iter::once((source.start, base))
        .chain(doc.runs[relevant].iter().map(|r| (r.start, &r.style)))
        .map(|(start, style)| (start, style, fonts.resolve(&style.font)))
        .collect();
    let locale = fonts.locale().to_owned();
    let FontSystem { fcx, lcx, .. } = fonts;
    let mut builder = lcx.ranged_builder(fcx, &display.text, 1.0, false);
    if let Ok(lang) = fontique::Language::parse(&locale) {
        builder.push_default(StyleProperty::Locale(Some(lang)));
    }
    builder.push_default(StyleProperty::FontSize(size(base) as f32));
    for (i, (start, style, face)) in faces.iter().enumerate() {
        let byte = |index| {
            display
                .map
                .get(display.map.partition_point(|(_, c)| *c < index))
                .map_or(display.text.len(), |(b, _)| *b)
        };
        let range = byte(*start)
            ..faces
                .get(i + 1)
                .map_or(display.text.len(), |(s, _, _)| byte(*s));
        if let FontMatch::Found(face) = face {
            builder.push(
                StyleProperty::FontWeight(FontWeight::new(face.weight)),
                range.clone(),
            );
            builder.push(
                StyleProperty::FontStyle(if face.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                }),
                range.clone(),
            );
            builder.push(
                StyleProperty::FontWidth(FontWidth::from_ratio(face.width)),
                range.clone(),
            );
            let families = vec![
                FontFamilyName::Named(face.family.as_str().into()),
                FontFamilyName::Generic(GenericFamily::SansSerif),
            ];
            builder.push(
                StyleProperty::FontFamily(FontFamily::List(families.into())),
                range.clone(),
            );
        }
        builder.push(StyleProperty::FontSize(size(style) as f32), range.clone());
        builder.push(
            StyleProperty::LetterSpacing(spacing(style) as f32),
            range.clone(),
        );
        builder.push(
            StyleProperty::LineHeight(parley::LineHeight::FontSizeRelative(AUTO_LEADING as f32)),
            range.clone(),
        );
        let features = match (
            individual_chars || faces.len() > 1 || out.vertical(),
            style.small_caps,
        ) {
            (true, true) => Some(r#""liga" 0, "clig" 0, "dlig" 0, "smcp" 1"#),
            (true, false) => Some(r#""liga" 0, "clig" 0, "dlig" 0"#),
            (false, true) => Some(r#""smcp" 1"#),
            _ => None,
        };
        if out.vertical() {
            // Apply vertical substitutions only to upright spans. A combined
            // cell still shapes with the normal horizontal font features.
            let mut spans = display.text[range.clone()].char_indices().peekable();
            while let Some((start, c)) = spans.next() {
                let upright = vertical::vertical_forms(c);
                while spans
                    .next_if(|(_, ch)| vertical::vertical_forms(*ch) == upright)
                    .is_some()
                {}
                let end = spans.peek().map_or(range.len(), |(i, _)| *i);
                let span = range.start + start..range.start + end;
                if upright {
                    let tags = match features {
                        Some(f) => format!(r#"{f}, "vert" 1, "vrt2" 1"#),
                        None => r#""vert" 1, "vrt2" 1"#.into(),
                    };
                    builder.push(StyleProperty::FontFeatures(tags.as_str().into()), span);
                } else if let Some(f) = features {
                    builder.push(StyleProperty::FontFeatures(f.into()), span);
                }
            }
        } else if let Some(f) = features {
            builder.push(StyleProperty::FontFeatures(f.into()), range);
        }
    }
    let mut layout = builder.build(&display.text);
    layout.set_text_indent(p.first_line_indent as f32, Default::default());
    let box_width = paragraph.map(|r| (r.width() - p.left_indent - p.right_indent).max(1.) as f32);
    if let (Some(_), true) = (box_width, justification.is_justify()) {
        let last = match justification.simple() {
            Justification::Center => Alignment::Center,
            Justification::Right => Alignment::Right,
            _ if justification == Justification::JustifyAll => Alignment::Justify,
            _ => Alignment::Left,
        };
        layout.set_alignment(
            Alignment::Justify,
            AlignmentOptions {
                align_when_overflowing: false,
                last_line_alignment: Some(last),
            },
        );
    } else {
        layout.set_alignment(Alignment::Left, AlignmentOptions::default());
    }
    layout.break_all_lines(box_width);

    let mut word = out.chars.last().map_or(0, |c| c.word);
    let mut prev_breaks_word = true;
    for (local_line, line) in layout.lines().enumerate() {
        if local_line > 0 && line.text_range().is_empty() && display.text.ends_with('\n') {
            break;
        }
        let line_index = out.lines.len();
        let bytes = line.text_range();
        let line_style = |i| doc.style_at(display.source_at(i));
        let mut leading = 0.0_f64;
        for run in line.runs() {
            for cluster in run.clusters() {
                let style = line_style(cluster.text_range().start);
                leading = leading.max(
                    style
                        .leading
                        .filter(|n| *n > 0.)
                        .unwrap_or(AUTO_LEADING * style.font_size),
                );
            }
        }
        if leading == 0. {
            leading = base.leading.unwrap_or(AUTO_LEADING * base.font_size);
        }
        let letter_spacing = display.text[bytes.clone()]
            .trim_end()
            .char_indices()
            .next_back()
            .map_or(0., |(i, _)| spacing(line_style(bytes.start + i)));
        let m = line.metrics();
        let ascent = f64::from(m.baseline - m.content_block_min_coord).max(0.0);
        let descent = f64::from(m.content_block_max_coord - m.baseline).max(0.0);
        let baseline = match (line_index, paragraph) {
            (0, Some(b)) => b.y0 + ascent,
            (0, None) => 0.0,
            _ => out.lines[line_index - 1].baseline + leading,
        } + if local_line == 0 {
            p.space_before
                + if source.start > 0 {
                    doc.paragraph_at(source.start - 1).space_after
                } else {
                    0.
                }
        } else {
            0.
        };
        if let Some(b) = paragraph
            && line_index > 0
            && baseline + descent > b.y1 + 1e-6
        {
            out.overset = true;
            break;
        }
        // Width for alignment: trailing spaces hang, trailing tracking is excluded.
        let visible = f64::from(m.advance - m.hanging_advance);
        let has_glyphs = line.runs().any(|r| !r.is_empty());
        let width = if has_glyphs {
            (visible - letter_spacing).max(0.0)
        } else {
            0.0
        };
        let justified_line = paragraph.is_some() && justification.is_justify();
        let indent = if local_line == 0 {
            p.first_line_indent
        } else {
            0.
        };
        let x = if justified_line {
            paragraph.map_or(0.0, |b| b.x0) + p.left_indent + f64::from(m.offset)
        } else {
            let (start, avail) = match paragraph {
                Some(b) => (
                    b.x0 + p.left_indent + indent,
                    b.width() - p.left_indent - p.right_indent - indent,
                ),
                None => (p.left_indent + indent, 0.0),
            };
            match (justification.simple(), paragraph.is_some()) {
                (Justification::Center, true) => start + (avail - width) / 2.0,
                (Justification::Right, true) => start + avail - width,
                (Justification::Center, false) => start - width / 2.0,
                (Justification::Right, false) => start - width,
                _ => start,
            }
        };
        // parley positions are relative to its own line origin (offset included
        // when justifying); place them at (x, baseline).
        let mut pen = x;
        let line_chars_start = out.chars.len();
        for run in line.runs() {
            let synthesis = run.synthesis();
            let mut clusters = run.visual_clusters().peekable();
            while let Some(first) = clusters.peek() {
                let style = line_style(first.text_range().start);
                let paint = document_paint(style);
                let shift = style.baseline_shift
                    + match style.baseline_option {
                        BaselineOption::Normal => 0.,
                        BaselineOption::Superscript => style.font_size * 0.35,
                        BaselineOption::Subscript => -style.font_size * 0.2,
                    };
                let mut out_run = GlyphRun {
                    basis: Affine::IDENTITY,
                    paint,
                    font: run.font().clone(),
                    font_size: run.font_size(),
                    normalized_coords: run
                        .normalized_coords()
                        .iter()
                        .map(|c| c.to_bits())
                        .collect(),
                    embolden: ((f64::from(style.faux_bold) + f64::from(synthesis.embolden()))
                        * FAUX_BOLD_OFFSET_EM
                        * size(style)) as f32,
                    skew_degrees: (if style.faux_italic {
                        FAUX_ITALIC_DEGREES
                    } else {
                        0.
                    }) as f32
                        + synthesis.skew().unwrap_or(0.),
                    glyphs: Vec::new(),
                };
                while let Some(cluster) =
                    clusters.next_if(|c| std::ptr::eq(style, line_style(c.text_range().start)))
                {
                    let src = display.source_range(cluster.text_range(), doc_len);
                    let kind = if cluster.is_hard_line_break() || is_break(display, &cluster) {
                        CharKind::LineBreak
                    } else if cluster.is_space_or_nbsp()
                        || display.text[cluster.text_range()]
                            .chars()
                            .all(char::is_whitespace)
                    {
                        CharKind::Space
                    } else {
                        CharKind::Glyph
                    };
                    let starts_word = kind == CharKind::Glyph && prev_breaks_word;
                    if starts_word && !out.chars.is_empty() {
                        word += 1;
                    }
                    prev_breaks_word = kind != CharKind::Glyph;
                    let char_index = out.chars.len();
                    let origin = Point::new(pen, baseline - shift);
                    let mut gx = pen;
                    for g in cluster.glyphs() {
                        if kind == CharKind::Glyph {
                            out_run.glyphs.push(PositionedGlyph {
                                id: g.id,
                                x: (gx + f64::from(g.x)) as f32,
                                y: (baseline + f64::from(g.y) - shift) as f32,
                                char_index,
                            });
                        }
                        gx += f64::from(g.advance);
                    }
                    pen += f64::from(cluster.advance());
                    out.chars.push(TextChar {
                        right_to_left: run.is_rtl(),
                        combined_em: None,
                        paint,
                        source: src,
                        line: line_index,
                        word,
                        origin,
                        advance: f64::from(cluster.advance()),
                        kind,
                    });
                }
                if !out_run.glyphs.is_empty() {
                    out.runs.push(out_run);
                }
            }
        }
        out.lines.push(TextLine {
            baseline,
            x,
            width,
            ascent,
            descent,
            chars: line_chars_start..out.chars.len(),
        });
    }
}

/// Whether a cluster is a paragraph break or soft return in the display text.
fn is_break<B: parley::Brush>(display: &DisplayText, cluster: &parley::Cluster<'_, B>) -> bool {
    display.text[cluster.text_range()]
        .chars()
        .all(|c| matches!(c, '\n' | '\u{2028}'))
        && !cluster.text_range().is_empty()
}

/// The paragraph box in layer pixels, if the document is paragraph text.
pub fn box_rect(doc: &TextDocument) -> Option<Rect> {
    let [w, h] = doc.box_size?;
    let [x, y] = doc.box_position.unwrap_or([0.0, 0.0]);
    Some(Rect::new(x, y, x + w.max(1.0), y + h.max(1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> TextDocument {
        serde_json_like(text)
    }

    fn serde_json_like(text: &str) -> TextDocument {
        TextDocument::new(text, "ArialMT", 36.0)
    }

    #[test]
    fn display_text_normalises_breaks_and_caps() {
        let mut d = doc("ab\r\ncd\u{3}eß");
        d.style.all_caps = true;
        let t = DisplayText::new(&d, &d.text, 0);
        assert_eq!(t.text, "AB\nCD\u{2028}ESS");
        // "SS" both map back to "ß" (char 8).
        assert_eq!(t.source_at(t.text.len() - 1), 8);
        assert_eq!(t.source_at(2), 2, "CRLF break maps to the CR");
        assert_eq!(t.source_range(3..4, 9), 4..5, "C after CRLF is char 4");
    }

    #[test]
    fn justification_tokens() {
        assert_eq!(Justification::from_token("center"), Justification::Center);
        assert_eq!(
            Justification::from_token("justify_last_right").simple(),
            Justification::Right
        );
        assert!(Justification::from_token("justify_all").is_justify());
        assert_eq!(Justification::from_token("bogus"), Justification::Left);
    }

    #[test]
    fn paint_and_box() {
        let mut d = doc("x");
        d.style.stroke_color = Some([0.0, 0.0, 0.0, 1.0]);
        d.style.stroke_width = 3.0;
        let p = document_paint(&d.style);
        assert_eq!(p.stroke.map(|s| s.width), Some(3.0));
        d.style.stroke_width = 0.0;
        assert!(document_paint(&d.style).stroke.is_none());
        assert!(box_rect(&d).is_none());
        d.box_size = Some([200.0, 100.0]);
        d.box_position = Some([-100.0, -50.0]);
        assert_eq!(box_rect(&d), Some(Rect::new(-100.0, -50.0, 100.0, 50.0)));
    }

    #[test]
    fn empty_text_lays_out_nothing() {
        let mut fonts = FontSystem::without_system_fonts();
        let l = layout_text(&mut fonts, &doc(""));
        assert!(l.is_empty());
        assert!(l.bounds(true).is_none());
    }
}
