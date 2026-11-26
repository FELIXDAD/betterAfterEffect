mod glyphs;
mod legacy;
use crate::Registry;

pub(crate) fn register(registry: &mut Registry) {
    registry
        .effects
        .extend([glyphs::numbers(), glyphs::timecode()]);
    registry.prewarm.push(glyphs::prewarm);
    legacy::register(registry);
}
