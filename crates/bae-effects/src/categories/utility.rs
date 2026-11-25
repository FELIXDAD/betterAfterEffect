use crate::{Registry, effects};
pub(crate) fn register(registry: &mut Registry) {
    registry.effects.push(effects::lut::builtin());
}
