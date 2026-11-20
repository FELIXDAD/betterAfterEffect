use std::path::PathBuf;

use bae_model::ModelError;

/// Errors of the sync layer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SyncError {
    #[error("invalid client ID {0:?}: use 1-64 ASCII letters, digits, '_' or '-'")]
    InvalidClientId(String),
    #[error("invalid document key {0:?}")]
    InvalidDocKey(String),
    #[error("malformed message: {0}")]
    Decode(String),
    /// A message type from a newer protocol version; receivers skip it.
    #[error("unknown message type {0}")]
    UnknownMessage(u8),
    #[error("peer speaks sync protocol {theirs}, this build speaks {ours}")]
    ProtocolMismatch { ours: u32, theirs: u32 },
    #[error("CRDT error: {0}")]
    Crdt(String),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("transport closed")]
    Closed,
    #[error("client ID {0} is already in use by another session on this project")]
    ClientIdInUse(crate::ClientId),
    #[error("{0} is not a project directory")]
    NotADirectory(PathBuf),
    #[error("hub connection: {0}")]
    Hub(String),
    #[error("no named version {0}")]
    UnknownCheckpoint(String),
    /// The version includes ops of sublayer `.0` that have not arrived here yet.
    #[error("that version of sublayer {0} is not available locally (not synced yet)")]
    VersionNotAvailable(bae_model::SublayerId),
}

impl SyncError {
    pub(crate) fn io(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> SyncError + '_ {
        move |source| SyncError::Io {
            path: path.to_owned(),
            source,
        }
    }
}

impl From<bae_model::loro::LoroError> for SyncError {
    fn from(e: bae_model::loro::LoroError) -> Self {
        SyncError::Crdt(e.to_string())
    }
}

impl From<bae_model::loro::LoroEncodeError> for SyncError {
    fn from(e: bae_model::loro::LoroEncodeError) -> Self {
        SyncError::Crdt(e.to_string())
    }
}
