//! The project, its named versions and the command registry an agent works on.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bae_model::{CommandRegistry, DynCommand, Project};
use bae_sync::{History, SyncError};
use parking_lot::Mutex;

/// An open project plus the commands agents may call.
///
/// The stdio server owns one; the desktop app (HTTP transport, later) shares its own
/// through [`SharedWorkspace`].
#[derive(Debug)]
pub struct Workspace {
    pub project: Project,
    pub registry: CommandRegistry,
    /// Named versions (`bae-sync`): metadata next to the stage, not in it. Loaded
    /// from and saved to `history.loro` in a `.bae` folder; in memory otherwise.
    pub history: History,
}

/// A workspace shared between the MCP server and the rest of the app.
pub type SharedWorkspace = Arc<Mutex<Workspace>>;

impl Workspace {
    /// A workspace with the built-in model commands, this crate's commands
    /// (`proposal.*`), the packaging commands (`package.*`, `asset.relink`, …) and
    /// `version.revert`.
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
        bae_sync::register_commands(&mut registry);
        for cmd in extra {
            let id = cmd.info().id.clone();
            if registry.register_dyn(cmd).is_err() {
                tracing::debug!("module command {id} is already registered");
            }
        }
        Self::with_registry(project, registry)
    }

    pub fn with_registry(project: Project, registry: CommandRegistry) -> Self {
        let history = match history_dir(&project) {
            Some(dir) => History::load(&dir).unwrap_or_else(|e| {
                tracing::warn!("named versions in {} unreadable: {e}", dir.display());
                History::new()
            }),
            None => History::new(),
        };
        Workspace {
            project,
            registry,
            history,
        }
    }

    pub fn shared(self) -> SharedWorkspace {
        Arc::new(Mutex::new(self))
    }

    /// Save the named versions next to the project. `Ok(false)` when they cannot
    /// be persisted (unsaved project or a `.baez` file): they live in memory only.
    pub fn save_history(&self) -> Result<bool, SyncError> {
        match history_dir(&self.project) {
            Some(dir) => self.history.save(dir).map(|()| true),
            None => Ok(false),
        }
    }
}

/// The `.bae` folder named versions are stored in, if the project has one.
fn history_dir(project: &Project) -> Option<PathBuf> {
    project
        .location()
        .filter(|p| p.is_dir())
        .map(Path::to_path_buf)
}
