//! Layer content bounds for the `sourceRectAtTime` expression method. Text needs
//! font layout and shapes their geometry, which other crates own: the app
//! registers a [`SourceBounds`] once (bae-text's text provider does both).

use std::sync::{Arc, RwLock};

use bae_model::TextDocument;

use crate::EvaluatedPrim;

/// A layer-space rectangle: left, top, width, height.
pub type SourceRect = [f64; 4];

/// What `sourceRectAtTime` measures for text and shape layers.
pub trait SourceBounds: Send + Sync {
    /// The laid-out text's bounds; `include_extents` adds the stroke.
    fn text(&self, doc: &TextDocument, include_extents: bool) -> Option<SourceRect>;
    /// The drawn shapes' bounds; `include_extents` adds the strokes.
    fn shapes(&self, contents: &[EvaluatedPrim], include_extents: bool) -> Option<SourceRect>;
}

static BOUNDS: RwLock<Option<Arc<dyn SourceBounds>>> = RwLock::new(None);

/// Registers how text and shapes are measured (once, at startup).
pub fn set_source_bounds(bounds: Arc<dyn SourceBounds>) {
    *BOUNDS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(bounds);
}

pub(crate) fn source_bounds() -> Option<Arc<dyn SourceBounds>> {
    BOUNDS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}
