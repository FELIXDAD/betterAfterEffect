//! [`SyncSession`]: the transport-independent half of the protocol.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use bae_model::StageEvent;
use bae_model::loro::{Counter, PeerID, VersionVector};

use crate::client::ClientId;
use crate::docs::{DocSet, Imported};
use crate::error::SyncError;
use crate::presence::{PeerPresence, PresenceState, PresenceTable, initial_seq};
use crate::protocol::{DocKey, Message};
use crate::transport::Transport;

/// Payloads received in one pump, per document: `(version, data)`.
type Incoming = BTreeMap<DocKey, Vec<(VersionVector, Vec<u8>)>>;

/// Settings of a [`SyncSession`].
#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// Our client ID, sent in `Hello`s and presence.
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
    /// SMB appends per second.
    pub min_send_interval: Duration,
    /// Re-send our presence this often while nothing changes (heartbeat).
    pub presence_heartbeat: Duration,
    /// Others drop us after this long without a heartbeat (sent along with each
    /// update, so receivers do not need to agree on it).
    pub presence_ttl: Duration,
    /// Backpressure: no local changes are sent while the transport has more than
    /// this many bytes waiting ([`Transport::backlog`]); they go out on a later
    /// pump. A slow link then gets one large message at a time, and what waits
    /// for it stays in the documents instead of in memory.
    pub max_backlog: usize,
}

impl SessionConfig {
    /// Default [`max_backlog`](Self::max_backlog): 1 MiB.
    pub const DEFAULT_MAX_BACKLOG: usize = 1 << 20;

    /// Default settings for `client`.
    pub fn new(client: ClientId) -> Self {
        SessionConfig {
            client,
            hello_interval: Some(Duration::from_secs(5)),
            max_messages_per_pump: 256,
            min_send_interval: Duration::ZERO,
            presence_heartbeat: Duration::from_secs(5),
            presence_ttl: Duration::from_secs(20),
            max_backlog: Self::DEFAULT_MAX_BACKLOG,
        }
    }
}

/// What one [`SyncSession::pump`] did.
///
/// A pump that fails on a transport error keeps what it did up to the error; the
/// next pump's outcome includes it, unless it is taken earlier with
/// [`SyncSession::take_interrupted`].
#[derive(Debug, Default)]
pub struct PumpOutcome {
    /// Events of remote imports into the stage, in order.
    pub events: Vec<StageEvent>,
    /// Messages received.
    pub received: usize,
    /// Messages sent.
    pub sent: usize,
    /// Documents the remote has that the local set does not (e.g. a sublayer a
    /// collaborator created). Once the document exists locally the session asks
    /// the remote for its contents.
    pub unknown_docs: Vec<DocKey>,
    /// Problems with single documents that were skipped while the link stayed
    /// usable: received data that could not be imported (corrupt payload, model
    /// error), a document that could not be exported (for a reply or for
    /// sending), or one whose update is larger than the transport may send
    /// ([`SyncError::MessageTooLarge`]; tried again only once the remote got more
    /// of the ops it lacked, e.g. another way, or after [`SyncSession::reset`],
    /// so it is reported once, not on every edit of the document), or one the
    /// remote refuses to store ([`SyncError::NewerLogVersion`]; reported once,
    /// not sent again until [`SyncSession::reset`], and listed by
    /// [`SyncSession::unsynced`] meanwhile). Messages of unknown types never
    /// get here: transports skip them.
    pub errors: Vec<SyncError>,
    /// Someone came online, went offline or changed their presence.
    pub presence_changed: bool,
    /// Presence messages received, for the relay to forward. Empty in the outcome
    /// of a failed pump (the relay drops that link and announces its client left).
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
    /// What a pump that failed did before the error, until it is reported (by the
    /// next pump or [`SyncSession::take_interrupted`]).
    interrupted: Option<PumpOutcome>,
    /// Encoded bytes of every message handed to the transport so far (the relay's
    /// forwarded presence included, see `forward`). Minus the transport's
    /// backlog, this is how far its queue has been written out (it sends in
    /// order), which tells when a message left this process.
    accepted: u64,
    /// Data sent but not yet confirmed by a remote `Hello`, per document. A
    /// `Hello` sent before the data arrived does not undo it (see `handle`).
    in_flight: BTreeMap<DocKey, InFlight>,
    /// Documents listed in the remote's last `Hello`.
    listed: BTreeSet<DocKey>,
    /// Documents whose update did not fit in one message: not exported again
    /// until it can have become smaller (see `TooLarge`).
    too_large: BTreeMap<DocKey, TooLarge>,
    /// Documents the remote refuses to store ([`SyncError::refused_doc`]): not
    /// sent again until `reset`, and unsynced meanwhile.
    refused: BTreeSet<DocKey>,
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
            interrupted: None,
            accepted: 0,
            in_flight: BTreeMap::new(),
            listed: BTreeSet::new(),
            too_large: BTreeMap::new(),
            refused: BTreeSet::new(),
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
    ///
    /// The outcome of a failed pump is kept: the next pump still reports it (see
    /// [`take_interrupted`](Self::take_interrupted)).
    pub fn reset(&mut self) {
        self.remote = None;
        self.remote_client = None;
        self.remote_protocol = None;
        self.last_hello = None;
        self.announced.clear();
        self.in_flight.clear();
        self.listed.clear();
        self.too_large.clear();
        self.refused.clear();
    }

    /// Documents with local ops the remote is not known to have yet ("unsynced
    /// changes"). Everything counts as unsynced before the remote said hello,
    /// and so does a document the remote refused to store
    /// ([`SyncError::NewerLogVersion`]), until [`reset`](Self::reset).
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
                    Some(r) => {
                        self.refused.contains(key)
                            || !r.get(key).is_some_and(|k| k.includes_vv(&local))
                    }
                }
            })
            .collect()
    }

    /// What the last pump did before it failed, if it failed and that has not been
    /// reported yet. Otherwise the next pump includes it in its own outcome; take
    /// it to act on the imported remote changes at once (e.g. invalidate render
    /// caches) before reconnecting. Each outcome is reported only once.
    pub fn take_interrupted(&mut self) -> Option<PumpOutcome> {
        self.interrupted.take()
    }

    /// Exchange messages without blocking: say hello when due, import what
    /// arrived, send what the remote lacks. Call it after local edits and whenever
    /// the transport signals incoming data (or on a timer).
    ///
    /// The outcome also reports what an earlier, failed pump did, unless that was
    /// taken with [`take_interrupted`](Self::take_interrupted).
    ///
    /// # Errors
    ///
    /// Errors are transport failures; the link must be re-established (then call
    /// [`reset`](Self::reset)). What arrived before the failure is still imported
    /// (the remote considers it delivered), and what the pump did up to the error
    /// (events, unknown documents, skipped messages, counts) is kept for the next
    /// pump or [`take_interrupted`](Self::take_interrupted). Problems with single
    /// documents do not fail the pump; they are in [`PumpOutcome::errors`].
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
        let mut out = self.interrupted.take().unwrap_or_default();
        let mut incoming = Incoming::new();
        match self.exchange(docs, transport, now, &mut incoming, &mut out) {
            Ok(()) => Ok(out),
            Err(e) => {
                // The remote considers what it sent delivered and will not send
                // it again before a reconnect: import it before reporting the
                // error. Empty if the error came after the import.
                self.import(docs, incoming, &mut out);
                out.presence_received.clear();
                self.interrupted = Some(out);
                Err(e)
            }
        }
    }

    /// The steps of one pump. Leaves `incoming` empty once it was imported.
    fn exchange(
        &mut self,
        docs: &mut dyn DocSet,
        transport: &mut dyn Transport,
        now: Instant,
        incoming: &mut Incoming,
        out: &mut PumpOutcome,
    ) -> Result<(), SyncError> {
        self.note_written(transport, now);
        self.maybe_hello(docs, transport, now, out)?;
        for _ in 0..self.config.max_messages_per_pump {
            let msg = match transport.recv() {
                Ok(Some(msg)) => msg,
                Ok(None) => break,
                // A write that failed in the background (`ThreadedTransport`):
                // only that document is refused.
                Err(e) if e.refused_doc().is_some() => {
                    refuse(&mut self.refused, &mut self.in_flight, e, out);
                    continue;
                }
                Err(e) => return Err(e),
            };
            out.received += 1;
            self.stats.messages_received += 1;
            self.handle(docs, transport, msg, incoming, now, out)?;
        }
        self.import(docs, std::mem::take(incoming), out);
        out.presence_changed |= self.peers.expire(now);
        self.maybe_presence(transport, now, out)?;
        // A document that appeared since the last hello (created locally, or the
        // app added a sublayer the remote told us about): announce it so the
        // remote sends its contents.
        self.maybe_hello(docs, transport, now, out)?;
        let send_due = self
            .last_send
            .is_none_or(|t| now.saturating_duration_since(t) >= self.config.min_send_interval);
        if send_due && self.flush(docs, transport, out)? {
            self.last_send = Some(now);
        }
        self.note_written(transport, now);
        Ok(())
    }

    /// Note which data in flight has left this process by now: the transport
    /// writes its queue in order, so everything handed to it up to `accepted`
    /// minus what is still waiting is gone. Later messages (our own `Hello`s,
    /// presence) wait behind it and do not hold it back; a large message counts
    /// as waiting until its last byte is written. A transport whose backlog is
    /// always zero lets everything go at once.
    fn note_written(&mut self, transport: &dyn Transport, now: Instant) {
        let written = self.accepted.saturating_sub(transport.backlog() as u64);
        for f in self.in_flight.values_mut() {
            for part in std::iter::once(&mut f.oldest).chain(f.newer.as_mut()) {
                if part.left.is_none() && part.end <= written {
                    part.left = Some(now);
                }
            }
        }
    }

    /// How long after it left this process data counts as on its way: a remote
    /// `Hello` that arrives within this time and lacks it was probably sent
    /// before the data arrived. Two of our hello intervals (the remote's is
    /// usually the same), 10 s without one.
    fn in_flight_grace(&self) -> Duration {
        self.config
            .hello_interval
            .unwrap_or(Duration::from_secs(5))
            .saturating_mul(2)
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
        transport: &mut dyn Transport,
        msg: Message,
        incoming: &mut Incoming,
        now: Instant,
        out: &mut PumpOutcome,
    ) -> Result<(), SyncError> {
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
                // Replace, not merge: if the remote lacks something we sent (it
                // was lost, refused, or dropped because the remote did not have
                // the document yet), we must send it again.
                let mut remote: BTreeMap<DocKey, VersionVector> = versions.into_iter().collect();
                let listed: BTreeSet<DocKey> = remote.keys().cloned().collect();
                // Except data still on its way: the remote sent this `Hello`
                // before it arrived (a large message on a slow link takes many
                // hello intervals). Sending it again would only double the
                // transfer.
                let grace = self.in_flight_grace();
                let before = &self.listed;
                self.in_flight.retain(|doc, f| {
                    let has = remote.get(doc);
                    // The remote just added the document: it may have dropped
                    // what came before (2.5).
                    let added = has.is_some() && !before.contains(doc);
                    // All of it arrived, or its oldest part left this process
                    // long ago: lost, not late.
                    if added || f.confirm(has) || f.overdue(now, grace) {
                        return false;
                    }
                    remote.entry(doc.clone()).or_default().merge(f.version());
                    true
                });
                self.listed = listed;
                self.remote = Some(remote);
            }
            Message::Updates { doc, version, data } | Message::Snapshot { doc, version, data } => {
                self.stats.bytes_received += data.len() as u64;
                if let Some(remote) = &mut self.remote {
                    remote.entry(doc.clone()).or_default().merge(&version);
                }
                // Imported after the receive loop, one batch per document.
                incoming.entry(doc).or_default().push((version, data));
            }
            Message::Presence { ref client, .. } => {
                if client != &self.config.client {
                    out.presence_changed |= self.peers.apply(&msg, now).unwrap_or(false);
                    out.presence_received.push(msg);
                }
            }
            Message::SnapshotRequest { doc } => {
                let Some(version) = docs.version(&doc) else {
                    return Ok(());
                };
                match docs.export_snapshot(&doc) {
                    Ok(data) => {
                        let msg = Message::Snapshot {
                            doc: doc.clone(),
                            version: version.clone(),
                            data,
                        };
                        match send(&mut self.stats, &mut self.accepted, transport, msg, out) {
                            Ok(()) => {
                                if let Some(remote) = &mut self.remote {
                                    remote.entry(doc.clone()).or_default().merge(&version);
                                }
                                note_sent(&mut self.in_flight, doc, &version, self.accepted);
                            }
                            // Only this message was refused.
                            Err(e @ SyncError::MessageTooLarge { .. }) => out.errors.push(e),
                            Err(e) if e.refused_doc().is_some() => {
                                refuse(&mut self.refused, &mut self.in_flight, e, out);
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    Err(e) => out.errors.push(e),
                }
            }
        }
        Ok(())
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
    fn import(&mut self, docs: &mut dyn DocSet, incoming: Incoming, out: &mut PumpOutcome) {
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

    /// Send every document's ops the remote is not known to have, one document
    /// at a time, until the transport's backlog is over
    /// [`SessionConfig::max_backlog`]: the rest waits for a later pump. Returns
    /// whether anything was sent.
    fn flush(
        &mut self,
        docs: &dyn DocSet,
        transport: &mut dyn Transport,
        out: &mut PumpOutcome,
    ) -> Result<bool, SyncError> {
        let Some(remote) = &mut self.remote else {
            return Ok(false);
        };
        let mut sent = false;
        for key in docs.doc_keys() {
            if transport.backlog() > self.config.max_backlog {
                break;
            }
            let Some(local) = docs.version(&key) else {
                continue;
            };
            let known = remote.entry(key.clone()).or_default();
            if known.includes_vv(&local)
                || self.refused.contains(&key)
                || self.too_large.get(&key).is_some_and(|t| !t.may_fit(known))
            {
                continue;
            }
            let data = match docs.export_updates(&key, known) {
                Ok(data) => data,
                Err(e) => {
                    out.errors.push(e);
                    continue;
                }
            };
            let msg = Message::Updates {
                doc: key.clone(),
                version: local.clone(),
                data,
            };
            match send(&mut self.stats, &mut self.accepted, transport, msg, out) {
                Ok(()) => {
                    known.merge(&local);
                    self.too_large.remove(&key);
                    note_sent(&mut self.in_flight, key, &local, self.accepted);
                    sent = true;
                }
                // Only this document is stuck (spec 3.1); the others go on. It
                // is tried again once the remote has more of the ops it lacked.
                Err(e @ SyncError::MessageTooLarge { .. }) => {
                    self.too_large.insert(key, TooLarge::new(&local, known));
                    out.errors.push(e);
                }
                // The remote will not store this document (spec 2.4); the
                // others go on.
                Err(e) if e.refused_doc().is_some() => {
                    refuse(&mut self.refused, &mut self.in_flight, e, out);
                }
                Err(e) => return Err(e),
            }
        }
        Ok(sent)
    }

    fn send(
        &mut self,
        transport: &mut dyn Transport,
        msg: Message,
        out: &mut PumpOutcome,
    ) -> Result<(), SyncError> {
        send(&mut self.stats, &mut self.accepted, transport, msg, out)
    }

    /// Send a message on this session's transport for someone else (the relay
    /// forwarding presence). Not in the session's stats, but counted in
    /// `accepted`: it waits in the same queue (see `note_written`).
    pub(crate) fn forward(
        &mut self,
        transport: &mut dyn Transport,
        msg: Message,
    ) -> Result<(), SyncError> {
        let len = msg.encoded_len() as u64;
        transport.send(msg)?;
        self.accepted += len;
        Ok(())
    }
}

/// The remote refuses to store a document for as long as the link lasts
/// (`e` has a [`SyncError::refused_doc`]): send none of it until `reset`,
/// stop waiting for what was sent of it, and report it once (a transport that
/// writes in the background may refuse several messages that were already
/// queued).
fn refuse(
    refused: &mut BTreeSet<DocKey>,
    in_flight: &mut BTreeMap<DocKey, InFlight>,
    e: SyncError,
    out: &mut PumpOutcome,
) {
    if let Some(doc) = e.refused_doc() {
        in_flight.remove(doc);
        if !refused.insert(doc.clone()) {
            return;
        }
    }
    out.errors.push(e);
}

/// What a document's update that did not fit in one message consisted of: for
/// each peer whose ops the remote lacked, how far the remote had them. Local
/// ops only add to the update, and so does the remote getting other peers'
/// ops (collaborators' edits) or losing some; it can only get smaller once the
/// remote has more of one of these peers' ops (it got them another way, e.g.
/// from a shared folder or another client of the hub). Until then exporting
/// it again (hundreds of MiB) would only fail again.
#[derive(Debug)]
struct TooLarge {
    lacked: Vec<(PeerID, Counter)>,
}

impl TooLarge {
    fn new(local: &VersionVector, known: &VersionVector) -> Self {
        let lacked = local
            .iter()
            .filter_map(|(peer, &end)| {
                let had = counter(known, peer);
                (end.max(0) > had).then_some((*peer, had))
            })
            .collect();
        TooLarge { lacked }
    }

    /// Whether the update may have become smaller now that the remote has
    /// `known`.
    fn may_fit(&self, known: &VersionVector) -> bool {
        self.lacked
            .iter()
            .any(|(peer, had)| counter(known, peer) > *had)
    }
}

/// How many ops of `peer` `vv` includes.
fn counter(vv: &VersionVector, peer: &PeerID) -> Counter {
    vv.get(peer).copied().unwrap_or(0).max(0)
}

/// Data of one document sent but not confirmed by a remote `Hello` yet: the
/// oldest unconfirmed message, and the ones sent after it as one group. Each
/// part's version includes everything sent before it (local versions only
/// grow).
#[derive(Debug)]
struct InFlight {
    oldest: Part,
    newer: Option<Part>,
}

/// One or more messages of a document in flight.
#[derive(Debug)]
struct Part {
    /// The remote has this once the part arrived.
    version: VersionVector,
    /// `SyncSession::accepted` right after its last message was handed to the
    /// transport: it has left this process once the transport wrote that far.
    end: u64,
    /// The first pump that saw it gone from the transport's queue.
    left: Option<Instant>,
}

impl InFlight {
    /// What the remote has once everything arrived.
    fn version(&self) -> &VersionVector {
        &self.newer.as_ref().unwrap_or(&self.oldest).version
    }

    /// Apply the remote's version of the document (from its `Hello`): true if it
    /// has everything. If it has the oldest part, the newer group becomes the
    /// oldest, so the clock for lost data runs from when that group left.
    fn confirm(&mut self, has: Option<&VersionVector>) -> bool {
        let Some(has) = has else {
            return false;
        };
        if has.includes_vv(self.version()) {
            return true;
        }
        if has.includes_vv(&self.oldest.version)
            && let Some(newer) = self.newer.take()
        {
            self.oldest = newer;
        }
        false
    }

    /// The oldest part left this process at least `grace` ago and the remote
    /// still lacks it: it was lost.
    fn overdue(&self, now: Instant, grace: Duration) -> bool {
        self.oldest
            .left
            .is_some_and(|t| now.saturating_duration_since(t) >= grace)
    }
}

/// Record data of `doc` up to `version`, whose message ends at `end` (see
/// `Part::end`), as on its way. An earlier send that is not confirmed yet stays
/// the oldest part: the grace for lost data counts from when it left, so steady
/// edits do not put off its repair. Later sends join the newer group, which
/// counts as gone once its latest message is.
fn note_sent(
    in_flight: &mut BTreeMap<DocKey, InFlight>,
    doc: DocKey,
    version: &VersionVector,
    end: u64,
) {
    let mut part = Part {
        version: version.clone(),
        end,
        left: None,
    };
    match in_flight.entry(doc) {
        Entry::Vacant(e) => {
            e.insert(InFlight {
                oldest: part,
                newer: None,
            });
        }
        Entry::Occupied(mut e) => {
            let f = e.get_mut();
            part.version.merge(f.version());
            f.newer = Some(part);
        }
    }
}

/// Send `msg` and count it; `accepted` grows by its encoded size.
fn send(
    stats: &mut SyncStats,
    accepted: &mut u64,
    transport: &mut dyn Transport,
    msg: Message,
    out: &mut PumpOutcome,
) -> Result<(), SyncError> {
    let data_len = match &msg {
        Message::Updates { data, .. } | Message::Snapshot { data, .. } => data.len() as u64,
        _ => 0,
    };
    let len = msg.encoded_len() as u64;
    transport.send(msg)?;
    *accepted += len;
    stats.bytes_sent += data_len;
    stats.messages_sent += 1;
    out.sent += 1;
    Ok(())
}
