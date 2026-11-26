//! Audio effects expose their controls in the effect stack; bae-media processes PCM.
use crate::{Builtin, catalog::DefBuilder, categories::controls::Control, rust_builtin};
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

mod delay;
mod eq;
mod filter;
mod processing;
mod reverb;
mod reverse;
mod stereo;
pub(crate) fn register(registry: &mut crate::Registry) {
    registry.effects.extend([
        stereo::effect(),
        reverse::effect(),
        filter::effect(),
        eq::effect(),
        delay::effect(),
        reverb::effect(),
        eq::alias(),
        processing::compressor(),
        processing::gate(),
        processing::distortion(),
        processing::modulator(),
        processing::tone(),
        processing::flange_chorus(),
    ]);
}
