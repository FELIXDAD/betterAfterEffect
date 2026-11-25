use super::packages;
use crate::Registry;
use crate::effects;

pub(crate) fn register(registry: &mut Registry) {
    registry.prewarm.push(effects::prewarm);
    packages(
        registry,
        &[
            embedded!("invert", ["shaders/invert.wgsl"]),
            embedded!("shift-channels", ["shaders/shift_channels.wgsl"]),
        ],
    );
    registry.effects.push(effects::set_matte::builtin());
    registry.effects.push(effects::morphology::minimax());
}
