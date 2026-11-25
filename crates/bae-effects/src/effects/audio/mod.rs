//! Audio effects expose their controls in the effect stack; bae-media processes PCM.
use crate::{Builtin, catalog::DefBuilder, effects::controls::Control, rust_builtin};
use std::sync::Arc;

fn audio_control(slug: &str, zh: &str, builder: DefBuilder, static_params: &[u32]) -> Builtin {
    let (mut def, names) = builder.build();
    def.gpu = false;
    for param in &mut def.params {
        if static_params.contains(&param.id) {
            param.flags.cannot_animate = true;
            param.flags.cannot_time_vary = true;
        }
    }
    rust_builtin((Arc::new(Control(def)), names), slug, zh, &["audio", zh])
}

mod reverse;
pub(crate) fn register(registry: &mut crate::Registry) {
    registry.effects.extend([reverse::effect()]);
}
