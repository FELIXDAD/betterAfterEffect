//! BAE collaboration (ARCHITECTURE §7, `docs/spec/sync.md`).
//!
//! - [`Message`]: the sync protocol, the same for every transport. Documents are
//!   addressed by [`DocKey`] (a sublayer, or the project history) and exchanged as
//!   Loro updates; version vectors say who has what.
//! - [`SyncSession`]: keeps a [`DocSet`] (usually the [`bae_model::Stage`]) in sync
//!   with one remote over a [`Transport`]. Remote imports go through
//!   `Stage::import_updates`, so they emit `origin = remote` events and never enter
//!   an undo stack.
//! - [`Relay`]: the hub side (fan-out between clients), used by `services/bae-hub`.
//!
//! This crate has no GPU, window or platform dependencies.

mod client;
mod docs;
mod error;
mod protocol;
mod relay;
mod session;
mod transport;

pub use client::ClientId;
pub use docs::{DocSet, Imported, MemDocs};
pub use error::SyncError;
pub use protocol::{DocKey, Message, PROTOCOL_VERSION};
pub use relay::{PeerHandle, Relay, RelayOutcome};
pub use session::{PumpOutcome, SessionConfig, SyncSession, SyncStats};
pub use transport::{MemoryTransport, Transport};
