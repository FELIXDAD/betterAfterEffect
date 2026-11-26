//! Layer-space GPU filters and their AE parameter mappings.
mod definitions;
use crate::effects::sampled::{Sampled, Shader, unchanged};
use crate::{Builtin, gpu, rust_builtin};
use bae_gpu::wgpu;
use bae_render::RenderError;
pub(crate) use definitions::entries;
use std::sync::Arc;
static CC_SPHERE: Shader = Shader {
    label: "bae.effects/cc_sphere",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("../../../shaders/layer_sampling.wgsl"),
        include_str!("projection.wgsl"),
        include_str!("cc_sphere.wgsl")
    ),
};
static CC_CYLINDER: Shader = Shader {
    label: "bae.effects/cc_cylinder",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("../../../shaders/layer_sampling.wgsl"),
        include_str!("projection.wgsl"),
        include_str!("cc_cylinder.wgsl")
    ),
};
static CC_SPOTLIGHT: Shader = Shader {
    label: "bae.effects/cc_spotlight",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("../../../shaders/layer_sampling.wgsl"),
        include_str!("cc_spotlight.wgsl")
    ),
};

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for shader in [&CC_SPHERE, &CC_CYLINDER, &CC_SPOTLIGHT] {
        shader.kernel(rt, format)?;
    }
    Ok(())
}

fn source_bounds(v: crate::params::Params<'_>, roi: bae_render::IRect) -> bae_render::IRect {
    let bounds = crate::effects::image_grid::layer_rect(v.p);
    if bounds.is_empty() { roi } else { bounds }
}
