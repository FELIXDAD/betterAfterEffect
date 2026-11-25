//! Inverse-mapped, single-layer distortions. Coordinates refer to pixel edges
//! in layer space; only the output dispatch is restricted to the requested tile.
mod catalog;
pub(crate) mod warps;

use crate::{Builtin, EffectTarget, TargetEffect, common, gpu, params::Params, rust_builtin};
use bae_gpu::wgpu;
use bae_model::{EffectDef, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
pub(crate) use catalog::entries as builtins;
pub use catalog::options_zh;
use glam::{DMat3, DVec2, DVec3};
use gpu::{Binding, KernelKey, KernelSource, Resource, UniformWriter};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum Kind {
    Transform,
    TurbulentDisplace,
    WaveWarp,
    Bulge,
    Twirl,
    PolarCoordinates,
    CornerPin,
    Offset,
    Mirror,
    Spherize,
    Ripple,
    OpticsCompensation,
    PowerPin,
}

struct Distortion {
    kind: Kind,
    def: EffectDef,
}

// The same space is used for parameter defaults, the matrix and the shader.
fn layer(p: Params<'_>, input: IRect) -> (DVec2, DVec2) {
    let (r, s) = if p.p.layer_bounds.is_empty() {
        (input, 1.)
    } else {
        (p.p.layer_bounds, p.p.scale)
    };
    (
        DVec2::new(r.x0 as f64, r.y0 as f64) * s,
        DVec2::new(r.width() as f64, r.height() as f64) * s,
    )
}

fn point(p: Params<'_>, id: u32, origin: DVec2, size: DVec2) -> DVec2 {
    if let Some(Value::Vec2(v)) = p.p.value(id) {
        return DVec2::from_array(*v) * p.p.scale;
    }
    let Some(Value::Vec2(v)) = p.value(id) else {
        return origin;
    };
    origin + size * DVec2::from_array(*v) / DVec2::new(640., 360.)
}

fn square_to_quad([a, b, c, d]: [DVec2; 4]) -> DMat3 {
    let u = b - d;
    let v = c - d;
    let delta = a - b - c + d;
    let det = u.perp_dot(v);
    if det == 0. {
        return DMat3::ZERO;
    }
    let g = delta.perp_dot(v) / det;
    let h = u.perp_dot(delta) / det;
    DMat3::from_cols(
        (b - a + g * b).extend(g),
        (c - a + h * c).extend(h),
        a.extend(1.),
    )
}

impl Distortion {
    fn matrix(&self, p: Params<'_>, origin: DVec2, size: DVec2) -> DMat3 {
        let pt = |id| point(p, id, origin, size);
        if matches!(self.kind, Kind::CornerPin | Kind::PowerPin) {
            let power = self.kind == Kind::PowerPin;
            let first = if power { 2 } else { 1 };
            let pins = [first, first + 1, first + 2, first + 3].map(pt);
            let mut quad = square_to_quad(pins);
            if power {
                let low = DVec2::new(p.f(10), p.f(9)) * 0.01;
                let high = DVec2::new(p.f(11), p.f(12)) * 0.01;
                quad = quad
                    * DMat3::from_translation(-low)
                    * DMat3::from_scale(DVec2::ONE + low + high);
            }
            let map = quad * DMat3::from_scale(size.recip()) * DMat3::from_translation(-origin);
            return if power && p.b(7) && map.determinant() != 0. {
                map.inverse()
            } else {
                map
            };
        }
        let axis = p.f(6).to_radians();
        let shear = DMat3::from_cols(
            DVec3::new(1., p.f(5).to_radians().tan(), 0.),
            DVec3::Y,
            DVec3::Z,
        );
        let scale = DVec2::new(if p.b(11) { p.f(3) } else { p.f(4) }, p.f(3)) / 100.;
        DMat3::from_translation(pt(2))
            * DMat3::from_angle(-axis)
            * shear
            * DMat3::from_angle(axis + p.f(7).to_radians())
            * DMat3::from_scale(scale)
            * DMat3::from_translation(-pt(1))
    }
}

impl Effect for Distortion {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn output_bounds(&self, params: &EffectParams<'_>, input: IRect) -> IRect {
        if input.is_empty() {
            return input;
        }
        let p = Params::new(&self.def, params);
        let (origin, size) = layer(p, input);
        match self.kind {
            Kind::CornerPin | Kind::PowerPin | Kind::Transform => {
                let m = self.matrix(p, origin, size);
                if m.determinant() == 0. {
                    return IRect::EMPTY;
                }
                let mut lo = DVec2::splat(f64::INFINITY);
                let mut hi = -lo;
                for x in [input.x0, input.x1] {
                    for y in [input.y0, input.y1] {
                        let q = m * DVec3::new(x as f64, y as f64, 1.);
                        let xy = q.truncate() / q.z;
                        lo = lo.min(xy);
                        hi = hi.max(xy);
                    }
                }
                IRect::new(
                    lo.x.floor() as i32,
                    lo.y.floor() as i32,
                    hi.x.ceil() as i32,
                    hi.y.ceil() as i32,
                )
            }
            Kind::TurbulentDisplace if p.b(13) => {
                let d = (p.f(2).abs() * p.p.scale).ceil() as i32;
                IRect::new(input.x0 - d, input.y0 - d, input.x1 + d, input.y1 + d)
            }
            Kind::OpticsCompensation if p.b(2) && p.popup(6) > 1 && p.f(1) > 0. => {
                let phi = p.f(1).to_radians() * 0.5;
                let center = point(p, 4, origin, size);
                let reference = match p.popup(3) {
                    2 => size.y,
                    3 => size.length(),
                    _ => size.x,
                } * 0.5;
                let extent = (center - origin).abs().max((origin + size - center).abs());
                let tangent = phi.tan() * extent.length() / reference;
                let factor = 1. / (1. - tangent * tangent).max(f64::EPSILON).sqrt();
                let limit = match p.popup(6) {
                    2 => 2.,
                    3 => 4.,
                    _ => f64::INFINITY,
                };
                let spread = (size * (factor.clamp(1., limit) - 1.) * 0.5)
                    .min(DVec2::splat((1 << 26) as f64))
                    .ceil()
                    .as_ivec2();
                IRect::new(
                    input.x0.saturating_sub(spread.x),
                    input.y0.saturating_sub(spread.y),
                    input.x1.saturating_add(spread.x),
                    input.y1.saturating_add(spread.y),
                )
            }
            _ => input,
        }
    }
    fn input_roi(&self, _: &EffectParams<'_>, roi: IRect) -> IRect {
        // Wrapping and projective maps can reach the opposite side of the
        // previous effect. The render graph intersects this with its extent.
        if roi.is_empty() {
            roi
        } else {
            IRect::new(-1 << 26, -1 << 26, 1 << 26, 1 << 26)
        }
    }
    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, p, input, roi)
    }
}

const UNIFORM_SIZE: usize = 352;
// Indexed by Kind. Each entry compiles only the sampling functions it uses.
const COLORS: [&str; 13] = [
    "transform_color",
    "turbulent_color",
    "wave_color",
    "bulge_color",
    "twirl_color",
    "polar_color",
    "corner_color",
    "offset_color",
    "mirror_color",
    "spherize_color",
    "ripple_color",
    "optics_color",
    "corner_color",
];
fn pipeline(
    rt: &gpu::Runtime,
    format: wgpu::TextureFormat,
    color: &'static str,
) -> Result<Arc<gpu::Kernel>, RenderError> {
    let key = KernelKey {
        owner: "bae.effects/distort".into(),
        entry: color.into(),
        variant: gpu::format_key(format),
    };
    rt.kernel(&key, || {
        Ok(KernelSource {
            label: color,
            wgsl: include_str!("warp.wgsl")
                .replace("WORKING_FORMAT", crate::blur::format_name(format)?)
                .replace("SAMPLE_COLOR", color),
            entry: "warp",
            bindings: vec![
                (0, Binding::Uniform(UNIFORM_SIZE as u32)),
                (1, Binding::Texture),
                (2, Binding::Storage(format)),
            ],
            workgroup: [8, 8, 1],
        })
    })
}
pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for color in COLORS {
        pipeline(rt, format, color)?;
    }
    Ok(())
}

impl TargetEffect for Distortion {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        params: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let p = Params::new(&self.def, params);
        let (origin, size) = layer(p, input.rect);
        // A collapsed transform has zero coverage, including in standalone renders.
        let out = target.allocate(roi, input.pixel_scale)?;
        let input = common::premultiplied(target, input)?;
        let rt = gpu::runtime(target.gpu());
        let kernel = pipeline(&rt, target.format(), COLORS[self.kind as usize])?;
        let mut w = UniformWriter::with_size(UNIFORM_SIZE);
        w.i32s(
            0,
            &[input.rect.x0, input.rect.y0, input.rect.x1, input.rect.y1],
        )
        .i32s(16, &[roi.x0, roi.y0, roi.x1, roi.y1]);
        for (i, f) in [origin.x, origin.y, size.x, size.y].into_iter().enumerate() {
            w.f32(32 + 4 * i, f as f32);
        }
        w.f32(52, params.scale as f32)
            .f32(56, params.time.as_seconds() as f32)
            .u32(60, u32::from(params.float));
        if self.kind == Kind::Bulge {
            let rx = p.f(1) * params.scale;
            let ry = p.f(2) * params.scale;
            if rx > 0. && ry > 0. {
                // AE normalizes Y in X-radius units using double precision. At an
                // exact ellipse boundary, express that rounding error in f32 ulps.
                let aspect = (ry * rx.recip()).recip();
                w.f32(
                    48,
                    (aspect.mul_add(ry, -rx) / ry * (1u32 << 29) as f64) as f32,
                );
            }
        }
        w.f32(64, f32::from(self.kind == Kind::PowerPin));
        for param in &self.def.params {
            let at = 64 + 16 * param.id as usize;
            if matches!(param.default, Some(Value::Vec2(_))) {
                let v = point(p, param.id, origin, size);
                w.f32(at, v.x as f32).f32(at + 4, v.y as f32);
            } else {
                w.f32(at, p.f(param.id) as f32);
            }
        }
        if matches!(
            self.kind,
            Kind::Transform | Kind::CornerPin | Kind::PowerPin
        ) {
            let m = self.matrix(p, origin, size);
            let inv = if m.determinant() == 0. {
                DMat3::ZERO
            } else {
                m.inverse()
            };
            for (j, col) in inv.to_cols_array_2d().into_iter().enumerate() {
                for (i, c) in col.into_iter().enumerate() {
                    w.f32(304 + 16 * j + 4 * i, c as f32);
                }
            }
        }
        let offset = gpu::push_uniform(&rt, &w);
        let group = rt.bind_group(
            &kernel,
            &[
                Resource::Uniform,
                Resource::View(&input.view),
                Resource::View(&out.view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("bae.effects.distort"),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &kernel, &group, &[offset], roi.size());
        drop(pass);
        Ok(out)
    }
}
