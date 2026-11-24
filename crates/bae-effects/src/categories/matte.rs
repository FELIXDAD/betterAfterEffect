use crate::Registry;
use crate::effects;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.push(effects::morphology::simple_choker());
    registry.effects.push(effects::spatial::choker::builtin());
}
