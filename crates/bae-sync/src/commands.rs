//! Commands defined by bae-sync (agents, scripts and menus reach them through the
//! `CommandRegistry`; `bae-agent` turns them into MCP tools).

use bae_model::{Command, CommandError, CommandRegistry, PrimPath, SublayerId, Transaction};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::history::{ProjectVersion, RevertScope, VersionJson, revert_edits};

/// Register bae-sync's commands (`version.revert`).
pub fn register_commands(registry: &mut CommandRegistry) {
    registry
        .register(Revert)
        .expect("bae-sync command IDs are unique");
}

/// Parameters of `version.revert`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevertParams {
    /// The version to go back to: sublayer ID → (Loro peer ID as a decimal string
    /// → op count), exactly as a named version lists it.
    pub version: VersionJson,
    /// Only put back this sublayer.
    #[serde(default)]
    pub sublayer: Option<SublayerId>,
    /// Only put back this prim and its descendants (in every sublayer).
    #[serde(default)]
    pub prim: Option<PrimPath>,
}

#[derive(Debug, Serialize)]
pub struct RevertOutput {
    /// Number of edits applied.
    pub edits: usize,
}

struct Revert;

impl Command for Revert {
    type Params = RevertParams;
    type Output = RevertOutput;

    const ID: &'static str = "version.revert";
    const TITLE: &'static str = "Revert to version";
    const DESCRIPTION: &'static str = "Put the project, one sublayer or one prim subtree back the \
        way it was at an earlier version, as one new undoable step. History is not rewritten; \
        collaborators receive the change like any other edit. Prims that did not exist are \
        deleted, prims that existed are restored with their subtree, and types, references and \
        attribute opinions are set back. Give either `sublayer` or `prim`, or neither for the \
        whole project.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        params: RevertParams,
    ) -> Result<RevertOutput, CommandError> {
        let invalid = |message: String| CommandError::InvalidParams {
            command: Self::ID.to_owned(),
            message,
        };
        let scope = match (params.sublayer, params.prim) {
            (None, None) => RevertScope::Project,
            (Some(id), None) => RevertScope::Sublayer(id),
            (None, Some(path)) => RevertScope::Prim(path),
            (Some(_), Some(_)) => {
                return Err(invalid("give `sublayer` or `prim`, not both".into()));
            }
        };
        let version =
            ProjectVersion::from_json(&params.version).map_err(|e| invalid(e.to_string()))?;
        let plan = revert_edits(tx.stage(), &version, &scope)
            .map_err(|e| CommandError::Failed(e.to_string()))?;
        // Like undo, a rollback restores locked layers too.
        let edits = tx.allow_locked(|tx| plan.apply(tx))?;
        Ok(RevertOutput { edits })
    }
}
