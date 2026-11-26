pub(crate) mod aliases;
use crate::{Registry, effects};
pub(crate) fn register(registry: &mut Registry) {
    super::packages(
        registry,
        &[embedded!("utility", ["shaders/cc_overbrights.wgsl"])],
    );
    registry.effects.extend(aliases::entries());
    registry.effects.push(effects::lut::builtin());
}
