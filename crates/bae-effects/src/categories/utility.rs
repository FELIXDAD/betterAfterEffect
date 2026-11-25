use crate::{Registry, effects};
pub(crate) fn register(registry: &mut Registry) {
    registry.effects.extend(effects::utility_aliases::entries());
    registry.effects.push(effects::lut::builtin());
}
