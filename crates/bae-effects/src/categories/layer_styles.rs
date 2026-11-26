//! Photoshop shading of a layer's alpha silhouette.
mod edge;
mod photoshop;

use crate::{Registry, gpu};
use bae_gpu::wgpu;
use bae_render::RenderError;

pub(crate) fn register(r: &mut Registry) {
    r.effects.extend(photoshop::entries());
    r.prewarm.push(prewarm);
}

fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    edge::SHADER.kernel(rt, format)?;
    Ok(())
}
