//! Commands and the command registry (ARCHITECTURE §2 rule 1, §4.5).
//!
//! A command has an ID, a description and a JSON Schema for its parameters, and runs
//! inside a [`Transaction`] so it is one undo step. `bae-agent` exposes the registry
//! as MCP tools; menus and shortcuts bind to command IDs.

use std::sync::Arc;

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::ModelError;
use crate::event::StageEvent;
use crate::ids::AuthorId;
use crate::stage::{Stage, Transaction};

/// A statically typed command.
///
/// Parameter doc comments become descriptions in the generated JSON Schema, which is
/// what agents read: document every field.
pub trait Command: Send + Sync + 'static {
    type Params: DeserializeOwned + JsonSchema;
    type Output: Serialize;

    /// Stable ID, `<domain>.<verb>`, e.g. `prim.create`.
    const ID: &'static str;
    /// Short English title (UI strings are localized by the UI via the ID).
    const TITLE: &'static str;
    /// What it does, for agents and the command palette.
    const DESCRIPTION: &'static str;
    /// Accepts raw filesystem paths or performs filesystem I/O.
    const FILESYSTEM: bool = false;

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        params: Self::Params,
    ) -> Result<Self::Output, CommandError>;
}

/// Type-erased command, implemented for every [`Command`] and directly by plugins.
pub trait DynCommand: Send + Sync {
    fn info(&self) -> &CommandInfo;
    /// Whether invocation requires authority over files outside the project model.
    fn requires_filesystem(&self) -> bool {
        false
    }
    fn run_json(
        &self,
        tx: &mut Transaction<'_>,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, CommandError>;
}

/// Command metadata (exported to MCP tool definitions).
#[derive(Clone, Debug, Serialize)]
pub struct CommandInfo {
    pub id: String,
    pub title: String,
    pub description: String,
    /// JSON Schema of the parameters object.
    pub params_schema: serde_json::Value,
    /// Plugin ID for plugin commands; `None` for built-in and app-module commands.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("unknown command {0}")]
    Unknown(String),
    #[error("command {0} is already registered")]
    Duplicate(String),
    #[error("invalid parameters for {command}: {message}")]
    InvalidParams { command: String, message: String },
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("{0}")]
    Failed(String),
}

/// Result of [`CommandRegistry::execute`].
#[derive(Clone, Debug)]
pub struct CommandOutput {
    pub output: serde_json::Value,
    pub event: StageEvent,
}

struct Typed<C: Command> {
    cmd: C,
    info: CommandInfo,
}

impl<C: Command> DynCommand for Typed<C> {
    fn info(&self) -> &CommandInfo {
        &self.info
    }

    fn requires_filesystem(&self) -> bool {
        C::FILESYSTEM
    }

    fn run_json(
        &self,
        tx: &mut Transaction<'_>,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, CommandError> {
        let params: C::Params =
            serde_json::from_value(params).map_err(|e| CommandError::InvalidParams {
                command: C::ID.to_owned(),
                message: e.to_string(),
            })?;
        let out = self.cmd.run(tx, params)?;
        serde_json::to_value(out).map_err(|e| CommandError::Failed(e.to_string()))
    }
}

/// All registered commands, in registration order.
#[derive(Default)]
pub struct CommandRegistry {
    commands: IndexMap<String, Arc<dyn DynCommand>>,
}

impl std::fmt::Debug for CommandRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.commands.keys()).finish()
    }
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry with the built-in model commands ([`crate::commands`]).
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        crate::commands::register_builtins(&mut r);
        r
    }

    pub fn register<C: Command>(&mut self, cmd: C) -> Result<(), CommandError> {
        self.register_owned(cmd, None)
    }

    /// Register a typed command on behalf of a plugin.
    pub fn register_owned<C: Command>(
        &mut self,
        cmd: C,
        owner: Option<&str>,
    ) -> Result<(), CommandError> {
        let schema = schemars::schema_for!(C::Params);
        let info = CommandInfo {
            id: C::ID.to_owned(),
            title: C::TITLE.to_owned(),
            description: C::DESCRIPTION.to_owned(),
            params_schema: serde_json::to_value(schema).expect("schema serializes"),
            owner: owner.map(str::to_owned),
        };
        self.register_dyn(Arc::new(Typed { cmd, info }))
    }

    pub fn register_dyn(&mut self, cmd: Arc<dyn DynCommand>) -> Result<(), CommandError> {
        let id = cmd.info().id.clone();
        if self.commands.contains_key(&id) {
            return Err(CommandError::Duplicate(id));
        }
        self.commands.insert(id, cmd);
        Ok(())
    }

    pub fn unregister(&mut self, id: &str) -> bool {
        self.commands.shift_remove(id).is_some()
    }

    /// Remove every command a plugin registered; returns how many.
    pub fn unregister_owner(&mut self, owner: &str) -> usize {
        let before = self.commands.len();
        self.commands
            .retain(|_, c| c.info().owner.as_deref() != Some(owner));
        before - self.commands.len()
    }

    pub fn get(&self, id: &str) -> Option<&Arc<dyn DynCommand>> {
        self.commands.get(id)
    }

    pub fn infos(&self) -> impl Iterator<Item = &CommandInfo> {
        self.commands.values().map(|c| c.info())
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Run a command as one undo step by `author`, writing to the stage's edit target.
    pub fn execute(
        &self,
        stage: &mut Stage,
        author: &AuthorId,
        id: &str,
        params: serde_json::Value,
    ) -> Result<CommandOutput, CommandError> {
        let cmd = self
            .commands
            .get(id)
            .ok_or_else(|| CommandError::Unknown(id.to_owned()))?;
        let label = cmd.info().title.clone();
        let (output, event) = stage.transact(author, &label, |tx| cmd.run_json(tx, params))?;
        Ok(CommandOutput { output, event })
    }
}
