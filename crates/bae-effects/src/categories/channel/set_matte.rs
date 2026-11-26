//! Set Matte (ADBE Set Matte3).

use crate::{
    Builtin,
    catalog::DefBuilder,
    effects::sampled::{Sampled, Shader, unchanged},
    gpu, rust_builtin,
};
use bae_gpu::wgpu;
use bae_render::RenderError;
use std::sync::Arc;

const MATCH_NAME: &str = "ADBE Set Matte3";
static MATTE: Shader = Shader {
    label: "bae.effects.set_matte",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("set_matte.wgsl")
    ),
};

pub(crate) fn builtin() -> Builtin {
    let (def, zh) = DefBuilder::new(MATCH_NAME, "Set Matte", "Channel")
        .layer(1, "Take Matte From Layer", "从图层获取遮罩")
        .layer_stage(1)
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
        (
            Arc::new(Sampled {
                lights: None,
                def,
                shader: &MATTE,
                mode: 0,
                inputs: &[(1, true)],
                read: unchanged,
                bounds: unchanged,
            }),
            zh,
        ),
        "set-matte",
        "设置遮罩",
        &["matte", "alpha", "channel", "遮罩", "通道"],
    )
}

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    MATTE.kernel(rt, format)?;
    Ok(())
}
