//! Frame-wide histogram corrections. Only the input ROI spans the whole layer.
use std::sync::Arc;

use bae_model::EffectDef;
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};

use crate::catalog::DefBuilder;
use crate::effects::image_grid;
use crate::gpu::UniformWriter;
use crate::params::Params;
use crate::{Builtin, EffectTarget, TargetEffect, rust_builtin};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Levels,
    Contrast,
    Color,
    Equalize,
}

#[derive(Debug)]
struct Auto {
    def: EffectDef,
    mode: Mode,
}

pub(crate) fn builtins() -> Vec<Builtin> {
    [
        (
            Mode::Levels,
            "ADBE AutoLevels",
            "Auto Levels",
            "自动色阶",
            "auto-levels",
        ),
        (
            Mode::Contrast,
            "ADBE AutoContrast",
            "Auto Contrast",
            "自动对比度",
            "auto-contrast",
        ),
        (
            Mode::Color,
            "ADBE AutoColor",
            "Auto Color",
            "自动颜色",
            "auto-color",
        ),
        (
            Mode::Equalize,
            "ADBE Equalize",
            "Equalize",
            "色调均化",
            "equalize",
        ),
    ]
    .into_iter()
    .map(|(mode, mn, en, zh, id)| {
        let mut b = DefBuilder::new(mn, en, "Color Correction");
        if mode == Mode::Equalize {
            b = b
                .popup(
                    1,
                    "Equalize",
                    "色调均化",
                    3,
                    &["RGB", "Brightness", "Photoshop Style"],
                )
                .slider(
                    2,
                    "Amount to Equalize",
                    "色调均化量",
                    100.,
                    [0., 100.],
                    [0., 100.],
                    1,
                    "percent",
                );
        } else {
            b = b
                .slider(
                    1,
                    "Temporal Smoothing (seconds)",
                    "瞬时平滑（秒）",
                    0.,
                    [0., 10.],
                    [0., 10.],
                    2,
                    "none",
                )
                .checkbox(2, "Scene Detect", "场景检测", false)
                .slider(
                    3,
                    "Black Clip",
                    "修剪黑色",
                    0.1,
                    [0., 10.],
                    [0., 10.],
                    2,
                    "percent",
                )
                .slider(
                    4,
                    "White Clip",
                    "修剪白色",
                    0.1,
                    [0., 10.],
                    [0., 10.],
                    2,
                    "percent",
                );
            if mode == Mode::Color {
                b = b.checkbox(5, "Snap Neutral Midtones", "对齐中性中间调", false);
            }
            let blend = if mode == Mode::Color { 6 } else { 5 };
            b = b.slider(
                blend,
                "Blend With Original",
                "与原始图像混合",
                0.,
                [0., 100.],
                [0., 100.],
                1,
                "percent",
            );
        }
        let (mut def, names) = b.build();
        if mode != Mode::Equalize {
            // Temporal smoothing needs the pre-effect frames on either side.
            for p in &mut def.params[..2] {
                p.flags.disabled = true;
            }
        }
        rust_builtin(
            (Arc::new(Auto { def, mode }), names),
            id,
            zh,
            &["直方图", "histogram"],
        )
    })
    .collect()
}

impl Effect for Auto {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn input_roi(&self, _: &EffectParams<'_>, roi: IRect) -> IRect {
        if roi.is_empty() {
            roi
        } else {
            IRect::new(i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2)
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

impl TargetEffect for Auto {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        let amount = match self.mode {
            Mode::Equalize => v.f(2) / 100.,
            Mode::Color => 1. - v.f(6) / 100.,
            _ => 1. - v.f(5) / 100.,
        };
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let grid = image_grid::grid_scale(p, input);
        let bins = target.allocate(IRect::from_size(256, 256), 1.)?;
        let snap = self.mode == Mode::Color && v.b(5);
        let mut map = target.allocate(IRect::from_size(if snap { 258 } else { 256 }, 1), 1.)?;
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let mut u = UniformWriter::with_size(80);
        image_grid::image_coords(&mut u, 0, input, grid);
        u.i32s(32, &[roi.x0, roi.y0])
            .u32(40, roi.width())
            .u32(44, roi.height())
            .u32(48, self.mode as u32)
            .u32(52, v.popup(1) as u32)
            .u32(56, u32::from(snap))
            .f32(60, amount as f32)
            .f32(64, (v.f(3) / 100.) as f32)
            .f32(68, (v.f(4) / 100.) as f32);
        super::dispatch(target, "auto_histogram", &u, &[input], &bins, [128, 128])?;
        super::dispatch(target, "auto_mapping", &u, &[&bins], &map, [8, 8])?;
        if snap {
            let midtones = target.allocate(IRect::from_size(32, 256), 1.)?;
            let corrected = target.allocate(IRect::from_size(256, 1), 1.)?;
            super::dispatch(
                target,
                "auto_midtones",
                &u,
                &[input, &map],
                &midtones,
                [128, 128],
            )?;
            super::dispatch(
                target,
                "auto_snap",
                &u,
                &[&map, &midtones],
                &corrected,
                [8, 8],
            )?;
            map = corrected;
        }
        super::dispatch(target, "auto_apply", &u, &[input, &map], &out, roi.size())?;
        Ok(out)
    }
}
