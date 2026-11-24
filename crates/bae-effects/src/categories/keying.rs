use super::packages;
use crate::Registry;

pub(crate) fn register(registry: &mut Registry) {
    packages(
        registry,
        &[embedded!(
            "keying",
            [
                "shaders/color_difference_key.wgsl",
                "shaders/color_range.wgsl",
                "shaders/linear_color_key.wgsl",
                "shaders/advanced_spill_suppressor.wgsl",
            ]
        )],
    );
}
