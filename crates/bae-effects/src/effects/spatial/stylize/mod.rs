//! Stylize and perspective filters with spatial sampling or intermediate images.
mod definitions;

use super::{blurred, color, dispatch};
use crate::{
    EffectTarget, TargetEffect,
    blur::{self, Edge},
    common,
    params::Params,
};
use bae_model::EffectDef;
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};

pub(in crate::effects::spatial) use definitions::options_zh;

pub(crate) use definitions::{radial_shadow_builtin, stylize_builtins};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Emboss,
    ColorEmboss,
    Cartoon,
    Roughen,
    Tile,
    RadialShadow,
}

#[derive(Debug)]
struct Stylize {
    kind: Kind,
    def: EffectDef,
}

fn rect_point(rect: IRect, fraction: f64) -> [f64; 2] {
    [
        f64::from(rect.x0) + f64::from(rect.width()) * fraction,
        f64::from(rect.y0) + f64::from(rect.height()) * fraction,
    ]
}

fn scaled(rect: IRect, pivot: [f64; 2], factors: [f64; 2]) -> IRect {
    if rect.is_empty() || factors.contains(&0.) {
        return IRect::EMPTY;
    }
    let [x, y] = pivot;
    let [sx, sy] = factors;
    IRect::new(
        ((f64::from(rect.x0) - x) * sx + x).floor() as i32,
        ((f64::from(rect.y0) - y) * sy + y).floor() as i32,
        ((f64::from(rect.x1) - x) * sx + x).ceil() as i32,
        ((f64::from(rect.y1) - y) * sy + y).ceil() as i32,
    )
}

fn layer(params: &EffectParams<'_>, fallback: IRect) -> IRect {
    if params.layer_bounds.is_empty() {
        fallback
    } else {
        scaled(params.layer_bounds, [0.; 2], [params.scale; 2])
    }
}

fn pos(p: Params<'_>, id: u32, fallback: [f64; 2]) -> [f64; 2] {
    p.p.point(id).map_or(fallback, |v| v.map(|x| x * p.p.scale))
}

fn relief(p: Params<'_>) -> [f32; 2] {
    let angle = p.f(1).to_radians();
    let distance = p.f(2).clamp(0., 1000.) * p.p.scale;
    [angle.sin(), -angle.cos()].map(|v| (v * distance) as f32)
}

fn cartoon_radius(p: Params<'_>) -> f64 {
    p.f(2).clamp(0.001, 40.) * p.p.scale
}

fn cartoon_width(p: Params<'_>) -> f64 {
    p.f(10).clamp(0.001, 20.) * p.p.scale
}

fn cartoon_margin(p: Params<'_>) -> i32 {
    cartoon_radius(p).ceil() as i32 + (2.5 * cartoon_width(p)).ceil() as i32 + 3
}

fn rough_border(p: Params<'_>) -> f64 {
    p.f(2).clamp(0., 500.) * p.p.scale
}

fn rough_sigma(p: Params<'_>) -> [f64; 2] {
    [0.4175 * rough_border(p); 2]
}

fn shadow_zoom(p: Params<'_>) -> f64 {
    1. / (1. - p.f(4).clamp(0., 100.) * 0.01).max(0.01)
}

fn shadow_sigma(p: Params<'_>) -> f64 {
    // Match the Gaussian spread measured at softness 1, 4, 10 and 30 in AE.
    let softness = p.f(5).clamp(0., 100.);
    let sigma = if softness <= 1. {
        0.76 * softness
    } else {
        0.50 + 0.265 * softness
    };
    sigma * p.p.scale
}

impl Effect for Stylize {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn output_bounds(&self, params: &EffectParams<'_>, input: IRect) -> IRect {
        let p = Params::new(&self.def, params);
        match self.kind {
            Kind::Tile => {
                let layer = layer(params, input);
                let scales = [p.f(4), p.f(5)].map(|v| v.clamp(0., 30000.) * 0.01);
                scaled(layer, rect_point(layer, 0.5), scales)
            }
            Kind::Roughen if matches!(p.popup(1), 1 | 2 | 4) => input
                .inflate(rough_border(p).ceil() as i32)
                .intersect(&layer(params, input)),
            Kind::RadialShadow if p.b(9) => {
                let light = pos(p, 3, rect_point(layer(params, input), 0.25));
                let shadow = scaled(input, light, [shadow_zoom(p); 2])
                    .inflate(blur::radius(shadow_sigma(p)) + 1);
                if p.b(8) { shadow } else { input.union(&shadow) }
            }
            _ => input,
        }
    }

    fn input_roi(&self, params: &EffectParams<'_>, roi: IRect) -> IRect {
        let p = Params::new(&self.def, params);
        match self.kind {
            Kind::Tile | Kind::RadialShadow => IRect::new(-1 << 26, -1 << 26, 1 << 26, 1 << 26),
            Kind::Emboss | Kind::ColorEmboss => {
                let [x, y] = relief(p).map(|v| v.abs().ceil() as i32);
                IRect::new(roi.x0 - x, roi.y0 - y, roi.x1 + x, roi.y1 + y)
            }
            Kind::Cartoon => roi.inflate(cartoon_margin(p)),
            Kind::Roughen => {
                blur::plan_gaussian(IRect::EMPTY, rough_sigma(p), Edge::Transparent, roi)
                    .read
                    .union(&roi)
            }
        }
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        out: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, params, input, out)
    }
}

impl TargetEffect for Stylize {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        params: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let p = Params::new(&self.def, params);
        let out = roi.intersect(&self.output_bounds(params, input.rect));
        if out.is_empty() {
            return Ok(input.clone());
        }
        let input = common::premultiplied(target, input)?;
        match self.kind {
            Kind::Emboss | Kind::ColorEmboss => {
                let [x, y] = relief(p);
                dispatch(
                    target,
                    "emboss",
                    &input,
                    &input,
                    out,
                    &[
                        [x, y, p.f(3) as f32 * 0.01, p.f(4) as f32 * 0.01],
                        [
                            if self.kind == Kind::ColorEmboss {
                                1.
                            } else {
                                0.
                            },
                            0.,
                            0.,
                            0.,
                        ],
                    ],
                    params.float,
                )
            }
            Kind::Cartoon => cartoon(target, &input, out, p),
            Kind::Roughen => roughen(target, &input, out, p),
            Kind::Tile => motion_tile(target, &input, out, p),
            Kind::RadialShadow => radial_shadow(target, &input, out, p),
        }
    }
}

fn motion_tile(
    target: &mut dyn EffectTarget,
    input: &GpuImage,
    out: IRect,
    p: Params<'_>,
) -> Result<GpuImage, RenderError> {
    let layer = layer(p.p, input.rect);
    let [x, y] = pos(p, 1, rect_point(layer, 0.5)).map(|v| v as f32);
    let [w, h] = layer.size().map(|v| v as f32);
    let [tile_x, tile_y] = [p.f(2), p.f(3)].map(|v| (v.clamp(0., 100.) * 0.01) as f32);
    let [cx, cy] = rect_point(layer, 0.5).map(|v| v as f32);
    let [hw, hh] = [w * p.f(4) as f32, h * p.f(5) as f32].map(|v| v * 0.005);

    // A zero-sized tile axis uses the mean of the source along that axis.
    let mut sampled = input.clone();
    let mut width = w;
    let mut height = h;
    if tile_x == 0. {
        let r = sampled.rect;
        sampled = dispatch(
            target,
            "tile_collapse",
            &sampled,
            &sampled,
            IRect::new(layer.x0, r.y0, layer.x0 + 1, r.y1),
            &[[1., layer.x0 as f32, w, 0.]],
            true,
        )?;
        width = 1.;
    }
    if tile_y == 0. {
        let r = sampled.rect;
        sampled = dispatch(
            target,
            "tile_collapse",
            &sampled,
            &sampled,
            IRect::new(r.x0, layer.y0, r.x1, layer.y0 + 1),
            &[[0., layer.y0 as f32, h, 0.]],
            true,
        )?;
        height = 1.;
    }
    dispatch(
        target,
        "motion_tile",
        &sampled,
        &sampled,
        out,
        &[
            [x, y, tile_x * w, tile_y * h],
            [layer.x0 as f32, layer.y0 as f32, width, height],
            [
                (p.f(7) / 360.).rem_euclid(1.) as f32,
                flag(p.b(6)),
                flag(p.b(8)),
                0.,
            ],
            [cx - hw, cy - hh, cx + hw, cy + hh],
            [tile_x, tile_y, 0., 0.],
        ],
        p.p.float,
    )
}

fn flag(value: bool) -> f32 {
    f32::from(u8::from(value))
}

fn cartoon(
    target: &mut dyn EffectTarget,
    input: &GpuImage,
    out: IRect,
    p: Params<'_>,
) -> Result<GpuImage, RenderError> {
    let width = cartoon_width(p) as f32;
    let radius = cartoon_radius(p) as f32;
    let mode = p.popup(1);
    let field = out
        .inflate((2.5 * width).ceil() as i32 + 2)
        .intersect(&input.rect);
    let soft = if radius > 0.5 {
        // Two 1D passes keep large detail radii bounded in cost.
        let threshold = (p.f(3) * 0.0015) as f32;
        let wide = dispatch(
            target,
            "cartoon_smooth",
            input,
            input,
            field.inflate(radius.ceil() as i32).intersect(&input.rect),
            &[[1., 0., radius, threshold]],
            true,
        )?;
        dispatch(
            target,
            "cartoon_smooth",
            input,
            &wide,
            field,
            &[[0., 1., radius, threshold]],
            true,
        )?
    } else {
        input.clone()
    };
    dispatch(
        target,
        "cartoon_output",
        input,
        &soft,
        out,
        &[
            [
                mode as f32,
                p.f(5) as f32,
                (p.f(6) * 0.01) as f32,
                p.f(9) as f32,
            ],
            [
                width,
                (p.f(11) * 0.01) as f32,
                (p.f(12) * 0.01) as f32,
                (p.f(15) * 0.01) as f32,
            ],
            [p.f(16) as f32, p.f(17) as f32, 0., 0.],
        ],
        p.p.float,
    )
}

fn roughen(
    target: &mut dyn EffectTarget,
    input: &GpuImage,
    out: IRect,
    p: Params<'_>,
) -> Result<GpuImage, RenderError> {
    let border = rough_border(p);
    if border == 0. {
        return Ok(input.clone());
    }
    let soft = blurred(target, input, rough_sigma(p), Edge::Transparent, out)?;
    let point = pos(p, 7, [0.; 2]).map(|v| v as f32);
    let stretch = 2.0_f64.powf(p.f(6).clamp(-100., 100.) / 25.);
    let space = (p.f(5).clamp(10., 1000.) * 0.08 * p.p.scale) as f32;
    let evolution = p.f(9) / 360.;
    let cycle = if p.b(12) {
        p.f(13).clamp(1., 88.).round()
    } else {
        0.
    };
    let evolution = if cycle > 0. {
        evolution.rem_euclid(cycle)
    } else {
        evolution
    };
    dispatch(
        target,
        "rough_output",
        input,
        &soft,
        out,
        &[
            [
                border as f32,
                p.f(3) as f32,
                p.f(4) as f32,
                p.popup(1) as f32,
            ],
            [
                point[0],
                point[1],
                space * stretch.max(1.) as f32,
                space / stretch.min(1.) as f32,
            ],
            [
                p.f(8) as f32,
                evolution as f32,
                cycle as f32,
                p.f(14) as f32,
            ],
            color(p, 10),
        ],
        p.p.float,
    )
}

fn radial_shadow(
    target: &mut dyn EffectTarget,
    input: &GpuImage,
    out: IRect,
    p: Params<'_>,
) -> Result<GpuImage, RenderError> {
    let [x, y] = pos(p, 3, rect_point(layer(p.p, input.rect), 0.25));
    let sigma = shadow_sigma(p);
    let projected = dispatch(
        target,
        "radial_shadow_project",
        input,
        input,
        blur::plan_gaussian(IRect::EMPTY, [sigma; 2], Edge::Transparent, out).read,
        &[
            [x as f32, y as f32, shadow_zoom(p) as f32, p.f(2) as f32],
            color(p, 1),
            [if p.popup(6) == 2 { p.f(7) as f32 } else { 0. }, 0., 0., 0.],
        ],
        true,
    )?;
    let soft = blurred(target, &projected, [sigma; 2], Edge::Transparent, out)?;
    if p.b(8) {
        return Ok(soft);
    }
    dispatch(
        target,
        "radial_shadow_over",
        input,
        &soft,
        out,
        &[],
        p.p.float,
    )
}
