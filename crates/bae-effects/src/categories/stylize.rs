mod filters;
use super::packages;
use crate::Registry;
use crate::effects::spatial;

pub(crate) fn register(registry: &mut Registry) {
    registry.effects.extend(filters::entries());
    registry.prewarm.push(filters::prewarm);
    registry
        .effects
        .push(crate::effects::color::lumetri::cc_vignette());
    registry.effects.push(crate::effects::compound::texturize());
    packages(
        registry,
        &[
            embedded!("find-edges", ["shaders/find_edges.wgsl"]),
            embedded!("posterize", ["shaders/posterize.wgsl"]),
            embedded!(
                "threshold",
                [
                    "shaders/threshold.wgsl",
                    "shaders/threshold_math.wgsl",
                    "shaders/cc_threshold.wgsl",
                    "shaders/cc_threshold_rgb.wgsl"
                ]
            ),
            embedded!(
                "stylize-brush",
                ["shaders/scatter.wgsl", "shaders/brush_strokes.wgsl"]
            ),
            embedded!("strobe", ["shaders/strobe.wgsl"]),
        ],
    );
    registry.effects.extend(spatial::definitions::stylize());
    registry
        .effects
        .extend(spatial::stylize::stylize_builtins());
}
