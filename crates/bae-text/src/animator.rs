//! Apply sampled AE animator groups after shaping and line breaking (AE-TXT-053..072).

use bae_anim::EvaluatedPrim;
use bae_model::{PrimType, Value};
use bae_vector::paint::{Rgba, paint_color};
use kurbo::{Affine, Vec2};

use crate::{CharKind, TextLayout, TextPaint};

/// Glyph-space adjustment in layer coordinates, and paint after all animator groups.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterStyle {
    pub transform: Affine,
    pub paint: TextPaint,
    /// Applied to the composited character, including its stroke.
    pub opacity: f32,
}

struct Adjustment {
    position: Vec2,
    scale: Vec2,
    rotation: f64,
    tracking: f64,
    opacity: f64,
    fill: Option<Rgba>,
}

fn token<'a>(p: &'a EvaluatedPrim, attr: &str, default: &'a str) -> &'a str {
    p.get(attr).and_then(Value::as_str).unwrap_or(default)
}

fn vec2(p: &EvaluatedPrim, attr: &str, default: f64) -> Vec2 {
    match p.get(attr) {
        Some(Value::Vec3([x, y, _])) => Vec2::new(*x, *y),
        _ => Vec2::new(default, default),
    }
}

/// Accumulate one square/add selector in source order. Tracking and rendering later
/// walk visual order, which can differ within bidirectional lines.
fn select(layout: &TextLayout, order: &[usize], s: &EvaluatedPrim, weights: &mut [f64]) {
    if token(s, "selector_type", "range") != "range"
        || token(s, "mode", "add") != "add"
        || token(s, "shape", "square") != "square"
    {
        return;
    }
    let based_on = token(s, "based_on", "characters");
    let mut count: usize = 0;
    let mut in_word = false;
    let mut units = Vec::with_capacity(order.len());
    for &index in order {
        let c = &layout.chars[index];
        let unit = match (based_on, c.kind) {
            (_, CharKind::LineBreak) => None,
            ("characters_excluding_spaces", CharKind::Space) => None,
            ("lines", _) => Some(c.line),
            ("words", kind) => {
                if kind == CharKind::Glyph && !in_word {
                    count += 1;
                }
                count.checked_sub(1)
            }
            _ => {
                count += 1;
                Some(count - 1)
            }
        };
        in_word = c.kind == CharKind::Glyph;
        units.push(unit);
    }
    if based_on == "lines" {
        count = layout.lines.len();
    }
    let index_units = token(s, "units", "percentage") == "index";
    let factor = if index_units {
        1.0
    } else {
        count as f64 / 100.0
    };
    let offset = s.f64("offset").unwrap_or(0.0);
    let a = (s.f64("start").unwrap_or(0.0) + offset) * factor;
    let b = (s.f64("end").unwrap_or(100.0) + offset) * factor;
    let amount = s.f64("amount").unwrap_or(100.0) / 100.0;
    let smooth = (s.f64("smoothness").unwrap_or(100.0) / 100.0).clamp(0.0, 1.0);
    for (&index, unit) in order.iter().zip(units) {
        if let Some(unit) = unit {
            let i = unit as f64;
            let coverage = ((i + 1.0).min(a.max(b)) - i.max(a.min(b))).clamp(0.0, 1.0);
            let weight = if smooth == 0.0 {
                f64::from(coverage >= 0.5)
            } else {
                ((coverage - (1.0 - smooth) / 2.0) / smooth).clamp(0.0, 1.0)
            };
            weights[index] = (weights[index] + weight * amount).clamp(-1.0, 1.0);
        }
    }
}

/// Shape a document and apply the evaluated animator groups for this frame.
pub fn layout_animated(
    fonts: &mut crate::FontSystem,
    doc: &bae_model::TextDocument,
    groups: &[EvaluatedPrim],
) -> TextLayout {
    let individual_chars = groups.iter().any(|g| {
        g.children.iter().any(|s| {
            s.prim_type == Some(PrimType::TEXT_SELECTOR)
                && matches!(
                    token(s, "based_on", "characters"),
                    "characters" | "characters_excluding_spaces"
                )
        })
    });
    let mut layout = crate::layout::shape_text(fonts, doc, individual_chars);
    apply_animators(&mut layout, groups);
    layout
}

/// Replaces a layout's per-character styles using the evaluated groups at one time.
/// Original cluster positions remain available for editing and for sampling another frame.
pub fn apply_animators(layout: &mut TextLayout, groups: &[EvaluatedPrim]) {
    layout.styles.clear();
    if groups.is_empty() || layout.chars.is_empty() {
        return;
    }
    let mut order: Vec<_> = (0..layout.chars.len()).collect();
    order.sort_by_key(|&i| layout.chars[i].source.start);
    let mut adjustments: Vec<_> = layout
        .chars
        .iter()
        .map(|c| Adjustment {
            position: Vec2::ZERO,
            scale: Vec2::new(1.0, 1.0),
            rotation: 0.0,
            tracking: 0.0,
            opacity: 1.0,
            fill: c.paint.fill,
        })
        .collect();
    for group in groups
        .iter()
        .filter(|p| p.prim_type == Some(PrimType::TEXT_ANIMATOR))
    {
        let mut selectors = group
            .children
            .iter()
            .filter(|p| p.prim_type == Some(PrimType::TEXT_SELECTOR))
            .peekable();
        let mut weights = vec![f64::from(selectors.peek().is_none()); order.len()];
        for s in selectors {
            select(layout, &order, s, &mut weights);
        }
        let position = vec2(group, "animator:position", 0.0);
        let scale = vec2(group, "animator:scale", 100.0) / 100.0;
        let rotation = group.f64("animator:rotation").unwrap_or(0.0).to_radians();
        let tracking = group.f64("animator:tracking").unwrap_or(0.0);
        let opacity = group.f64("animator:opacity").map(|v| v / 100.0);
        let color = match group.get("animator:fill_color") {
            Some(Value::Color(c)) => Some(paint_color(*c)),
            _ => None,
        };
        for (adj, w) in adjustments.iter_mut().zip(weights) {
            adj.position += position * w;
            adj.scale.x *= 1.0 + (scale.x - 1.0) * w;
            adj.scale.y *= 1.0 + (scale.y - 1.0) * w;
            adj.rotation += rotation * w;
            adj.tracking += tracking * w;
            if let Some(o) = opacity {
                adj.opacity += (o - adj.opacity) * w.max(0.0);
            }
            if let (Some(fill), Some(color)) = (&mut adj.fill, color) {
                for (channel, target) in fill.iter_mut().zip(color) {
                    *channel += (target - *channel) * w.max(0.0) as f32;
                }
            }
        }
    }
    let mut tracking = 0.0;
    let mut previous = 0.0;
    let mut line = None;
    for (c, adj) in layout.chars.iter().zip(adjustments) {
        if line == Some(c.line) {
            tracking += (previous + adj.tracking) / 2.0;
        } else {
            tracking = 0.0;
            line = Some(c.line);
        }
        previous = adj.tracking;
        let pivot = c.origin.to_vec2() + Vec2::new(c.advance * 0.5, 0.0);
        let transform = Affine::translate(adj.position + Vec2::new(tracking, 0.0) + pivot)
            * Affine::rotate(adj.rotation)
            * Affine::scale_non_uniform(adj.scale.x, adj.scale.y)
            * Affine::translate(-pivot);
        let mut paint = c.paint;
        paint.fill = adj.fill;
        let opacity = adj.opacity.clamp(0.0, 1.0) as f32;
        layout.styles.push(CharacterStyle {
            transform,
            paint,
            opacity,
        });
    }
}
