use crate::Registry;
use crate::effects;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.push(crate::effects::keying::builtin(
        crate::effects::keying::Kind::Soft,
    ));
    registry.effects.push(crate::effects::keying::builtin(
        crate::effects::keying::Kind::Hard,
    ));
    registry.effects.push(effects::morphology::simple_choker());
    registry.effects.push(effects::spatial::choker::builtin());
}
