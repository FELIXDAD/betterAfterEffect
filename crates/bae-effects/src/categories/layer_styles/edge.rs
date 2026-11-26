use crate::{
    EffectTarget,
    blur::{self, Edge},
    effects::{image_grid, sampled::Shader},
    params::Params,
};
use bae_render::{GpuImage, IRect, RenderError};

pub(super) static SHADER: Shader = Shader {
    label: "bae.effects/layer-style-edge",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("../../shaders/layer_sampling.wgsl"),
        include_str!("../../shaders/generated_blend.wgsl"),
        include_str!("blend.wgsl"),
        include_str!("edge.wgsl")
    ),
};

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub(super) enum Kind {
    DropShadow,
    InnerShadow,
    OuterGlow,
    InnerGlow,
    Bevel,
}

/// Settings in render pixels. The original silhouette remains available when
/// several styles contribute to the same layer.
pub(super) struct Shading {
    pub kind: Kind,
    pub color: [f32; 4],
    pub shadow_color: [f32; 4],
    pub offset: [f64; 2],
    pub sigma: f64,
    pub gain: f32,
    pub center: bool,
    pub light: [f32; 2],
    pub pillow: bool,
    pub mode: u32,
    pub shadow_mode: u32,
    pub bevel_style: u32,
    pub technique: u32,
    pub width: f32,
    pub noise: f32,
    pub jitter: f32,
    pub knockout: bool,
}

#[derive(Default)]
pub(super) struct Paint<'a> {
    pub backdrop: bool,
    pub ramp: Option<&'a GpuImage>,
    pub distance: Option<&'a GpuImage>,
}

impl Shading {
    fn sample_rect(&self, r: IRect) -> IRect {
        if self.kind == Kind::Bevel {
            return r.inflate(1);
        }
        let [x, y] = self.offset;
        IRect::new(
            r.x0 - x.ceil() as i32,
            r.y0 - y.ceil() as i32,
            r.x1 - x.floor() as i32,
            r.y1 - y.floor() as i32,
        )
    }

    pub fn bounds(&self, r: IRect) -> IRect {
        if self.kind == Kind::Bevel && self.bevel_style != 2 && !r.is_empty() {
            return r.inflate((f64::from(self.width) + self.sigma * 3.).ceil() as i32);
        }
        if r.is_empty() || !matches!(self.kind, Kind::DropShadow | Kind::OuterGlow) {
            return r;
        }
        let [x, y] = self.offset;
        r.union(
            &IRect::new(
                r.x0 + x.floor() as i32,
                r.y0 + y.floor() as i32,
                r.x1 + x.ceil() as i32,
                r.y1 + y.ceil() as i32,
            )
            .inflate(blur::radius(self.sigma)),
        )
    }

    pub fn read(&self, r: IRect) -> IRect {
        if r.is_empty() {
            return r;
        }
        let plan = blur::plan_gaussian(
            IRect::EMPTY,
            [self.sigma; 2],
            Edge::Transparent,
            self.sample_rect(r),
        );
        r.union(&plan.read).inflate(if self.technique > 1 {
            self.width.ceil() as i32 + 1
        } else {
            0
        })
    }

    pub fn encode(
        &self,
        target: &mut dyn EffectTarget,
        params: Params<'_>,
        source: &GpuImage,
        composite: &GpuImage,
        roi: IRect,
        paint: Paint<'_>,
    ) -> Result<GpuImage, RenderError> {
        if roi.is_empty() {
            return Ok(composite.clone());
        }
        let soft = if blur::radius(self.sigma) > 0 {
            let grid = image_grid::grid_scale(params.p, source);
            let sample = self.sample_rect(roi);
            let mut alpha = source.clone();
            if source.pixel_scale != grid || source.alpha == bae_render::ImageAlpha::Opaque {
                let read = self
                    .read(roi)
                    .intersect(&image_grid::pixel_rect(source.rect, 1. / grid));
                alpha = SHADER.dispatch(target, params, read, source, [], [5, 0])?;
            }
            // The shared blur works in texels. Restore composition coordinates
            // on its result before sampling it alongside the original image.
            alpha.rect = image_grid::pixel_rect(alpha.rect, 1. / grid);
            alpha.pixel_scale = 1.;
            let mut soft =
                blur::plan_gaussian(alpha.rect, [self.sigma; 2], Edge::Transparent, sample)
                    .encode(target, &alpha)?;
            soft.rect = image_grid::pixel_rect(soft.rect, grid);
            soft.pixel_scale *= grid;
            soft
        } else {
            source.clone()
        };
        SHADER.dispatch_at(
            "main",
            target,
            params,
            roi,
            source,
            [Some(&soft), Some(composite), paint.distance, paint.ramp],
            [self.kind as u32, u32::from(paint.backdrop)],
            |u, header| {
                // These slots follow the definition's parameters in the shared uniform.
                u.f32s(header + 200 * 16, &self.color)
                    .f32s(header + 201 * 16, &self.shadow_color)
                    .f32s(
                        header + 202 * 16,
                        &[
                            self.offset[0] as f32,
                            self.offset[1] as f32,
                            self.gain,
                            f32::from(self.center),
                        ],
                    )
                    .f32s(
                        header + 203 * 16,
                        &[
                            self.light[0],
                            self.light[1],
                            f32::from(self.pillow),
                            self.mode as f32,
                        ],
                    )
                    .f32s(
                        header + 204 * 16,
                        &[
                            self.shadow_mode as f32,
                            self.bevel_style as f32,
                            self.technique as f32,
                            self.width,
                        ],
                    )
                    .f32s(
                        header + 205 * 16,
                        &[
                            self.noise,
                            self.jitter,
                            f32::from(paint.ramp.is_some()),
                            f32::from(self.knockout),
                        ],
                    );
            },
        )
    }
}
