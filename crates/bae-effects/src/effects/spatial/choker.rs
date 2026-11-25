//! AE Matte Choker applies two blur/levels stages per iteration.
use super::pass::dispatch_to;
use crate::{
    Builtin, EffectTarget, TargetEffect, catalog::DefBuilder, common, params::Params, rust_builtin,
};
use bae_model::EffectDef;
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use std::sync::Arc;

#[derive(Debug)]
struct MatteChoker(EffectDef);

pub(crate) fn builtin() -> Builtin {
    let mut b = DefBuilder::new("ADBE Matte Choker", "Matte Choker", "Matte");
    for (id, en, zh, default, range, display) in [
        (
            1,
            "Geometric Softness 1",
            "几何柔和度 1",
            4.,
            [0., 100.],
            "pixels",
        ),
        (2, "Choke 1", "阻塞 1", 75., [-127., 127.], "none"),
        (
            3,
            "Gray Level Softness 1",
            "灰色阶柔和度 1",
            0.100006,
            [0., 1.],
            "percent",
        ),
        (
            4,
            "Geometric Softness 2",
            "几何柔和度 2",
            0.,
            [0., 100.],
            "pixels",
        ),
        (5, "Choke 2", "阻塞 2", 0., [-127., 127.], "none"),
        (
            6,
            "Gray Level Softness 2",
            "灰色阶柔和度 2",
            1.,
            [0., 1.],
            "percent",
        ),
    ] {
        b = b.slider(id, en, zh, default, range, range, 2, display);
    }
    let (def, zh) = b.int(7, "Iterations", "迭代", 1, [1, 100]).build();
    rust_builtin(
        (Arc::new(MatteChoker(def)), zh),
        "matte-choker",
        "遮罩阻塞工具",
        &["choke", "matte", "阻塞", "遮罩"],
    )
}

impl MatteChoker {
    fn stages(&self, p: &EffectParams<'_>) -> ([[f32; 3]; 2], i32) {
        let v = Params::new(&self.0, p);
        let stages = [1, 4].map(|id| {
            [
                (v.f(id) * p.scale) as f32,
                v.f(id + 1) as f32 / 255.,
                v.f(id + 2) as f32,
            ]
        });
        (stages, v.f(7).round().clamp(1., 100.) as i32)
    }
}

fn reach(radius: f32) -> i32 {
    if radius == 0. {
        0
    } else {
        (radius + 2.).ceil() as i32
    }
}

impl Effect for MatteChoker {
    fn def(&self) -> &EffectDef {
        &self.0
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        let (stages, iterations) = self.stages(p);
        let reach: i32 = stages.iter().map(|s| reach(s[0])).sum();
        if roi.is_empty() {
            return roi;
        }
        roi.inflate(reach * iterations)
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
impl TargetEffect for MatteChoker {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let (stages, iterations) = self.stages(p);
        let read = self.input_roi(p, roi).intersect(&input.rect);
        if read.is_empty() || stages == [[0., 0., 1.]; 2] {
            return Ok(input.clone());
        }
        let input = common::premultiplied(target, input)?;
        // Alternate two textures, including at 100 iterations. Each pass only
        // overwrites a surface after the previous pass has finished reading it.
        let horizontal = target.allocate(read, input.pixel_scale)?;
        let vertical = target.allocate(read, input.pixel_scale)?;
        let mut current = &input;
        for _ in 0..iterations {
            for [radius, choke, softness] in stages {
                if [radius, choke, softness] == [0., 0., 1.] {
                    continue;
                }
                dispatch_to(
                    target,
                    "matte_choke",
                    current,
                    &input,
                    &horizontal,
                    &[[radius, choke, softness, 0.]],
                    p.float,
                )?;
                dispatch_to(
                    target,
                    "matte_choke",
                    &horizontal,
                    &input,
                    &vertical,
                    &[[radius, choke, softness, 1.]],
                    p.float,
                )?;
                current = &vertical;
            }
        }
        Ok(vertical)
    }
}
