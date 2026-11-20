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
//! - [`Relay`]: the hub side (fan-out between clients), used by `services/bae-hub`.
//! - [`History`]: named versions ([`Checkpoint`]), read-only views of any version
//!   ([`history::view_at`]) and per-prim differences with authors
//!   ([`history::diff`]). Sync it with the stage through [`ProjectDocs`].
//!
//! This crate has no GPU, window or platform dependencies.

mod client;
mod dir;
mod docs;
mod error;
pub mod history;
mod protocol;
mod relay;
mod session;
mod threaded;
mod transport;

pub use client::ClientId;
pub use dir::{DirConfig, DirStats, DirTransport, LOG_FORMAT, LOG_VERSION};
pub use docs::{DocSet, Imported, MemDocs};
pub use error::SyncError;
pub use history::{
    Checkpoint, CheckpointId, History, PrimChange, PrimDiff, ProjectDocs, ProjectVersion,
    VersionView,
};
pub use protocol::{DocKey, Message, PROTOCOL_VERSION};
pub use relay::{PeerHandle, Relay, RelayOutcome};
pub use session::{PumpOutcome, SessionConfig, SyncSession, SyncStats};
pub use threaded::ThreadedTransport;
pub use transport::{MemoryTransport, Transport};
