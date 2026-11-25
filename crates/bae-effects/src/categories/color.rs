use super::packages;
use crate::Registry;
use crate::{effects, rust_builtin};
use std::sync::Arc;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.extend(effects::ocio::entries());
    registry.prewarm.push(effects::color::prewarm);
    packages(
        registry,
        &[
            embedded!(
                "color-finishing",
                [
                    "shaders/video_limiter.wgsl",
                    "shaders/cc_toner.wgsl",
                    "shaders/cc_color_offset.wgsl",
                    "shaders/cc_kernel.wgsl"
                ]
            ),
            embedded!("black-white", ["shaders/black_white.wgsl"]),
            embedded!("brightness-contrast", ["shaders/brightness_contrast.wgsl"]),
            embedded!(
                "color-adjustments",
                [
                    "shaders/channel_mixer.wgsl",
                    "shaders/change_color.wgsl",
                    "shaders/change_to_color.wgsl",
                    "shaders/color_balance_hls.wgsl",
                    "shaders/hls.wgsl"
                ]
            ),
            embedded!("color-balance", ["shaders/color_balance.wgsl"]),
            embedded!(
                "color-selection",
                [
                    "shaders/gamma_pedestal_gain.wgsl",
                    "shaders/leave_color.wgsl",
                    "shaders/broadcast_colors.wgsl",
                    "shaders/selective_color.wgsl",
                ]
            ),
            embedded!("exposure", ["shaders/exposure.wgsl"]),
            embedded!("hue-saturation", ["shaders/hue_saturation.wgsl"]),
            embedded!("levels", ["shaders/levels.wgsl"]),
            embedded!("photo-filter", ["shaders/photo_filter.wgsl"]),
            embedded!("pro-levels", ["shaders/pro_levels.wgsl"]),
            embedded!("tint", ["shaders/tint.wgsl"]),
            embedded!("tritone", ["shaders/tritone.wgsl"]),
            embedded!("vibrance", ["shaders/vibrance.wgsl"]),
        ],
    );
    registry.effects.extend(effects::color::auto::builtins());
    registry.effects.push(effects::color::colorama::builtin());
    registry.effects.push(effects::color::lumetri::builtin());
    let (curves, zh) = effects::color::Curves::new();
    registry.effects.push(rust_builtin(
        (Arc::new(curves), zh),
        "curves",
        "曲线",
        &["curve", "曲线"],
    ));
    let (shadow, zh) = effects::color::ShadowHighlight::new();
    registry.effects.push(rust_builtin(
        (Arc::new(shadow), zh),
        "shadow-highlight",
        "阴影/高光",
        &["shadows", "highlights", "阴影", "高光"],
    ));
}
