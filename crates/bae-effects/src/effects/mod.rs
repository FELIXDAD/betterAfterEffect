//! Built-in effects implemented in Rust (`bae_render::Effect` + own kernels):
//! the ones whose pass structure depends on parameters (blur pyramids,
//! iterations) or that need resources declarative effects cannot declare.

pub mod blur;
pub(crate) mod image_grid;
pub(crate) mod layer_input;
pub(crate) mod lut;
pub(crate) mod morphology;
pub(crate) mod sampled;

pub use blur::{BoxBlur, GaussianBlur};

pub(crate) mod color;
pub(crate) mod compound;
pub mod distort;
pub mod liquify;
pub mod puppet;
pub mod spatial;
pub(crate) mod temporal;

/// Localized dropdown options for the Color Correction Rust effects.
pub fn options_zh(match_name: &str, param_id: u32) -> Option<&'static [&'static str]> {
    match (match_name, param_id) {
        ("ADBE Aud HiLo", 1) => Some(&["高通", "低通"]),
        ("APC Colorama", id) => color::colorama_definition::options_zh(id),
        ("ADBE Equalize", 1) => Some(&["RGB", "亮度", "Photoshop 样式"]),
        ("ADBE CurvesCustom", 2) => Some(&["RGB", "红色", "绿色", "蓝色", "Alpha"]),
        _ => None,
    }
}
