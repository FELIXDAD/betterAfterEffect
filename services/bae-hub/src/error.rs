use std::path::{Path, PathBuf};

use bae_sync::SyncError;

/// Errors of the hub.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HubError {
    #[error("configuration: {0}")]
    Config(String),
    #[error("invalid project ID {0:?}: use 1-64 ASCII letters, digits, '_' or '-'")]
    InvalidProjectId(String),
    #[error("no project {0}")]
    NoSuchProject(String),
    /// The project was unloaded or failed while a connection was being set up.
    #[error("project {0} is closed")]
    ProjectClosed(String),
    #[error("another bae-hub is already using the data directory {0}")]
    DataDirInUse(PathBuf),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Network(std::io::Error),
    #[error(transparent)]
    Sync(#[from] SyncError),
}

impl HubError {
    pub(crate) fn io(path: &Path) -> impl FnOnce(std::io::Error) -> HubError + '_ {
        move |source| HubError::Io {
            path: path.to_owned(),
            source,
        }
    }
}
