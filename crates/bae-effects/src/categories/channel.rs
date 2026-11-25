mod filters;
mod set_matte;
use super::packages;
use crate::Registry;
use crate::effects;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.extend(filters::entries());
    registry.prewarm.push(filters::prewarm);
    registry.prewarm.push(set_matte::prewarm);
    registry.prewarm.push(effects::morphology::prewarm);
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
    registry.effects.push(set_matte::builtin());
    registry.effects.push(effects::morphology::minimax());
}
