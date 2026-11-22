//! BAE text (workstream 05): AE text layers laid out with parley, drawn with Vello.
//!
//! ```text
//! TextDocument (bae-model, the Source Text value at this time)
//!     ──► layout_text (fonts.rs + layout.rs: parley shaping, fallback, line breaking,
//!         AE placement) ──► TextLayout (glyph runs, lines, characters, bounds)
//!     ──► render::encode (Vello) ──► render::TextSource (bae-render LayerSource)
//! ```
//!
//! Public interface:
//!
//! * [`FontSystem`] – system font enumeration ([`FontSystem::families`],
//!   [`FontSystem::faces`]), matching a stored PostScript name
//!   ([`FontSystem::resolve`]), fallback locale (Simplified Chinese by default).
//! * [`layout_text`] → [`TextLayout`]: the result type — positioned
//!   [`GlyphRun`]s, [`TextLine`]s, per-character [`TextChar`]s (for animators),
//!   paint, ink bounds (`sourceRectAtTime`), overset flag and font resolution.
//! * `render` (feature `render`) – Vello encoding, [`render::TextSource`] and
//!   [`render::TextProvider`] ([`TEXT_PROVIDER`]).
//!
//! Layer space as AE: the origin is the start of the first baseline, y down,
//! font size in layer pixels. Without the `render` feature this crate has no GPU
//! dependency; fonts come from the OS through fontique.

pub mod fonts;
pub mod layout;
#[cfg(feature = "render")]
pub mod render;

pub use fonts::{DEFAULT_LOCALE, FontFace, FontMatch, FontSystem};
pub use layout::{
    CharKind, GlyphRun, Justification, PositionedGlyph, TextChar, TextLayout, TextLine, TextPaint,
    TextStroke, layout_text,
};

/// [`bae_render::SourceProvider`] ID for text layers. `CustomContent::data` holds
/// an `Arc<bae_anim::EvaluatedLayer>` (the text layer as 03 evaluated it), an
/// `Arc<bae_model::TextDocument>` or a ready `Arc<TextLayout>`.
pub const TEXT_PROVIDER: &str = "bae.text";
