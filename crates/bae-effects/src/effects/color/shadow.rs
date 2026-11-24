use crate::blur::{self, Edge};
use crate::catalog::DefBuilder;
use crate::gpu::UniformWriter;
use crate::params::Params;
use crate::target::{EffectTarget, TargetEffect};
use bae_model::{EffectDef, ParamKind};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};

pub(super) const UNIFORM_BYTES: usize = 80;

#[derive(Debug)]
pub(crate) struct ShadowHighlight(EffectDef);

impl ShadowHighlight {
    pub fn new() -> (Self, Vec<(u32, String)>) {
        let mut b = DefBuilder::new(
            "ADBE ShadowHighlight",
            "Shadow/Highlight",
            "Color Correction",
        )
        .checkbox(1, "Auto Amounts", "自动数量", true);
        let slider = |b: DefBuilder, id, name, zh, default, min, max, unit| {
            b.slider(id, name, zh, default, [min, max], [min, max], 2, unit)
        };
        b = slider(b, 2, "Shadow Amount", "阴影数量", 50., 0., 100., "percent");
        b = slider(
            b,
            3,
            "Highlight Amount",
            "高光数量",
            0.,
            0.,
            100.,
            "percent",
        );
        b = slider(
            b,
            4,
            "Temporal Smoothing (seconds)",
            "瞬时平滑（秒）",
            0.,
            0.,
            10.,
            "seconds",
        );
        b = b.checkbox(5, "Scene Detect", "场景检测", false);
        b.push(6, "More Options", "更多选项", ParamKind::GroupStart, None);
        for (id, name, zh, default, min, max, unit) in [
            (
                7,
                "Shadow Tonal Width",
                "阴影色调宽度",
                50.,
                0.,
                100.,
                "percent",
            ),
            (8, "Shadow Radius", "阴影半径", 30., 0., 100., "pixels"),
            (
                9,
                "Highlight Tonal Width",
                "高光色调宽度",
                50.,
                0.,
                100.,
                "percent",
            ),
            (10, "Highlight Radius", "高光半径", 30., 0., 100., "pixels"),
            (11, "Color Correction", "颜色校正", 20., -100., 100., "none"),
            (
                12,
                "Midtone Contrast",
                "中间调对比度",
                0.,
                -100.,
                100.,
                "none",
            ),
            (13, "Black Clip", "修剪黑色", 0.01, 0., 50., "percent"),
            (14, "White Clip", "修剪白色", 0.01, 0., 50., "percent"),
        ] {
            b = slider(b, id, name, zh, default, min, max, unit);
        }
        b.push(15, "More Options", "更多选项", ParamKind::GroupEnd, None);
        let (mut def, zh) = slider(
            b,
            16,
            "Blend With Original",
            "与原始图像混合",
            0.,
            0.,
            100.,
            "percent",
        )
        .build();
        // These two controls require pre-effect pixels at neighbouring times.
        for p in &mut def.params {
            if matches!(p.id, 4 | 5) {
                p.flags.disabled = true;
            }
        }
        (Self(def), zh)
    }
}

impl Effect for ShadowHighlight {
    fn def(&self) -> &EffectDef {
        &self.0
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        let v = Params::new(&self.0, p);
        if roi.is_empty() || v.f(16) >= 100. {
            return roi;
        }
        if v.b(1) || v.f(13) > 0. || v.f(14) > 0. {
            // Histogram percentiles depend on the whole upstream image,
            // including any growth by earlier effects. Graph clips to its bounds.
            return IRect::new(i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2);
        }
        let sigma = v.f(8).max(v.f(10)) * 0.4 * p.scale;
        blur::plan_gaussian(IRect::EMPTY, [sigma; 2], Edge::Repeat, roi).read
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

impl TargetEffect for ShadowHighlight {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.0, p);
        let roi = roi.intersect(&input.rect);
        if roi.is_empty() || v.f(16) >= 100. {
            return Ok(input.clone());
        }
        let input = crate::common::premultiplied(target, input)?;
        let out = target.allocate(roi, input.pixel_scale)?;
        let local = |target: &mut dyn EffectTarget, id| {
            let sigma = v.f(id) * 0.4 * p.scale;
            if blur::radius(sigma) == 0 {
                return Ok(input.clone());
            }
            blur::plan_gaussian(input.rect, [sigma; 2], Edge::Repeat, roi).encode(target, &input)
        };
        let shadows = local(target, 8)?;
        let highlights = if v.f(8) == v.f(10) {
            shadows.clone()
        } else {
            local(target, 10)?
        };
        let mut u = UniformWriter::with_size(UNIFORM_BYTES);
        u.i32s(
            0,
            &[
                roi.x0 - input.rect.x0,
                roi.y0 - input.rect.y0,
                roi.x0 - shadows.rect.x0,
                roi.y0 - shadows.rect.y0,
                roi.x0 - highlights.rect.x0,
                roi.y0 - highlights.rect.y0,
            ],
        )
        .u32(24, u32::from(v.b(1)));
        let histogram = v.b(1) || v.f(13) > 0. || v.f(14) > 0.;
        u.u32(28, u32::from(histogram));
        for (i, id) in [2, 3, 7, 9, 11, 12, 13, 14, 16].into_iter().enumerate() {
            u.f32(32 + i * 4, (v.f(id) / 100.) as f32);
        }
        let stats = if histogram {
            let bins = target.allocate(IRect::from_size(64, 256), 1.)?;
            let stats = target.allocate(IRect::from_size(1, 1), 1.)?;
            super::dispatch(target, "histogram", &u, &[&input], &bins, [128, 128])?;
            super::dispatch(target, "percentiles", &u, &[&bins], &stats, [8, 8])?;
            stats
        } else {
            input.clone()
        };
        super::dispatch(
            target,
            "shadow_highlight",
            &u,
            &[&input, &shadows, &highlights, &stats],
            &out,
            roi.size(),
        )?;
        Ok(out)
    }
}
