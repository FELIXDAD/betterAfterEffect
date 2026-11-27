use super::keying::keyer;
use crate::Registry;
use crate::effects;

mod definition;
pub(crate) mod fill;
mod roto;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.push(roto::builtin());
    registry.prewarm.push(roto::prewarm);
    registry.prewarm.push(fill::prewarm);
    registry.effects.push(keyer::builtin(keyer::Kind::Soft));
    registry.effects.push(keyer::builtin(keyer::Kind::Hard));
    registry.effects.push(effects::morphology::simple_choker());
    registry.effects.push(effects::spatial::choker::builtin());
}
