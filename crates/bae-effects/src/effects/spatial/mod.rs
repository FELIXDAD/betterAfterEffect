//! Spatial filters which need reduced intermediate textures, a derived input,
//! or access to the full source extent when rendering a tile.
pub(crate) mod choker;
mod definitions;
mod pass;

use pass::dispatch;
pub(crate) use pass::prewarm;

use crate::{
    Builtin, EffectTarget, TargetEffect,
    blur::{self, Edge},
    common,
    params::Params,
};
use bae_model::{EffectDef, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};

pub(crate) fn builtins() -> [Builtin; 6] {
    definitions::entries()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Directional,
    Radial,
    Mosaic,
    Unsharp,
    Glow,
    Shadow,
}

#[derive(Debug)]
struct Spatial {
    kind: Kind,
    def: EffectDef,
}

fn translated(r: IRect, d: [f64; 2]) -> IRect {
    IRect::new(
        r.x0 + d[0].floor() as i32,
        r.y0 + d[1].floor() as i32,
        r.x1 + d[0].ceil() as i32,
        r.y1 + d[1].ceil() as i32,
    )
}

fn offset(p: Params<'_>) -> [f64; 2] {
    let angle = p.f(3).to_radians();
    let d = p.f(4).clamp(0., 4000.) * p.p.scale;
    [angle.sin() * d, -angle.cos() * d].map(|x| (x + x.signum() * 1e-8).trunc())
}

fn color(p: Params<'_>, id: u32) -> [f32; 4] {
    match p.value(id) {
        Some(Value::Color(v)) => v.map(|c| c as f32),
        _ => [0.; 4],
    }
}

// Horizontal edge-alpha profiles measured at lengths 1, 2, 4, 8, 10, 15,
// 20 and 50 in AE 26: sigma 1.14, 1.38, 2.16, 3.53, 4.36, 6.26, 8.17, 19.58.
fn directional(p: Params<'_>) -> ([f64; 2], f64) {
    let length = p.f(2).clamp(0., 1000.);
    let sigma = if length < 1. {
        length * 1.14
    } else if length < 2. {
        1.14 + (length - 1.) * 0.24
    } else if length < 4. {
        1.38 + (length - 2.) * 0.39
    } else if length < 8. {
        2.16 + (length - 4.) * 0.343
    } else {
        3.53 + (length - 8.) * 0.383
    };
    let angle = p.f(1).to_radians();
    (
        [angle.sin(), -angle.cos()].map(|v| if v.abs() < 1e-8 { 0. } else { v }),
        sigma * p.p.scale,
    )
}

impl Spatial {
    fn sigma(&self, p: Params<'_>) -> [f64; 2] {
        let scale = p.p.scale;
        match self.kind {
            Kind::Directional => {
                let (axis, sigma) = directional(p);
                axis.map(|v| sigma * v.abs())
            }
            Kind::Unsharp => [(0.8 * p.f(2).clamp(0.100006, 500.) + 0.4) * scale; 2],
            Kind::Shadow => [0.265 * p.f(5).clamp(0., 30000.) * scale; 2],
            Kind::Glow => super::blur::dimensions(p.popup(14))
                .map(|d| d * 0.4 * p.f(3).clamp(0., 1000.) * scale),
            _ => [0.; 2],
        }
    }
}

impl Effect for Spatial {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn output_bounds(&self, params: &EffectParams<'_>, input: IRect) -> IRect {
        if input.is_empty() {
            return input;
        }
        let p = Params::new(&self.def, params);
        let [x, y] = self.sigma(p).map(|s| {
            if self.kind == Kind::Directional {
                (s * 3.).ceil() as i32
            } else {
                blur::radius(s)
            }
        });
        let spread = IRect::new(input.x0 - x, input.y0 - y, input.x1 + x, input.y1 + y);
        match self.kind {
            Kind::Shadow => {
                let shadow = translated(spread, offset(p));
                if p.b(6) { shadow } else { input.union(&shadow) }
            }
            Kind::Glow | Kind::Directional => spread,
            _ => input,
        }
    }
    fn input_roi(&self, params: &EffectParams<'_>, roi: IRect) -> IRect {
        if roi.is_empty() {
            return roi;
        }
        let p = Params::new(&self.def, params);
        match self.kind {
            // Both filters depend on the previous effect's full extent (which
            // may have grown beyond the layer). The renderer clips this request
            // against that upstream extent before allocating a texture.
            Kind::Radial | Kind::Mosaic => IRect::new(-1 << 26, -1 << 26, 1 << 26, 1 << 26),
            Kind::Directional => {
                let (axis, sigma) = directional(p);
                if axis.contains(&0.) && sigma >= 6. {
                    blur::plan_gaussian(IRect::EMPTY, self.sigma(p), Edge::Transparent, roi).read
                } else {
                    let [x, y] = axis.map(|v| (v.abs() * (3. * sigma).ceil()).ceil() as i32);
                    IRect::new(roi.x0 - x, roi.y0 - y, roi.x1 + x, roi.y1 + y)
                }
            }
            _ => {
                let sample = if self.kind == Kind::Shadow {
                    translated(roi, offset(p).map(|v| -v))
                } else {
                    roi
                };
                let plan =
                    blur::plan_gaussian(IRect::EMPTY, self.sigma(p), Edge::Transparent, sample);
                roi.union(&plan.read)
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

impl TargetEffect for Spatial {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        params: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let p = Params::new(&self.def, params);
        let out = roi.intersect(&self.output_bounds(params, input.rect));
        if out.is_empty() || (matches!(self.kind, Kind::Unsharp | Kind::Radial) && p.f(1) == 0.) {
            return Ok(input.clone());
        }
        let input = common::premultiplied(target, input)?;
        let float = params.float;
        let sigma = self.sigma(p);
        match self.kind {
            Kind::Directional => {
                let (axis, sd) = directional(p);
                if axis.contains(&0.) && sd >= 6. {
                    return blurred(target, &input, sigma, Edge::Transparent, out);
                }
                dispatch(
                    target,
                    "directional",
                    &input,
                    &input,
                    out,
                    &[[axis[0] as f32, axis[1] as f32, sd as f32, 0.]],
                    float,
                )
            }
            Kind::Radial => {
                let center = match params.value(2) {
                    Some(Value::Vec2(v)) => v.map(|x| (x * params.scale) as f32),
                    _ => {
                        let layer = params.layer_bounds;
                        if layer.is_empty() {
                            [
                                (input.rect.x0 + input.rect.x1) as f32 * 0.5,
                                (input.rect.y0 + input.rect.y1) as f32 * 0.5,
                            ]
                        } else {
                            [(layer.x0 + layer.x1) as f32, (layer.y0 + layer.y1) as f32]
                                .map(|c| c * 0.5 * params.scale as f32)
                        }
                    }
                };
                dispatch(
                    target,
                    "radial",
                    &input,
                    &input,
                    out,
                    &[
                        [
                            center[0],
                            center[1],
                            p.f(1).clamp(0., 1000.) as f32,
                            p.popup(3) as f32,
                        ],
                        [p.popup(4) as f32, p.f(6) as f32, 0., 0.],
                    ],
                    float,
                )
            }
            Kind::Mosaic => {
                let [w, h] = input.rect.size();
                let n = [(p.f(1) as u32).clamp(1, w), (p.f(2) as u32).clamp(1, h)];
                let grid = [[n[0] as f32, n[1] as f32, p.b(3) as u8 as f32, 0.]];
                if p.b(3) {
                    return dispatch(target, "mosaic_output", &input, &input, out, &grid, float);
                }
                let rows = dispatch(
                    target,
                    "mosaic_rows",
                    &input,
                    &input,
                    IRect::from_size(n[0], h),
                    &grid,
                    true,
                )?;
                let cells = dispatch(
                    target,
                    "mosaic_cells",
                    &rows,
                    &rows,
                    IRect::from_size(n[0], n[1]),
                    &grid,
                    true,
                )?;
                dispatch(target, "mosaic_output", &input, &cells, out, &grid, float)
            }
            Kind::Unsharp => {
                let soft = blurred(target, &input, sigma, Edge::Repeat, out)?;
                dispatch(
                    target,
                    "unsharp",
                    &input,
                    &soft,
                    out,
                    &[[p.f(1) as f32 / 100., p.f(3) as f32, p.b(4) as u8 as f32, 0.]],
                    float,
                )
            }
            Kind::Shadow => {
                let d = offset(p);
                let soft = blurred(
                    target,
                    &input,
                    sigma,
                    Edge::Transparent,
                    translated(out, d.map(|x| -x)),
                )?;
                dispatch(
                    target,
                    "shadow",
                    &input,
                    &soft,
                    out,
                    &[
                        [
                            d[0] as f32,
                            d[1] as f32,
                            p.f(2) as f32 / 255.,
                            p.b(6) as u8 as f32,
                        ],
                        color(p, 1),
                    ],
                    float,
                )
            }
            Kind::Glow => {
                let settings = [
                    [
                        p.popup(1) as f32,
                        p.f(2) as f32 / 255.,
                        p.f(4) as f32,
                        p.popup(5) as f32,
                    ],
                    [
                        p.popup(6) as f32,
                        p.popup(7) as f32,
                        p.popup(8) as f32,
                        p.f(9) as f32,
                    ],
                    [p.f(10) as f32 / 360., p.f(11) as f32, 0., 0.],
                    color(p, 12),
                    color(p, 13),
                ];
                let read = blur::plan_gaussian(input.rect, sigma, Edge::Transparent, out)
                    .read
                    .intersect(&input.rect);
                if read.is_empty() {
                    return Ok(input);
                }
                let bright = dispatch(
                    target,
                    "glow_extract",
                    &input,
                    &input,
                    read,
                    &settings,
                    true,
                )?;
                let soft = blurred(target, &bright, sigma, Edge::Transparent, out)?;
                dispatch(
                    target,
                    "glow_composite",
                    &input,
                    &soft,
                    out,
                    &settings,
                    float,
                )
            }
        }
    }
}

fn blurred(
    target: &mut dyn EffectTarget,
    input: &GpuImage,
    sigma: [f64; 2],
    edge: Edge,
    out: IRect,
) -> Result<GpuImage, RenderError> {
    if sigma.map(blur::radius) == [0, 0] {
        return Ok(input.clone());
    }
    blur::plan_gaussian(input.rect, sigma, edge, out).encode(target, input)
}

/// Simplified Chinese popup labels in AE menu order (disk values start at one).
pub fn options_zh(match_name: &str, param_id: u32) -> Option<&'static [&'static str]> {
    Some(match (match_name, param_id) {
        ("ADBE Box Blur2", 3) => &["水平和垂直", "水平", "垂直"],
        ("ADBE Radial Blur", 3) => &["旋转", "缩放"],
        ("ADBE Radial Blur", 4) => &["低", "高"],
        ("ADBE Glo2", 1) => &["Alpha 通道", "颜色通道"],
        ("ADBE Glo2", 5) => &["顶端", "后面", "无"],
        ("ADBE Glo2", 6) => &[
            "无",
            "正常",
            "相加",
            "相乘",
            "溶解",
            "滤色",
            "叠加",
            "柔光",
            "强光",
            "变暗",
            "变亮",
            "差值",
            "色相",
            "饱和度",
            "颜色",
            "发光度",
            "颜色减淡",
            "颜色加深",
            "排除",
            "模板 Alpha",
            "模板亮度",
            "轮廓 Alpha",
            "轮廓亮度",
            "冷光预乘",
            "Alpha 添加",
        ],
        ("ADBE Glo2", 7) => &["原始颜色", "A 和 B 颜色", "任意映射"],
        ("ADBE Glo2", 8) => &["锯齿 A>B", "锯齿 B>A", "三角形 A>B>A", "三角形 B>A>B"],
        ("ADBE Glo2", 14) => &["水平和垂直", "水平", "垂直"],
        _ => return None,
    })
}
