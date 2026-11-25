//! Wipes that read a second layer or a composition camera.
mod definitions;
use super::{image_grid, layer_input};
use crate::{
    EffectTarget, TargetEffect,
    gpu::{self, Resource, UniformWriter},
    params::Params,
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};
use glam::{DMat3, DVec2, DVec3, EulerRot};

pub(crate) use definitions::{card_wipe, cc_image_wipe};
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    CardWipe,
    CcImageWipe,
}
struct LayerWipe {
    def: EffectDef,
    mode: Mode,
}
const HEADER: usize = 240;
const BYTES: u32 = HEADER as u32 + 16 * 108;
const LABEL: &str = "bae.effects.layer-wipe";

fn kernel(
    rt: &gpu::Runtime,
    format: wgpu::TextureFormat,
) -> Result<std::sync::Arc<gpu::Kernel>, RenderError> {
    image_grid::kernel_with_inputs(rt, format, LABEL, include_str!("layer_wipe.wgsl"), BYTES, 3)
}
pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    kernel(rt, format)?;
    Ok(())
}

impl LayerWipe {
    fn layers(&self, p: &EffectParams<'_>) -> [Option<LayerDependency>; 2] {
        match self.mode {
            Mode::CardWipe => [
                layer_input::dependency(p, 6, true),
                (Params::new(&self.def, p).popup(20) == 9)
                    .then(|| layer_input::dependency(p, 22, true))
                    .flatten(),
            ],
            Mode::CcImageWipe => [None, layer_input::dependency(p, 5, true)],
        }
    }
}
impl Effect for LayerWipe {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        image_grid::layer_rect(p).union(&roi)
    }
    fn uses_scene(&self, p: &EffectParams<'_>) -> bool {
        let v = Params::new(&self.def, p);
        self.mode == Mode::CardWipe && (v.popup(28) == 3 || v.popup(64) == 3)
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        let mut deps: Vec<_> = self.layers(p).into_iter().flatten().collect();
        deps.dedup();
        deps
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

fn point(v: Params<'_>, id: u32, rect: IRect) -> DVec2 {
    if let Some(xy) = v.p.point(id) {
        return DVec2::from_array(xy) * v.p.scale;
    }
    let Some(Value::Vec2(xy)) = v.value(id) else {
        return DVec2::ZERO;
    };
    DVec2::new(
        xy[0] * f64::from(rect.width()) / 640.,
        xy[1] * f64::from(rect.height()) / 360.,
    ) + DVec2::new(rect.x0.into(), rect.y0.into())
}

// Map the unit square to four screen-space corners. The third coordinate is
// retained through inversion so texture coordinates stay projectively correct.
fn corners(q: [DVec2; 4]) -> DMat3 {
    let [a, b, c, d] = q;
    let residual = a - b - c + d;
    let m = glam::DMat2::from_cols(b - d, c - d);
    let g = if m.determinant().abs() < 1e-12 {
        DVec2::ZERO
    } else {
        m.inverse() * residual
    };
    DMat3::from_cols(
        (b - a + g.x * b).extend(g.x),
        (c - a + g.y * c).extend(g.y),
        a.extend(1.),
    )
}

fn inverse_projection(v: Params<'_>, rect: IRect) -> DMat3 {
    let size = DVec2::new(rect.width().into(), rect.height().into()).max(DVec2::ONE);
    let matrix = match v.popup(28) {
        2 => corners([44, 46, 48, 50].map(|id| point(v, id, rect))),
        3 if v.p.scene.is_some() => {
            let scale = v.p.scale;
            DMat3::from_diagonal(DVec3::new(scale, scale, 1.))
                * v.p.scene.unwrap().projection
                * DMat3::from_diagonal((size / scale).extend(1.))
        }
        _ => {
            let order = v.popup(42) - 1;
            let (axes, angles) = match order % 6 {
                1 => (EulerRot::XZY, [30, 34, 32]),
                2 => (EulerRot::YXZ, [32, 30, 34]),
                3 => (EulerRot::YZX, [32, 34, 30]),
                4 => (EulerRot::ZXY, [34, 30, 32]),
                5 => (EulerRot::ZYX, [34, 32, 30]),
                _ => (EulerRot::XYZ, [30, 32, 34]),
            };
            let [x, y, z] = angles.map(|id| v.f(id).to_radians());
            let rot = DMat3::from_euler(axes, x, y, z);
            let center = size * 0.5;
            let focal = size.x * v.f(40) / 35.;
            let pan = point(v, 36, rect) - center;
            let translate = pan.extend(focal * (v.f(38) / 2. - 1.0));
            let origin = if order < 6 {
                rot * (-center.extend(0.)) + translate
            } else {
                rot * (translate - center.extend(0.))
            };
            let projection = |axis: DVec3, w: f64| {
                let d = axis.z + focal * w;
                (axis.truncate() * focal + center * d).extend(d)
            };
            DMat3::from_cols(
                projection(rot.x_axis * size.x, 0.),
                projection(rot.y_axis * size.y, 0.),
                projection(origin, 1.),
            )
        }
    };
    if matrix.determinant().abs() < 1e-10 {
        DMat3::ZERO
    } else {
        matrix.inverse()
    }
}
impl TargetEffect for LayerWipe {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let grid = image_grid::grid_scale(p, input);
        let rect = if p.layer_bounds.is_empty() {
            image_grid::pixel_rect(input.rect, 1. / grid)
        } else {
            image_grid::layer_rect(p)
        };
        let deps = self.layers(p);
        let resolve =
            |d: &Option<LayerDependency>| d.as_ref().and_then(|d| target.effect_layer(d)).cloned();
        let [back, map] = deps.each_ref().map(resolve);
        let mut u = UniformWriter::with_size(BYTES as usize);
        for (i, img) in [
            input,
            back.as_ref().unwrap_or(input),
            map.as_ref().unwrap_or(input),
        ]
        .into_iter()
        .enumerate()
        {
            image_grid::image_coords(&mut u, i * 32, img, grid);
        }
        let v = Params::new(&self.def, p);
        u.i32s(96, &image_grid::rect4(roi))
            .i32s(112, &image_grid::rect4(rect))
            .f32s(
                128,
                &[
                    p.scale as f32,
                    p.time.as_seconds() as f32,
                    f32::from(p.float),
                    f32::from(self.mode == Mode::CardWipe),
                ],
            );
        if self.mode == Mode::CardWipe {
            let projection = inverse_projection(v, rect);
            for (i, col) in [projection.x_axis, projection.y_axis, projection.z_axis]
                .into_iter()
                .enumerate()
            {
                u.f32s(144 + i * 16, &col.as_vec3().to_array());
            }
            if let Some((pos, rgb)) = p.scene.and_then(|scene| scene.light) {
                u.f32s(192, &pos.map(|x| (x * p.scale) as f32))
                    .f32(204, 1.)
                    .f32s(208, &rgb);
            }
        }
        for param in &self.def.params {
            let id = param.id;
            if id >= 108 {
                continue;
            }
            let at = HEADER + id as usize * 16;
            match v.value(id) {
                Some(Value::Vec2(_)) => {
                    u.f32s(at, &point(v, id, rect).as_vec2().to_array());
                }
                Some(Value::Color(c)) => {
                    u.f32s(at, &c.map(|x| x as f32));
                }
                _ => {
                    u.f32(at, v.f(id) as f32);
                }
            }
        }
        u.u32(224, u32::from(back.is_some() || p.value(6).is_none()))
            .u32(
                228,
                u32::from(
                    map.is_some()
                        || p.value(if self.mode == Mode::CardWipe { 22 } else { 5 })
                            .is_none(),
                ),
            );
        let rt = gpu::runtime(target.gpu());
        let pipeline = kernel(&rt, target.format())?;
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let group = rt.bind_group(
            &pipeline,
            &[
                Resource::Uniform,
                Resource::View(&input.view),
                Resource::View(&back.as_ref().unwrap_or(input).view),
                Resource::View(&map.as_ref().unwrap_or(input).view),
                Resource::View(&out.view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(LABEL),
                timestamp_writes: None,
            });
        gpu::dispatch(
            &mut pass,
            &pipeline,
            &group,
            &[gpu::push_uniform(&rt, &u)],
            roi.size(),
        );
        Ok(out)
    }
}
