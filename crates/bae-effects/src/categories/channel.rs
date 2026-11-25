use super::packages;
use crate::Registry;
use crate::effects;

pub(crate) fn register(registry: &mut Registry) {
    registry
        .effects
        .extend(crate::effects::channel_filters::entries());
    registry
        .prewarm
        .push(crate::effects::channel_filters::prewarm);
    registry.prewarm.push(effects::prewarm);
    packages(
        registry,
        &[
            embedded!(
                "channel-composite",
                [
                    "shaders/remove_color_matting.wgsl",
                    "shaders/solid_composite.wgsl"
                ]
            ),
            embedded!("invert", ["shaders/invert.wgsl"]),
            embedded!("shift-channels", ["shaders/shift_channels.wgsl"]),
        ],
    );
    registry.effects.push(effects::set_matte::builtin());
    registry.effects.push(effects::morphology::minimax());
}
