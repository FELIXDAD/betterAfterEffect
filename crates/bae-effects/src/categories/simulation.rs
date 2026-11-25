use crate::Registry;
use crate::effects::simulation::{Kind, builtin, prewarm};

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
        ]
        .map(builtin),
    );
}
