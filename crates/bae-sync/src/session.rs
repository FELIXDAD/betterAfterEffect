//! [`SyncSession`]: the transport-independent half of the protocol.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use bae_model::StageEvent;
use bae_model::loro::VersionVector;

use crate::client::ClientId;
use crate::docs::{DocSet, Imported};
use crate::error::SyncError;
use crate::presence::{PeerPresence, PresenceState, PresenceTable, initial_seq};
use crate::protocol::{DocKey, Message};
use crate::transport::Transport;

/// Payloads received in one pump, per document: `(version, data)`.
type Payloads = BTreeMap<DocKey, Vec<(VersionVector, Vec<u8>)>>;

/// What one pump received that is handled after its receive loop.
#[derive(Default)]
struct Incoming {
    /// Imported one batch per document.
    payloads: Payloads,
    /// Documents the remote asked a snapshot of. They are answered after the
    /// import, so a snapshot holds everything that arrived before the request:
    /// the shared folder asks right after it delivered the records of our own log
    /// (on start, or re-delivered), and replaces that log with the answer.
    snapshot_requests: Vec<DocKey>,
}

/// Settings of a [`SyncSession`].
#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub client: ClientId,
    /// Re-announce our versions this often so the remote can resend anything that
    /// got lost. `None`: only on connect and when a new document appears (enough
    /// for the shared folder, which never loses data).
    pub hello_interval: Option<Duration>,
    /// At most this many received messages are handled per [`SyncSession::pump`],
    /// so one pump never stalls the thread that owns the stage for long.
    pub max_messages_per_pump: usize,
    /// Send local changes at most this often. Zero sends on every pump; the shared
    /// folder uses a few hundred milliseconds so a drag does not become dozens of
    /// SMB appends per second. What it holds back waits for a later pump: call
    /// [`SyncSession::flush`] before closing the link.
    pub min_send_interval: Duration,
    /// Re-send our presence this often while nothing changes (heartbeat).
    pub presence_heartbeat: Duration,
    /// Others drop us after this long without a heartbeat (sent along with each
    /// update, so receivers do not need to agree on it).
    pub presence_ttl: Duration,
}

impl SessionConfig {
    pub fn new(client: ClientId) -> Self {
        SessionConfig {
            client,
            hello_interval: Some(Duration::from_secs(5)),
            max_messages_per_pump: 256,
            min_send_interval: Duration::ZERO,
            presence_heartbeat: Duration::from_secs(5),
            presence_ttl: Duration::from_secs(20),
        }
    }
}

/// What one [`SyncSession::pump`] did.
#[derive(Debug, Default)]
pub struct PumpOutcome {
    /// Events of remote imports into the stage, in order.
    pub events: Vec<StageEvent>,
    pub received: usize,
    pub sent: usize,
    /// Documents the remote has that the local set does not (e.g. a sublayer a
    /// collaborator created). Once the document exists locally the session asks
    /// the remote for its contents.
    pub unknown_docs: Vec<DocKey>,
    /// Messages that were skipped (corrupt payloads, unknown message types).
    pub errors: Vec<SyncError>,
    /// Someone came online, went offline or changed their presence.
    pub presence_changed: bool,
    /// Presence messages received, for the relay to forward.
    pub(crate) presence_received: Vec<Message>,
}

/// Running totals of a session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    /// Payloads imported into a document.
    pub imports: u64,
    /// Payloads skipped because the document already had everything in them.
    pub redundant: u64,
}

/// Keeps a [`DocSet`] in sync with one remote over a [`Transport`].
///
/// The session remembers, per document, which version the remote is known to
/// have. Every [`pump`](Self::pump) imports what arrived and sends the ops the
/// remote is missing. CRDT imports are idempotent and order-independent, so the
/// only repair needed for lost messages is the periodic `Hello`, which resets that
/// knowledge to what the remote really has.
#[derive(Debug)]
pub struct SyncSession {
    config: SessionConfig,
    /// What the remote has, per document. `None` until its first `Hello`: before
    /// that we do not know what to send.
    remote: Option<BTreeMap<DocKey, VersionVector>>,
    remote_client: Option<ClientId>,
    remote_protocol: Option<u32>,
    last_hello: Option<Instant>,
    last_send: Option<Instant>,
    /// Documents announced in our last `Hello`.
    announced: BTreeSet<DocKey>,
    stats: SyncStats,
    presence: Option<PresenceState>,
    presence_dirty: bool,
    /// A "left" message is due.
    presence_leave: bool,
    presence_seq: u64,
    last_presence: Option<Instant>,
    peers: PresenceTable,
}

impl SyncSession {
    pub fn new(config: SessionConfig) -> Self {
        SyncSession {
            config,
            remote: None,
            remote_client: None,
            remote_protocol: None,
            last_hello: None,
            last_send: None,
            announced: BTreeSet::new(),
            stats: SyncStats::default(),
            presence: None,
            presence_dirty: false,
            presence_leave: false,
            presence_seq: initial_seq(),
            last_presence: None,
            peers: PresenceTable::default(),
        }
    }

    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    /// Change settings (takes effect on the next pump; the client ID should not
    /// change while connected).
    pub fn config_mut(&mut self) -> &mut SessionConfig {
        &mut self.config
    }

    pub fn client(&self) -> &ClientId {
        &self.config.client
    }

    /// The remote's client ID, once it said hello.
    pub fn remote_client(&self) -> Option<&ClientId> {
        self.remote_client.as_ref()
    }

    /// Protocol version the remote announced.
    pub fn remote_protocol(&self) -> Option<u32> {
        self.remote_protocol
    }

    /// Whether the remote has said hello (we know what it has).
    pub fn is_connected(&self) -> bool {
        self.remote.is_some()
    }

    pub fn stats(&self) -> &SyncStats {
        &self.stats
    }

    /// Publish our presence (sent on the next pump, then as a heartbeat).
    pub fn set_presence(&mut self, state: PresenceState) {
        if self.presence.as_ref() != Some(&state) {
            self.presence = Some(state);
            self.presence_dirty = true;
            self.presence_leave = false;
        }
    }

    /// Our current presence.
    pub fn local_presence(&self) -> Option<&PresenceState> {
        self.presence.as_ref()
    }

    /// Tell the others we left (sent on the next pump). Call before closing.
    pub fn clear_presence(&mut self) {
        if self.presence.take().is_some() {
            self.presence_leave = true;
        }
    }

    /// Everyone else who is online, by client ID.
    pub fn presence(&self) -> Vec<PeerPresence> {
        self.peers.peers()
    }

    /// Others who declared they are editing `path` (or an ancestor or descendant
    /// of it): show a hint, do not block.
    pub fn editing_by_others(&self, path: &bae_model::PrimPath) -> Vec<PeerPresence> {
        self.peers
            .peers()
            .into_iter()
            .filter(|p| p.is_editing(path))
            .collect()
    }

    /// Forget what the remote has, e.g. after reconnecting to a hub. The next pump
    /// says hello again and nothing is sent until the remote answers.
    pub fn reset(&mut self) {
        self.remote = None;
        self.remote_client = None;
        self.remote_protocol = None;
        self.last_hello = None;
        self.announced.clear();
    }

    /// What the remote is known to have, per document: its last `Hello`, plus
    /// what it sent and what we sent since. `None` until it said hello (after
    /// [`reset`](Self::reset), until it says hello again).
    pub fn remote_versions(&self) -> Option<&BTreeMap<DocKey, VersionVector>> {
        self.remote.as_ref()
    }

    /// Whether we have everything the remote is known to have: it said hello
    /// since the last [`reset`](Self::reset), and every document of `docs` it
    /// listed is included in the local version. Documents `docs` does not hold
    /// (reported in [`PumpOutcome::unknown_docs`]) are not waited for.
    ///
    /// After a reconnect (`reset`, then pumps), the first time this holds is when
    /// to call `ConflictTracker::caught_up`. Hub: its `Hello` lists everything it
    /// has and comes before the updates, so this waits for them. Shared folder:
    /// `DirTransport` answers our `Hello` by reading the folder again and queues
    /// its own after the records it read, so this holds once everything the
    /// folder had at that moment has been received, however many pumps that
    /// takes.
    pub fn is_caught_up(&self, docs: &dyn DocSet) -> bool {
        let Some(remote) = &self.remote else {
            return false;
        };
        remote.iter().all(|(key, theirs)| {
            docs.version(key)
                .is_none_or(|ours| ours.includes_vv(theirs))
        })
    }

    /// Documents with local ops the remote is not known to have yet ("unsynced
    /// changes"). Everything counts as unsynced before the remote said hello.
    pub fn unsynced(&self, docs: &dyn DocSet) -> Vec<DocKey> {
        docs.doc_keys()
            .into_iter()
            .filter(|key| {
                let Some(local) = docs.version(key) else {
                    return false;
                };
                if local.is_empty() {
                    return false;
                }
                match &self.remote {
                    None => true,
                    Some(r) => !r.get(key).is_some_and(|k| k.includes_vv(&local)),
                }
            })
            .collect()
    }

    /// Exchange messages without blocking: say hello when due, import what
    /// arrived, send what the remote lacks. Call it after local edits and whenever
    /// the transport signals incoming data (or on a timer).
    ///
    /// Errors are transport failures; the link must be re-established (then call
    /// [`reset`](Self::reset)). Payloads received before the failure are still
    /// imported (their [`StageEvent`]s reach `Stage::subscribe` listeners; the
    /// outcome itself is lost with the error), and snapshot requests received
    /// before it are still answered. Problems with single messages are in
    /// [`PumpOutcome::errors`].
    pub fn pump(
        &mut self,
        docs: &mut dyn DocSet,
        transport: &mut dyn Transport,
    ) -> Result<PumpOutcome, SyncError> {
        self.pump_at(docs, transport, Instant::now())
    }

    /// [`pump`](Self::pump) with an explicit clock (simulations, tests).
    pub fn pump_at(
        &mut self,
        docs: &mut dyn DocSet,
        transport: &mut dyn Transport,
        now: Instant,
    ) -> Result<PumpOutcome, SyncError> {
        let mut out = PumpOutcome::default();
        let hello = self.maybe_hello(docs, transport, now, &mut out);
        let mut incoming = Incoming::default();
        let received = self.receive(docs, transport, &mut incoming, now, &mut out);
        // Import what arrived even when the link failed part-way (a periodic
        // `Hello` that could not be sent included): the remote counts it as
        // delivered (a client that sends its last edits and closes at once must
        // not lose them), and the CRDT import does not need the link.
        self.import(docs, incoming.payloads, &mut out);
        // Answer every request taken off the transport, even when the link
        // failed or an earlier answer did: a request taken here is in no queue
        // any more, and the remote may not ask again soon (the shared folder
        // asks again for a compaction only once the log doubled). The first
        // error is returned.
        let mut answered = Ok(());
        for doc in incoming.snapshot_requests {
            let r = self.send_snapshot(docs, transport, doc, &mut out);
            answered = answered.and(r);
        }
        hello?;
        received?;
        answered?;
        out.presence_changed |= self.peers.expire(now);
        self.maybe_presence(transport, now, &mut out)?;
        // A document that appeared since the last hello (created locally, or the
        // app added a sublayer the remote told us about): announce it so the
        // remote sends its contents.
        self.maybe_hello(docs, transport, now, &mut out)?;
        let send_due = self
            .last_send
            .is_none_or(|t| now.saturating_duration_since(t) >= self.config.min_send_interval);
        if send_due && self.send_missing(docs, transport, &mut out)? {
            self.last_send = Some(now);
        }
        Ok(out)
    }

    /// Send every document's ops the remote is not known to have right now,
    /// ignoring `min_send_interval`. Nothing is received and no `Hello` or
    /// presence goes out. Call it after the last [`pump`](Self::pump) before
    /// closing a link whose sends may be held back, or the last edits never
    /// leave. Sends nothing before the remote's first `Hello` (we do not know
    /// what it has).
    ///
    /// Only [`PumpOutcome::sent`] and [`PumpOutcome::errors`] are filled in.
    /// Errors are transport failures, as in [`pump`](Self::pump).
    pub fn flush(
        &mut self,
        docs: &dyn DocSet,
        transport: &mut dyn Transport,
    ) -> Result<PumpOutcome, SyncError> {
        self.flush_at(docs, transport, Instant::now())
    }

    /// [`flush`](Self::flush) with an explicit clock (it restarts the
    /// `min_send_interval` wait).
    pub fn flush_at(
        &mut self,
        docs: &dyn DocSet,
        transport: &mut dyn Transport,
        now: Instant,
    ) -> Result<PumpOutcome, SyncError> {
        let mut out = PumpOutcome::default();
        if self.send_missing(docs, transport, &mut out)? {
            self.last_send = Some(now);
        }
        Ok(out)
    }

    /// Handle up to `max_messages_per_pump` received messages; payloads and
    /// snapshot requests are collected in `incoming` for after the loop
    /// ([`import`](Self::import), [`send_snapshot`](Self::send_snapshot)).
    /// Stops at the first transport error, keeping what was collected before it
    /// (both are still handled).
    fn receive(
        &mut self,
        docs: &dyn DocSet,
        transport: &mut dyn Transport,
        incoming: &mut Incoming,
        now: Instant,
        out: &mut PumpOutcome,
    ) -> Result<(), SyncError> {
        for _ in 0..self.config.max_messages_per_pump {
            let Some(msg) = transport.recv()? else {
                break;
            };
            out.received += 1;
            self.stats.messages_received += 1;
            self.handle(docs, msg, incoming, now, out);
        }
        Ok(())
    }

    fn maybe_hello(
        &mut self,
        docs: &dyn DocSet,
        transport: &mut dyn Transport,
        now: Instant,
        out: &mut PumpOutcome,
    ) -> Result<(), SyncError> {
        let keys = docs.doc_keys();
        let due = match (self.last_hello, self.config.hello_interval) {
            (None, _) => true,
            (Some(t), Some(every)) => now.saturating_duration_since(t) >= every,
            (Some(_), None) => false,
        };
        let new_doc = keys.iter().any(|k| !self.announced.contains(k));
        if !due && !new_doc {
            return Ok(());
        }
        let versions: Vec<(DocKey, VersionVector)> = keys
            .iter()
            .filter_map(|k| Some((k.clone(), docs.version(k)?)))
            .collect();
        self.announced = keys.into_iter().collect();
        self.last_hello = Some(now);
        self.send(
            transport,
            Message::hello(self.config.client.clone(), versions),
            out,
        )
    }

    fn handle(
        &mut self,
        docs: &dyn DocSet,
        msg: Message,
        incoming: &mut Incoming,
        now: Instant,
        out: &mut PumpOutcome,
    ) {
        match msg {
            Message::Hello {
                protocol,
                client,
                versions,
            } => {
                self.remote_protocol = Some(protocol);
                self.remote_client = Some(client);
                for (key, _) in &versions {
                    if docs.version(key).is_none() && !out.unknown_docs.contains(key) {
                        out.unknown_docs.push(key.clone());
                    }
                }
                // Replace, not merge: after a hub restart it may have less than we
                // sent, and then we must send it again.
                self.remote = Some(versions.into_iter().collect());
            }
            Message::Updates { doc, version, data } | Message::Snapshot { doc, version, data } => {
                self.stats.bytes_received += data.len() as u64;
                if let Some(remote) = &mut self.remote {
                    remote.entry(doc.clone()).or_default().merge(&version);
                }
                // Imported after the receive loop, one batch per document.
                incoming
                    .payloads
                    .entry(doc)
                    .or_default()
                    .push((version, data));
            }
            Message::Presence { ref client, .. } => {
                if client != &self.config.client {
                    out.presence_changed |= self.peers.apply(&msg, now).unwrap_or(false);
                    out.presence_received.push(msg);
                }
            }
            Message::SnapshotRequest { doc } => {
                if !incoming.snapshot_requests.contains(&doc) {
                    incoming.snapshot_requests.push(doc);
                }
            }
        }
    }

    /// Answer a `SnapshotRequest` (after this pump's import): a snapshot of the
    /// whole document. Documents we do not have are not answered.
    fn send_snapshot(
        &mut self,
        docs: &dyn DocSet,
        transport: &mut dyn Transport,
        doc: DocKey,
        out: &mut PumpOutcome,
    ) -> Result<(), SyncError> {
        let Some(version) = docs.version(&doc) else {
            return Ok(());
        };
        match docs.export_snapshot(&doc) {
            Ok(data) => {
                if let Some(remote) = &mut self.remote {
                    remote.entry(doc.clone()).or_default().merge(&version);
                }
                self.send(transport, Message::Snapshot { doc, version, data }, out)
            }
            Err(e) => {
                out.errors.push(e);
                Ok(())
            }
        }
    }

    /// Send our presence when it changed or the heartbeat is due, or "left".
    fn maybe_presence(
        &mut self,
        transport: &mut dyn Transport,
        now: Instant,
        out: &mut PumpOutcome,
    ) -> Result<(), SyncError> {
        let state = if self.presence_leave {
            None
        } else if let Some(p) = &self.presence {
            let due = self.presence_dirty
                || self.last_presence.is_none_or(|t| {
                    now.saturating_duration_since(t) >= self.config.presence_heartbeat
                });
            if !due {
                return Ok(());
            }
            Some(serde_json::to_vec(p).expect("presence serializes"))
        } else {
            return Ok(());
        };
        self.presence_seq += 1;
        let msg = Message::Presence {
            client: self.config.client.clone(),
            seq: self.presence_seq,
            ttl_ms: u32::try_from(self.config.presence_ttl.as_millis()).unwrap_or(u32::MAX),
            state,
        };
        self.send(transport, msg, out)?;
        self.presence_dirty = false;
        self.presence_leave = false;
        self.last_presence = Some(now);
        Ok(())
    }

    /// Import what arrived, one batch per document (one mirror rebuild and one
    /// event per sublayer instead of one per message).
    fn import(&mut self, docs: &mut dyn DocSet, incoming: Payloads, out: &mut PumpOutcome) {
        for (doc, items) in incoming {
            // A set may create documents on import (the relay does), so a missing
            // document is only "unknown" if the set says so.
            let have = docs.version(&doc).unwrap_or_default();
            let total = items.len();
            let batch: Vec<(VersionVector, Vec<u8>)> = items
                .into_iter()
                .filter(|(version, _)| !have.includes_vv(version))
                .collect();
            self.stats.redundant += (total - batch.len()) as u64;
            if batch.is_empty() {
                continue;
            }
            match docs.import_batch(&doc, &batch) {
                Ok(Imported::Done(event)) => {
                    self.stats.imports += batch.len() as u64;
                    if let Some(ev) = event.filter(|e| !e.is_empty()) {
                        out.events.push(ev);
                    }
                }
                Ok(Imported::UnknownDoc) => {
                    if !out.unknown_docs.contains(&doc) {
                        out.unknown_docs.push(doc);
                    }
                }
                Err(e) => out.errors.push(e),
            }
        }
    }

    /// Send every document's ops the remote is not known to have. Returns whether
    /// anything was sent. A failed send does not stop the others: they are
    /// already counted as sent, and a transport can fail one message and take
    /// the next (a shared folder whose log for one document cannot be written,
    /// a message over the hub's size limit). The first error is returned.
    fn send_missing(
        &mut self,
        docs: &dyn DocSet,
        transport: &mut dyn Transport,
        out: &mut PumpOutcome,
    ) -> Result<bool, SyncError> {
        let Some(remote) = &mut self.remote else {
            return Ok(false);
        };
        let mut batch = Vec::new();
        for key in docs.doc_keys() {
            let Some(local) = docs.version(&key) else {
                continue;
            };
            let known = remote.entry(key.clone()).or_default();
            if known.includes_vv(&local) {
                continue;
            }
            match docs.export_updates(&key, known) {
                Ok(data) => {
                    known.merge(&local);
                    batch.push(Message::Updates {
                        doc: key,
                        version: local,
                        data,
                    });
                }
                Err(e) => out.errors.push(e),
            }
        }
        let sent = !batch.is_empty();
        let mut failed = None;
        for msg in batch {
            if let Err(e) = self.send(transport, msg, out) {
                failed.get_or_insert(e);
            }
        }
        match failed {
            Some(e) => Err(e),
            None => Ok(sent),
        }
    }

    fn send(
        &mut self,
        transport: &mut dyn Transport,
        msg: Message,
        out: &mut PumpOutcome,
    ) -> Result<(), SyncError> {
        if let Message::Updates { data, .. } | Message::Snapshot { data, .. } = &msg {
            self.stats.bytes_sent += data.len() as u64;
        }
        transport.send(msg)?;
        self.stats.messages_sent += 1;
        out.sent += 1;
        Ok(())
    }
}
