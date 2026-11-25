//! Wide blurs run their neighborhood passes on a globally aligned half-size grid.
use super::{Mode, Shader, image_grid};
use crate::{EffectTarget, params::Params};
use bae_render::{EffectParams, GpuImage, IRect, RenderError};

pub(super) const HALF_RADIUS: f64 = 16.;

fn half_rect(r: IRect) -> IRect {
    IRect::new(
        r.x0.div_euclid(2),
        r.y0.div_euclid(2),
        (r.x1 + 1).div_euclid(2),
        (r.y1 + 1).div_euclid(2),
    )
}

pub(super) fn render(
    target: &mut dyn EffectTarget,
    v: Params<'_>,
    mode: Mode,
    input: &GpuImage,
    map: Option<&GpuImage>,
    roi: IRect,
) -> Result<GpuImage, RenderError> {
    let radius = v.f(1) * v.p.scale;
    if roi.is_empty() || radius <= 0. {
        return Ok(input.clone());
    }
    let lens = mode == Mode::Lens;
    let shader = if lens {
        shader!("lens")
    } else {
        shader!("bilateral")
    };
    let grid = image_grid::grid_scale(v.p, input);
    let source = image_grid::pixel_rect(input.rect, 1. / grid);
    let prepared = if lens {
        shader.dispatch(target, v, source, input, [map], [0, 0])?
    } else {
        input.clone()
    };
    let wide = radius >= HALF_RADIUS;
    let low_params = EffectParams {
        scale: v.p.scale * 0.5,
        ..*v.p
    };
    let (working, stage, out) = if wide {
        (
            image_grid::half_level(target, &prepared, IRect::EMPTY, grid, !lens || v.b(21))?,
            Params::new(v.def, &low_params),
            half_rect(roi).inflate(1),
        )
    } else {
        (prepared.clone(), v, roi)
    };
    let smart = u32::from(mode == Mode::Smart);
    let filtered = if !lens {
        shader.dispatch(target, stage, out, &working, [], [smart, 0])?
    } else if map.is_none()
        && v.popup(3) == 4
        && v.f(4) == 0.
        && stage.f(1) * stage.p.scale * stage.f(5).max(1.) <= 256.
    {
        // A regular hexagon is the union of three equal parallelograms.
        // AB and AC share the first line integral along A; BC uses B.
        let margin = (stage.f(1) * stage.p.scale * stage.f(5).max(1.)).ceil() as i32 + 1;
        let needed = out.inflate(margin);
        let a = shader.dispatch(target, stage, needed, &working, [], [3, 0])?;
        let b = shader.dispatch(target, stage, needed, &working, [], [3, 1])?;
        shader.dispatch(target, stage, out, &working, [Some(&a), Some(&b)], [4, 0])?
    } else {
        shader.dispatch(
            target,
            stage,
            out,
            &working,
            [Some(input)],
            [if map.is_some() { 1 } else { 2 }, u32::from(wide)],
        )?
    };
    if !wide {
        return Ok(filtered);
    }
    if lens {
        return shader.dispatch(
            target,
            v,
            roi,
            input,
            [Some(&filtered), map.map(|_| &prepared)],
            [5, 0],
        );
    }
    shader.dispatch(
        target,
        v,
        roi,
        input,
        [Some(&filtered), Some(&working)],
        [smart, 1],
    )
}
