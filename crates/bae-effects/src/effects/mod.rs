//! Built-in effects implemented in Rust (`bae_render::Effect` + own kernels):
//! the ones whose pass structure depends on parameters (blur pyramids,
//! iterations) or that need resources declarative effects cannot declare.

pub mod blur;
pub(crate) mod controls;
mod image_grid;
pub(crate) mod morphology;
pub(crate) mod set_matte;

pub use blur::{BoxBlur, GaussianBlur};

pub mod spatial;

pub(crate) fn prewarm(
    rt: &crate::gpu::Runtime,
    format: bae_gpu::wgpu::TextureFormat,
) -> Result<(), bae_render::RenderError> {
    set_matte::prewarm(rt, format)?;
    morphology::prewarm(rt, format)
}
