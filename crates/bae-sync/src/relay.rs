//! [`Relay`]: the hub side of the protocol, without networking.
//!
//! `services/bae-hub` (workstream 10) wraps this in WebSocket connections: one
//! [`Transport`] per connected client. The relay keeps every document as a plain
//! Loro document ([`MemDocs`]) and runs one [`SyncSession`] per client against it,
//! so whatever one client sends reaches all others, and a client that reconnects
//! gets exactly what it missed. Each session holds document data back while its
//! peer's transport lags ([`crate::SessionConfig::max_backlog`]), so a client
//! that reads slowly does not make the relay queue the project again for each
//! of its `Hello`s.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use crate::client::ClientId;
use crate::docs::MemDocs;
use crate::error::SyncError;
use crate::presence::PresenceTable;
use crate::protocol::Message;
use crate::session::{SessionConfig, SyncSession};
use crate::transport::{LinkNotes, Transport};

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
    /// Clients whose presence did not fit in one message to this peer
    /// (reported once, until one of theirs fits again).
    presence_too_large: BTreeSet<ClientId>,
}

impl Peer {
    /// Forward another client's presence to this peer. One that does not fit
    /// in one message to it ([`SyncError::MessageTooLarge`]: a very long
    /// selection, and a peer that takes little) is skipped like document
    /// data: reported in `out.errors` once per sender until one of that
    /// sender's fits again, and the link goes on. Dropping the peer instead
    /// would make it reconnect, get the same presence and be dropped again,
    /// for as long as that collaborator keeps the selection.
    fn forward_presence(&mut self, msg: &Message, out: &mut RelayOutcome) -> Result<(), SyncError> {
        let Message::Presence { client, .. } = msg else {
            return Ok(());
        };
        match self.transport.send(msg.clone()) {
            Ok(()) => {
                out.sent += 1;
                self.presence_too_large.remove(client);
                Ok(())
            }
            Err(e @ SyncError::MessageTooLarge { .. }) => {
                if self.presence_too_large.insert(client.clone()) {
                    out.errors.push((self.handle, e));
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

/// What one [`Relay::pump`] did.
#[derive(Debug, Default)]
pub struct RelayOutcome {
    pub received: usize,
    pub sent: usize,
    /// Peers whose transport failed; they were disconnected.
    pub disconnected: Vec<(PeerHandle, SyncError)>,
    /// Problems with single messages (skipped).
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
    /// messages on lossy links). Applies to clients connected afterwards. `None`
    /// also gives up keeping large transfers as on their way (see
    /// [`SessionConfig::hello_interval`]).
    pub fn set_hello_interval(&mut self, every: Option<Duration>) {
        self.hello_interval = every;
    }

    /// The merged documents (persist them with `export_snapshot`).
    pub fn docs(&self) -> &MemDocs {
        &self.docs
    }

    /// The session settings [`connect`](Self::connect) gives a peer: the relay's
    /// client ID and hello interval, defaults otherwise. Start from it to give one
    /// peer its own settings with [`connect_with`](Self::connect_with).
    pub fn peer_config(&self) -> SessionConfig {
        let mut config = SessionConfig::new(self.client.clone());
        config.hello_interval = self.hello_interval;
        config
    }

    /// Connect a client with [`peer_config`](Self::peer_config).
    pub fn connect(&mut self, transport: Box<dyn Transport + Send>) -> PeerHandle {
        let config = self.peer_config();
        self.connect_with(transport, config)
    }

    /// Connect a peer with its own session settings, e.g. a store that should be
    /// written at most every few hundred milliseconds (`min_send_interval`) and
    /// needs no periodic `Hello`. A peer with a `min_send_interval` may have
    /// sends pending: [`flush`](Self::flush) before dropping the relay (and
    /// [`close_peer`](Self::close_peer) it to learn whether they arrived).
    ///
    /// `config.client` is ignored and replaced by the relay's own ID: the relay is
    /// one client to everyone it talks to, and its sessions drop presence that
    /// carries that ID.
    pub fn connect_with(
        &mut self,
        transport: Box<dyn Transport + Send>,
        mut config: SessionConfig,
    ) -> PeerHandle {
        let handle = PeerHandle(self.next_handle);
        self.next_handle += 1;
        config.client = self.client.clone();
        self.peers.push(Peer {
            handle,
            session: SyncSession::new(config),
            transport,
            greeted: false,
            presence_too_large: BTreeSet::new(),
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
        {
            // No "left" when its presence already expired (it went quiet).
            let leave = self.presence.leave(client, now);
            for p in &mut self.peers {
                if let Some(leave) = &leave {
                    let _ = p.transport.send(leave.clone());
                }
                // It left: a presence of its that did not fit is no longer
                // being skipped, and a later one is reported again.
                p.presence_too_large.remove(client);
            }
        }
        true
    }

    /// Like [`disconnect`](Self::disconnect), but first let the peer's transport
    /// finish delivering what was sent ([`Transport::close`]; blocks until it
    /// did) and return what went wrong. For a store whose `send` only queues
    /// (a [`crate::ThreadedTransport`]), this is where the writes of the last
    /// [`flush`](Self::flush) happen and where their failure shows. `None` if
    /// there is no such peer.
    pub fn close_peer(&mut self, handle: PeerHandle) -> Option<Result<(), SyncError>> {
        let peer = self.peers.iter_mut().find(|p| p.handle == handle)?;
        let closed = peer.transport.close();
        self.disconnect(handle);
        Some(closed)
    }

    /// A peer's [`Transport::notes`] (the hub: problems its project
    /// directory works around, documents it does not write) and their
    /// [`Transport::notes_seq`], to copy them only when that moved. `None`
    /// if there is no such peer.
    pub fn peer_notes(&self, handle: PeerHandle) -> Option<(u64, LinkNotes)> {
        let peer = self.peers.iter().find(|p| p.handle == handle)?;
        Some((peer.transport.notes_seq(), peer.transport.notes()))
    }

    /// A peer's [`Transport::notes_seq`]: cheap, call it often, and
    /// [`peer_notes`](Self::peer_notes) when it moved.
    pub fn peer_notes_seq(&self, handle: PeerHandle) -> Option<u64> {
        let peer = self.peers.iter().find(|p| p.handle == handle)?;
        Some(peer.transport.notes_seq())
    }

    /// Connected peers and the client IDs they announced.
    pub fn peers(&self) -> Vec<(PeerHandle, Option<ClientId>)> {
        self.peers
            .iter()
            .map(|p| (p.handle, p.session.remote_client().cloned()))
            .collect()
    }

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
                match peer
                    .session
                    .pump_at(&mut self.docs, &mut peer.transport, now)
                {
                    Ok(o) => {
                        received += o.received;
                        out.sent += o.sent;
                        out.errors
                            .extend(o.errors.into_iter().map(|e| (peer.handle, e)));
                        presence.extend(o.presence_received.into_iter().map(|m| (peer.handle, m)));
                    }
                    Err(e) => failed.push((peer.handle, e)),
                }
                if !peer.greeted {
                    peer.greeted = true;
                    for (_, msg) in self.presence.raw() {
                        if let Err(e) = peer.forward_presence(msg, &mut out) {
                            failed.push((peer.handle, e));
                            break;
                        }
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
                    if let Err(e) = peer.forward_presence(&msg, &mut out) {
                        failed.push((peer.handle, e));
                    }
                }
            }
            failed.sort_by_key(|(h, _)| *h);
            failed.dedup_by_key(|(h, _)| *h);
            // A failed peer's last messages were imported before its error (see
            // `SyncSession::pump_at`) but are not counted in `received`: go round
            // once more so they reach the others in this call too.
            let any_failed = !failed.is_empty();
            for (handle, e) in failed {
                self.disconnect_at(handle, now);
                out.disconnected.push((handle, e));
            }
            out.received += received;
            if received == 0 && !any_failed {
                break;
            }
        }
        out
    }

    /// Send every peer what it is missing right now, ignoring each session's
    /// `min_send_interval` and `max_backlog`, and answer every snapshot
    /// request still waiting (see [`SyncSession::flush`]); nothing is
    /// received.
    /// Call it after the last [`pump`](Self::pump) before dropping the relay, or
    /// edits held back for a peer connected with
    /// [`connect_with`](Self::connect_with) are never sent. Peers whose transport
    /// fails are disconnected, as in `pump`.
    pub fn flush(&mut self) -> RelayOutcome {
        self.flush_at(Instant::now())
    }

    /// [`flush`](Self::flush) with an explicit clock.
    pub fn flush_at(&mut self, now: Instant) -> RelayOutcome {
        let mut out = RelayOutcome::default();
        let mut failed = Vec::new();
        for peer in &mut self.peers {
            match peer.session.flush_at(&self.docs, &mut peer.transport, now) {
                Ok(o) => {
                    out.sent += o.sent;
                    out.errors
                        .extend(o.errors.into_iter().map(|e| (peer.handle, e)));
                }
                Err(e) => failed.push((peer.handle, e)),
            }
        }
        for (handle, e) in failed {
            self.disconnect_at(handle, now);
            out.disconnected.push((handle, e));
        }
        out
    }
}
