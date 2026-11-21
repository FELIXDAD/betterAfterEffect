//! Adobe Illustrator interchange (workstream 16).
//!
//! ```text
//! .ai (PDF compatible) / .pdf ──┐
//! bridges/ai export JSON ───────┼─► AiDocument ──plan_import──► CommandBatch ──execute──► Stage
//! clipboard (PDF / SVG) ────────┘   (doc.rs)     (import.rs)     (batch.rs)    (one undo step)
//! ```
//!
//! - [`AiDocument`]: what every reader produces — artboards, layers, groups,
//!   (compound) paths with bae-vector paints, point text — in artboard space.
//! - [`plan_import`]: the AE-style conversion ("Import as Composition" +
//!   "Create Shapes from Vector Layer"): a comp per artboard, a shape layer per
//!   Illustrator layer. Read only; returns an [`ImportPlan`] whose
//!   [`CommandBatch`] uses only `prim.create` / `attr.set` and runs as one
//!   undo step.
//! - [`import_ai`]: read a file and plan its import.
//! - [`ImportAi`] (`ai.import`): the same as a command, for menus and agents.
//!
//! Readers: `.ai` (PDF compatible) and PDF files are read by the built-in PDF
//! reader (`pdf/`, no external PDF library). The bridge JSON written by
//! `bridges/ai/bae-ai-export.jsx` in Illustrator (layer, group and artboard
//! names, locks, symbols: what the PDF does not carry) is read by
//! [`read_bridge`] (`bridge.rs`). Exports (PDF, SVG, the Illustrator rebuild
//! script) and clipboard paste come after the importers.
//!
//! No GPU, window or platform dependencies.

mod batch;
mod bridge;
mod command;
mod doc;
mod error;
mod import;
mod pdf;

use std::path::Path;

pub use batch::{BatchStep, CommandBatch};
pub use bridge::{BRIDGE_FORMAT, BridgeOptions, read_bridge, read_bridge_with};
pub use command::{ImportAi, ImportAiParams, register_commands};
pub use doc::{AiDocument, AiGroup, AiItem, AiLayer, AiPath, AiText, Artboard, SourceFormat};
pub use error::Error;
pub use import::{
    ARTBOARDS_FOLDER, ATTR_ARTBOARD, COMPS_FOLDER, ImportOptions, ImportPlan, ImportReport,
    TextMode, ae_gradient_colors, blend_token, plan_import,
};

use bae_model::Stage;

/// File extensions the importer accepts (for file dialogs and the importer registry).
pub const EXTENSIONS: &[&str] = &["ai", "pdf", "json"];

/// What a file looks like, from its first bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// PDF, including Illustrator files saved with PDF compatibility.
    Pdf,
    /// An Illustrator file saved without PDF compatibility (PostScript-based,
    /// Illustrator 8 and older, or "Create PDF Compatible File" off).
    LegacyAi,
    /// JSON from the Illustrator bridge script.
    Bridge,
    Unknown,
}

/// Format detection from the first bytes of a file.
pub fn sniff(bytes: &[u8]) -> FileKind {
    let head = &bytes[..bytes.len().min(1024)];
    let find = |needle: &[u8]| head.windows(needle.len()).any(|w| w == needle);
    if find(b"%PDF-") {
        return FileKind::Pdf;
    }
    if head.starts_with(b"%!PS-Adobe") {
        return FileKind::LegacyAi;
    }
    let text = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    let trimmed = text.trim_ascii_start();
    if trimmed.starts_with(b"{") && find(b"bae-ai") {
        return FileKind::Bridge;
    }
    FileKind::Unknown
}

/// Reads an Illustrator, PDF or bridge file.
pub fn read_document(path: &Path) -> Result<AiDocument, Error> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    read_bytes(&name, &bytes)
}

/// Reads a document from memory; `name` becomes the document name.
pub fn read_bytes(name: &str, bytes: &[u8]) -> Result<AiDocument, Error> {
    match sniff(bytes) {
        FileKind::Pdf => pdf::read_pdf(name, bytes),
        FileKind::Bridge => read_bridge(name, bytes),
        FileKind::LegacyAi => Err(Error::Unsupported(
            "Illustrator files saved without PDF compatibility (re-save with \"Create PDF Compatible File\")"
                .into(),
        )),
        FileKind::Unknown => Err(Error::UnknownFormat(name.to_owned())),
    }
}

/// Reads `path` and plans its import into `stage` (`import_ai(path) -> command
/// batch` of the workstream interface). Run the batch with
/// [`CommandBatch::execute`].
pub fn import_ai(path: &Path, stage: &Stage, options: &ImportOptions) -> Result<ImportPlan, Error> {
    plan_import(&read_document(path)?, stage, options)
}
