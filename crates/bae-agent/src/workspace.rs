//! The project and command registry an agent works on.

use std::sync::Arc;

use bae_model::{CommandRegistry, Project};
use parking_lot::Mutex;

/// An open project plus the commands agents may call.
///
/// The stdio server owns one; the desktop app (HTTP transport, later) shares its own
/// through [`SharedWorkspace`].
#[derive(Debug)]
pub struct Workspace {
    pub project: Project,
    pub registry: CommandRegistry,
}

/// A workspace shared between the MCP server and the rest of the app.
pub type SharedWorkspace = Arc<Mutex<Workspace>>;

impl Workspace {
    /// A workspace with the built-in model commands and this crate's commands.
    pub fn new(project: Project) -> Self {
        let mut registry = CommandRegistry::with_builtins();
        crate::register_commands(&mut registry);
        Workspace { project, registry }
    }

    pub fn with_registry(project: Project, registry: CommandRegistry) -> Self {
        Workspace { project, registry }
    }

    pub fn shared(self) -> SharedWorkspace {
        Arc::new(Mutex::new(self))
    }
}
