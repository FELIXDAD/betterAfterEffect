//! [`SyncSession`]: the transport-independent half of the protocol.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use bae_model::StageEvent;
use bae_model::loro::VersionVector;

use crate::client::ClientId;
use crate::docs::{DocSet, Imported};
use crate::error::SyncError;
use crate::presence::{PeerPresence, PresenceState, PresenceTable, initial_seq};
use crate::protocol::{DocKey, Message, PROTOCOL_VERSION};
use crate::transport::Transport;

/// Default of [`SessionConfig::max_backlog`] (and of `HubLimits::max_backlog`).
pub(crate) const DEFAULT_MAX_BACKLOG: usize = 16 << 20;

/// Payloads received in one pump, per document: `(version, data)`.
type Payloads = BTreeMap<DocKey, Vec<(VersionVector, Vec<u8>)>>;

/// What one pump received that is handled after its receive loop.
#[derive(Default)]
struct Incoming {
    /// Imported one batch per document.
    payloads: Payloads,
    /// Documents the remote asked a snapshot of. They are answered after the
    /// import (of this pump, or of a later one when more wait than
    /// `max_snapshots_per_pump`), so a snapshot holds everything that arrived
    /// before the request:
    /// the shared folder asks right after it delivered the records of our own log
    /// (on start, or re-delivered), and replaces that log with the answer.
    snapshot_requests: Vec<DocKey>,
}

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
    /// At most this many `SnapshotRequest`s are answered per
    /// [`SyncSession::pump`] (at least one): each answer exports a whole
    /// document on the thread that owns the stage, and a shared folder asks
    /// for many at once after a NAS hiccup (a repair for every document whose
    /// write failed) or a compaction. The rest wait in the session, oldest
    /// first, and are answered on the next pumps; none is dropped (a request
    /// taken off the transport is in no other queue, and the folder may not
    /// ask again soon). [`SyncSession::pending_snapshot_requests`] says how
    /// many wait: pump again soon while it is not zero ([`crate::Connection`]
    /// does). [`SyncSession::reset`] drops them with the link. Default 2.
    pub max_snapshots_per_pump: usize,
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
    /// Flow control: no document data is sent while more than this many bytes
    /// wait in the transport ([`Transport::backlog`]: a hub connection whose
    /// peer reads slowly, a queue a background thread has not written yet).
    /// Receiving, `Hello`s, presence and snapshot answers go on. Once it
    /// drained, sending goes on with the document it stopped at, so every
    /// document gets its turn. Checked before each document, so one message
    /// of any size goes out when the backlog is below this (a newcomer's
    /// first sync of a large sublayer): at most this plus one message waits.
    ///
    /// It bounds memory: without it, everything the peer lacks is queued at
    /// once (a newcomer joining a large project over a VPN). Resends are
    /// another matter: a peer's `Hello` that acknowledges ours does not make
    /// us queue again what is on its way (the in-flight records, see
    /// [`Message::Hello`]'s `ack`). One that does not (an older peer, or our
    /// `hello_interval` is `None`) makes us queue everything the peer lacks
    /// once more; flow control limits that to once per drained backlog
    /// instead of once per `Hello`. `crate::Connection::close` sends what it
    /// flushes with this flow control, on its closing thread.
    ///
    /// [`SyncSession::flush`] ignores it. `None`: no flow control. Default
    /// 16 MiB, the hub link's default `HubLimits::max_backlog`.
    pub max_backlog: Option<usize>,
    /// Send document data (`Updates`, and `Snapshot` answers to snapshot
    /// requests; also not on [`flush`](SyncSession::flush)). `false` on a link
    /// that may not write: a hub connection with a read-only token (the
    /// handshake's `x-bae-access: read`, `HubTransport::access`; `GET
    /// /v1/status` says `"access": "read"`), a shared folder this user cannot
    /// write ([`crate::DirTransport::open_read_only`]). [`crate::Connection`]
    /// sets it from the access the remote granted. The hub drops such data
    /// anyway, and takes only small messages from read-only connections (its
    /// `read_only_max_incoming`): a viewer with more local edits than that in
    /// one document would be dropped each time it sent them, which is after
    /// every `Hello` of the hub and every reconnect. `Hello`s, presence and
    /// receiving go on as usual; local edits stay local
    /// ([`unsynced`](SyncSession::unsynced) keeps listing them, nothing counts
    /// as on its way). Default `true`.
    pub send_data: bool,
}

impl SessionConfig {
    pub fn new(client: ClientId) -> Self {
        SessionConfig {
            client,
            hello_interval: Some(Duration::from_secs(5)),
            max_messages_per_pump: 256,
            max_snapshots_per_pump: 2,
            min_send_interval: Duration::ZERO,
            presence_heartbeat: Duration::from_secs(5),
            presence_ttl: Duration::from_secs(20),
            max_backlog: Some(DEFAULT_MAX_BACKLOG),
            send_data: true,
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
    /// Times document data was held back because the transport's backlog was
    /// over [`SessionConfig::max_backlog`].
    pub held_back: u64,
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
    /// Flow control stopped before this document: the next send starts here.
    next_doc: Option<DocKey>,
    /// Snapshot requests taken off the transport and not answered yet
    /// ([`SessionConfig::max_snapshots_per_pump`]), oldest first, each
    /// document once.
    snapshot_requests: VecDeque<DocKey>,
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
            next_doc: None,
            snapshot_requests: VecDeque::new(),
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
    /// says hello again and nothing is sent until the remote answers. Snapshot
    /// requests of the old link that are still waiting are dropped.
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
        self.next_doc = None;
        // The old link's requests: a new link asks for what it needs itself.
        self.snapshot_requests.clear();
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

    /// What the remote showed it has, per document: its last `Hello`, plus what
    /// it sent since. Not what we sent it: that may still be on its way, or
    /// lost ([`unsynced`](Self::unsynced) covers it). `None` until it said
    /// hello (after [`reset`](Self::reset), until it says hello again). After
    /// [`forget_unconfirmed`](Self::forget_unconfirmed) (the link failed) it
    /// stays what the old link showed until the next `reset`.
    pub fn remote_versions(&self) -> Option<&BTreeMap<DocKey, VersionVector>> {
        self.confirmed.as_ref()
    }

    /// Whether we have everything the remote showed it has
    /// ([`remote_versions`](Self::remote_versions)): it said hello since the
    /// last [`reset`](Self::reset), and every document of `docs` it listed is
    /// included in the local version. Documents `docs` does not hold (reported
    /// in [`PumpOutcome::unknown_docs`]) are not waited for.
    ///
    /// Reads what the remote confirmed, not what we count as sent (which also
    /// holds what we handed to the link, and what may still be on its way,
    /// see [`Message::Hello`]'s `ack`): the question is whether we imported
    /// what the remote has, and what we sent is part of our own version
    /// anyway, so the answer is the same either way; the confirmed versions
    /// say so without relying on it.
    ///
    /// After a reconnect (`reset`, then pumps), the first time this holds is when
    /// to call `ConflictTracker::caught_up`. Hub: its `Hello` lists everything it
    /// has and comes before the updates, so this waits for them. Shared folder:
    /// `DirTransport` answers our `Hello` by reading the folder again and queues
    /// its own after the records it read, so this holds once everything the
    /// folder had at that moment has been received, however many pumps that
    /// takes. Between a link failure and the next `reset` it still answers for
    /// the old link: with [`crate::Connection`], which resets the session when
    /// it adopts a new link, ask it only while `Connection::is_online` (the new
    /// remote said hello).
    pub fn is_caught_up(&self, docs: &dyn DocSet) -> bool {
        let Some(remote) = &self.confirmed else {
            return false;
        };
        remote.iter().all(|(key, theirs)| {
            docs.version(key)
                .is_none_or(|ours| ours.includes_vv(theirs))
        })
    }

    /// Snapshot requests of the remote waiting for an answer
    /// ([`SessionConfig::max_snapshots_per_pump`]): pump again soon while this
    /// is not zero.
    pub fn pending_snapshot_requests(&self) -> usize {
        self.snapshot_requests.len()
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
    /// before it are still kept and answered (at most
    /// [`SessionConfig::max_snapshots_per_pump`] per pump, the rest on later
    /// pumps). Problems with single messages are in
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
        // Keep every request taken off the transport (a request taken here
        // is in no queue any more, and the remote may not ask again soon: the
        // shared folder asks again for a compaction only once the log
        // doubled) and answer the oldest few, even when the link failed or an
        // earlier answer did. The first error is returned.
        for doc in incoming.snapshot_requests {
            if !self.snapshot_requests.contains(&doc) {
                self.snapshot_requests.push_back(doc);
            }
        }
        let mut answered = Ok(());
        for _ in 0..self.config.max_snapshots_per_pump.max(1) {
            let Some(doc) = self.snapshot_requests.pop_front() else {
                break;
            };
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
        if send_due && self.send_missing(docs, transport, true, &mut out)? {
            self.last_send = Some(now);
        }
        Ok(out)
    }

    /// Send every document's ops the remote is not known to have right now,
    /// ignoring `min_send_interval` and `max_backlog`. Nothing is received and
    /// no `Hello` or presence goes out. Call it after the last [`pump`](Self::pump) before
    /// closing a link whose sends may be held back, or the last edits never
    /// leave. Sends nothing before the remote's first `Hello` (we do not know
    /// what it has), and nothing without [`SessionConfig::send_data`].
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
        if self.send_missing(docs, transport, false, &mut out)? {
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
    /// whole document. Documents we do not have are not answered, nor is
    /// anything without `send_data`.
    fn send_snapshot(
        &mut self,
        docs: &dyn DocSet,
        transport: &mut dyn Transport,
        doc: DocKey,
        out: &mut PumpOutcome,
    ) -> Result<(), SyncError> {
        if !self.config.send_data {
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
    ///
    /// With `flow`, stops before a document while the transport's backlog is
    /// over `max_backlog`; the next call starts with that document. Sends
    /// nothing without `send_data`.
    fn send_missing(
        &mut self,
        docs: &dyn DocSet,
        transport: &mut dyn Transport,
        flow: bool,
        out: &mut PumpOutcome,
    ) -> Result<bool, SyncError> {
        if !self.config.send_data {
            return Ok(false);
        }
        let Some(mut remote) = self.remote.take() else {
            return Ok(false);
        };
        let max_backlog = self.config.max_backlog.filter(|_| flow);
        let mut keys = docs.doc_keys();
        if let Some(next) = self.next_doc.take()
            && let Some(i) = keys.iter().position(|k| *k == next)
        {
            keys.rotate_left(i);
        }
        let mut sent = false;
        let mut failed = None;
        for key in keys {
            let Some(local) = docs.version(&key) else {
                continue;
            };
            let known = remote.entry(key.clone()).or_default();
            if known.includes_vv(&local) {
                continue;
            }
            if let Some(max) = max_backlog
                && transport.backlog() > max
            {
                self.next_doc = Some(key);
                self.stats.held_back += 1;
                break;
            }
            match docs.export_updates(&key, known) {
                Ok(data) => {
                    known.merge(&local);
                    if self.config.hello_interval.is_some() {
                        self.in_flight.sent(&key, self.hello_seq, &local);
                    }
                    sent = true;
                    let msg = Message::Updates {
                        doc: key,
                        version: local,
                        data,
                    };
                    if let Err(e) = self.send(transport, msg, out) {
                        failed.get_or_insert(e);
                    }
                }
                Err(e) => out.errors.push(e),
            }
        }
        self.remote = Some(remote);
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::docs::MemDocs;

    /// A link whose backlog grows with every message sent and drains only when
    /// the test says so.
    #[derive(Default)]
    struct Lagging {
        inbox: VecDeque<Message>,
        sent: Vec<Message>,
        backlog: usize,
    }

    impl Transport for Lagging {
        fn send(&mut self, msg: Message) -> Result<(), SyncError> {
            self.backlog += msg.approx_len();
            self.sent.push(msg);
            Ok(())
        }

        fn recv(&mut self) -> Result<Option<Message>, SyncError> {
            Ok(self.inbox.pop_front())
        }

        fn backlog(&self) -> usize {
            self.backlog
        }
    }

    impl Lagging {
        /// Documents of the data messages sent since the last call.
        fn data_sent(&mut self) -> Vec<String> {
            self.sent
                .drain(..)
                .filter_map(|m| match m {
                    Message::Updates { doc, .. } | Message::Snapshot { doc, .. } => {
                        Some(doc.to_string())
                    }
                    _ => None,
                })
                .collect()
        }
    }

    fn key(s: &str) -> DocKey {
        DocKey::parse(s).unwrap()
    }

    /// `n` pseudo-random characters (Loro does not get them much smaller).
    fn text(seed: u64, n: usize) -> String {
        let mut x = seed | 1;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                char::from(b'a' + (x % 26) as u8)
            })
            .collect()
    }

    fn set(docs: &MemDocs, doc: &str, value: String) {
        let d = docs.get(&key(doc)).unwrap();
        d.get_map("m").insert("v", value).unwrap();
        d.commit();
    }

    #[test]
    fn document_data_waits_while_the_transport_lags() {
        let mut docs = MemDocs::new();
        for (i, doc) in ["a", "b", "c"].into_iter().enumerate() {
            let d = bae_model::loro::LoroDoc::new();
            d.get_map("m")
                .insert("v", text(i as u64 + 1, 4000))
                .unwrap();
            d.commit();
            let snapshot = d.export(bae_model::loro::ExportMode::Snapshot).unwrap();
            docs.load(key(doc), &snapshot).unwrap();
        }
        let mut config = SessionConfig::new(ClientId::new("relay").unwrap());
        config.hello_interval = None;
        config.max_backlog = Some(1000);
        let mut s = SyncSession::new(config);
        s.set_presence(PresenceState::default());
        let peer = ClientId::new("peer").unwrap();
        // The peer has nothing, and says so twice (the second time while the
        // first message is still on its way).
        let mut t = Lagging::default();
        t.inbox.push_back(Message::hello(peer.clone(), Vec::new()));
        let pump = |s: &mut SyncSession, docs: &mut MemDocs, t: &mut Lagging| {
            s.pump(docs, t).unwrap();
            t.data_sent()
        };

        // One document goes out, however large (over `max_backlog` on its
        // own), then the rest waits.
        assert_eq!(pump(&mut s, &mut docs, &mut t), ["a"]);
        assert!(t.backlog > 4000);
        assert_eq!(s.stats().held_back, 1);
        // Waiting: the stale `Hello` is taken in, presence still goes out, no
        // document data does.
        t.inbox.push_back(Message::hello(peer.clone(), Vec::new()));
        s.set_presence(PresenceState {
            name: "relay".into(),
            ..PresenceState::default()
        });
        s.pump(&mut docs, &mut t).unwrap();
        assert!(t.inbox.is_empty());
        assert!(
            t.sent.iter().any(|m| matches!(m, Message::Presence { .. })),
            "{:?}",
            t.sent
        );
        assert!(t.data_sent().is_empty());
        assert_eq!(s.unsynced(&docs).len(), 3);
        // Each time the backlog drained, the next document: the one it stopped
        // at first. The stale `Hello` costs one more copy of `a`, not one per
        // `Hello`.
        for want in ["b", "c", "a"] {
            t.backlog = 0;
            assert_eq!(pump(&mut s, &mut docs, &mut t), [want]);
        }
        t.backlog = 0;
        assert!(pump(&mut s, &mut docs, &mut t).is_empty());
        assert!(s.unsynced(&docs).is_empty());

        // `flush` does not wait.
        set(&docs, "b", "edit".into());
        t.backlog = 1 << 20;
        s.pump(&mut docs, &mut t).unwrap();
        assert!(t.data_sent().is_empty());
        s.flush(&docs, &mut t).unwrap();
        assert_eq!(t.data_sent(), ["b"]);
        // Without flow control, nothing waits.
        set(&docs, "c", "edit".into());
        s.config_mut().max_backlog = None;
        assert_eq!(pump(&mut s, &mut docs, &mut t), ["c"]);
    }

    /// A burst of snapshot requests (a shared folder after a NAS hiccup) is
    /// answered a few per pump, oldest first: none is dropped, a repeated one
    /// is answered once, and `reset` drops what the old link asked for.
    #[test]
    fn snapshot_requests_are_answered_a_few_per_pump() {
        let mut docs = MemDocs::new();
        let names = ["a", "b", "c", "d", "e"];
        for (i, doc) in names.into_iter().enumerate() {
            let d = bae_model::loro::LoroDoc::new();
            d.get_map("m").insert("v", text(i as u64 + 1, 50)).unwrap();
            d.commit();
            let snapshot = d.export(bae_model::loro::ExportMode::Snapshot).unwrap();
            docs.load(key(doc), &snapshot).unwrap();
        }
        let mut s = SyncSession::new(SessionConfig::new(ClientId::new("me").unwrap()));
        let mut t = Lagging::default();
        let folder = ClientId::new("shared-folder").unwrap();
        let all: Vec<_> = names
            .iter()
            .map(|n| (key(n), docs.version(&key(n)).unwrap()))
            .collect();
        t.inbox
            .push_back(Message::hello(folder.clone(), all.clone()));
        for doc in names.into_iter().chain(["b"]) {
            t.inbox
                .push_back(Message::SnapshotRequest { doc: key(doc) });
        }
        let snapshots = |t: &mut Lagging| -> Vec<String> {
            t.sent
                .drain(..)
                .filter_map(|m| match m {
                    Message::Snapshot { doc, .. } => Some(doc.to_string()),
                    _ => None,
                })
                .collect()
        };
        s.pump(&mut docs, &mut t).unwrap();
        assert_eq!(snapshots(&mut t), ["a", "b"]);
        assert_eq!(s.pending_snapshot_requests(), 3);
        // A request for one that still waits is not queued twice.
        t.inbox
            .push_back(Message::SnapshotRequest { doc: key("d") });
        s.pump(&mut docs, &mut t).unwrap();
        assert_eq!(snapshots(&mut t), ["c", "d"]);
        s.pump(&mut docs, &mut t).unwrap();
        assert_eq!(snapshots(&mut t), ["e"]);
        assert_eq!(s.pending_snapshot_requests(), 0);
        s.pump(&mut docs, &mut t).unwrap();
        assert!(snapshots(&mut t).is_empty());

        // More per pump when configured; zero still answers one.
        s.config_mut().max_snapshots_per_pump = 0;
        for doc in ["a", "b"] {
            t.inbox
                .push_back(Message::SnapshotRequest { doc: key(doc) });
        }
        s.pump(&mut docs, &mut t).unwrap();
        assert_eq!(snapshots(&mut t), ["a"]);
        s.config_mut().max_snapshots_per_pump = 10;
        s.pump(&mut docs, &mut t).unwrap();
        assert_eq!(snapshots(&mut t), ["b"]);

        // The requests of a link that is gone are not answered on the next.
        s.config_mut().max_snapshots_per_pump = 1;
        for doc in ["c", "d"] {
            t.inbox
                .push_back(Message::SnapshotRequest { doc: key(doc) });
        }
        s.pump(&mut docs, &mut t).unwrap();
        assert_eq!(snapshots(&mut t), ["c"]);
        s.reset();
        assert_eq!(s.pending_snapshot_requests(), 0);
        t.inbox.push_back(Message::hello(folder, all));
        s.pump(&mut docs, &mut t).unwrap();
        assert!(snapshots(&mut t).is_empty());
    }

    /// A session on a link that may not write (a read-only hub token) sends no
    /// document data at all: not what the remote lacks, not snapshot answers,
    /// not on `flush`. It still says hello, publishes presence and receives.
    #[test]
    fn a_session_without_send_data_only_receives() {
        let mut docs = MemDocs::new();
        for doc in ["a", "b"] {
            let d = bae_model::loro::LoroDoc::new();
            d.get_map("m").insert("v", text(7, 100)).unwrap();
            d.commit();
            let snapshot = d.export(bae_model::loro::ExportMode::Snapshot).unwrap();
            docs.load(key(doc), &snapshot).unwrap();
        }
        let mut config = SessionConfig::new(ClientId::new("viewer").unwrap());
        config.send_data = false;
        let mut s = SyncSession::new(config);
        s.set_presence(PresenceState::default());
        let hub = ClientId::new("hub").unwrap();
        let mut t = Lagging::default();
        t.inbox.push_back(Message::hello(hub.clone(), Vec::new()));
        t.inbox
            .push_back(Message::SnapshotRequest { doc: key("a") });
        // What the hub has: `b` with an edit we lack.
        let theirs = bae_model::loro::LoroDoc::new();
        theirs
            .import(&docs.export_snapshot(&key("b")).unwrap())
            .unwrap();
        theirs.get_map("m").insert("w", "theirs").unwrap();
        theirs.commit();
        t.inbox.push_back(Message::Updates {
            doc: key("b"),
            version: theirs.oplog_vv(),
            data: theirs
                .export(bae_model::loro::ExportMode::updates(
                    &docs.version(&key("b")).unwrap(),
                ))
                .unwrap(),
        });
        let out = s.pump(&mut docs, &mut t).unwrap();
        assert_eq!(out.received, 3);
        assert_eq!(docs.version(&key("b")).unwrap(), theirs.oplog_vv());
        set(&docs, "a", "local".into());
        s.pump(&mut docs, &mut t).unwrap();
        s.flush(&docs, &mut t).unwrap();
        let kinds: Vec<_> = t
            .sent
            .iter()
            .map(|m| match m {
                Message::Hello { .. } => "hello",
                Message::Presence { .. } => "presence",
                _ => "data",
            })
            .collect();
        assert!(
            kinds.contains(&"hello") && kinds.contains(&"presence"),
            "{kinds:?}"
        );
        assert!(!kinds.contains(&"data"), "{kinds:?}");
        assert_eq!(s.stats().bytes_sent, 0);
        assert_eq!(s.unsynced(&docs), [key("a")]);
        // Nothing sent, nothing on its way: the hub's next `Hello`, which
        // acknowledges ours, still leaves the edit unsynced.
        assert!(s.in_flight.0.is_empty());
        let seq = s.hello_seq;
        t.inbox.push_back(Message::Hello {
            protocol: PROTOCOL_VERSION,
            client: hub,
            versions: vec![(key("b"), theirs.oplog_vv())],
            seq: 2,
            ack: Some(seq),
        });
        s.pump(&mut docs, &mut t).unwrap();
        assert!(s.in_flight.0.is_empty());
        assert_eq!(s.unsynced(&docs), [key("a")]);
    }

    /// Flow control and the in-flight records (`Hello` `seq`/`ack`) work
    /// together: a document held back for the backlog was not handed to the
    /// transport, so it is not on its way. The remote's `Hello` that
    /// acknowledges ours and lacks it leaves it to be sent once the backlog
    /// drained, and unsynced until then (were it recorded as in flight, it
    /// would count as sent for good and never go out).
    #[test]
    fn documents_held_back_are_not_on_their_way() {
        let mut docs = MemDocs::new();
        for (i, doc) in ["a", "b", "c"].into_iter().enumerate() {
            let d = bae_model::loro::LoroDoc::new();
            d.get_map("m")
                .insert("v", text(i as u64 + 11, 4000))
                .unwrap();
            d.commit();
            let snapshot = d.export(bae_model::loro::ExportMode::Snapshot).unwrap();
            docs.load(key(doc), &snapshot).unwrap();
        }
        let mut config = SessionConfig::new(ClientId::new("app").unwrap());
        // Our `Hello`s are numbered and acknowledged (a hub link); only the
        // first goes out here.
        config.hello_interval = Some(Duration::from_secs(3600));
        config.max_backlog = Some(1000);
        let mut s = SyncSession::new(config);
        let hub = ClientId::new("hub").unwrap();
        let hello = |seq, ack| Message::Hello {
            protocol: PROTOCOL_VERSION,
            client: hub.clone(),
            versions: Vec::new(),
            seq,
            ack,
        };
        let mut t = Lagging::default();
        // The hub has nothing (its first `Hello`, before it saw ours).
        t.inbox.push_back(hello(1, None));
        s.pump(&mut docs, &mut t).unwrap();
        assert_eq!(t.data_sent(), ["a"]);
        let in_flight = |s: &SyncSession| {
            s.in_flight
                .0
                .keys()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        };
        assert_eq!(in_flight(&s), ["a"]);
        assert_eq!(s.unsynced(&docs), [key("b"), key("c")]);
        // The hub acknowledges our `Hello` and still lists nothing: `a` may
        // be on its way, `b` and `c` were never sent.
        t.inbox.push_back(hello(2, Some(s.hello_seq)));
        s.pump(&mut docs, &mut t).unwrap();
        assert!(t.data_sent().is_empty(), "the backlog holds them back");
        assert_eq!(in_flight(&s), ["a"]);
        assert_eq!(s.unsynced(&docs), [key("b"), key("c")]);
        for want in ["b", "c"] {
            t.backlog = 0;
            s.pump(&mut docs, &mut t).unwrap();
            assert_eq!(t.data_sent(), [want]);
        }
        assert_eq!(in_flight(&s), ["a", "b", "c"]);
        assert!(s.unsynced(&docs).is_empty());
    }
}
