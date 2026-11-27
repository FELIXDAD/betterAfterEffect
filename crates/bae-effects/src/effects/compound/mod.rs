//! Effects that sample a second layer or a mask in the destination layer's space.
mod blur_map;
mod definitions;

use bae_gpu::wgpu;
use bae_model::{EffectDef, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError,
    effect::{LayerDependency, MaskSelect},
};

use crate::{
    EffectTarget, TargetEffect,
    blur::{self, Edge},
    gpu::{self, Resource, UniformWriter},
    params::Params,
};

use super::{image_grid, layer_input};

pub(crate) use definitions::{compound_blur, displacement, fill, gradient, texturize};

const LABEL: &str = "bae.effects.compound";
const SOURCE: &str = include_str!("compound.wgsl");
const UNIFORM_SIZE: u32 = 144;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Gradient,
    Displacement,
    Texturize,
    Fill,
    Blur,
}

struct Compound {
    def: EffectDef,
    mode: Mode,
}

impl Compound {
    fn dependency(&self, p: &EffectParams<'_>) -> Option<LayerDependency> {
        match self.mode {
            Mode::Gradient => layer_input::dependency(p, 3, true),
            Mode::Displacement | Mode::Blur => layer_input::dependency(p, 1, true),
            Mode::Texturize => layer_input::dependency(p, 1, false),
            Mode::Fill => None,
        }
    }

    fn mask(&self, p: &EffectParams<'_>) -> Option<MaskSelect> {
        if self.mode != Mode::Fill {
            return None;
        }
        let v = Params::new(&self.def, p);
        if v.b(7) {
            Some(MaskSelect::All)
        } else if let Some(Value::PrimRef(mask)) = v.value(1) {
            Some(MaskSelect::One(mask.clone()))
        } else {
            None
        }
    }
}

fn bounds_padding(r: IRect, xy: [f64; 2]) -> IRect {
    let [x, y] = xy.map(|n| n.abs().ceil() as i32);
    IRect::new(r.x0 - x, r.y0 - y, r.x1 + x, r.y1 + y)
}

impl Effect for Compound {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        if self.mode == Mode::Gradient && Params::new(&self.def, p).f(1) == 0. {
            return Vec::new();
        }
        self.dependency(p).into_iter().collect()
    }

    fn mask_dependencies(&self, p: &EffectParams<'_>) -> Vec<MaskSelect> {
        self.mask(p).into_iter().collect()
    }

    fn output_bounds(&self, p: &EffectParams<'_>, input: IRect) -> IRect {
        let v = Params::new(&self.def, p);
        if self.mode == Mode::Displacement && v.b(8) && !input.is_empty() {
            return bounds_padding(input, [v.f(3) * p.scale, v.f(5) * p.scale]);
        }
        input
    }

    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        let v = Params::new(&self.def, p);
        match self.mode {
            Mode::Displacement if v.b(7) => image_grid::layer_rect(p).union(&roi),
            Mode::Displacement => {
                bounds_padding(roi, [v.f(3) * p.scale, v.f(5) * p.scale]).inflate(1)
            }
            Mode::Blur => blur_map::input_roi(v.f(2), p.scale, roi),
            _ => roi,
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

impl TargetEffect for Compound {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        if roi.is_empty() || self.mode == Mode::Gradient && v.f(1) <= 0. {
            return Ok(input.clone());
        }
        let grid = image_grid::grid_scale(p, input);
        let layer = if p.layer_bounds.is_empty() {
            image_grid::pixel_rect(input.rect, 1. / grid)
        } else {
            image_grid::layer_rect(p)
        };
        let mask = self
            .mask(p)
            .and_then(|select| target.effect_mask(&select))
            .cloned();
        let has_mask = mask.is_some();
        let map = mask.or_else(|| {
            self.dependency(p)
                .and_then(|dep| target.effect_layer(&dep))
                .cloned()
        });
        let map = match map {
            Some(map) => map,
            None if self.mode == Mode::Fill => input.clone(),
            // Tests and tool previews can render a single image as their own source.
            None if self.mode != Mode::Texturize
                && p.layer_bounds.is_empty()
                && p.value(if self.mode == Mode::Gradient { 3 } else { 1 })
                    .is_none() =>
            {
                input.clone()
            }
            None => return Ok(input.clone()),
        };
        let pass = Pass { roi, layer, grid };
        let mut flags = [self.mode as u32, 0, 0, 0];
        let data;
        let mut secondary = [0.; 4];
        let mut map = map;
        match self.mode {
            Mode::Gradient => {
                flags[1] = v.popup(4) as u32;
                flags[2] = u32::from(v.b(5));
                flags[3] = if p.float {
                    0
                } else if p.sixteen_bpc {
                    32768
                } else {
                    255
                };
                data = [v.f(1) as f32, v.f(2) as f32, 0., 0.];
            }
            Mode::Displacement => {
                flags[1] = [2, 3, 1][(v.popup(6) - 1).clamp(0, 2) as usize];
                flags[2] = v.popup(2) as u32;
                flags[3] = v.popup(4) as u32;
                data = [(v.f(3) * p.scale) as f32, (v.f(5) * p.scale) as f32, 0., 0.];
                secondary = [
                    f32::from(v.b(7)),
                    f32::from(p.float),
                    f32::from(p.sixteen_bpc),
                    0.,
                ];
            }
            Mode::Texturize => {
                let angle = v.f(2).to_radians();
                flags[1] = v.popup(4) as u32;
                flags[2] = u32::from(p.float);
                data = [
                    (angle.sin() * p.scale) as f32,
                    (-angle.cos() * p.scale) as f32,
                    v.f(3) as f32,
                    0.,
                ];
            }
            Mode::Fill => {
                let sigma = [v.f(3), v.f(4)].map(|r| r * p.scale * grid / map.pixel_scale * 0.4);
                if has_mask && sigma.map(blur::radius) != [0, 0] {
                    map = blur::plan_gaussian(map.rect, sigma, Edge::Transparent, map.rect)
                        .encode(target, &map)?;
                }
                flags[1] = u32::from(v.b(6));
                flags[2] = u32::from(p.float);
                flags[3] = u32::from(has_mask);
                let rgb = p.color(2).unwrap_or([1., 0., 0., 1.]);
                data = [rgb[0] as f32, rgb[1] as f32, rgb[2] as f32, v.f(5) as f32];
            }
            Mode::Blur => {
                let radius = v.f(2);
                if radius <= 0. {
                    return Ok(input.clone());
                }
                return blur_map::encode(target, &pass, input, &map, v);
            }
        }
        pass.dispatch(target, [input, &map], flags, data, secondary)
    }
}

struct Pass {
    roi: IRect,
    layer: IRect,
    grid: f64,
}

impl Pass {
    fn dispatch(
        &self,
        target: &mut dyn EffectTarget,
        images: [&GpuImage; 2],
        flags: [u32; 4],
        data: [f32; 4],
        secondary: [f32; 4],
    ) -> Result<GpuImage, RenderError> {
        let rt = gpu::runtime(target.gpu());
        let kernel =
            image_grid::kernel_with_inputs(&rt, target.format(), LABEL, SOURCE, UNIFORM_SIZE, 2)?;
        let out = target.allocate(image_grid::pixel_rect(self.roi, self.grid), self.grid)?;
        let mut u = UniformWriter::with_size(UNIFORM_SIZE as usize);
        for (i, image) in images.into_iter().enumerate() {
            image_grid::image_coords(&mut u, i * 32, image, self.grid);
        }
        u.i32s(64, &image_grid::rect4(self.roi))
            .i32s(80, &image_grid::rect4(self.layer))
            .f32s(112, &data)
            .f32s(128, &secondary);
        for (i, flag) in flags.into_iter().enumerate() {
            u.u32(96 + 4 * i, flag);
        }
        let offset = gpu::push_uniform(&rt, &u);
        let group = rt.bind_group(
            &kernel,
            &[
                Resource::Uniform,
                Resource::View(&images[0].view),
                Resource::View(&images[1].view),
                Resource::View(&out.view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(LABEL),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &kernel, &group, &[offset], self.roi.size());
        Ok(out)
    }
}

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    image_grid::kernel_with_inputs(rt, format, LABEL, SOURCE, UNIFORM_SIZE, 2)?;
    blur_map::prewarm(rt, format)
}
