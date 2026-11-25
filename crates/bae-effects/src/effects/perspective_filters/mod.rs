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
const CC_SPHERE: &str = concat!(
    include_str!("../sampled.wgsl"),
    include_str!("projection.wgsl"),
    include_str!("cc_sphere.wgsl")
);
const CC_CYLINDER: &str = concat!(
    include_str!("../sampled.wgsl"),
    include_str!("projection.wgsl"),
    include_str!("cc_cylinder.wgsl")
);
const CC_SPOTLIGHT: &str = concat!(
    include_str!("../sampled.wgsl"),
    include_str!("cc_spotlight.wgsl")
);

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for (label, shader) in [
        ("bae.effects/cc_sphere", CC_SPHERE),
        ("bae.effects/cc_cylinder", CC_CYLINDER),
        ("bae.effects/cc_spotlight", CC_SPOTLIGHT),
    ] {
        image_grid::kernel_with_inputs(rt, format, label, shader, UNIFORM, 5)?;
    }
    Ok(())
}
