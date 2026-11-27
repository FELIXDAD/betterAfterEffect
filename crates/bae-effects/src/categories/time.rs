mod time_blend;

use crate::{Registry, effects::temporal};

pub(crate) fn register(registry: &mut Registry) {
    time_blend::register(registry);
    registry.prewarm.push(temporal::prewarm);
    registry.effects.extend(temporal::builtins());
}
