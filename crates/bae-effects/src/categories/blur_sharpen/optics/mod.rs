pub(crate) mod definitions;
use crate::effects::{
    blur::{BLURRINESS_TO_SIGMA, dimensions},
    image_grid, layer_input,
    sampled::Shader,
    spatial::dispatch,
};
use crate::{Builtin, EffectTarget, TargetEffect, params::Params, rust_builtin};
use crate::{blur, common};
use bae_model::EffectDef;
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Lens,
    Bilateral,
    Smart,
    Channel,
    Radial,
    Fast,
    Vector,
    Cross,
}
struct Optics {
    mode: Mode,
    def: EffectDef,
}
macro_rules! shader {
    ($name:literal) => {
        Shader {
            label: concat!("bae.effects.optics.", $name),
            source: concat!(
                include_str!("../../../effects/sampled.wgsl"),
                include_str!(concat!($name, ".wgsl"))
            ),
        }
    };
}
pub(crate) fn prewarm(
    rt: &crate::gpu::Runtime,
    format: bae_gpu::wgpu::TextureFormat,
) -> Result<(), RenderError> {
    for shader in [
        shader!("lens"),
        shader!("bilateral"),
        shader!("channels"),
        shader!("vector"),
        shader!("cross"),
    ] {
        shader.kernel(rt, format)?;
    }
    Ok(())
}

pub(crate) fn builtins() -> [Builtin; 8] {
    [
        (
            Mode::Lens,
            definitions::lens(),
            "camera-lens-blur",
            "摄像机镜头模糊",
        ),
        (
            Mode::Bilateral,
            definitions::bilateral(),
            "bilateral-blur",
            "双向模糊",
        ),
        (Mode::Smart, definitions::smart(), "smart-blur", "智能模糊"),
        (
            Mode::Channel,
            definitions::channel(),
            "channel-blur",
            "通道模糊",
        ),
        (
            Mode::Radial,
            definitions::radial(),
            "cc-radial-blur",
            "CC Radial Blur",
        ),
        (
            Mode::Fast,
            definitions::radial_fast(),
            "cc-radial-fast-blur",
            "CC Radial Fast Blur",
        ),
        (
            Mode::Vector,
            definitions::vector(),
            "cc-vector-blur",
            "CC Vector Blur",
        ),
        (
            Mode::Cross,
            definitions::cross(),
            "cc-cross-blur",
            "CC Cross Blur",
        ),
    ]
    .map(|(mode, b, id, zh)| {
        let (def, names) = b.reference_points().build();
        rust_builtin(
            (Arc::new(Optics { def, mode }), names),
            id,
            zh,
            &[id, "模糊"],
        )
    })
}
impl Optics {
    fn dependency(&self, p: &EffectParams<'_>) -> Option<LayerDependency> {
        match self.mode {
            Mode::Lens => layer_input::dependency(p, 10, false),
            Mode::Vector => layer_input::dependency(p, 5, false),
            _ => None,
        }
    }
    fn radius(&self, v: Params<'_>) -> f64 {
        (match self.mode {
            Mode::Lens => v.f(1) * v.f(5).max(1.) + 1.,
            Mode::Cross => v.f(1).max(v.f(2)) * 3.,
            Mode::Channel => {
                (1..=4).map(|i| v.f(i)).fold(0., f64::max) * BLURRINESS_TO_SIGMA * 4. + 4.
            }
            Mode::Vector => v.f(2).abs() + v.f(4) + v.f(7) * 2. + 2.,
            _ => v.f(1),
        }) * v.p.scale
    }
}
impl Effect for Optics {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        self.dependency(p).into_iter().collect()
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        if roi.is_empty() {
            return roi;
        }
        if matches!(self.mode, Mode::Radial | Mode::Fast) {
            let v = Params::new(&self.def, p);
            let amount = v.f(2).abs();
            if amount == 0. {
                return roi;
            }
            let fast = self.mode == Mode::Fast;
            let center = p
                .point(if fast { 1 } else { 4 })
                .map(|pt| pt.map(|x| x * p.scale));
            let [cx, cy] = match center {
                Some(pt) => pt,
                None if !p.layer_bounds.is_empty() => {
                    let layer = image_grid::layer_rect(p);
                    [
                        0.5 * f64::from(layer.x0 + layer.x1),
                        0.5 * f64::from(layer.y0 + layer.y1),
                    ]
                }
                // The default center comes from the complete source in standalone renders.
                None => return IRect::new(-1 << 26, -1 << 26, 1 << 26, 1 << 26),
            };
            let dx = (f64::from(roi.x0) - cx)
                .abs()
                .max((f64::from(roi.x1) - cx).abs());
            let dy = (f64::from(roi.y0) - cy)
                .abs()
                .max((f64::from(roi.y1) - cy).abs());
            let travel = if fast || v.popup(1) < 5 {
                amount / 200.
            } else {
                (0.5 * amount.to_radians()).min(2.)
            };
            return roi.inflate((dx.hypot(dy) * travel).ceil() as i32 + 1);
        }
        roi.inflate(self.radius(Params::new(&self.def, p)).ceil() as i32 + 2)
    }
    fn output_bounds(&self, p: &EffectParams<'_>, input: IRect) -> IRect {
        let v = Params::new(&self.def, p);
        let grow = match self.mode {
            Mode::Channel => !v.b(5),
            Mode::Cross => !v.b(4),
            Mode::Lens => !v.b(21),
            _ => false,
        };
        if grow && !input.is_empty() {
            input.inflate(self.radius(v).ceil() as i32)
        } else {
            input
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
impl TargetEffect for Optics {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        let map = self
            .dependency(p)
            .and_then(|d| target.effect_layer(&d))
            .cloned();
        let input = common::premultiplied(target, input)?;
        match self.mode {
            Mode::Lens => {
                if v.f(1) == 0. {
                    return Ok(input);
                }
                shader!("lens").dispatch(target, v, roi, &input, [map.as_ref(), None, None], [0, 0])
            }
            Mode::Bilateral | Mode::Smart => {
                if v.f(1) == 0. {
                    return Ok(input);
                }
                shader!("bilateral").dispatch(
                    target,
                    v,
                    roi,
                    &input,
                    [None; 3],
                    [u32::from(self.mode == Mode::Smart), 0],
                )
            }
            Mode::Vector => {
                if v.f(2) == 0. {
                    return Ok(input);
                }
                let map = map.as_ref().unwrap_or(&input);
                let sigma = v.f(7) * p.scale * 0.4;
                let smooth =
                    blur::plan_gaussian(map.rect, [sigma; 2], blur::Edge::Repeat, map.rect)
                        .encode(target, map)?;
                shader!("vector").dispatch(
                    target,
                    v,
                    roi,
                    &input,
                    [Some(&smooth), None, None],
                    [0, 0],
                )
            }
            Mode::Channel => {
                let dimensions = dimensions(v.popup(6));
                let edge = if v.b(5) {
                    blur::Edge::Repeat
                } else {
                    blur::Edge::Transparent
                };
                let mut channels = Vec::with_capacity(4);
                for id in 1..=4 {
                    let r = v.f(id);
                    if r == 0. {
                        channels.push(input.clone());
                        continue;
                    }
                    if let Some(i) = (1..id).find(|&other| v.f(other) == r) {
                        channels.push(channels[i as usize - 1].clone());
                        continue;
                    }
                    let sigma = dimensions.map(|k| k * r * BLURRINESS_TO_SIGMA * p.scale);
                    channels.push(
                        blur::plan_gaussian(input.rect, sigma, edge, roi).encode(target, &input)?,
                    );
                }
                shader!("channels").dispatch(
                    target,
                    v,
                    roi,
                    &channels[0],
                    [Some(&channels[1]), Some(&channels[2]), Some(&channels[3])],
                    [0, 0],
                )
            }
            Mode::Cross => {
                let mut axes = Vec::with_capacity(2);
                for (id, axis) in [(1, [1., 0.]), (2, [0., 1.])] {
                    let sigma = (v.f(id) * p.scale * BLURRINESS_TO_SIGMA) as f32;
                    let edge = f32::from(v.b(4));
                    let image = if sigma <= 0. {
                        input.clone()
                    } else {
                        dispatch(
                            target,
                            "directional",
                            &input,
                            &input,
                            roi,
                            &[[axis[0], axis[1], sigma, edge]],
                            p.float,
                        )?
                    };
                    axes.push(image);
                }
                shader!("cross").dispatch(
                    target,
                    v,
                    roi,
                    &input,
                    [Some(&axes[0]), Some(&axes[1]), None],
                    [0, 0],
                )
            }
            Mode::Radial | Mode::Fast => {
                let fast = self.mode == Mode::Fast;
                let center_id = if fast { 1 } else { 4 };
                let center = p
                    .point(center_id)
                    .map(|pt| pt.map(|x| (x * p.scale) as f32))
                    .unwrap_or_else(|| {
                        let r = if p.layer_bounds.is_empty() {
                            input.rect
                        } else {
                            image_grid::layer_rect(p)
                        };
                        [(r.x0 + r.x1) as f32 * 0.5, (r.y0 + r.y1) as f32 * 0.5]
                    });
                let amount = v.f(2) as f32;
                if amount == 0. {
                    return Ok(input);
                }
                let kind = if fast { 1 } else { v.popup(1) };
                let zoom = u32::from(kind < 5) + 1;
                let quality = if fast { 0.35 } else { v.f(3) as f32 * 0.01 };
                dispatch(
                    target,
                    "radial",
                    &input,
                    &input,
                    roi,
                    &[
                        [center[0], center[1], amount, zoom as f32],
                        [1., 0., quality, kind as f32],
                        [if fast { v.popup(3) as f32 } else { 1. }, 0., 0., 0.],
                    ],
                    p.float,
                )
            }
        }
    }
}
