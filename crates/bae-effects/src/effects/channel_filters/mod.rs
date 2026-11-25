//! Layer-space GPU filters and their AE parameter mappings.
mod definitions;
use super::{
    image_grid,
    sampled::{Sampled, UNIFORM, unchanged},
};
use crate::{Builtin, gpu, rust_builtin};
use bae_gpu::wgpu;
use bae_render::RenderError;
pub(crate) use definitions::entries;
use std::sync::Arc;
const SET_CHANNELS: &str = concat!(
    include_str!("../sampled.wgsl"),
    include_str!("set_channels.wgsl")
);
const CHANNEL_COMBINER: &str = concat!(
    include_str!("../sampled.wgsl"),
    include_str!("channel_combiner.wgsl")
);
const BLEND: &str = concat!(
    include_str!("../sampled.wgsl"),
    include_str!("../../shaders/generated_blend.wgsl"),
    include_str!("blend.wgsl")
);

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for (label, shader) in [
        ("bae.effects/channel_combiner", CHANNEL_COMBINER),
        ("bae.effects/set_channels", SET_CHANNELS),
        ("bae.effects/channel_blend", BLEND),
    ] {
        image_grid::kernel_with_inputs(rt, format, label, shader, UNIFORM, 5)?;
    }
    Ok(())
}
