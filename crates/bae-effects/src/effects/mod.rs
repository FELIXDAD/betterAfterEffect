//! Built-in effects implemented in Rust (`bae_render::Effect` + own kernels):
//! the ones whose pass structure depends on parameters (blur pyramids,
//! iterations) or that need resources declarative effects cannot declare.

pub(crate) mod audio_visual;
pub mod blur;
pub(crate) mod controls;
mod image_grid;
mod layer_input;
pub(crate) mod lut;
pub(crate) mod morphology;
pub(crate) mod set_matte;

pub use blur::{BoxBlur, GaussianBlur};

pub(crate) mod color;
pub(crate) mod compound;
pub mod distort;
pub mod spatial;
pub(crate) mod temporal;

pub(crate) fn prewarm(
    rt: &crate::gpu::Runtime,
    format: bae_gpu::wgpu::TextureFormat,
) -> Result<(), bae_render::RenderError> {
    set_matte::prewarm(rt, format)?;
    morphology::prewarm(rt, format)
}

/// Localized dropdown options for the Color Correction Rust effects.
pub fn options_zh(match_name: &str, param_id: u32) -> Option<&'static [&'static str]> {
    match (match_name, param_id) {
        ("ADBE AudSpect", 20) => Some(&["数字", "模拟谱线", "模拟频点"]),
        ("ADBE AudSpect", 21) => Some(&["A 面", "B 面", "A 和 B 面"]),
        ("APC Colorama", id) => color::colorama_definition::options_zh(id),
        ("ADBE Equalize", 1) => Some(&["RGB", "亮度", "Photoshop 样式"]),
        ("ADBE CurvesCustom", 2) => Some(&["RGB", "红色", "绿色", "蓝色", "Alpha"]),
        _ => None,
    }
}
