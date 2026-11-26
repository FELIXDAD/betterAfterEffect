//! AE's Paint effect (`ADBE Paint`): the Brush, Clone Stamp and Eraser strokes
//! painted in the Layer panel. The comp conversion hands over the strokes
//! visible at the frame (`bae_render::paint`); each is a row of brush dabs
//! along its path, composited with the shared dab compositor, newest on top.

use bae_model::Value;
use bae_render::paint::{self, Erase, Stroke, Tool};
use bae_render::{EffectParams, GpuImage, IRect, RenderError};
use glam::DVec2;

use super::Generator;
use super::paint::{CLONE, Mark, Paint, RESTORE};
use crate::{Builtin, EffectTarget, catalog::DefBuilder};

/// The compositor's styles: paint over the layer, paint only, erase.
const OVER: u32 = 1;
const ONLY: u32 = 0;
const ERASE: u32 = 5;
/// Each stroke's dabs do not build up (AE: Opacity is the stroke's ceiling).
const MAX: u32 = 1;

pub(crate) fn paint_effect() -> Builtin {
    // AE: Paint on Transparent, then the Strokes group (ADBE Paint Group),
    // which holds the strokes; here they are `PaintStroke` prims, not a
    // parameter group (an accepted AE difference).
    let def = DefBuilder::new(paint::PAINT_EFFECT, "Paint", "Generate")
        .checkbox(1, "Paint on Transparent", "在透明背景上绘画", false)
        .ae_match_name("ADBE Paint On Transparent");
    Generator::Paint.builtin(def, "paint", "绘画")
}

/// Dab centres every `spacing` along the polyline (its first point always).
fn dabs(points: &[DVec2], spacing: f64) -> Vec<DVec2> {
    let Some(&first) = points.first() else {
        return Vec::new();
    };
    let mut out = vec![first];
    let mut left = spacing;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        let mut at = left;
        while at <= len {
            out.push(a.lerp(b, at / len));
            at += spacing;
        }
        left = at - len;
    }
    out
}

fn marks(s: &Stroke) -> Vec<Mark> {
    let rgb = [s.color[0], s.color[1], s.color[2]];
    let step = (s.spacing * s.diameter).max(0.5);
    dabs(&s.points, step)
        .into_iter()
        .map(|c| Mark::dab(c, s.diameter / 2.0, rgb, s.hardness, s.flow))
        .collect()
}

/// The layer with the strokes `p` carries painted on it (or on transparent).
pub(super) fn render(
    ctx: &mut dyn EffectTarget,
    p: &EffectParams<'_>,
    on_transparent: bool,
    input: &GpuImage,
    roi: IRect,
) -> Result<GpuImage, RenderError> {
    let strokes = p
        .effect
        .param(paint::STROKES_PARAM)
        .map(paint::unpack)
        .unwrap_or_default();
    let mut image = input.clone();
    if on_transparent {
        image = Paint::new(ONLY, MAX, 0.0).render(ctx, p, &image, roi)?;
    }
    // The layer before any paint (Eraser: Paint Only) and the image before
    // the last Brush or Clone stroke (Eraser: Last Stroke Only).
    let source = image.clone();
    let mut before_last: Option<GpuImage> = None;
    for s in strokes
        .iter()
        .filter(|s| s.diameter > 0.0 && !s.points.is_empty())
    {
        let mut paint = match (s.tool, s.erase) {
            (Tool::Brush, _) => Paint::new(OVER, MAX, s.opacity),
            // The Clone Stamp shows the image at its offset through its dabs
            // (from this layer; another source layer and Source Time Shift
            // are not drawn yet).
            (Tool::Clone, _) => {
                let mut paint = Paint::new(CLONE, MAX, s.opacity);
                paint.offset = s.clone_offset;
                paint
            }
            (Tool::Eraser, Erase::LayerAndPaint) => Paint::new(ERASE, MAX, s.opacity),
            (Tool::Eraser, Erase::PaintOnly) => {
                let mut paint = Paint::new(RESTORE, MAX, s.opacity);
                paint.restore = Some(source.clone());
                paint
            }
            (Tool::Eraser, Erase::LastStroke) => {
                let mut paint = Paint::new(RESTORE, MAX, s.opacity);
                paint.restore = Some(before_last.clone().unwrap_or_else(|| image.clone()));
                paint
            }
        };
        if s.tool != Tool::Eraser {
            before_last = Some(image.clone());
        }
        paint.marks = marks(s);
        image = paint.render(ctx, p, &image, roi)?;
    }
    Ok(image)
}

/// Paint on Transparent.
pub(super) fn on_transparent(v: crate::params::Params<'_>) -> bool {
    matches!(v.value(1), Some(Value::Bool(true)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dabs_follow_the_path_at_the_spacing() {
        let d = dabs(
            &[DVec2::ZERO, DVec2::new(10.0, 0.0), DVec2::new(10.0, 5.0)],
            4.0,
        );
        assert_eq!(
            d,
            vec![
                DVec2::ZERO,
                DVec2::new(4.0, 0.0),
                DVec2::new(8.0, 0.0),
                DVec2::new(10.0, 2.0),
            ]
        );
    }
}
