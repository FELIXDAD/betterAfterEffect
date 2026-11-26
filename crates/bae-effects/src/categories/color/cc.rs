use crate::{
    Builtin, EffectTarget, Registry, TargetEffect,
    catalog::DefBuilder,
    effects::{color::auto, layer_input, sampled::Shader},
    gpu,
    params::Params,
    rust_builtin,
};
use bae_gpu::wgpu;
use bae_model::EffectDef;
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};
use std::sync::Arc;

static SHADER: Shader = Shader {
    label: "bae.effects.color.cc",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("../../shaders/generated_blend.wgsl"),
        include_str!("cc.wgsl")
    ),
};

pub(super) fn register(registry: &mut Registry) {
    registry.prewarm.push(prewarm);
    registry.effects.extend([color_link()]);
}

fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    SHADER.kernel(rt, format)?;
    Ok(())
}

fn color_link() -> Builtin {
    let mut b = DefBuilder::new("ADBE Color Link", "Color Link", "Color Correction");
    b = b.layer(1, "Source Layer", "源图层").layer_stage(1);
    b = b.popup(
        2,
        "Sample",
        "示例",
        1,
        &[
            "Average",
            "Median",
            "Brightest",
            "Darkest",
            "Max RGB",
            "Min RGB",
            "(-",
            "Alpha Average",
            "Alpha Median",
            "Alpha Max",
            "Alpha Min",
        ],
    );
    b = b.slider(
        6,
        "Clip (%)",
        "剪切 (%)",
        5.0,
        [0.0, 50.0],
        [0.0, 50.0],
        2,
        "none",
    );
    b = b.checkbox(3, "Stencil Original Alpha", "模板原始 Alpha", false);
    b = b.slider(
        4,
        "Opacity",
        "不透明度",
        100.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "percent",
    );
    b = b.popup(
        5,
        "Blending Mode",
        "混合模式",
        1,
        &[
            "Normal",
            "(-",
            "Add",
            "Multiply",
            "Screen",
            "Overlay",
            "Soft Light",
            "Hard Light",
            "(-",
            "Color Dodge",
            "Color Burn",
            "(-",
            "Darken",
            "Lighten",
            "Difference",
            "Exclusion",
            "(-",
            "Hue",
            "Saturation",
            "Color",
            "Luminosity",
        ],
    );
    let (def, labels) = b.reference_points().build();
    let mut builtin = rust_builtin(
        (Arc::new(ColorLink(def)), labels),
        "color-link",
        "颜色链接",
        &["Color Link", "颜色校正"],
    );
    builtin.info.param_options_zh.push((
        2,
        [
            "平均值",
            "中间值",
            "最亮值",
            "最暗值",
            "RGB 最大值",
            "RGB 最小值",
            "(-",
            "Alpha 平均值",
            "Alpha 中间值",
            "Alpha 最大值",
            "Alpha 最小值",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    ));
    builtin.info.param_options_zh.push((
        5,
        [
            "正常",
            "(-",
            "相加",
            "相乘",
            "滤色",
            "叠加",
            "柔光",
            "强光",
            "(-",
            "颜色减淡",
            "颜色加深",
            "(-",
            "变暗",
            "变亮",
            "差值",
            "排除",
            "(-",
            "色相",
            "饱和度",
            "颜色",
            "发光度",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    ));
    builtin
}

struct ColorLink(EffectDef);

impl Effect for ColorLink {
    fn def(&self) -> &EffectDef {
        &self.0
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        layer_input::dependency(p, 1, true).into_iter().collect()
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
impl TargetEffect for ColorLink {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.0, p);
        if roi.is_empty() || v.f(4) == 0. {
            return Ok(input.clone());
        }
        let map = layer_input::dependency(p, 1, true)
            .and_then(|d| target.effect_layer(&d).cloned())
            .or_else(|| (p.layer_bounds.is_empty() && p.value(1).is_none()).then(|| input.clone()));
        let Some(map) = map else {
            return Ok(input.clone());
        };
        let color = auto::linked_color(target, &map, v.popup(2) as u32, (v.f(6) * 0.01) as f32)?;
        SHADER.dispatch(target, v, roi, input, [Some(&color)], [0, 0])
    }
}
