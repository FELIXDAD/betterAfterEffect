pub(crate) mod keyer;
use super::packages;
use crate::Registry;
mod cleanup;

pub(crate) fn register(registry: &mut Registry) {
    registry.prewarm.push(keyer::prewarm);
    registry.effects.push(keyer::builtin(keyer::Kind::Keylight));
    registry.effects.push(keyer::builtin(keyer::Kind::Cleaner));
    registry.effects.extend(cleanup::builtins());
    registry.prewarm.push(cleanup::prewarm);
    packages(
        registry,
        &[embedded!(
            "keying",
            [
                "shaders/color_difference_key.wgsl",
                "shaders/color_range.wgsl",
                "shaders/extract.wgsl",
                "shaders/unmult.wgsl",
                "shaders/linear_color_key.wgsl",
                "shaders/advanced_spill_suppressor.wgsl",
            ]
        )],
    );
}
