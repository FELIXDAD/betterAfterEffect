use crate::Registry;

use super::packages;

pub(crate) fn register(registry: &mut Registry) {
    crate::effects::audio_visual::register(registry);
    registry.effects.push(crate::effects::compound::fill());
    packages(
        registry,
        &[
            embedded!("gradient-ramp", ["shaders/gradient_ramp.wgsl"]),
            embedded!(
                "procedural",
                [
                    "shaders/four_color_gradient.wgsl",
                    "shaders/grid.wgsl",
                    "shaders/cell_pattern.wgsl",
                    "shaders/checkerboard.wgsl",
                    "shaders/circle.wgsl",
                    "shaders/ellipse.wgsl",
                ]
            ),
        ],
    )
}
