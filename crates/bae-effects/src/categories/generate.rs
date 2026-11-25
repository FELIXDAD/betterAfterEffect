pub(crate) mod strokes;
use crate::Registry;

use super::packages;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.push(crate::effects::compound::fill());
    registry.effects.extend([
        strokes::stroke(),
        strokes::write_on(),
        strokes::paint_effect(),
        strokes::scribble(),
        strokes::vegas(),
    ]);
    registry.prewarm.push(strokes::prewarm);
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
