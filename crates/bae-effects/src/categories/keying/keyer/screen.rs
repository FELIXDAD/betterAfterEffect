//! Screen-matte extraction, spatial cleanup and final colour on the render grid.
use super::{Shader, filter, image_grid, mask};
use crate::effects::morphology::{self, Extremum};
use crate::{EffectTarget, blur, params::Params};
use bae_render::{GpuImage, IRect, RenderError};

fn sigma(v: Params<'_>, id: u32) -> f64 {
    v.f(id) * v.p.scale * 0.5
}

fn gaussian_read(roi: IRect, sigma: f64) -> IRect {
    blur::plan_gaussian(roi, [sigma; 2], blur::Edge::Repeat, roi).read
}

// Repeat, reflect and wrap can sample the cropped interior from an exterior tile.
fn crop_read(v: Params<'_>, roi: IRect, layer: IRect) -> IRect {
    if layer.is_empty() {
        return roi;
    }
    let axis = |min: i32, max: i32, origin: i32, size: u32, id: u32, mode: u32| {
        let pos = |id| f64::from(origin) + f64::from(size) * v.f(id) * 0.01;
        let a = pos(id).floor() as i32;
        let b = pos(id + 1).ceil() as i32;
        if v.popup(mode) > 1 && (min < a || max > b) {
            (min.min(a), max.max(b))
        } else {
            (min, max)
        }
    };
    let (x0, x1) = axis(roi.x0, roi.x1, layer.x0, layer.width(), 75, 71);
    let (y0, y1) = axis(roi.y0, roi.y1, layer.y0, layer.height(), 77, 72);
    IRect::new(x0, y0, x1, y1)
}

struct Matte {
    sample: IRect,
    extrema_out: IRect,
    raw: IRect,
    operations: [Extremum; 5],
}

impl Matte {
    fn new(v: Params<'_>, roi: IRect, domain: IRect) -> Self {
        let bounded = |r: IRect| {
            if domain.is_empty() {
                r
            } else {
                r.intersect(&domain)
            }
        };
        let sample = bounded(crop_read(v, roi, domain));
        let px = |id| (v.f(id) * v.p.scale).round() as i32;
        let grow = px(15);
        let black = px(17);
        let white = px(18);
        let operations = [
            if grow < 0 {
                Extremum::Min(-grow)
            } else {
                Extremum::Max(grow)
            },
            Extremum::Max(black),
            Extremum::Min(black),
            Extremum::Min(white),
            Extremum::Max(white),
        ];
        let extrema_out = bounded(gaussian_read(sample, sigma(v, 16)));
        let edge_width = ((v.f(54) + v.f(55).abs()) * v.p.scale).ceil() as i32;
        let raw = bounded(
            extrema_out
                .inflate(operations.iter().map(Extremum::radius).sum())
                .union(&gaussian_read(sample, sigma(v, 14)))
                .union(&sample.inflate(edge_width)),
        );
        Self {
            sample,
            extrema_out,
            raw,
            operations,
        }
    }
}

pub(super) fn read(v: Params<'_>, roi: IRect) -> IRect {
    let layer = image_grid::layer_rect(v.p);
    if v.popup(2) <= 2 {
        return crop_read(v, roi, layer);
    }
    let matte = Matte::new(v, roi, layer);
    gaussian_read(matte.raw, sigma(v, 10)).union(&roi)
}

fn gaussian(
    target: &mut dyn EffectTarget,
    image: &GpuImage,
    sigma: f64,
    out: IRect,
) -> Result<GpuImage, RenderError> {
    if blur::radius(sigma) == 0 {
        return Ok(image.clone());
    }
    let bounds = image_grid::pixel_rect(image.rect, 1. / image.pixel_scale);
    filter(
        target,
        image,
        blur::plan_gaussian(bounds, [sigma; 2], blur::Edge::Repeat, out),
    )
}

// Source layers and masks may arrive at a different texel density.
fn preblur(
    target: &mut dyn EffectTarget,
    v: Params<'_>,
    image: &GpuImage,
    amount: f64,
    roi: IRect,
) -> Result<GpuImage, RenderError> {
    if blur::radius(amount) == 0 {
        return Ok(image.clone());
    }
    let grid = image_grid::grid_scale(v.p, image);
    let bounds = image_grid::pixel_rect(image.rect, 1. / grid);
    let read = gaussian_read(roi, amount).intersect(&bounds);
    if read.is_empty() {
        return Ok(image.clone());
    }
    let sampled = shader!("keylight").dispatch(target, v, read, image, [None; 3], [2, 0])?;
    gaussian(target, &sampled, amount, roi)
}

pub(super) fn encode(
    target: &mut dyn EffectTarget,
    v: Params<'_>,
    input: &GpuImage,
    roi: IRect,
) -> Result<GpuImage, RenderError> {
    let shader = shader!("keylight");
    if v.popup(2) <= 2 {
        return shader.dispatch(target, v, roi, input, [None; 3], [0, 0]);
    }
    let layer = image_grid::layer_rect(v.p);
    let domain = if layer.is_empty() {
        image_grid::pixel_rect(input.rect, 1. / input.pixel_scale)
    } else {
        layer
    };
    let matte = Matte::new(v, roi, domain);
    if matte.raw.is_empty() {
        return shader.dispatch(target, v, roi, input, [None; 3], [0, 0]);
    }
    let mut masks = [None, None];
    for (i, id) in [23, 31].into_iter().enumerate() {
        if let Some(image) = mask(v.p, id).and_then(|m| target.effect_mask(&m)).cloned() {
            masks[i] = Some(preblur(target, v, &image, sigma(v, id + 1), matte.raw)?);
        }
    }
    let source = preblur(target, v, input, sigma(v, 10), matte.raw)?;
    let raw = shader.dispatch(
        target,
        v,
        matte.raw,
        &source,
        [masks[0].as_ref(), masks[1].as_ref(), None],
        [1, 0],
    )?;
    let extrema =
        morphology::filter_alpha(target, v.p, &raw, matte.extrema_out, &matte.operations)?;
    let screen = gaussian(target, &extrema, sigma(v, 16), matte.sample)?;
    let rollback = if v.f(14) > 0. {
        Some(gaussian(target, &raw, sigma(v, 14), matte.sample)?)
    } else {
        None
    };
    shader.dispatch(
        target,
        v,
        roi,
        input,
        [Some(&raw), Some(&screen), rollback.as_ref()],
        [0, 0],
    )
}
