mod basic_3d;
mod camera_tracker;
mod cc;
mod filters;
mod track_points;
use super::packages;
use crate::Registry;
use crate::effects::spatial;

pub(crate) fn register(registry: &mut Registry) {
    cc::register(registry);
    basic_3d::register(registry);
    registry.effects.push(camera_tracker::entry());
    registry.prewarm.push(track_points::prewarm);
    registry.effects.extend(filters::entries());
    registry.prewarm.push(filters::prewarm);
    packages(
        registry,
        &[
            embedded!("bevel-edges", ["shaders/bevel_edges.wgsl"]),
            embedded!("bevel-alpha", ["shaders/bevel_alpha.wgsl"]),
        ],
    );
    registry.effects.push(spatial::definitions::perspective());
    registry
        .effects
        .push(spatial::stylize::radial_shadow_builtin());
}
