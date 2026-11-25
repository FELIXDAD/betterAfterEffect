use crate::Registry;

use super::packages;

pub(crate) fn register(registry: &mut Registry) {
    use crate::effects::generate::*;
    registry.effects.extend([
        radio_waves(),
        paint_bucket(),
        cc_light_rays(),
        cc_light_burst_2_5(),
    ]);
    registry.prewarm.push(prewarm);
    registry.effects.push(crate::effects::compound::fill());
    packages(
        registry,
        &[
            embedded!(
                "generate",
                [
                    "shaders/lens_flare.wgsl",
                    "shaders/beam.wgsl",
                    "shaders/advanced_lightning.wgsl",
                    "shaders/eyedropper_fill.wgsl",
                    "shaders/cc_light_sweep.wgsl"
                ]
            ),
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
