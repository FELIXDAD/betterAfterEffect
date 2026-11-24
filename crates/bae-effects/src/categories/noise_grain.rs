use crate::Registry;

use super::packages;

pub(crate) fn register(registry: &mut Registry) {
    packages(
        registry,
        &[
            embedded!("add-grain", ["shaders/add_grain.wgsl"]),
            embedded!("fractal-noise", ["shaders/fractal_noise.wgsl"]),
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
