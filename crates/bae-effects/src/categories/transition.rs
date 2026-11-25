use crate::Registry;

use super::packages;

pub(crate) fn register(registry: &mut Registry) {
    use crate::effects::transition::{card_wipe, cc_image_wipe, prewarm};
    registry.effects.extend([card_wipe(), cc_image_wipe()]);
    registry.prewarm.push(prewarm);
    registry.effects.push(crate::effects::compound::gradient());
    packages(
        registry,
        &[
            embedded!(
                "transition",
                [
                    "shaders/iris_wipe.wgsl",
                    "shaders/cc_grid_wipe.wgsl",
                    "shaders/cc_light_wipe.wgsl",
                    "shaders/cc_radial_scalewipe.wgsl",
                    "shaders/cc_scale_wipe.wgsl"
                ]
            ),
            embedded!("block-dissolve", ["shaders/block_dissolve.wgsl"]),
            embedded!("linear-wipe", ["shaders/linear_wipe.wgsl"]),
            embedded!("radial-wipe", ["shaders/radial_wipe.wgsl"]),
            embedded!("venetian-blinds", ["shaders/venetian_blinds.wgsl"]),
        ],
    )
}
