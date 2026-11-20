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
