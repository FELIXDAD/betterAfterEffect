//! Command batches: what an import produces.
//!
//! An importer never edits the stage itself (ARCHITECTURE §2, rule 1). It
//! plans a list of ordinary `bae-model` commands (`prim.create`, `attr.set`)
//! that the caller runs as **one undo step**, all or nothing. The JSON shape
//! is the same as the parameters of the agent tool `command.batch`
//! (`bae-agent`), so an agent can inspect a planned import and submit it as is.

use bae_model::{AuthorId, CommandError, CommandRegistry, Stage, StageEvent};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::Error;

/// One command of a batch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BatchStep {
    /// Command ID, e.g. `prim.create`.
    pub command: String,
    /// The command's parameters.
    pub params: serde_json::Value,
}

/// Commands to run in order as one undo step.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CommandBatch {
    /// Undo/history label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub commands: Vec<BatchStep>,
}

impl CommandBatch {
    /// An empty batch with an undo label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            commands: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Appends a step.
    pub fn push(&mut self, command: impl Into<String>, params: serde_json::Value) {
        self.commands.push(BatchStep {
            command: command.into(),
            params,
        });
    }

    /// Runs every step through `registry` as one undo step by `author`. If a
    /// step fails nothing is applied and the error names the step.
    pub fn execute(
        &self,
        registry: &CommandRegistry,
        stage: &mut Stage,
        author: &AuthorId,
    ) -> Result<StageEvent, Error> {
        let label = self.label.as_deref().unwrap_or("Import");
        let mut failed_at = 0;
        let result = stage.transact(author, label, |tx| {
            for (i, step) in self.commands.iter().enumerate() {
                failed_at = i;
                let cmd = registry
                    .get(&step.command)
                    .ok_or_else(|| CommandError::Unknown(step.command.clone()))?;
                cmd.run_json(tx, step.params.clone())?;
            }
            Ok::<_, CommandError>(())
        });
        match result {
            Ok(((), event)) => Ok(event),
            Err(source) => Err(Error::Batch {
                step: failed_at,
                command: self
                    .commands
                    .get(failed_at)
                    .map_or_else(String::new, |s| s.command.clone()),
                source,
            }),
        }
    }
}
