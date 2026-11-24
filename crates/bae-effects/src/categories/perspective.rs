use super::packages;
use crate::Registry;
use crate::effects::spatial;

pub(crate) fn register(registry: &mut Registry) {
    packages(
        registry,
        &[embedded!("bevel-alpha", ["shaders/bevel_alpha.wgsl"])],
    );
    registry.effects.push(spatial::definitions::perspective());
}
