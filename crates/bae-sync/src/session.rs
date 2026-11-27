//! [`SyncSession`]: the transport-independent half of the protocol.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use bae_model::StageEvent;
use bae_model::loro::VersionVector;

use crate::client::ClientId;
use crate::docs::{DocSet, Imported};
use crate::error::SyncError;
use crate::presence::{PeerPresence, PresenceState, PresenceTable, initial_seq};
use crate::protocol::{DocKey, Message, PROTOCOL_VERSION};
use crate::transport::Transport;

/// Payloads received in one pump, per document: `(version, data)`.
type Incoming = BTreeMap<DocKey, Vec<(VersionVector, Vec<u8>)>>;

/// At most this many entries per document in [`InFlight`] (more only pile up
/// while the remote's `Hello`s do not acknowledge ours); the oldest are merged.
const MAX_IN_FLIGHT: usize = 64;

/// Our data messages the remote may not have taken in yet, per document: for
/// each `Hello` of ours they followed (its `seq`, oldest first), our version
/// after the last of them. The first stays on its own (it shows whether the
/// remote had the document, see [`on_hello`](Self::on_hello)).
///
/// The remote's `Hello` with `ack: Some(k)` cannot show what we sent after our
/// `Hello` `k` (it may still be on its way), but should show everything we
/// sent before it: that arrived before our `Hello` `k`. So the last entry
/// before `k` is checked against its versions. If they do not include it,
/// something got lost (or could not be imported, or a reordering link has not
/// delivered it yet): the document is replaced, not kept, and everything it
/// lacks is sent again. Version vectors cannot describe a hole, so keeping
/// only the last send (whose version covers a lost one) would never send the
/// lost ops again as long as the sends go on, and the remote would keep the
/// later ones pending for want of them.
#[derive(Debug, Default)]
struct InFlight(BTreeMap<DocKey, Vec<(u64, VersionVector)>>);

impl InFlight {
    /// We sent `doc` up to `version` after our `Hello` `after`.
    fn sent(&mut self, doc: &DocKey, after: u64, version: &VersionVector) {
        let sends = self.0.entry(doc.clone()).or_default();
        let first_only = sends.len() == 1;
        match sends.last_mut() {
            Some((seq, v)) if *seq == after && !first_only => v.clone_from(version),
            _ => sends.push((after, version.clone())),
        }
        if sends.len() > MAX_IN_FLIGHT {
            // Merge the two oldest under the older `Hello`: the merged one is
            // checked against an earlier `ack` than it needs to be, so at
            // worst what is on its way is sent again.
            let (seq, _) = sends.remove(0);
            sends[0].0 = seq;
        }
    }

    /// The remote's `Hello` acknowledging our `Hello` `ack`: `remote` holds its
    /// versions and gets what may still be on its way added. `listed_before`:
    /// what the remote showed it has before this `Hello` (its last `Hello` and
    /// what it sent since).
    fn on_hello(
        &mut self,
        ack: u64,
        listed_before: Option<&BTreeMap<DocKey, VersionVector>>,
        remote: &mut BTreeMap<DocKey, VersionVector>,
    ) {
        self.0.retain(|doc, sends| {
            // A document it lists for the first time (a sublayer the app just
            // added) dropped what we sent while it did not have it: send it
            // again now, unless it took our first message (a hub creates the
            // document from it; the rest is still on its way).
            if let Some(have) = remote.get(doc)
                && !listed_before.is_some_and(|l| l.contains_key(doc))
                && !have.includes_vv(&sends[0].1)
            {
                return false;
            }
            let split = sends.partition_point(|(after, _)| *after < ack);
            if let Some((_, before)) = split.checked_sub(1).map(|i| &sends[i])
                && !remote.get(doc).is_some_and(|v| v.includes_vv(before))
            {
                return false;
            }
            sends.drain(..split);
            match sends.last() {
                Some((_, version)) => {
                    remote.entry(doc.clone()).or_default().merge(version);
                    true
                }
                None => false,
            }
        });
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

/// Settings of a [`SyncSession`].
#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub client: ClientId,
    /// Re-announce our versions this often so the remote can resend anything that
    /// got lost. `None`: only on connect and when a new document appears (enough
    /// for the shared folder, which never loses data). The remote's `Hello`s
    /// then cannot acknowledge a later `Hello` of ours, which a lost send
    /// needs to be sent again, so nothing we send is kept as on its way (see
    /// [`Message::Hello`]'s `ack`): each of the remote's `Hello`s replaces
    /// what we think it has, and a transfer that outlasts its hello interval
    /// is sent again.
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
    /// The remote does not take our edits (a read-only hub token, a folder we
    /// cannot write): never send document data (`Updates`, `Snapshot`, also
    /// not in answer to a `SnapshotRequest`). `Hello` and presence still go
    /// out and everything is received as usual. Local edits stay in
    /// [`SyncSession::unsynced`].
    pub read_only: bool,
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
            read_only: false,
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
/// knowledge to what the remote really has (keeping what may still be on its
/// way, see [`Message::Hello`]'s `ack`).
#[derive(Debug)]
pub struct SyncSession {
    config: SessionConfig,
    /// What the remote has, per document. `None` until its first `Hello`: before
    /// that we do not know what to send. Includes what we handed to the
    /// transport, which may still be on its way.
    remote: Option<BTreeMap<DocKey, VersionVector>>,
    /// What the remote showed it has: its last `Hello`, plus what it sent us
    /// since. [`forget_unconfirmed`](Self::forget_unconfirmed) goes back to it.
    confirmed: Option<BTreeMap<DocKey, VersionVector>>,
    /// What we sent that may still be on its way: the remote's `Hello`s do
    /// not undo it.
    in_flight: InFlight,
    /// The `seq` of our last `Hello` (never reset, so it keeps growing).
    hello_seq: u64,
    /// The highest `seq` of the remote's `Hello`s on this link (our `ack`).
    remote_hello_seq: u64,
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
            confirmed: None,
            in_flight: InFlight::default(),
            hello_seq: 0,
            remote_hello_seq: 0,
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
    ///
    /// Presence starts over too: our own goes out again on the next pump (the
    /// new link has not seen it), and who is online is whatever the new link
    /// reports. The next sequence number skips the one a hub uses to announce
    /// our departure when the old connection dropped (last seen + 1), so the
    /// others see us again at once.
    pub fn reset(&mut self) {
        self.remote = None;
        self.confirmed = None;
        self.in_flight.clear();
        self.remote_hello_seq = 0;
        self.remote_client = None;
        self.remote_protocol = None;
        self.last_hello = None;
        self.announced.clear();
        self.presence_dirty = self.presence.is_some();
        self.presence_seq = (self.presence_seq + 1).max(initial_seq());
        self.peers = PresenceTable::default();
    }

    /// The link failed: what was handed to it may never have arrived. From
    /// now on only what the remote showed it has counts as synced (its last
    /// `Hello` and what it sent since), so [`unsynced`](Self::unsynced) reports
    /// edits that were sent but not confirmed. The next link needs
    /// [`reset`](Self::reset) as usual.
    pub fn forget_unconfirmed(&mut self) {
        self.remote = self.confirmed.clone();
        self.in_flight.clear();
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
    /// outcome itself is lost with the error). Problems with single messages are
    /// in [`PumpOutcome::errors`].
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
        let mut incoming = Incoming::new();
        let received = self.receive(docs, transport, &mut incoming, now, &mut out);
        // Import what arrived even when the link failed part-way (a periodic
        // `Hello` that could not be sent included): the remote counts it as
        // delivered (a client that sends its last edits and closes at once must
        // not lose them), and the CRDT import does not need the link.
        self.import(docs, incoming, &mut out);
        hello?;
        received?;
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
    /// what it has), and nothing when [`SessionConfig::read_only`].
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

    /// Handle up to `max_messages_per_pump` received messages; payloads are
    /// collected in `incoming` for [`import`](Self::import). Stops at the first
    /// transport error, keeping what was collected before it.
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
            self.handle(docs, transport, msg, incoming, now, out)?;
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
        self.hello_seq += 1;
        let hello = Message::Hello {
            protocol: PROTOCOL_VERSION,
            client: self.config.client.clone(),
            versions,
            seq: self.hello_seq,
            // `versions` include what the remote sent before that `Hello`: a
            // pump imports what it received before it says hello again (what
            // could not be imported is missing, and the remote sees that).
            ack: Some(self.remote_hello_seq),
        };
        self.send(transport, hello, out)
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
                seq,
                ack,
            } => {
                self.remote_protocol = Some(protocol);
                self.remote_client = Some(client);
                self.remote_hello_seq = self.remote_hello_seq.max(seq);
                for (key, _) in &versions {
                    if docs.version(key).is_none() && !out.unknown_docs.contains(key) {
                        out.unknown_docs.push(key.clone());
                    }
                }
                // Replace, not merge: if the remote lost something (a message
                // dropped on the way) we must send it again. Except what we sent
                // after the `Hello` of ours it acknowledges, as long as what we
                // sent before it arrived: that may still be on its way (a large
                // transfer over a slow link outlasts the hello interval), and
                // sending it again would only pile up copies.
                let versions: BTreeMap<DocKey, VersionVector> = versions.into_iter().collect();
                let listed_before = self.confirmed.replace(versions.clone());
                let mut remote = versions;
                match ack.filter(|_| self.config.hello_interval.is_some()) {
                    Some(ack) => self
                        .in_flight
                        .on_hello(ack, listed_before.as_ref(), &mut remote),
                    None => self.in_flight.clear(),
                }
                self.remote = Some(remote);
            }
            Message::Updates { doc, version, data } | Message::Snapshot { doc, version, data } => {
                self.stats.bytes_received += data.len() as u64;
                if let Some(remote) = &mut self.remote {
                    remote.entry(doc.clone()).or_default().merge(&version);
                }
                if let Some(confirmed) = &mut self.confirmed {
                    confirmed.entry(doc.clone()).or_default().merge(&version);
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
                if self.config.read_only {
                    return Ok(());
                }
                let Some(version) = docs.version(&doc) else {
                    return Ok(());
                };
                match docs.export_snapshot(&doc) {
                    Ok(data) => {
                        if let Some(remote) = &mut self.remote {
                            remote.entry(doc.clone()).or_default().merge(&version);
                            if self.config.hello_interval.is_some() {
                                self.in_flight.sent(&doc, self.hello_seq, &version);
                            }
                        }
                        self.send(transport, Message::Snapshot { doc, version, data }, out)?;
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
                // Dropped: our `Hello`s do not show it, so the remote sends it
                // again (a document we do not have: at once when our `Hello`
                // first lists it).
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
        if self.config.read_only {
            return Ok(false);
        }
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
                    if self.config.hello_interval.is_some() {
                        self.in_flight.sent(&key, self.hello_seq, &local);
                    }
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
