//! The project and command registry an agent works on.

use std::sync::Arc;

use bae_model::{CommandRegistry, DynCommand, Project};
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
    /// A workspace with the built-in model commands, this crate's commands
    /// (`proposal.*`) and the packaging commands (`package.*`, `asset.relink`, …).
    pub fn new(project: Project) -> Self {
        Self::with_commands(project, std::iter::empty())
    }

    /// [`Workspace::new`] plus module commands, e.g. everything app modules
    /// published in `bae_app::Registry<dyn DynCommand>` (`bae-motion`'s `comp.create`,
    /// `layer.create` …). A command whose ID is already registered is skipped.
    pub fn with_commands(
        project: Project,
        extra: impl IntoIterator<Item = Arc<dyn DynCommand>>,
    ) -> Self {
        let mut registry = CommandRegistry::with_builtins();
        crate::register_commands(&mut registry);
        bae_package::register_commands(&mut registry);
        for cmd in extra {
            let id = cmd.info().id.clone();
            if registry.register_dyn(cmd).is_err() {
                tracing::debug!("module command {id} is already registered");
            }
        }
        Workspace { project, registry }
    }

    pub fn with_registry(project: Project, registry: CommandRegistry) -> Self {
        Workspace { project, registry }
    }

    pub fn shared(self) -> SharedWorkspace {
        Arc::new(Mutex::new(self))
    }
}
