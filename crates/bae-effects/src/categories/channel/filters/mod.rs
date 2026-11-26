//! Layer-space GPU filters and their AE parameter mappings.
mod definitions;
use crate::effects::sampled::{Sampled, Shader, unchanged};
use crate::{Builtin, gpu, rust_builtin};
use bae_gpu::wgpu;
use bae_render::RenderError;
pub(crate) use definitions::entries;
use std::sync::Arc;
static SET_CHANNELS: Shader = Shader {
    label: "bae.effects/set_channels",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("../../../shaders/layer_sampling.wgsl"),
        include_str!("set_channels.wgsl")
    ),
};
static CHANNEL_COMBINER: Shader = Shader {
    label: "bae.effects/channel_combiner",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("../../../shaders/layer_sampling.wgsl"),
        include_str!("channel_combiner.wgsl")
    ),
};
static BLEND: Shader = Shader {
    label: "bae.effects/channel_blend",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("../../../shaders/layer_sampling.wgsl"),
        include_str!("../../../shaders/generated_blend.wgsl"),
        include_str!("blend.wgsl")
    ),
};

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for shader in [&CHANNEL_COMBINER, &SET_CHANNELS, &BLEND] {
        shader.kernel(rt, format)?;
    }
    Ok(())
}
