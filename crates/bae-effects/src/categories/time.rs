use crate::{Registry, effects::temporal};

pub(crate) fn register(registry: &mut Registry) {
    registry.prewarm.push(temporal::prewarm);
    registry.effects.extend(temporal::builtins());
}
