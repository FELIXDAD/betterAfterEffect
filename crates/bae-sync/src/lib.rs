//! BAE collaboration (ARCHITECTURE §7, `docs/spec/sync.md`).
//!
//! - [`Message`]: the sync protocol, the same for every transport. Documents are
//!   addressed by [`DocKey`] (a sublayer, or the project history) and exchanged as
//!   Loro updates; version vectors say who has what.
//! - [`SyncSession`]: keeps a [`DocSet`] (usually the [`bae_model::Stage`]) in sync
//!   with one remote over a [`Transport`]. Remote imports go through
//!   `Stage::import_updates`, so they emit `origin = remote` events and never enter
//!   an undo stack.
//! - [`DirTransport`]: shared-folder mode (no server): append-only per-client logs
//!   under `updates/` in the project directory. Wrap it in a [`ThreadedTransport`]
//!   so NAS latency never blocks the stage thread.
//! - Presence ([`PresenceState`]): who is online, selection, playhead, workspace and
//!   advisory "I am editing this" locks, over the same transports
//!   ([`SyncSession::set_presence`], [`SyncSession::presence`]).
//! - `HubTransport` (feature `hub`): WebSocket connection to the collaboration hub.
//! - [`Relay`]: the hub side (fan-out between clients), used by `services/bae-hub`.
//! - [`Connection`]: what the app and agents use to share or join a project
//!   (shared folder or hub) from the stage thread: background connect,
//!   reconnect with backoff, access level, status, presence, clean close.
//! - [`History`]: named versions ([`Checkpoint`]), read-only views of any version
//!   ([`history::view_at`]) and per-prim differences with authors
//!   ([`history::diff`]). Sync it with the stage through [`ProjectDocs`].
//!
//! This crate has no GPU, window or platform dependencies.

mod client;
mod commands;
mod conflicts;
mod connection;
mod dir;
mod docs;
mod error;
pub mod history;
#[cfg(feature = "hub")]
mod hub;
mod identity;
mod ops;
mod presence;
mod protocol;
mod relay;
mod session;
mod threaded;
mod transport;

pub use client::ClientId;
pub use commands::{RevertOutput, RevertParams, register_commands};
pub use conflicts::{
    CaughtUp, CaughtUpDetached, ConcurrentEdit, ConflictConfig, ConflictOutcome, ConflictStats,
    ConflictTracker, DetachedMerge, Element, ElementValue, MergeSummary, OfflineEdit, OfflinePrim,
    OfflinePrimChange, merge_summary,
};
pub use connection::{
    Backoff, Closing, Connection, ConnectionConfig, ConnectionOutcome, ConnectionState,
    ConnectionStatus, FOLDER_SEND_INTERVAL, Failure, Target,
};
pub use dir::{DirConfig, DirStats, DirTransport, LOG_FORMAT, LOG_VERSION};
pub use docs::{DocSet, Imported, MemDocs};
pub use error::SyncError;
pub use history::{
    Checkpoint, CheckpointId, History, PrimChange, PrimDiff, ProjectDocs, ProjectVersion,
    RevertScope, VersionJson, VersionView,
};
#[cfg(feature = "hub")]
pub use hub::{ACCESS_HEADER, HubLimits, HubTransport};
pub use presence::{PeerPresence, Playhead, PresenceState};
pub use protocol::{DocKey, Message, PROTOCOL_VERSION};
pub use relay::{PeerHandle, Relay, RelayOutcome};
pub use session::{PumpOutcome, SessionConfig, SyncSession, SyncStats};
pub use threaded::ThreadedTransport;
pub use transport::{Access, MemoryTransport, Transport};
