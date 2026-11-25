use crate::Registry;
use crate::effects;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.extend(effects::controls::builtins());
}
