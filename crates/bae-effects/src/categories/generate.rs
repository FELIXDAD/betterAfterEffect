use crate::Registry;

use super::packages;

pub(crate) fn register(registry: &mut Registry) {
    packages(
        registry,
        &[
            embedded!("fill", ["shaders/fill.wgsl"]),
            embedded!("gradient-ramp", ["shaders/gradient_ramp.wgsl"]),
            embedded!(
                "procedural",
                ["shaders/four_color_gradient.wgsl", "shaders/grid.wgsl"]
            ),
        ],
    )
}
