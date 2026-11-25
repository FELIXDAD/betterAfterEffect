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
const CC_KALEIDA: &str = concat!(
    include_str!("../sampled.wgsl"),
    include_str!("cc_kaleida.wgsl")
);
const CC_REPETILE: &str = concat!(
    include_str!("../sampled.wgsl"),
    include_str!("cc_repetile.wgsl")
);
const CC_GLASS: &str = concat!(
    include_str!("../sampled.wgsl"),
    include_str!("cc_glass.wgsl")
);

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for (label, shader) in [
        ("bae.effects/cc_kaleida", CC_KALEIDA),
        ("bae.effects/cc_repetile", CC_REPETILE),
        ("bae.effects/cc_glass", CC_GLASS),
    ] {
        image_grid::kernel_with_inputs(rt, format, label, shader, UNIFORM, 5)?;
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
