use super::packages;
use crate::Registry;
use crate::effects::spatial;

pub(crate) fn register(registry: &mut Registry) {
    packages(
        registry,
        &[
            embedded!("find-edges", ["shaders/find_edges.wgsl"]),
            embedded!("posterize", ["shaders/posterize.wgsl"]),
        ],
    );
    registry.effects.extend(spatial::definitions::stylize());
}
