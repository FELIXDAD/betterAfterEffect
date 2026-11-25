use crate::Registry;

use super::packages;

pub(crate) fn register(registry: &mut Registry) {
    registry.prewarm.push(crate::effects::grain::prewarm);
    registry.effects.extend(crate::effects::grain::builtins());
    packages(
        registry,
        &[
            embedded!(
                "add-grain",
                ["shaders/add_grain.wgsl", "shaders/field.wgsl"]
            ),
            embedded!(
                "fractal-noise",
                [
                    "shaders/fractal_noise.wgsl",
                    "shaders/fractal.wgsl",
                    "shaders/turbulent_noise.wgsl"
                ]
            ),
            embedded!(
                "noise-variants",
                [
                    "shaders/noise_alpha.wgsl",
                    "shaders/hls.wgsl",
                    "shaders/noise_hls.wgsl",
                    "shaders/noise_hls_auto.wgsl"
                ]
            ),
            embedded!("noise", ["shaders/noise.wgsl"]),
            embedded!(
                "rank-filters",
                [
                    "shaders/dust_scratches.wgsl",
                    "shaders/rank.wgsl",
                    "shaders/median.wgsl",
                ]
            ),
        ],
    )
}
