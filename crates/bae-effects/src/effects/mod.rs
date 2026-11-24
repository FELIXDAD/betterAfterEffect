//! Built-in effects implemented in Rust (`bae_render::Effect` + own kernels):
//! the ones whose pass structure depends on parameters (blur pyramids,
//! iterations) or that need resources declarative effects cannot declare.

pub mod blur;

pub use blur::{BoxBlur, GaussianBlur};

pub(crate) mod color;
pub mod spatial;

/// Localized dropdown options for the Color Correction Rust effects.
pub fn options_zh(match_name: &str, param_id: u32) -> Option<&'static [&'static str]> {
    match (match_name, param_id) {
        ("ADBE CurvesCustom", 2) => Some(&["RGB", "红色", "绿色", "蓝色", "Alpha"]),
        _ => None,
    }
}
