use super::keying::keyer;
use crate::Registry;
use crate::effects;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.push(keyer::builtin(keyer::Kind::Soft));
    registry.effects.push(keyer::builtin(keyer::Kind::Hard));
    registry.effects.push(effects::morphology::simple_choker());
    registry.effects.push(effects::spatial::choker::builtin());
}
