use crate::{Registry, effects::generate};

use super::packages;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.push(crate::effects::compound::fill());
    registry.effects.extend([
        generate::stroke(),
        generate::write_on(),
        generate::scribble(),
        generate::vegas(),
    ]);
    registry.prewarm.push(generate::prewarm);
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
