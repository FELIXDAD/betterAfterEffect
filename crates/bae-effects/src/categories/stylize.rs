use super::packages;
use crate::Registry;
use crate::effects::spatial;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.push(crate::effects::compound::texturize());
    packages(
        registry,
        &[
            embedded!("find-edges", ["shaders/find_edges.wgsl"]),
            embedded!("posterize", ["shaders/posterize.wgsl"]),
            embedded!("threshold", ["shaders/threshold.wgsl"]),
            embedded!("strobe", ["shaders/strobe.wgsl"]),
        ],
    );
    registry.effects.extend(spatial::definitions::stylize());
    registry
        .effects
        .extend(spatial::stylize::stylize_builtins());
}
