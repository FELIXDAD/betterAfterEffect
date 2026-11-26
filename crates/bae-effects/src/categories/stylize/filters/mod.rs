//! Layer-space GPU filters and their AE parameter mappings.
mod definitions;
use crate::effects::sampled::{Sampled, Shader, unchanged};
use crate::{Builtin, gpu, rust_builtin};
use bae_gpu::wgpu;
use bae_render::RenderError;
pub(crate) use definitions::entries;
use std::sync::Arc;
static CC_KALEIDA: Shader = Shader {
    label: "bae.effects/cc_kaleida",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("../../../shaders/layer_sampling.wgsl"),
        include_str!("cc_kaleida.wgsl")
    ),
};
static CC_REPETILE: Shader = Shader {
    label: "bae.effects/cc_repetile",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("../../../shaders/layer_sampling.wgsl"),
        include_str!("cc_repetile.wgsl")
    ),
};
static CC_GLASS: Shader = Shader {
    label: "bae.effects/cc_glass",
    source: concat!(
        include_str!("../../../effects/sampled.wgsl"),
        include_str!("../../../shaders/layer_sampling.wgsl"),
        include_str!("cc_glass.wgsl")
    ),
};

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for shader in [&CC_KALEIDA, &CC_REPETILE, &CC_GLASS] {
        shader.kernel(rt, format)?;
    }
    Ok(())
}
fn repeat_bounds(v: crate::params::Params<'_>, r: bae_render::IRect) -> bae_render::IRect {
    if r.is_empty() {
        return r;
    }
    let expand = |id| (v.f(id) * v.p.scale).ceil() as i32;
    bae_render::IRect::new(
        r.x0 - expand(2),
        r.y0 - expand(4),
        r.x1 + expand(1),
        r.y1 + expand(3),
    )
}

fn source_bounds(v: crate::params::Params<'_>, roi: bae_render::IRect) -> bae_render::IRect {
    let bounds = crate::effects::image_grid::layer_rect(v.p);
    if bounds.is_empty() { roi } else { bounds }
}

fn repeat_input(v: crate::params::Params<'_>, roi: bae_render::IRect) -> bae_render::IRect {
    let bounds = source_bounds(v, roi);
    if bounds.contains(&roi) { roi } else { bounds }
}

fn glass_input(v: crate::params::Params<'_>, roi: bae_render::IRect) -> bae_render::IRect {
    roi.inflate((v.f(6).abs() * v.p.scale).ceil() as i32)
}
