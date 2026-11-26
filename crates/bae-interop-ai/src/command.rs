//! `ai.import`: the import as a `bae-model` command, so the UI (File > Import),
//! scripts and agents (MCP) run it as one undo step.

use std::path::Path;

use bae_model::{Command, CommandError, CommandRegistry, Transaction};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::import::{ImportOptions, ImportReport, plan_writes};

/// Register the Illustrator interchange commands.
pub fn register_commands(r: &mut CommandRegistry) -> Result<(), CommandError> {
    r.register(ImportAi)
}

/// `ai.import`.
pub struct ImportAi;

#[derive(Deserialize, JsonSchema)]
pub struct ImportAiParams {
    /// The file to import: an Illustrator file saved with PDF compatibility
    /// (.ai, the default), a PDF, or JSON written by the BAE Illustrator bridge
    /// script (bridges/ai).
    pub file: String,
    #[serde(flatten)]
    pub options: ImportOptions,
}

impl Command for ImportAi {
    type Params = ImportAiParams;
    type Output = ImportReport;
    const ID: &'static str = "ai.import";
    const FILESYSTEM: bool = true;
    const TITLE: &'static str = "Import Illustrator File";
    const DESCRIPTION: &'static str = "Import an Illustrator (.ai), PDF or Illustrator bridge file: one comp per artboard, one editable shape layer per Illustrator layer (groups, compound paths, fills, strokes, gradients), point text as text layers. Returns the created comps and what was approximated.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<ImportReport, CommandError> {
        let doc = crate::read_document(Path::new(&p.file))?;
        let (writes, report) = plan_writes(&doc, tx.stage(), &p.options)?;
        for w in writes {
            w.apply(tx)?;
        }
        Ok(report)
    }
}
