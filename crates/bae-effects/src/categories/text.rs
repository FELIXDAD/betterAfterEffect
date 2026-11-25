use crate::{Registry, effects::text};

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.extend([text::numbers(), text::timecode()]);
    registry.prewarm.push(text::prewarm);
}
