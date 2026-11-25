//! RGB guided filtering of the matte. Both local regressions and the averaging
//! of their coefficients use the same separable box kernel.
use super::{Kind, Shader, filter, image_grid};
use crate::{EffectTarget, blur, params::Params};
use bae_render::{GpuImage, IRect, RenderError};

pub(super) fn radius(kind: Kind, v: Params<'_>) -> f64 {
    let mut r = match kind {
        Kind::Cleaner => v.f(6),
        Kind::Soft => v.f(36) + v.f(27) + if v.b(24) { v.f(25) } else { 0. },
        Kind::Hard => v.f(24),
        Kind::Keylight => unreachable!(),
    };
    if kind != Kind::Cleaner && v.b(11) {
        r += v.f(2);
    }
    r * v.p.scale
}

fn mean(
    target: &mut dyn EffectTarget,
    image: &GpuImage,
    r: f64,
    out: IRect,
) -> Result<GpuImage, RenderError> {
    let bounds = image_grid::pixel_rect(image.rect, 1. / image.pixel_scale);
    filter(
        target,
        image,
        blur::plan_box(bounds, [r; 2], 1, blur::Edge::Repeat, out),
    )
}

pub(super) fn encode(
    target: &mut dyn EffectTarget,
    kind: Kind,
    v: Params<'_>,
    input: &GpuImage,
    previous: Option<&GpuImage>,
    roi: IRect,
) -> Result<GpuImage, RenderError> {
    let mode = match kind {
        Kind::Cleaner => 0,
        Kind::Soft => 1,
        Kind::Hard => 2,
        Kind::Keylight => unreachable!(),
    };
    let shader = shader!("refine");
    let radius = radius(kind, v);
    if radius <= 0. {
        return shader.dispatch(target, v, roi, input, [previous, None, None], [mode, 7]);
    }
    let grid = image_grid::grid_scale(v.p, input);
    let bounds = image_grid::pixel_rect(input.rect, 1. / grid);
    let halo = radius.ceil() as i32;
    let coeff_roi = roi.inflate(halo).intersect(&bounds);
    if coeff_roi.is_empty() {
        return shader.dispatch(target, v, roi, input, [previous, None, None], [mode, 7]);
    }
    let stats_roi = coeff_roi.inflate(halo).intersect(&bounds);
    let mut moments = |index| {
        let field = shader.dispatch(target, v, stats_roi, input, [None; 3], [mode, index])?;
        mean(target, &field, radius, coeff_roi)
    };
    let colour = moments(0)?;
    let squares = moments(1)?;
    let products = moments(2)?;
    let colour_alpha = moments(3)?;
    let coeff = shader.dispatch(
        target,
        v,
        coeff_roi,
        &colour,
        [Some(&squares), Some(&products), Some(&colour_alpha)],
        [mode, 4],
    )?;
    let averaged = mean(target, &coeff, radius, roi)?;
    // Key Cleaner only needs the edge statistics when a previous frame is available.
    let context = if kind != Kind::Cleaner || previous.is_some() {
        Some(shader.dispatch(
            target,
            v,
            roi,
            &colour,
            [Some(&squares), None, Some(&colour_alpha)],
            [mode, 5],
        )?)
    } else {
        None
    };
    shader.dispatch(
        target,
        v,
        roi,
        input,
        [previous, Some(&averaged), context.as_ref()],
        [mode, 6],
    )
}
