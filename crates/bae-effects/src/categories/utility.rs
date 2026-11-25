pub(crate) mod aliases;
use crate::{Registry, effects};
pub(crate) fn register(registry: &mut Registry) {
    registry.effects.extend(aliases::entries());
    registry.effects.push(effects::lut::builtin());
}
