//! Vertical-rl composition. Parley supplies font fallback, styled glyph clusters
//! and hard breaks. Columns wrap at Unicode line-break opportunities, using the
//! vertical advances of the shaped glyphs and an indivisible cell for tate-chu-yoko.
use super::*;
use bae_model::BaselineDirection;
use icu_properties::{CodePointMapData, props::VerticalOrientation};
use icu_segmenter::LineSegmenter;
use skrifa::raw::TableProvider;

pub(super) fn vertical_forms(c: char) -> bool {
    orientation(c) != VerticalOrientation::Rotated
}

fn orientation(c: char) -> VerticalOrientation {
    CodePointMapData::<VerticalOrientation>::new().get(c)
}

#[derive(Clone, Copy)]
struct Metrics {
    rotate: bool,
    word_char: bool,
    combine: bool,
    em: f64,
    ascent: f64,
    advance: f64,
    origin_y: f64,
    spacing: f64,
    leading: f64,
}

struct Cell {
    chars: Range<usize>,
    advance: f64,
    em: f64,
    leading: f64,
    trailing: f64,
    break_before: bool,
    word_char: bool,
}

pub(super) fn compose(layout: &mut TextLayout, doc: &TextDocument) {
    let mut bytes: Vec<_> = doc.text.char_indices().map(|(b, _)| b).collect();
    bytes.push(doc.text.len());
    let break_points: Vec<_> = LineSegmenter::new_for_non_complex_scripts(Default::default())
        .segment_str(&doc.text)
        .collect();
    let first = |c: &TextChar| {
        doc.text[bytes[c.source.start]..]
            .chars()
            .next()
            .unwrap_or(' ')
    };
    let metrics = cluster_metrics(layout, doc, first);
    let mut placements = vec![(Affine::IDENTITY, Vec2::ZERO); layout.chars.len()];
    let mut visible = 0;
    let mut paragraph_start = 0;
    for hard_line in std::mem::take(&mut layout.lines) {
        let range = hard_line.chars.clone();
        let start = layout
            .chars
            .get(range.start)
            .map_or(bytes.len() - 1, |c| c.source.start);
        let paragraph = doc.paragraph_at(start);
        let starts_paragraph = start == paragraph_start;
        let cells = make_cells(layout, range.clone(), &metrics, &bytes, &break_points);
        let mut from = 0;
        loop {
            let first_column = starts_paragraph && from == 0;
            let indent = paragraph.left_indent
                + if first_column {
                    paragraph.first_line_indent
                } else {
                    0.
                };
            let available = layout
                .box_rect
                .map(|r| (r.height() - indent - paragraph.right_indent).max(1.));
            let end = from + column_end(&cells[from..], available);
            let group = &cells[from..end];
            let default = doc.style_at(start);
            let em = group
                .iter()
                .map(|c| c.em)
                .reduce(f64::max)
                .unwrap_or_else(|| size(default));
            let leading = group
                .iter()
                .map(|c| c.leading)
                .reduce(f64::max)
                .unwrap_or_else(|| default.leading.unwrap_or(AUTO_LEADING * default.font_size));
            let gap = if first_column {
                paragraph.space_before
                    + if start > 0 {
                        doc.paragraph_at(start - 1).space_after
                    } else {
                        0.
                    }
            } else {
                0.
            };
            let column = layout.lines.last().map_or_else(
                || layout.box_rect.map_or(0., |r| r.x1 - em / 2.),
                |l| l.baseline - leading,
            ) - gap;
            if layout
                .box_rect
                .is_some_and(|r| column - em / 2. < r.x0 - 1e-6)
                && !layout.lines.is_empty()
            {
                layout.overset = true;
                break;
            }
            let ink_end = group
                .iter()
                .rposition(|c| layout.chars[c.chars.start].kind == CharKind::Glyph);
            let width = ink_end.map_or(0., |k| {
                group[..=k].iter().map(|c| c.advance).sum::<f64>() - group[k].trailing
            });
            let alignment = Justification::from_token(&paragraph.justification);
            let justify = available.is_some()
                && alignment.is_justify()
                && (end < cells.len() || alignment == Justification::JustifyAll);
            let slack = available.unwrap_or(0.) - width;
            let extra = if justify {
                slack.max(0.) / ink_end.unwrap_or(0).max(1) as f64
            } else {
                0.
            };
            let mut y = layout.box_rect.map_or(0., |r| r.y0)
                + indent
                + if justify {
                    0.
                } else {
                    match alignment.simple() {
                        Justification::Center => slack / 2.,
                        Justification::Right => slack,
                        _ => 0.,
                    }
                };
            let top = y;
            let char_start = group.first().map_or(range.start, |c| c.chars.start);
            for cell in group {
                place_cell(
                    &mut layout.chars,
                    cell,
                    &metrics,
                    &mut placements,
                    hard_line.baseline,
                    layout.lines.len(),
                    Point::new(column, y),
                );
                y += cell.advance + extra;
            }
            visible = group.last().map_or(char_start, |c| c.chars.end);
            layout.lines.push(TextLine {
                baseline: column,
                x: top,
                width: if justify {
                    width + extra * ink_end.unwrap_or(0) as f64
                } else {
                    width
                },
                ascent: em / 2.,
                descent: em / 2.,
                chars: char_start..visible,
            });
            if end == cells.len() {
                break;
            }
            from = end;
        }
        if layout.overset {
            break;
        }
        if let Some(last) = range.last().map(|i| &layout.chars[i])
            && matches!(first(last), '\r' | '\n')
        {
            paragraph_start = last.source.end;
        }
    }
    layout.chars.truncate(visible);
    place_glyphs(layout, &placements);
}

fn cluster_metrics(
    layout: &TextLayout,
    doc: &TextDocument,
    first: impl Fn(&TextChar) -> char,
) -> Vec<Metrics> {
    let mut metrics: Vec<_> = layout
        .chars
        .iter()
        .map(|c| {
            let style = doc.style_at(c.source.start);
            let vo = orientation(first(c));
            let sideways = matches!(
                vo,
                VerticalOrientation::Rotated | VerticalOrientation::TransformedRotated
            );
            let em = size(style);
            Metrics {
                word_char: first(c).is_alphanumeric(),
                rotate: sideways && style.baseline_direction == BaselineDirection::Default,
                combine: vo == VerticalOrientation::Rotated
                    && c.kind != CharKind::LineBreak
                    && style.baseline_direction == BaselineDirection::TateChuYoko,
                em,
                ascent: em * 0.8,
                origin_y: em * 0.8,
                advance: em,
                spacing: spacing(style),
                leading: style
                    .leading
                    .filter(|v| *v > 0.)
                    .unwrap_or(AUTO_LEADING * style.font_size),
            }
        })
        .collect();
    for run in &layout.runs {
        let Ok(font) = skrifa::FontRef::from_index(run.font.data.as_ref(), run.font.index) else {
            continue;
        };
        let coords: Vec<_> = run
            .normalized_coords
            .iter()
            .map(|&v| NormalizedCoord::from_bits(v))
            .collect();
        let location = LocationRef::new(&coords);
        let size = Size::new(run.font_size);
        let fm = font.metrics(size, location);
        let units = f64::from(run.font_size) / f64::from(fm.units_per_em.max(1));
        let gm = font.glyph_metrics(size, location);
        let cmap = font.charmap();
        let vorg = font.vorg().ok();
        let vmtx = font.vmtx().ok();
        for group in run.glyphs.chunk_by(|a, b| a.char_index == b.char_index) {
            let g = group[0];
            let id = GlyphId::new(g.id);
            let m = &mut metrics[g.char_index];
            let ch = first(&layout.chars[g.char_index]);
            if orientation(ch) == VerticalOrientation::TransformedRotated
                && cmap.map(ch) != Some(id)
            {
                m.rotate = false;
            }
            m.ascent = f64::from(fm.ascent);
            m.origin_y = vorg
                .as_ref()
                .map(|v| f64::from(v.vertical_origin_y(id)) * units)
                .or_else(|| {
                    Some(
                        f64::from(gm.bounds(id)?.y_max)
                            + f64::from(vmtx.as_ref()?.side_bearing(id)?) * units,
                    )
                })
                .unwrap_or((m.em + f64::from(fm.ascent + fm.descent)) * 0.5);
            if let Some(advance) = vmtx.as_ref().and_then(|v| v.advance(id)) {
                m.advance = f64::from(advance) * units;
            }
        }
    }
    metrics
}

fn make_cells(
    layout: &TextLayout,
    range: Range<usize>,
    metrics: &[Metrics],
    bytes: &[usize],
    break_points: &[usize],
) -> Vec<Cell> {
    let mut cells = Vec::new();
    let mut i = range.start;
    while i < range.end {
        let begin = i;
        i += 1;
        if metrics[begin].combine {
            while i < range.end && metrics[i].combine {
                i += 1;
            }
        }
        let c = &layout.chars[begin];
        let group = &metrics[begin..i];
        let em = group.iter().map(|m| m.em).fold(0., f64::max);
        let leading = group.iter().map(|m| m.leading).fold(0., f64::max);
        let m = metrics[begin];
        let trailing = if c.kind == CharKind::LineBreak {
            0.
        } else {
            metrics[i - 1].spacing
        };
        let advance = if c.kind == CharKind::LineBreak {
            0.
        } else if m.combine {
            em + trailing
        } else if m.rotate {
            c.advance
        } else {
            m.advance + trailing
        };
        cells.push(Cell {
            chars: begin..i,
            advance,
            em,
            leading,
            trailing,
            word_char: m.word_char,
            break_before: break_points.binary_search(&bytes[c.source.start]).is_ok(),
        });
    }
    cells
}

fn column_end(cells: &[Cell], available: Option<f64>) -> usize {
    let Some(height) = available else {
        return cells.len();
    };
    let mut width = 0.;
    let mut last_break = None;
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 && cell.break_before {
            last_break = Some(i);
        }
        width += cell.advance;
        if width - cell.trailing > height && i > 0 {
            if let Some(at) = last_break {
                return at;
            }
            // Break an overlong word at a cluster boundary. Keep
            // upright punctuation attached according to Unicode line breaking.
            if cell.word_char && cells[i - 1].word_char {
                return i;
            }
        }
    }
    cells.len()
}

fn place_glyphs(layout: &mut TextLayout, placements: &[(Affine, Vec2)]) {
    for mut run in std::mem::take(&mut layout.runs) {
        let mut glyphs = std::mem::take(&mut run.glyphs);
        glyphs.retain(|g| g.char_index < layout.chars.len());
        for batch in
            glyphs.chunk_by_mut(|a, b| placements[a.char_index].0 == placements[b.char_index].0)
        {
            let mut placed = run.clone();
            placed.basis = placements[batch[0].char_index].0;
            for glyph in batch.iter_mut() {
                let (basis, offset) = placements[glyph.char_index];
                let p = basis * Point::new(f64::from(glyph.x), f64::from(glyph.y)) + offset;
                glyph.x = p.x as f32;
                glyph.y = p.y as f32;
            }
            placed.glyphs.extend_from_slice(batch);
            layout.runs.push(placed);
        }
    }
}

fn place_cell(
    chars: &mut [TextChar],
    cell: &Cell,
    metrics: &[Metrics],
    placements: &mut [(Affine, Vec2)],
    old_baseline: f64,
    line: usize,
    position: Point,
) {
    let Point { x: column, y } = position;
    let combined = metrics[cell.chars.start].combine;
    let natural: f64 = chars[cell.chars.clone()]
        .iter()
        .map(|c| c.advance)
        .sum::<f64>()
        - cell.trailing;
    let scale = if combined && natural > cell.em {
        cell.em / natural
    } else {
        1.
    };
    let mut left = column - natural * scale / 2.;
    for i in cell.chars.clone() {
        let c = &mut chars[i];
        let m = metrics[i];
        let old = c.origin;
        let shift = old_baseline - old.y;
        let center = column + shift;
        let ink_width = (c.advance - m.spacing).max(0.);
        let (basis, origin) = if combined {
            let origin = Point::new(left + shift, y + (cell.em - m.em) / 2. + m.origin_y);
            c.origin = Point::new(left + shift, y + cell.em / 2.);
            c.advance *= scale;
            left += c.advance;
            c.combined_em = Some(cell.em);
            (Affine::scale_non_uniform(scale, 1.), origin)
        } else {
            c.origin = Point::new(center, y);
            c.advance = cell.advance;
            if m.rotate {
                (
                    Affine::new([0., 1., -1., 0., 0., 0.]),
                    Point::new(center - m.ascent + m.em / 2., y),
                )
            } else {
                (
                    Affine::IDENTITY,
                    Point::new(center - ink_width / 2., y + m.origin_y),
                )
            }
        };
        c.line = line;
        placements[i] = (basis, origin - basis * old);
    }
}
