//! Set Matte (ADBE Set Matte3).

use std::sync::Arc;

use bae_gpu::wgpu;
use bae_model::{EffectDef, PrimPath, Value};
use bae_render::effect::LayerDependency;
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};

use crate::catalog::DefBuilder;
use crate::gpu::{self, Resource, UniformWriter};
use crate::params::Params;
use crate::target::{EffectTarget, TargetEffect};
use crate::{Builtin, rust_builtin};

use super::image_grid;

const MATCH_NAME: &str = "ADBE Set Matte3";
const LABEL: &str = "bae.effects.set_matte";
const SOURCE: &str = include_str!("../shaders/set_matte.wgsl");
const UNIFORM_SIZE: u32 = 128;

struct SetMatte(EffectDef);

pub(crate) fn builtin() -> Builtin {
    let (def, zh) = DefBuilder::new(MATCH_NAME, "Set Matte", "Channel")
        .layer(1, "Take Matte From Layer", "从图层获取遮罩")
        .popup(
            2,
            "Use For Matte",
            "用于遮罩",
            4,
            &[
                "Red Channel",
                "Green Channel",
                "Blue Channel",
                "Alpha Channel",
                "Luminance",
                "Hue",
                "Lightness",
                "Saturation",
                "Full",
                "Off",
            ],
        )
        .checkbox(3, "Invert Matte", "反转遮罩", false)
        .checkbox(4, "If Layer Sizes Differ", "如果图层大小不同", true)
        .checkbox(
            5,
            "Composite Matte with Original",
            "将遮罩与原始图像合成",
            true,
        )
        .checkbox(6, "Premultiply Matte Layer", "预乘遮罩图层", true)
        .build();
    rust_builtin(
        (Arc::new(SetMatte(def)), zh),
        "set-matte",
        "设置遮罩",
        &["matte", "alpha", "channel", "遮罩", "通道"],
    )
}

fn source_layer(params: &EffectParams<'_>) -> Option<PrimPath> {
    match params.value(1) {
        Some(Value::PrimRef(layer)) => Some(layer.clone()),
        // A new effect defaults to its own layer; sample the source before
        // effects. Some standalone fixtures don't have a separate source.
        None => params.effect.path.parent()?.parent(),
        _ => None,
    }
}

pub(super) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    image_grid::kernel(rt, format, LABEL, SOURCE, UNIFORM_SIZE)?;
    Ok(())
}

impl Effect for SetMatte {
    fn def(&self) -> &EffectDef {
        &self.0
    }

    fn layer_dependencies(&self, params: &EffectParams<'_>) -> Vec<LayerDependency> {
        source_layer(params)
            .into_iter()
            .map(|layer| LayerDependency::source(layer, params.comp_time))
            .collect()
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, params, input, roi)
    }
}

impl TargetEffect for SetMatte {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        params: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let Some(layer) = source_layer(params) else {
            return Ok(input.clone());
        };
        let matte = match target.effect_input(&layer) {
            Some(image) => image,
            None if params.layer_bounds.is_empty() && params.value(1).is_none() => input,
            None => return Ok(input.clone()),
        }
        .clone();
        let grid = image_grid::grid_scale(params, input);
        let bounds = if params.layer_bounds.is_empty() {
            image_grid::pixel_rect(input.rect, 1.0 / grid)
        } else {
            image_grid::layer_rect(params)
        };
        let p = Params::new(&self.0, params);
        let mode = match p.popup(2) {
            c @ 1..=3 => c as u32 + 1,
            4 => 1,
            c @ 5..=10 => c as u32,
            _ => 1,
        };
        let rt = gpu::runtime(target.gpu());
        let kernel = image_grid::kernel(&rt, target.format(), LABEL, SOURCE, UNIFORM_SIZE)?;
        let output = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let mut w = UniformWriter::with_size(UNIFORM_SIZE as usize);
        image_grid::image_coords(&mut w, 0, input, grid);
        image_grid::image_coords(&mut w, 32, &matte, grid);
        w.i32s(64, &image_grid::rect4(roi))
            .i32s(80, &image_grid::rect4(bounds))
            .f32(96, (f64::from(matte.rect.width()) / grid) as f32)
            .f32(100, (f64::from(matte.rect.height()) / grid) as f32)
            .u32(104, mode)
            .u32(108, u32::from(p.b(3)))
            .u32(112, u32::from(p.b(4)))
            .u32(116, u32::from(p.b(5)))
            .u32(120, u32::from(p.b(6)))
            .u32(124, u32::from(params.float));
        let offset = gpu::push_uniform(&rt, &w);
        let group = rt.bind_group(
            &kernel,
            &[
                Resource::Uniform,
                Resource::View(&input.view),
                Resource::View(&matte.view),
                Resource::View(&output.view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(LABEL),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &kernel, &group, &[offset], roi.size());
        Ok(output)
    }
}
