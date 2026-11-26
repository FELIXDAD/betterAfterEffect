use crate::{
    Builtin, EffectTarget, Registry, TargetEffect, blur, catalog::DefBuilder,
    effects::sampled::Shader, gpu, params::Params, rust_builtin,
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, ParamKind, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use std::sync::Arc;

static SHADER: Shader = Shader {
    label: "bae.effects.three_way",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("three_way.wgsl")
    ),
};

pub(super) fn register(r: &mut Registry) {
    r.prewarm.push(prewarm);
    r.effects.push(builtin());
}
fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    SHADER.kernel(rt, format)?;
    Ok(())
}
fn builtin() -> Builtin {
    let mut b = DefBuilder::new(
        "ADBE Three-Way Color Corrector",
        "Three-Way Color Corrector",
        "Obsolete",
    );
    let slider = |b: DefBuilder, id, en, zh, init, lo, hi| {
        b.slider(id, en, zh, init, [lo, hi], [lo, hi], 2, "none")
    };
    let color = |b: &mut DefBuilder, id, en, zh, c| {
        b.push(
            id,
            en,
            zh,
            ParamKind::Color,
            Some(Value::Color([c, c, c, 1.])),
        )
    };
    let start = |b: &mut DefBuilder, id, en, zh| b.push(id, en, zh, ParamKind::GroupStart, None);
    let end = |b: &mut DefBuilder, id| b.push(id, "", "", ParamKind::GroupEnd, None);
    b = b
        .popup(
            1,
            "Output",
            "输出",
            1,
            &["Composite", "Luma", "Mask", "Tonal Range"],
        )
        .checkbox(2, "Show Split View", "显示拆分视图", false)
        .popup(3, "Layout", "布局", 1, &["Horizontal", "Vertical"]);
    b = slider(b, 4, "Split View Percent", "拆分视图百分比", 50., 0., 100.);
    for (id, en, zh, c) in [
        (5, "Black Balance", "黑平衡", 0.),
        (6, "Gray Balance", "灰平衡", 0.501961),
        (7, "White Balance", "白平衡", 1.),
    ] {
        color(&mut b, id, en, zh, c);
    }
    start(&mut b, 8, "Tonal Range Definition", "色调范围定义");
    for (id, en, zh, init, hi) in [
        (10, "Shadow Threshold", "阴影阈值", 64., 255.),
        (11, "Shadow Softness", "阴影柔和度", 10., 255.),
        (12, "Highlight Threshold", "高光阈值", 192., 253.),
        (13, "Highlight Softness", "高光柔和度", 10., 255.),
    ] {
        b = slider(b, id, en, zh, init, 0., hi);
    }
    end(&mut b, 15);
    for (id, en, zh) in [
        (17, "Master Controls", "主参数"),
        (34, "Shadow Controls", "阴影参数"),
        (51, "Midtone Controls", "中间调参数"),
        (68, "Highlight Controls", "高亮参数"),
    ] {
        start(&mut b, id, en, zh);
        start(&mut b, id + 1, "Hue Balance and Angle", "色相平衡和角度");
        b.push(
            id + 2,
            "Hue Angle",
            "色相角度",
            ParamKind::Angle,
            Some(Value::Float(0.)),
        );
        b = slider(b, id + 3, "Balance Magnitude", "平衡数量级", 0., 0., 100.);
        b = slider(b, id + 4, "Balance Gain", "平衡增益", 50., 1., 100.);
        b.push(
            id + 5,
            "Balance Angle",
            "平衡角度",
            ParamKind::Angle,
            Some(Value::Float(0.)),
        );
        end(&mut b, id + 6);
        b = slider(b, id + 7, "Saturation", "饱和度", 100., 0., 200.);
        if id == 17 {
            for (lev, pick, en, zh, pe, pz, val, lo, hi, c) in [
                (
                    28,
                    25,
                    "Input Black Level",
                    "输入黑色阶",
                    "Input Black Level Eyedropper",
                    "输入黑色拾色器",
                    0.,
                    0.,
                    253.,
                    0.,
                ),
                (
                    29,
                    26,
                    "Input Gray Level",
                    "输入灰色阶",
                    "Input Gray Level Eyedropper",
                    "输入灰色拾色器",
                    1.,
                    0.1,
                    9.97,
                    0.501961,
                ),
                (
                    30,
                    27,
                    "Input White Level",
                    "输入白色阶",
                    "Input White Level Eyedropper",
                    "输入白色拾色器",
                    255.,
                    2.,
                    255.,
                    1.,
                ),
            ] {
                b = slider(b, lev, en, zh, val, lo, hi);
                color(&mut b, pick, pe, pz, c);
            }
            b = slider(b, 31, "Output Black Level", "输出黑色阶", 0., 0., 253.);
            b = slider(b, 32, "Output White Level", "输出白色阶", 255., 2., 255.);
        }
        end(&mut b, id + 16);
    }
    start(&mut b, 85, "Secondary Color Correction", "辅助颜色校正");
    for (group, first, en, zh) in [
        (89, 92, "Hue", "色相"),
        (98, 100, "Saturation", "饱和度"),
        (106, 108, "Luma", "亮度"),
    ] {
        start(&mut b, group, en, zh);
        for (i, en, zh, val) in [
            (0, "Start Threshold", "起始阈值", 0.),
            (1, "Start Softness", "起始柔和度", 0.),
            (2, "End Threshold", "结尾阈值", 100.),
            (3, "End Softness", "结尾柔和度", 0.),
        ] {
            b = slider(b, first + i, en, zh, val, 0., 100.);
        }
        end(&mut b, first + 5);
    }
    b = slider(b, 114, "Soften", "柔化", 0., 0., 50.);
    b = slider(b, 115, "Edge Thinning", "薄化边缘", 0., -100., 100.);
    b = b.popup(
        119,
        "Invert",
        "反转",
        1,
        &["None", "Invert Limit Color", "Invert Mask"],
    );
    end(&mut b, 118);
    b = b.checkbox(81, "[?]", "[?]", false);
    let (def, names) = b.build();
    let mut fx = rust_builtin(
        (Arc::new(ThreeWay(def)), names),
        "three-way-color-corrector",
        "三向颜色校正器",
        &["three-way", "color", "三向校色"],
    );
    fx.info.param_options_zh = [
        (1, &["合成", "亮度", "蒙版", "色调范围"][..]),
        (3, &["水平", "垂直"][..]),
        (119, &["无", "反转限制颜色", "反转蒙版"][..]),
    ]
    .into_iter()
    .map(|(id, o)| (id, o.iter().map(|s| s.to_string()).collect()))
    .collect();
    fx
}

struct ThreeWay(EffectDef);
impl Effect for ThreeWay {
    fn def(&self) -> &EffectDef {
        &self.0
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        roi.inflate(blur::radius(Params::new(&self.0, p).f(114) * p.scale * 0.5))
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
impl TargetEffect for ThreeWay {
    fn encode(
        &self,
        t: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let v = Params::new(&self.0, p);
        let sigma = v.f(114) * p.scale * 0.5;
        let mask = if blur::radius(sigma) > 0 {
            let read = self.input_roi(p, roi);
            let mut mask = SHADER.dispatch(t, v, read, input, [], [1, 0])?;
            let (rect, scale) = (mask.rect, mask.pixel_scale);
            let [w, h] = mask.texel_size();
            mask.rect = IRect::from_size(w, h);
            mask.pixel_scale = 1.;
            let mut result =
                blur::plan_gaussian(mask.rect, [sigma; 2], blur::Edge::Repeat, mask.rect)
                    .encode(t, &mask)?;
            result.rect = rect;
            result.pixel_scale = scale;
            Some(result)
        } else {
            None
        };
        SHADER.dispatch(t, v, roi, input, [mask.as_ref()], [0, 0])
    }
}
