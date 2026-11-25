use super::packages;
use crate::Registry;
use crate::effects::spatial;

pub(crate) fn register(registry: &mut Registry) {
    registry
        .effects
        .extend(crate::effects::perspective_filters::entries());
    registry
        .prewarm
        .push(crate::effects::perspective_filters::prewarm);
    packages(
        registry,
        &[
            embedded!("bevel-edges", ["shaders/bevel_edges.wgsl"]),
            embedded!("bevel-alpha", ["shaders/bevel_alpha.wgsl"]),
        ],
    );
    registry.effects.push(spatial::definitions::perspective());
    registry
        .effects
        .push(spatial::stylize::radial_shadow_builtin());
}
