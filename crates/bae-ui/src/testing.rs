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

/// For a harness that creates its own context and runs the first pass at once
/// (egui_kittest's `Harness::builder().build_ui_state(..)`), where
/// [`ctx_with_fonts`] cannot be used. Call it first in the harness's UI closure:
/// on the context's first pass it installs the UI fonts, which take effect from the
/// next pass, and discards this pass, so egui runs it again with the fonts; the
/// caller draws nothing and returns when it gets `true`. kittest's first pass has
/// no input, so the discarded pass loses no events.
pub fn first_pass_installs_fonts(ctx: &egui::Context) -> bool {
    let installed = egui::Id::new("bae_ui::testing::fonts_installed");
    if ctx.data(|d| d.get_temp::<bool>(installed)).is_some() {
        return false;
    }
    ctx.data_mut(|d| d.insert_temp(installed, true));
    theme::install_fonts(ctx);
    ctx.request_discard("bae_ui::testing: UI fonts installed");
    true
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

    /// The first pass that draws, in the run that installed the fonts, has them;
    /// later runs draw at once.
    #[test]
    fn the_first_drawn_pass_has_fonts_when_installed_on_the_first_pass() {
        let ctx = egui::Context::default();
        let mut widths = Vec::new();
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                if super::first_pass_installs_fonts(ui.ctx()) {
                    return;
                }
                widths.push(ui.label("Ag 中").rect.width());
            });
            out.textures_delta.clear();
        }
        assert_eq!(widths.len(), 2, "one drawn pass per run: {widths:?}");
        assert!(widths.iter().all(|w| *w > 10.0), "{widths:?}");
    }
}
