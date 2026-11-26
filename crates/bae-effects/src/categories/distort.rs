mod cc;

use crate::{Registry, effects};

pub(crate) fn register(registry: &mut Registry) {
    cc::register(registry);
    registry.prewarm.push(effects::distort::prewarm);
    registry.effects.extend(effects::distort::builtins());
    registry.effects.extend(effects::distort::warps::entries());
    registry.prewarm.push(effects::distort::warps::prewarm);
    registry.effects.push(effects::compound::displacement());
    registry.effects.push(effects::liquify::liquify());
    registry.prewarm.push(effects::liquify::prewarm);
}
