mod cc;
use super::packages;
use crate::Registry;
use crate::effects::spatial;

pub(crate) fn register(registry: &mut Registry) {
    cc::register(registry);
    packages(
        registry,
        &[embedded!("bevel-alpha", ["shaders/bevel_alpha.wgsl"])],
    );
    registry.effects.push(spatial::definitions::perspective());
    registry
        .effects
        .push(spatial::stylize::radial_shadow_builtin());
}
