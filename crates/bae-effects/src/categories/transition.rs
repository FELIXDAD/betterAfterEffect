mod cc;

use crate::Registry;

use super::packages;

pub(crate) fn register(registry: &mut Registry) {
    super::packages(
        registry,
        &[embedded!(
            "transition",
            ["shaders/cc_jaws.wgsl", "shaders/cc_line_sweep.wgsl"]
        )],
    );
    cc::register(registry);
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
