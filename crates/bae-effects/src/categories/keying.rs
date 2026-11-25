use super::packages;
use crate::Registry;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.push(crate::effects::keying::builtin(
        crate::effects::keying::Kind::Keylight,
    ));
    registry.effects.push(crate::effects::keying::builtin(
        crate::effects::keying::Kind::Cleaner,
    ));
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
