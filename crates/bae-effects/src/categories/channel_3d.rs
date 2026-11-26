//! Auxiliary footage effects in source-layer coordinates.
mod depth;
mod ids;
mod kernel;
pub mod settings;

use crate::{
    Builtin, EffectTarget, Registry, TargetEffect,
    catalog::DefBuilder,
    effects::{image_grid, layer_input},
    gpu,
    params::Params,
    rust_builtin,
};
use bae_gpu::{channels::FootageChannels, wgpu};
use bae_model::{EffectDef, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError,
    effect::{LayerDependency, LayerStage},
};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum Method {
    Extract,
    Focus,
    Matte,
    Fog,
    IdMatte,
    Crypto,
    Routing,
    Identifier,
}

struct ChannelEffect {
    def: EffectDef,
    method: Method,
}

pub(crate) fn register(r: &mut Registry) {
    r.effects.extend(depth::entries());
    r.effects.extend(ids::entries());
    r.prewarm.push(prewarm);
}

fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    kernel::pipeline(rt, format)?;
    Ok(())
}

fn entry(builder: DefBuilder, method: Method, slug: &str, zh: &str) -> Builtin {
    let (def, labels) = builder.build();
    rust_builtin(
        (Arc::new(ChannelEffect { def, method }), labels),
        slug,
        zh,
        &["EXR", "3D 通道"],
    )
}

fn source(p: &EffectParams<'_>) -> Option<LayerDependency> {
    Some(LayerDependency {
        layer: layer_input::own_layer(p)?,
        time: p.comp_time,
        stage: LayerStage::Source,
    })
}

impl Effect for ChannelEffect {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        if self.method == Method::Focus && !roi.is_empty() {
            let radius = Params::new(&self.def, p).f(2) * p.scale;
            roi.inflate(radius.ceil() as i32)
        } else {
            roi
        }
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        let mut deps: Vec<_> = source(p).into_iter().collect();
        if self.method == Method::Fog {
            deps.extend(layer_input::dependency(p, 7, false));
        }
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

impl TargetEffect for ChannelEffect {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let channels = input.channels.clone().or_else(|| {
            target
                .effect_layer(&source(p)?)
                .and_then(|im| im.channels.clone())
        });
        let Some(channels) = channels else {
            return Ok(input.clone());
        };
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let params = Params::new(&self.def, p);
        let ids = depth::planes(&channels, self.method, params);
        let ramp = (self.method == Method::Fog)
            .then(|| layer_input::dependency(p, 7, false))
            .flatten()
            .and_then(|dep| target.effect_layer(&dep))
            .cloned();
        let rt = gpu::runtime(target.gpu());
        let kernel = kernel::pipeline(&rt, target.format())?;
        let grid = image_grid::grid_scale(p, input);
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let mut u = gpu::UniformWriter::with_size(kernel::BYTES as usize);
        image_grid::image_coords(&mut u, 0, input, grid);
        image_grid::image_coords(&mut u, 32, ramp.as_ref().unwrap_or(input), grid);
        u.i32s(64, &image_grid::rect4(roi))
            .i32s(80, &ids)
            .u32(96, self.method as u32)
            .f32(100, grid as f32)
            .u32(104, u32::from(p.float))
            .u32(108, u32::from(ramp.is_some()));
        for id in 1..=8 {
            let at = 112 + id as usize * 16;
            if let Some(Value::Color(c)) = params.value(id) {
                u.f32s(at, &c.map(|x| x as f32));
            } else {
                u.f32(at, params.f(id) as f32);
            }
        }
        ids::uniforms(self.method, &channels, params, &mut u);
        let offset = gpu::push_uniform(&rt, &u);
        let group = rt.bind_group(
            &kernel,
            &[
                gpu::Resource::Uniform,
                gpu::Resource::View(&input.view),
                gpu::Resource::View(&channels.view),
                gpu::Resource::View(&ramp.as_ref().unwrap_or(input).view),
                gpu::Resource::View(&out.view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("bae.effects.channel_3d"),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &kernel, &group, &[offset], roi.size());
        Ok(out)
    }
}

/// Negative indices below -1 distinguish unsigned integer channels from FLOAT.
fn plane(channels: &FootageChannels, names: &[&str]) -> i32 {
    channels.info.find(names).map_or(-1, |i| {
        if channels.info.channels[i].unsigned {
            -(i as i32) - 2
        } else {
            i as i32
        }
    })
}
