use crate::Registry;
mod particles;
pub(crate) use particles::Engine;
use particles::{Kind, builtin, prewarm};

pub(crate) fn register(registry: &mut Registry) {
    registry.prewarm.push(prewarm);
    registry.effects.extend(
        [
            Kind::World,
            Kind::Systems,
            Kind::Rain,
            Kind::Snow,
            Kind::Bubbles,
            Kind::Drizzle,
            Kind::Stars,
            Kind::Scatter,
            Kind::Polly,
            Kind::Shatter,
            Kind::Cards,
            Kind::Playground,
            Kind::Ball,
            Kind::Mercury,
            Kind::Hair,
            Kind::Foam,
            Kind::Wave,
            Kind::Caustics,
        ]
        .map(builtin),
    );
}

pub use particles::projection::{particle_scene_projection, particle_world_projection};
