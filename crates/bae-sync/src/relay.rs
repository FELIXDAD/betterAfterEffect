//! [`Relay`]: the hub side of the protocol, without networking.
//!
//! `services/bae-hub` (workstream 10) wraps this in WebSocket connections: one
//! [`Transport`] per connected client. The relay keeps every document as a plain
//! Loro document ([`MemDocs`]) and runs one [`SyncSession`] per client against it,
//! so whatever one client sends reaches all others, and a client that reconnects
//! gets exactly what it missed.

use std::time::{Duration, Instant};

use crate::client::ClientId;
use crate::docs::MemDocs;
use crate::error::SyncError;
use crate::presence::PresenceTable;
use crate::protocol::{DocKey, Message};
use crate::session::{SessionConfig, SyncSession};
use crate::transport::Transport;

/// A connected client, as returned by [`Relay::connect`] and
/// [`Relay::connect_with`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PeerHandle(u64);

struct Peer {
    handle: PeerHandle,
    session: SyncSession,
    transport: Box<dyn Transport + Send>,
    /// Current presence of the others has been sent to this peer.
    greeted: bool,
}

/// What one [`Relay::pump`] did.
#[derive(Debug, Default)]
pub struct RelayOutcome {
    /// Messages received from all peers.
    pub received: usize,
    /// Messages sent to all peers.
    pub sent: usize,
    /// Peers whose transport failed; they were disconnected. What they sent
    /// before the error was imported and is forwarded to the others.
    pub disconnected: Vec<(PeerHandle, SyncError)>,
    /// Problems with single documents (skipped; see [`crate::PumpOutcome::errors`]).
    pub errors: Vec<(PeerHandle, SyncError)>,
}

/// Fans updates out between connected clients and stores the merged documents.
pub struct Relay {
    client: ClientId,
    hello_interval: Option<Duration>,
    docs: MemDocs,
    peers: Vec<Peer>,
    next_handle: u64,
    presence: PresenceTable,
}

impl std::fmt::Debug for Relay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Relay")
            .field("client", &self.client)
            .field("docs", &self.docs)
            .field("peers", &self.peers.len())
            .finish_non_exhaustive()
    }
}

impl Relay {
    /// An empty relay. `client` is the hub's own ID in `Hello`s.
    pub fn new(client: ClientId) -> Self {
        Self::with_docs(client, MemDocs::new())
    }

    /// A relay over previously persisted documents.
    pub fn with_docs(client: ClientId, docs: MemDocs) -> Self {
        Relay {
            client,
            hello_interval: Some(Duration::from_secs(5)),
            docs,
            peers: Vec::new(),
            next_handle: 0,
            presence: PresenceTable::default(),
        }
    }

    /// How often the relay re-announces its versions to each client (repairs lost
    /// messages on lossy links). Applies to clients connected afterwards.
    pub fn set_hello_interval(&mut self, every: Option<Duration>) {
        self.hello_interval = every;
    }

    /// The merged documents (persist them with `export_snapshot`).
    pub fn docs(&self) -> &MemDocs {
        &self.docs
    }

    /// The session settings [`connect`](Self::connect) gives a connection: the
    /// relay's client ID and hello interval, defaults otherwise. Start from them
    /// for [`connect_with`](Self::connect_with).
    pub fn session_config(&self) -> SessionConfig {
        let mut config = SessionConfig::new(self.client.clone());
        config.hello_interval = self.hello_interval;
        config
    }

    /// Attach a connection (a client, or a store such as the hub's project
    /// directory) with the default [`session_config`](Self::session_config).
    pub fn connect(&mut self, transport: Box<dyn Transport + Send>) -> PeerHandle {
        let config = self.session_config();
        self.connect_with(transport, config)
    }

    /// [`connect`](Self::connect) with session settings for this connection only,
    /// e.g. a `min_send_interval` for a peer that writes everything to disk, or
    /// a `max_backlog` of `usize::MAX` for one that is not a slow link (its
    /// queue only waits for the local disk).
    /// Start from [`session_config`](Self::session_config) and keep
    /// `config.client`: it is the relay's ID in the `Hello`s every peer sees.
    pub fn connect_with(
        &mut self,
        transport: Box<dyn Transport + Send>,
        config: SessionConfig,
    ) -> PeerHandle {
        let handle = PeerHandle(self.next_handle);
        self.next_handle += 1;
        self.peers.push(Peer {
            handle,
            session: SyncSession::new(config),
            transport,
            greeted: false,
        });
        handle
    }

    /// Drop a connection; the others are told its client left.
    pub fn disconnect(&mut self, handle: PeerHandle) -> bool {
        self.disconnect_at(handle, Instant::now())
    }

    fn disconnect_at(&mut self, handle: PeerHandle, now: Instant) -> bool {
        let Some(i) = self.peers.iter().position(|p| p.handle == handle) else {
            return false;
        };
        let peer = self.peers.remove(i);
        if let Some(client) = peer.session.remote_client()
            && !self
                .peers
                .iter()
                .any(|p| p.session.remote_client() == Some(client))
            && let Some(leave) = self.presence.leave(client, now)
        {
            for p in &mut self.peers {
                let _ = p.session.forward(&mut p.transport, leave.clone());
            }
        }
        true
    }

    /// Session settings of a connected peer, to change them while it stays
    /// connected (takes effect on the next pump; keep `client`). `None` if
    /// `handle` is not connected.
    pub fn peer_config_mut(&mut self, handle: PeerHandle) -> Option<&mut SessionConfig> {
        self.peers
            .iter_mut()
            .find(|p| p.handle == handle)
            .map(|p| p.session.config_mut())
    }

    /// Documents with changes the peer `handle` is not known to have yet (see
    /// [`SyncSession::unsynced`]: all of them before it said hello). `None` if
    /// it is not connected. E.g. to pump until a store has been sent everything.
    pub fn unsynced(&self, handle: PeerHandle) -> Option<Vec<DocKey>> {
        self.peers
            .iter()
            .find(|p| p.handle == handle)
            .map(|p| p.session.unsynced(&self.docs))
    }

    /// Connected peers and the client IDs they announced.
    pub fn peers(&self) -> Vec<(PeerHandle, Option<ClientId>)> {
        self.peers
            .iter()
            .map(|p| (p.handle, p.session.remote_client().cloned()))
            .collect()
    }

    /// [`pump_at`](Self::pump_at) with the current time.
    pub fn pump(&mut self) -> RelayOutcome {
        self.pump_at(Instant::now())
    }

    /// Pump every peer; repeats (up to a few rounds) while messages keep arriving
    /// so an update from one client reaches the others within the same call.
    pub fn pump_at(&mut self, now: Instant) -> RelayOutcome {
        let mut out = RelayOutcome::default();
        self.presence.expire(now);
        for _ in 0..4 {
            let mut received = 0;
            let mut failed = Vec::new();
            let mut presence: Vec<(PeerHandle, Message)> = Vec::new();
            for peer in &mut self.peers {
                let o = match peer
                    .session
                    .pump_at(&mut self.docs, &mut peer.transport, now)
                {
                    Ok(o) => o,
                    Err(e) => {
                        failed.push((peer.handle, e));
                        // What it sent before the error was imported: count it,
                        // so another round forwards it to the others.
                        peer.session.take_interrupted().unwrap_or_default()
                    }
                };
                received += o.received;
                out.sent += o.sent;
                out.errors
                    .extend(o.errors.into_iter().map(|e| (peer.handle, e)));
                presence.extend(o.presence_received.into_iter().map(|m| (peer.handle, m)));
                if !peer.greeted {
                    peer.greeted = true;
                    for (_, msg) in self.presence.raw() {
                        if let Err(e) = peer.session.forward(&mut peer.transport, msg.clone()) {
                            failed.push((peer.handle, e));
                            break;
                        }
                        out.sent += 1;
                    }
                }
            }
            // Fan presence out: everyone but the sender gets every accepted update
            // (heartbeats included, they keep the sender alive on the others).
            for (from, msg) in presence {
                if self.presence.apply(&msg, now).is_none() {
                    continue;
                }
                for peer in self.peers.iter_mut().filter(|p| p.handle != from) {
                    if let Err(e) = peer.session.forward(&mut peer.transport, msg.clone()) {
                        failed.push((peer.handle, e));
                    } else {
                        out.sent += 1;
                    }
                }
            }
            failed.sort_by_key(|(h, _)| *h);
            failed.dedup_by_key(|(h, _)| *h);
            for (handle, e) in failed {
                self.disconnect_at(handle, now);
                out.disconnected.push((handle, e));
            }
            out.received += received;
            if received == 0 {
                break;
            }
        }
        out
    }
}
