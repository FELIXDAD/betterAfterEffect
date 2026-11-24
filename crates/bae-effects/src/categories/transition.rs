use crate::Registry;

use super::packages;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.push(crate::effects::compound::gradient());
    packages(
        registry,
        &[
            embedded!("block-dissolve", ["shaders/block_dissolve.wgsl"]),
            embedded!("linear-wipe", ["shaders/linear_wipe.wgsl"]),
            embedded!("radial-wipe", ["shaders/radial_wipe.wgsl"]),
            embedded!("venetian-blinds", ["shaders/venetian_blinds.wgsl"]),
        ],
    )
}
