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

use bae_model::TextDocument;
use bae_vector::paint::{LineJoin, Rgba, paint_color};
use kurbo::{Point, Rect};
use parley::{
    Alignment, AlignmentOptions, FontData, FontFamily, FontFamilyName, FontStyle, FontWeight,
    FontWidth, GenericFamily, PositionedLayoutItem, StyleProperty,
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
    /// Char indices (Unicode scalar values) into `TextDocument.text`.
    pub source: Range<usize>,
    /// Index into [`TextLayout::lines`].
    pub line: usize,
    /// Word index (words are separated by whitespace and line breaks).
    pub word: usize,
    /// Left end of the advance on the baseline, layer pixels.
    pub origin: Point,
    /// Advance in pixels (tracking excluded).
    pub advance: f64,
    pub kind: CharKind,
}

/// One composed line.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    /// Baseline y in layer pixels.
    pub baseline: f64,
    /// x of the line start after alignment.
    pub x: f64,
    /// Measured width (trailing spaces and trailing tracking excluded).
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

    /// Ink bounds of all glyphs in layer space (`sourceRectAtTime` with
    /// `includeExtents = false`, AE-TXT-007); `with_stroke` adds half the stroke
    /// width (times the miter limit for miter joins). `None` when nothing is drawn.
    pub fn bounds(&self, with_stroke: bool) -> Option<Rect> {
        let stroke = match (with_stroke, self.paint.stroke) {
            (true, Some(s)) => {
                let k = match s.join {
                    LineJoin::Miter => s.miter_limit.max(1.0),
                    _ => 1.0,
                };
                s.width.max(0.0) / 2.0 * k
            }
            _ => 0.0,
        };
        let mut out: Option<Rect> = None;
        for run in &self.runs {
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
                    x + f64::from(b.x_min) - lean * bottom.abs().max(0.0) - grow,
                    y - top - grow,
                    x + f64::from(b.x_max) + lean * top.max(0.0) + grow,
                    y - bottom + grow,
                );
                let r = r.inflate(stroke, stroke);
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
        for run in &self.runs {
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
            style.paint.fill.map(|c| c.map(f32::to_bits)).hash(&mut h);
            style
                .paint
                .stroke
                .map(|s| s.color.map(f32::to_bits))
                .hash(&mut h);
        }
        let p = &self.paint;
        p.fill.map(|c| c.map(f32::to_bits)).hash(&mut h);
        if let Some(s) = p.stroke {
            s.color.map(f32::to_bits).hash(&mut h);
            s.width.to_bits().hash(&mut h);
            s.join.hash(&mut h);
            s.miter_limit.to_bits().hash(&mut h);
        }
        p.stroke_over_fill.hash(&mut h);
        h.finish()
    }
}

/// The paint of a document (AE-TXT-015..017).
pub fn document_paint(doc: &TextDocument) -> TextPaint {
    let stroke = doc
        .stroke_color
        .filter(|_| doc.stroke_width > 0.0)
        .map(|c| TextStroke {
            color: paint_color(c),
            width: doc.stroke_width,
            join: match doc.typography.line_join {
                bae_model::TextJoin::Miter => LineJoin::Miter,
                bae_model::TextJoin::Bevel => LineJoin::Bevel,
                bae_model::TextJoin::Round => LineJoin::Round,
            },
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
    fn new(doc: &TextDocument) -> Self {
        let mut text = String::with_capacity(doc.text.len());
        let mut map = Vec::with_capacity(doc.text.len());
        let chars: Vec<char> = doc.text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let mut push = |ch: char, text: &mut String| {
                map.push((text.len(), i));
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
                '0'..='9' if doc.typography.hindi_digits => push(
                    char::from_u32(0x0660 + c as u32 - '0' as u32).unwrap(),
                    &mut text,
                ),
                _ if doc.all_caps => {
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

fn hanging_advance(display: &DisplayText, cluster: &parley::layout::Cluster<'_, ()>) -> f64 {
    let hang = |c: char| {
        matches!(
            c,
            ',' | '.'
                | ':'
                | ';'
                | '!'
                | '?'
                | '‘'
                | '’'
                | '“'
                | '”'
                | '«'
                | '»'
                | '\''
                | '"'
                | '('
                | ')'
                | '['
                | ']'
        )
    };
    if display.text[cluster.text_range()].chars().all(hang) {
        f64::from(cluster.advance())
    } else {
        0.0
    }
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
    let paint = document_paint(doc);
    let base_size = doc.font_size.clamp(0.1, 1296.0);
    let script_offset = if doc.typography.superscript {
        base_size * 0.35
    } else if doc.typography.subscript {
        -base_size * 0.2
    } else {
        0.0
    };
    let size = if script_offset != 0.0 {
        base_size * 0.6
    } else {
        base_size
    };
    let baseline_shift = doc.baseline_shift + script_offset;
    let font = fonts.resolve(&doc.font);
    let display = DisplayText::new(doc);
    let doc_len = doc.text.chars().count();
    if display.text.is_empty() {
        let mut out = TextLayout::empty(paint);
        out.font = font;
        out.box_rect = box_rect(doc);
        return out;
    }

    let tracking = doc.tracking / 1000.0 * size;
    let faux_bold = if doc.faux_bold {
        FAUX_BOLD_OFFSET_EM * size
    } else {
        0.0
    };
    // Faux bold widens every advance by both outline offsets.
    let letter_spacing = tracking + 2.0 * faux_bold;
    let justification = Justification::from_token(&doc.justification);
    let paragraph = box_rect(doc);

    let locale = fonts.locale().to_owned();
    let FontSystem { fcx, lcx, .. } = fonts;
    let mut builder = lcx.ranged_builder(fcx, &display.text, 1.0, false);
    let family_owned;
    let families: Vec<FontFamilyName<'_>> = match &font {
        FontMatch::Found(face) => {
            family_owned = face.family.clone();
            builder.push_default(StyleProperty::FontWeight(FontWeight::new(face.weight)));
            builder.push_default(StyleProperty::FontStyle(if face.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            }));
            builder.push_default(StyleProperty::FontWidth(FontWidth::from_ratio(face.width)));
            vec![
                FontFamilyName::Named(family_owned.as_str().into()),
                FontFamilyName::Generic(GenericFamily::SansSerif),
            ]
        }
        FontMatch::Missing => vec![FontFamilyName::Generic(GenericFamily::SansSerif)],
    };
    builder.push_default(StyleProperty::FontFamily(FontFamily::List(families.into())));
    builder.push_default(StyleProperty::FontSize(size as f32));
    builder.push_default(StyleProperty::LetterSpacing(letter_spacing as f32));
    builder.push_default(StyleProperty::LineHeight(
        parley::LineHeight::FontSizeRelative(AUTO_LEADING as f32),
    ));
    if let Ok(lang) = fontique::Language::parse(&locale) {
        builder.push_default(StyleProperty::Locale(Some(lang)));
    }
    // A standard ligature (e.g. "fi") cannot carry two different transforms.
    // Preserve required script shaping and the document's small-caps feature.
    let features = match (
        individual_chars || !doc.typography.ligatures,
        doc.small_caps,
    ) {
        (true, true) => Some(r#""liga" 0, "clig" 0, "dlig" 0, "smcp" 1"#),
        (true, false) => Some(r#""liga" 0, "clig" 0, "dlig" 0"#),
        (false, true) => Some(r#""smcp" 1"#),
        (false, false) => None,
    };
    if let Some(features) = features {
        builder.push_default(StyleProperty::FontFeatures(features.into()));
    }
    let mut layout = builder.build(&display.text);

    let box_width = paragraph.map(|r| r.width() as f32);
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
    if doc.typography.no_break {
        layout.break_all_lines(None);
    } else {
        layout.break_all_lines(box_width);
        if doc.typography.every_line
            && let Some(width) = box_width
            && layout.len() > 1
        {
            // Keep the same line count while distributing words across lines.
            let lines = layout.len();
            let mut low = 0.0;
            let mut high = width;
            for _ in 0..8 {
                let middle = (low + high) * 0.5;
                layout.break_all_lines(Some(middle));
                if layout.len() > lines {
                    low = middle
                } else {
                    high = middle
                }
            }
            let mut breaker = layout.break_lines();
            breaker.state_mut().set_layout_max_advance(width);
            breaker.state_mut().set_line_max_advance(high);
            while breaker.break_next().is_some() {
                breaker.set_prior_line_width(width);
                breaker.state_mut().set_line_max_advance(high);
            }
            breaker.finish();
        }
    }

    let leading = doc
        .leading
        .filter(|l| *l > 0.0)
        .unwrap_or(AUTO_LEADING * size);
    let skew = if doc.faux_italic {
        FAUX_ITALIC_DEGREES
    } else {
        0.0
    };

    let mut out = TextLayout {
        runs: Vec::new(),
        lines: Vec::new(),
        chars: Vec::new(),
        styles: Vec::new(),
        paint,
        box_rect: paragraph,
        font,
        overset: false,
    };
    let mut word = 0usize;
    let mut prev_breaks_word = true;
    let mut baseline = 0.0f64;
    let mut previous_ascent = 0.;
    let mut previous_descent = 0.;
    for (line_index, line) in layout.lines().enumerate() {
        let m = line.metrics();
        let ascent = f64::from(m.baseline - m.content_block_min_coord).max(0.0);
        let descent = f64::from(m.content_block_max_coord - m.baseline).max(0.0);
        baseline = match (line_index, paragraph) {
            (0, Some(b)) => b.y0 + ascent,
            (0, None) => 0.0,
            _ if doc.typography.top_leading => baseline + leading + ascent - previous_ascent,
            _ => baseline + leading + previous_descent - descent,
        };
        previous_ascent = ascent;
        previous_descent = descent;
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
        let mut left_hang = 0.;
        let mut right_hang = 0.;
        if doc.typography.hanging_punctuation {
            let mut clusters = line
                .runs()
                .flat_map(|r| r.visual_clusters())
                .filter(|c| !c.is_space_or_nbsp() && !c.is_hard_line_break());
            if let Some(first) = clusters.next() {
                left_hang = hanging_advance(&display, &first);
                right_hang = clusters
                    .last()
                    .as_ref()
                    .map_or(left_hang, |c| hanging_advance(&display, c));
            }
        }
        let width = (width - left_hang - right_hang).max(0.);
        let x = if justified_line {
            paragraph.map_or(0.0, |b| b.x0)
        } else {
            let (start, avail) = match paragraph {
                Some(b) => (b.x0, b.width()),
                None => (0.0, 0.0),
            };
            match (justification.simple(), paragraph.is_some()) {
                (Justification::Center, true) => start + (avail - width) / 2.0,
                (Justification::Right, true) => start + avail - width,
                (Justification::Center, false) => -width / 2.0,
                (Justification::Right, false) => -width,
                _ => start,
            }
        };
        // parley positions are relative to its own line origin (offset included
        // when justifying); place them at (x, baseline).
        let x_base = if justified_line {
            x
        } else {
            x - f64::from(m.offset) - left_hang
        };
        let line_chars_start = out.chars.len();
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let synthesis = run.synthesis();
            let mut out_run = GlyphRun {
                font: run.font().clone(),
                font_size: run.font_size(),
                normalized_coords: run
                    .normalized_coords()
                    .iter()
                    .map(|c| c.to_bits())
                    .collect(),
                embolden: (faux_bold
                    + if synthesis.embolden() {
                        FAUX_BOLD_OFFSET_EM * size
                    } else {
                        0.0
                    }) as f32,
                skew_degrees: (skew + f64::from(synthesis.skew().unwrap_or(0.0))) as f32,
                glyphs: Vec::new(),
            };
            let run_baseline = glyph_run.baseline();
            let mut positioned = glyph_run.positioned_glyphs();
            let mut pen = f64::from(glyph_run.offset());
            for cluster in run.visual_clusters() {
                let src = display.source_range(cluster.text_range(), doc_len);
                let kind = if cluster.is_hard_line_break() || is_break(&display, &cluster) {
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
                let glyph_count = cluster.glyphs().count();
                let mut origin_x = None;
                for _ in 0..glyph_count {
                    let Some(g) = positioned.next() else { break };
                    let gx = x_base + f64::from(g.x);
                    origin_x.get_or_insert(gx);
                    pen = f64::from(g.x + g.advance);
                    if kind == CharKind::Glyph {
                        out_run.glyphs.push(PositionedGlyph {
                            id: g.id,
                            x: gx as f32,
                            y: (baseline + f64::from(g.y - run_baseline) - baseline_shift) as f32,
                            char_index,
                        });
                    }
                }
                let origin =
                    Point::new(origin_x.unwrap_or(x_base + pen), baseline - baseline_shift);
                out.chars.push(TextChar {
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
        out.lines.push(TextLine {
            baseline,
            x,
            width,
            ascent,
            descent,
            chars: line_chars_start..out.chars.len(),
        });
    }
    out
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
        TextDocument {
            text: text.to_owned(),
            font: "ArialMT".to_owned(),
            font_size: 36.0,
            fill_color: [1.0, 1.0, 1.0, 1.0],
            stroke_color: None,
            stroke_width: 0.0,
            stroke_over_fill: false,
            justification: "left".to_owned(),
            tracking: 0.0,
            leading: None,
            baseline_shift: 0.0,
            faux_bold: false,
            faux_italic: false,
            all_caps: false,
            small_caps: false,
            typography: Default::default(),
            box_size: None,
            box_position: None,
        }
    }

    #[test]
    fn hindi_digits_preserve_source_indices_and_join_reaches_stroke() {
        let mut d = doc("A19");
        d.typography.hindi_digits = true;
        d.typography.line_join = bae_model::TextJoin::Round;
        d.stroke_color = Some([1.; 4]);
        d.stroke_width = 2.;
        let display = DisplayText::new(&d);
        assert_eq!(display.text, "A١٩");
        assert_eq!(display.source_at(3), 2);
        assert_eq!(document_paint(&d).stroke.unwrap().join, LineJoin::Round);
    }
    #[test]
    fn display_text_normalises_breaks_and_caps() {
        let mut d = doc("ab\r\ncd\u{3}eß");
        d.all_caps = true;
        let t = DisplayText::new(&d);
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
        d.stroke_color = Some([0.0, 0.0, 0.0, 1.0]);
        d.stroke_width = 3.0;
        let p = document_paint(&d);
        assert_eq!(p.stroke.map(|s| s.width), Some(3.0));
        d.stroke_width = 0.0;
        assert!(document_paint(&d).stroke.is_none());
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
