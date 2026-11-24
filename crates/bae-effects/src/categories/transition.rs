use crate::Registry;

use super::packages;

pub(crate) fn register(registry: &mut Registry) {
    packages(
        registry,
        &[
            embedded!("block-dissolve", ["shaders/block_dissolve.wgsl"]),
            embedded!("gradient-wipe", ["shaders/gradient_wipe.wgsl"]),
            embedded!("linear-wipe", ["shaders/linear_wipe.wgsl"]),
            embedded!("radial-wipe", ["shaders/radial_wipe.wgsl"]),
            embedded!("venetian-blinds", ["shaders/venetian_blinds.wgsl"]),
        ],
    )
}
