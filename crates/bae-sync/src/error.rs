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
    /// A message is larger than the link takes in one piece
    /// ([`crate::Transport::max_message_size`]: a hub link's `max_message`,
    /// or less if the other side said it takes less). Nothing was sent and the
    /// link is still usable. For document data a session skips only that
    /// document (reported once in `PumpOutcome::errors`, listed by
    /// `SyncSession::too_large` and as unsynced) and goes on with the others
    /// (`docs/spec/sync.md` §2.4). A message without a document (`doc` is
    /// `None`: a `Hello`, presence) cannot be skipped, the link does not work
    /// without it: `SyncSession::pump` returns the error like a link failure
    /// (`Connection` reconnects with backoff, `Relay` drops the peer). That
    /// only happens on a link that takes less than a `Hello`, which grows
    /// with the documents and the sessions that edited them.
    /// `doc`: the document of the message, if it has one.
    #[error(
        "message of {size} bytes{} is over the link's limit of {max} bytes per message",
        doc.as_ref().map(|d| format!(" for document {d}")).unwrap_or_default()
    )]
    MessageTooLarge {
        doc: Option<crate::DocKey>,
        size: usize,
        max: usize,
    },
    /// The hub answered the WebSocket handshake with an HTTP error instead of
    /// accepting it. `bae-hub` uses 401 (bad or missing token), 403 (the token
    /// may not open this project), 404 (no such project, and the token may not
    /// create it), 400 (invalid project ID) and 503 (shutting down).
    /// `message` is the hub's `{"error": ...}` text when it arrived with the
    /// response, else the status' standard reason.
    #[error("hub refused the connection: {status} {message}")]
    HubRefused { status: u16, message: String },
    #[error("no named version {0}")]
    UnknownCheckpoint(String),
    /// The version includes ops of sublayer `.0` that have not arrived here yet.
    #[error("that version of sublayer {0} is not available locally (not synced yet)")]
    VersionNotAvailable(bae_model::SublayerId),
    /// A rollback of one prim (`RevertScope::Prim`) would restore `path`,
    /// but its parent at that version no longer exists, is outside the scope,
    /// and no prim has the parent's old path: there is nowhere to put it.
    /// `revert` names the top-most such ancestor: roll that back first (or
    /// the whole sublayer or project). Nothing was changed.
    #[error(
        "cannot restore {path} in sublayer {sublayer}: its parent no longer exists; \
         revert {revert} first (or the whole sublayer)"
    )]
    RevertParentGone {
        sublayer: bae_model::SublayerId,
        /// The prim's path at that version.
        path: bae_model::PrimPath,
        /// The top-most ancestor (path at that version) that is gone now.
        revert: bae_model::PrimPath,
    },
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
