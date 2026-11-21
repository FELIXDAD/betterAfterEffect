//! Helpers for tests that run the UI headless, in this crate and in the crates built
//! on it.
//!
//! egui's bundled fonts are off (see [`theme::system_ui_fonts`]), so an
//! `egui::Context::default()` has no fonts: text lays out empty, and a text edit's
//! selection collapses to nothing. Tests take their context from [`ctx_with_fonts`]
//! instead, which gets the fonts the app's window gets.

use crate::theme;

/// An egui context with the UI's system fonts installed, as the app installs them
/// before its first frame ([`theme::install_fonts`]).
///
/// The fonts load on the calling thread (once per process, cached), not on the
/// background thread the app starts with [`theme::prefetch_system_ui_fonts`], so
/// the first pass run on the returned context already has them.
pub fn ctx_with_fonts() -> egui::Context {
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    ctx
}

#[cfg(test)]
mod tests {
    fn first_pass_text_width(ctx: &egui::Context) -> f32 {
        let mut width = 0.0;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            width = ui.label("Ag 中").rect.width();
        });
        out.textures_delta.clear();
        width
    }

    #[test]
    fn the_first_pass_lays_text_out_with_the_ui_fonts() {
        let width = first_pass_text_width(&super::ctx_with_fonts());
        assert!(width > 10.0, "text laid out {width} wide");
    }

    /// What the helper is for: without it a test's text has no width.
    #[test]
    fn a_default_context_lays_text_out_empty() {
        let width = first_pass_text_width(&egui::Context::default());
        assert_eq!(width, 0.0);
    }
}
