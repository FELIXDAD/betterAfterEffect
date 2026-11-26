pub(crate) mod aliases;
use crate::{Registry, effects};
pub(crate) fn register(registry: &mut Registry) {
    registry
        .prewarm
        .push(|rt, format| aliases::GROW_BOUNDS.kernel(rt, format).map(drop));
    super::packages(
        registry,
        &[embedded!("utility", ["shaders/cc_overbrights.wgsl"])],
    );
    registry.effects.extend(aliases::entries());
    registry.effects.push(effects::lut::builtin());
}
