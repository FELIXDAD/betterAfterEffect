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
//! Readers: `.ai`/PDF and the bridge JSON land in the next PRs; until then
//! [`read_document`] recognises the formats and reports them as unsupported.
//! Exports (PDF, SVG, the Illustrator rebuild script) and clipboard paste come
//! after the importers.
//!
//! No GPU, window or platform dependencies.

mod batch;
mod command;
mod doc;
mod error;
mod import;

use std::path::Path;

pub use batch::{BatchStep, CommandBatch};
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
        FileKind::Pdf => Err(Error::Unsupported("reading .ai / PDF files".into())),
        FileKind::Bridge => Err(Error::Unsupported("reading Illustrator bridge files".into())),
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
