//! One open project: a [`Relay`] on its own thread, with the project's directory
//! attached as a peer for persistence.
//!
//! The directory uses the shared-folder format of `bae-sync` (`docs/spec/sync.md`
//! §4): the hub is client `hub` and appends to `updates/<doc>/hub.log`, compacted
//! into a snapshot when it grows. On start the relay reads it back through the
//! same transport, so loading, saving and crash repair are `bae-sync`'s code.
//! Like any shared-folder peer it is written at most every few hundred
//! milliseconds (`HubConfig::store_send_interval`, 300 ms). While clients are
//! connected that is what a crash can lose beyond what was already written
//! (they upload it again when they reconnect); what was held back is written as
//! soon as the last client leaves, and before the relay closes.
//!
//! Client connections are non-blocking WebSockets ([`bae_sync::HubTransport`]);
//! the relay thread polls them. It polls every `active_tick` (1 ms) right after
//! traffic and backs off to `quiet_tick` (10 ms) while they are quiet, so an
//! idle hub barely wakes up and an edit is forwarded within a few milliseconds.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use bae_sync::{
    ClientId, DirConfig, DirTransport, DocKey, DocSet, HubLimits, PeerHandle, Relay, SessionConfig,
    SyncError, ThreadedTransport, Transport,
};

use crate::config::{Access, HubConfig};
use crate::error::HubError;

/// The hub's own client ID: in its `Hello`s and as the name of its logs.
pub const HUB_CLIENT: &str = "hub";

/// How long loading the project directory may take before connections are
/// accepted anyway (they then upload what the hub has not read yet; harmless).
const LOAD_TIMEOUT: Duration = Duration::from_secs(60);
/// Nobody else writes to the hub's project directories, so the directory is only
/// re-read rarely (the first read on open is immediate).
const STORE_POLL: Duration = Duration::from_secs(60);
const STORE_TICK: Duration = Duration::from_millis(250);
/// Wait this long before reopening a project directory that failed.
const STORE_RETRY: Duration = Duration::from_secs(5);

/// Identifies a project on the hub; also its directory name. 1-64 ASCII letters,
/// digits, `_` and `-`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectId(Arc<str>);

impl ProjectId {
    pub fn new(s: &str) -> Result<Self, HubError> {
        let ok = !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if ok {
            Ok(ProjectId(Arc::from(s)))
        } else {
            Err(HubError::InvalidProjectId(s.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::fmt::Debug for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProjectId({})", self.0)
    }
}

/// Who is behind a connection (for logs and the status API).
#[derive(Clone, Debug)]
pub(crate) struct PeerInfo {
    pub token: Arc<str>,
    pub access: Access,
    pub addr: SocketAddr,
    pub connected: SystemTime,
    /// The largest message the client said it takes
    /// (`x-bae-max-incoming`); `None` for an older build. For the warning
    /// about a document that does not fit ([`too_large_hint`]).
    pub max_incoming: Option<usize>,
}

/// A connected client, as shown by the status API.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PeerStatus {
    /// The client ID from its `Hello` (absent until it said hello).
    pub client: Option<String>,
    /// Name of the token it used.
    pub token: String,
    pub access: Access,
    pub addr: String,
    /// Unix milliseconds.
    pub connected_ms: u64,
    /// The largest message the client said it takes in the handshake
    /// (`x-bae-max-incoming`; the hub sends it at most this much in one
    /// message); `null` for an older build.
    pub max_incoming: Option<u64>,
}

/// Snapshot of a project's state, refreshed after every pump.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectStatus {
    pub peers: Vec<PeerStatus>,
    pub docs: usize,
    /// The project directory is attached (edits are being saved).
    pub store_ok: bool,
    /// Documents whose edits are relayed but not saved: the hub's own log of
    /// them in the project directory is of a newer format version (a newer
    /// bae-hub ran on this data directory, then this older one; spec 4.3).
    pub not_saved: Vec<String>,
    /// Set while nobody is connected.
    pub idle_since: Option<Instant>,
    pub messages_received: u64,
    pub messages_sent: u64,
}

enum Event {
    Wake,
    Connect {
        transport: Box<dyn Transport + Send>,
        info: PeerInfo,
        reply: mpsc::Sender<PeerHandle>,
    },
    Shutdown,
}

/// Wakes the relay thread; repeated wakes before it runs collapse into one.
#[derive(Clone)]
struct Waker {
    events: mpsc::Sender<Event>,
    pending: Arc<AtomicBool>,
}

impl Waker {
    fn wake(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            let _ = self.events.send(Event::Wake);
        }
    }
}

/// Polling intervals of a relay thread.
#[derive(Clone, Copy, Debug)]
struct Ticks {
    active: Duration,
    quiet: Duration,
    empty: Duration,
}

/// Handle to a running project relay.
pub(crate) struct Project {
    id: ProjectId,
    events: mpsc::Sender<Event>,
    status: Arc<Mutex<ProjectStatus>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Project {
    /// Start the relay for the project directory `dir` (which must exist) and
    /// wait until what it holds is loaded. Blocks.
    pub fn open(id: ProjectId, dir: PathBuf, config: &HubConfig) -> Result<Arc<Project>, HubError> {
        let (ctx, events, status) = RelayThread::new(id.clone(), dir, config);
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name(format!("hub-{id}"))
            .spawn(move || ctx.run(ready_tx))
            .map_err(HubError::Network)?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Arc::new(Project {
                id,
                events,
                status,
                thread: Mutex::new(Some(thread)),
            })),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = thread.join();
                Err(HubError::ProjectClosed(id.to_string()))
            }
        }
    }

    pub fn id(&self) -> &ProjectId {
        &self.id
    }

    /// Attach a client connection to the relay. Blocks until the relay took it.
    pub fn connect(
        &self,
        transport: Box<dyn Transport + Send>,
        info: PeerInfo,
    ) -> Result<PeerHandle, HubError> {
        let (reply, rx) = mpsc::channel();
        self.events
            .send(Event::Connect {
                transport,
                info,
                reply,
            })
            .map_err(|_| HubError::ProjectClosed(self.id.to_string()))?;
        rx.recv()
            .map_err(|_| HubError::ProjectClosed(self.id.to_string()))
    }

    pub fn status(&self) -> ProjectStatus {
        self.status.lock().expect("status lock").clone()
    }

    /// The relay thread is still running.
    pub fn is_running(&self) -> bool {
        self.thread
            .lock()
            .expect("thread lock")
            .as_ref()
            .is_some_and(|t| !t.is_finished())
    }

    /// Stop the relay and wait for it: a final pump, then every connection is
    /// closed and everything queued for the project directory is written.
    pub fn shutdown(&self) {
        let _ = self.events.send(Event::Shutdown);
        let thread = self.thread.lock().expect("thread lock").take();
        if let Some(t) = thread {
            let _ = t.join();
        }
    }
}

struct RelayThread {
    id: ProjectId,
    dir: PathBuf,
    ticks: Ticks,
    store_send_interval: Duration,
    /// Flow control of client sessions (`HubConfig::limits`).
    client_backlog: usize,
    /// The largest message sent to clients (`HubConfig::limits`), for the
    /// warning about a document that does not fit ([`too_large_hint`]).
    max_message: usize,
    rx: mpsc::Receiver<Event>,
    waker: Waker,
    status: Arc<Mutex<ProjectStatus>>,
}

fn hub_client() -> ClientId {
    ClientId::new(HUB_CLIENT).expect("valid client ID")
}

/// Settings of the project directory's `DirTransport`.
fn store_dir_config() -> DirConfig {
    DirConfig {
        poll_interval: STORE_POLL,
        watch: false,
        // The cleanup of departed clients (spec 4.7) is off by default. A
        // project directory of the hub normally holds only the hub's own logs,
        // so here it only removes temporary files its crashes left; a
        // shared-folder project copied in has other clients' logs, which
        // nobody writes to any more and which go once the hub's logs hold
        // them. The open question that keeps it off elsewhere, how file locks
        // behave on NAS shares, does not arise: no other client runs on this
        // directory.
        inactive_after: Some(DirConfig::RECOMMENDED_INACTIVE_AFTER),
        ..DirConfig::default()
    }
}

fn open_store(dir: &Path, waker: &Waker) -> Result<ThreadedTransport, SyncError> {
    let store = DirTransport::open(dir, hub_client(), store_dir_config())?;
    let waker = waker.clone();
    Ok(ThreadedTransport::spawn(store, STORE_TICK, move || {
        waker.wake()
    }))
}

/// What the project directory's link works around (`Transport::notes`), as
/// far as the hub reports it.
#[derive(Default)]
struct StoreNotes {
    /// The link and the `notes_seq` copied last.
    seen: Option<(PeerHandle, u64)>,
    /// Documents whose edits are not saved (`LinkNotes::not_written`).
    not_saved: Vec<DocKey>,
}

impl StoreNotes {
    /// Look at the store's notes if they changed; warn about documents it
    /// newly refuses to write. Such a document's own log is of a newer
    /// format version (a newer bae-hub ran on this data directory): the hub
    /// leaves it as it is, relays the document's edits between clients, but
    /// saves none of them (clients keep theirs and send them again after a
    /// restart). Refusing to open the project instead would stop every other
    /// document too; a warning at load (and in `/v1/projects`, `not_saved`)
    /// tells the admin to run the newer hub again.
    fn check(&mut self, project: &ProjectId, relay: &Relay, store: Option<PeerHandle>) {
        let Some(handle) = store else {
            return;
        };
        let Some(seq) = relay.peer_notes_seq(handle) else {
            return;
        };
        if self.seen == Some((handle, seq)) {
            return;
        }
        let Some((seq, notes)) = relay.peer_notes(handle) else {
            return;
        };
        self.seen = Some((handle, seq));
        let new: Vec<String> = notes
            .not_written
            .iter()
            .filter(|d| !self.not_saved.contains(d))
            .map(|d| d.to_string())
            .collect();
        if !new.is_empty() {
            tracing::warn!(
                project = %project,
                docs = ?new,
                error = notes.last_error.as_deref().unwrap_or(""),
                "the hub's own log of these documents is of a newer format version (a newer \
                 bae-hub ran on this data directory): their edits are relayed but not saved; \
                 run the newer bae-hub again"
            );
        }
        self.not_saved = notes.not_written;
    }
}

/// What helps with a document that does not fit in one message to a client,
/// for the hub's warning: `len` is the message's size and `max` the limit it
/// was over (`SyncError::MessageTooLarge`), `hub_max` the hub's own
/// `max_message` (`max_message_mib`), `client` what the client said it takes
/// (`x-bae-max-incoming`, [`PeerInfo::max_incoming`]; `None` for an older
/// build, which takes what the hub sends). A link sends at most the smaller
/// of `hub_max` and `client`, so the hint says which of the two is below
/// `len` (or both), and by how much to raise it. Above 256 MiB (what clients
/// take by default; `HubConfig::validate` refuses more for the hub) nothing
/// helps: what does not fit is every op of the document the client lacks
/// (`SyncSession` sends Loro updates, not the current state), so deleting
/// content does not help, it adds ops; for a layer, only new layers with a
/// shorter history do.
fn too_large_hint(
    doc: &DocKey,
    len: usize,
    max: usize,
    hub_max: usize,
    client: Option<usize>,
) -> String {
    let most = HubLimits::default().max_incoming;
    if len > most {
        return if *doc == DocKey::History {
            format!(
                "{} is the most the hub sends in one message (max_message_mib cannot be set \
                 higher), so the project's version history cannot sync through the hub to \
                 this client; deleting versions does not help (what does not fit is its edit \
                 history)",
                size(most)
            )
        } else {
            format!(
                "{} is the most the hub sends in one message (max_message_mib cannot be set \
                 higher), so the layer cannot sync through the hub; deleting content from it \
                 does not help (what does not fit is its edit history, not its current \
                 size): move the content into new, smaller layers",
                size(most)
            )
        };
    }
    let need = len.div_ceil(1 << 20);
    let hub_short = hub_max < len;
    match client.filter(|c| *c < len) {
        Some(c) if hub_short => format!(
            "max_message_mib in [limits] is {} and the client takes at most {} per message \
             (its own setting); only raising both, to {need} MiB or more, lets it sync",
            size(hub_max),
            size(c)
        ),
        Some(c) => format!(
            "the client takes at most {} per message, which is its own setting; raising \
             max_message_mib does not help",
            size(c)
        ),
        None if hub_short => format!(
            "max_message_mib in [limits] is {}; raising it to {need} or more lets it sync",
            size(hub_max)
        ),
        None => format!("the link takes at most {} per message", size(max)),
    }
}

/// `n` bytes for logs, in the largest unit (MiB, KiB) it has a whole one
/// of, so a small limit does not read as "0.00 MiB".
fn size(n: usize) -> String {
    let (unit, name) = if n < 1 << 10 {
        return format!("{n} bytes");
    } else if n < 1 << 20 {
        (1 << 10, "KiB")
    } else {
        (1 << 20, "MiB")
    };
    if n.is_multiple_of(unit) {
        format!("{} {name}", n / unit)
    } else {
        format!("{:.2} {name}", n as f64 / unit as f64)
    }
}

/// Session settings for a client connection: flow control at the
/// connection's `max_backlog` (`HubConfig::limits`).
fn client_config(relay: &Relay, max_backlog: usize) -> SessionConfig {
    let mut config = relay.peer_config();
    config.max_backlog = Some(max_backlog);
    config
}

/// Session settings for the project directory as a relay peer.
fn store_config(relay: &Relay, send_interval: Duration) -> SessionConfig {
    let mut config = relay.peer_config();
    config.min_send_interval = send_interval;
    // The directory never loses what it was given, so no periodic `Hello` is
    // needed to repair it (spec 4.5). The first one still goes out at once, and
    // `load` waits for the directory's answer.
    config.hello_interval = None;
    config
}

enum Load {
    Done,
    TimedOut,
    StoreFailed(SyncError),
    Shutdown,
}

/// A relay whose project directory is connected and loaded (or still
/// loading, after `LOAD_TIMEOUT`).
struct Started {
    relay: Relay,
    store: PeerHandle,
    notes: StoreNotes,
}

impl RelayThread {
    /// The relay thread of the project directory `dir`, the sender of its
    /// events and its status.
    fn new(
        id: ProjectId,
        dir: PathBuf,
        config: &HubConfig,
    ) -> (RelayThread, mpsc::Sender<Event>, Arc<Mutex<ProjectStatus>>) {
        let (events, rx) = mpsc::channel();
        let waker = Waker {
            events: events.clone(),
            pending: Arc::new(AtomicBool::new(false)),
        };
        let status = Arc::new(Mutex::new(ProjectStatus::default()));
        let thread = RelayThread {
            id,
            dir,
            ticks: Ticks {
                active: config.active_tick,
                quiet: config.quiet_tick.max(config.active_tick),
                empty: config.empty_tick,
            },
            store_send_interval: config.store_send_interval,
            client_backlog: config.limits.max_backlog,
            max_message: config.limits.max_message,
            rx,
            waker,
            status: status.clone(),
        };
        (thread, events, status)
    }

    fn run(self, ready: mpsc::Sender<Result<(), HubError>>) {
        let started = match self.start() {
            Ok(Some(s)) => s,
            Ok(None) => return,
            Err(e) => {
                let _ = ready.send(Err(e));
                return;
            }
        };
        if ready.send(Ok(())).is_err() {
            return;
        }
        self.serve(started.relay, Some(started.store), started.notes);
    }

    /// Connect the project directory and load what it holds; warn about
    /// documents it will not write (before the project is ready). `None`:
    /// shut down while loading.
    fn start(&self) -> Result<Option<Started>, HubError> {
        let mut relay = Relay::new(hub_client());
        let store = open_store(&self.dir, &self.waker)?;
        let store_handle = relay.connect_with(
            Box::new(store),
            store_config(&relay, self.store_send_interval),
        );
        let started = Instant::now();
        let mut store_notes = StoreNotes::default();
        match self.load(&mut relay, store_handle) {
            Load::Done => {
                tracing::info!(
                    project = %self.id,
                    docs = relay.docs().doc_keys().len(),
                    ms = started.elapsed().as_millis() as u64,
                    "project loaded"
                );
                // The directory's first scan is done: a log it will not
                // write is known now.
                store_notes.check(&self.id, &relay, Some(store_handle));
            }
            Load::TimedOut => tracing::warn!(
                project = %self.id,
                "project still loading; accepting connections anyway"
            ),
            Load::StoreFailed(e) => return Err(e.into()),
            Load::Shutdown => return Ok(None),
        }
        Ok(Some(Started {
            relay,
            store: store_handle,
            notes: store_notes,
        }))
    }

    /// Pump until the project directory has delivered everything it holds (its
    /// `Hello` comes after all of its records).
    fn load(&self, relay: &mut Relay, store: PeerHandle) -> Load {
        let deadline = Instant::now() + LOAD_TIMEOUT;
        loop {
            self.waker.pending.store(false, Ordering::Release);
            let out = relay.pump();
            if let Some((_, e)) = out.disconnected.into_iter().find(|(h, _)| *h == store) {
                return Load::StoreFailed(e);
            }
            if relay
                .peers()
                .iter()
                .any(|(h, client)| *h == store && client.is_some())
            {
                return Load::Done;
            }
            if Instant::now() >= deadline {
                return Load::TimedOut;
            }
            match self.rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Event::Shutdown) | Err(RecvTimeoutError::Disconnected) => return Load::Shutdown,
                _ => {}
            }
        }
    }

    fn serve(self, mut relay: Relay, mut store: Option<PeerHandle>, mut notes: StoreNotes) {
        let mut peers: BTreeMap<PeerHandle, PeerInfo> = BTreeMap::new();
        let mut retry_store_at: Option<Instant> = None;
        let mut idle_since = Some(Instant::now());
        let (mut received, mut sent) = (0u64, 0u64);
        let mut tick = self.ticks.empty;
        loop {
            let mut shutdown = false;
            let first = match self.rx.recv_timeout(tick) {
                Ok(ev) => Some(ev),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => {
                    shutdown = true;
                    None
                }
            };
            let mut connected = false;
            for ev in first
                .into_iter()
                .chain(std::iter::from_fn(|| self.rx.try_recv().ok()))
            {
                match ev {
                    Event::Wake => {}
                    Event::Connect {
                        transport,
                        info,
                        reply,
                    } => {
                        let config = client_config(&relay, self.client_backlog);
                        let handle = relay.connect_with(transport, config);
                        tracing::info!(
                            project = %self.id,
                            peer = ?handle,
                            addr = %info.addr,
                            token = %info.token,
                            access = info.access.as_str(),
                            "client connected"
                        );
                        peers.insert(handle, info);
                        let _ = reply.send(handle);
                        connected = true;
                    }
                    Event::Shutdown => shutdown = true,
                }
            }
            self.waker.pending.store(false, Ordering::Release);
            let out = relay.pump();
            received += out.received as u64;
            sent += out.sent as u64;
            // Before the disconnects below forget who a failed peer was.
            self.log_skipped(out.errors, &peers);
            for (handle, e) in out.disconnected {
                if Some(handle) == store {
                    self.store_failed(&e, &mut store, &mut retry_store_at);
                } else if let Some(info) = peers.remove(&handle) {
                    tracing::info!(project = %self.id, peer = ?handle, addr = %info.addr, reason = %e, "client disconnected");
                }
            }
            notes.check(&self.id, &relay, store);
            // Nobody is editing, so holding writes back gains nothing, and the
            // next pump may be a whole empty tick away: write what the store's
            // interval held back (e.g. the last steps of a drag right before the
            // client quit) now. Cheap when there is nothing to write.
            if peers.is_empty() && store.is_some() {
                let out = relay.flush();
                sent += out.sent as u64;
                self.log_skipped(out.errors, &peers);
                if let Some((_, e)) = out
                    .disconnected
                    .into_iter()
                    .find(|(h, _)| Some(*h) == store)
                {
                    self.store_failed(&e, &mut store, &mut retry_store_at);
                }
            }
            // The old store was only dropped because its link failed, so nothing
            // could be flushed to it. Its directory claims only what it holds
            // (`DirTransport` writes nothing after a failed write), and the new
            // session sends everything it lacks as soon as it said hello (its
            // first send is never held back).
            if store.is_none() && retry_store_at.is_some_and(|t| Instant::now() >= t) {
                match open_store(&self.dir, &self.waker) {
                    Ok(t) => {
                        let config = store_config(&relay, self.store_send_interval);
                        store = Some(relay.connect_with(Box::new(t), config));
                        retry_store_at = None;
                        tracing::info!(project = %self.id, "project directory reattached");
                    }
                    Err(e) => {
                        tracing::warn!(project = %self.id, error = %e, "cannot reopen project directory");
                        retry_store_at = Some(Instant::now() + STORE_RETRY * 6);
                    }
                }
            }
            if peers.is_empty() {
                idle_since.get_or_insert_with(Instant::now);
            } else {
                idle_since = None;
            }
            // Poll fast right after traffic, then back off.
            tick = if peers.is_empty() {
                self.ticks.empty
            } else if out.received > 0 || connected {
                self.ticks.active
            } else {
                (tick * 2).clamp(self.ticks.active, self.ticks.quiet)
            };
            self.publish_status(&relay, &peers, store, &notes, idle_since, received, sent);
            if shutdown {
                break;
            }
        }
        // Unload and shutdown both end here. A final pump, then force out what
        // the store's interval held back (with clients still connected, the
        // last edits would otherwise never reach the disk), then wait until
        // the store's thread wrote it: its `send` only queues, so this is the
        // only place a failed last write shows. Dropping the relay closes every
        // connection.
        let out = relay.pump();
        let flushed = relay.flush();
        self.log_skipped(out.errors.into_iter().chain(flushed.errors), &peers);
        for (handle, e) in out.disconnected.into_iter().chain(flushed.disconnected) {
            if Some(handle) == store {
                tracing::error!(project = %self.id, error = %e, "project directory failed while closing; the last edits may not be saved");
                store = None;
            }
        }
        if let Some(handle) = store
            && let Some(Err(e)) = relay.close_peer(handle)
        {
            tracing::error!(project = %self.id, error = %e, "project directory failed while closing; the last edits may not be saved");
        }
        drop(relay);
        tracing::info!(project = %self.id, "project closed");
    }

    /// Log what the relay skipped for a peer (`RelayOutcome::errors`; the
    /// link goes on). A document that does not fit in one message to a
    /// client is logged when it is skipped, and again only when a retry does
    /// not fit either (spec 2.4 step 3), with what helps
    /// ([`too_large_hint`]); so is another client's presence that does not
    /// fit for it.
    fn log_skipped(
        &self,
        errors: impl IntoIterator<Item = (PeerHandle, SyncError)>,
        peers: &BTreeMap<PeerHandle, PeerInfo>,
    ) {
        for (handle, e) in errors {
            let info = peers.get(&handle);
            match &e {
                SyncError::MessageTooLarge {
                    doc: Some(doc),
                    size,
                    max,
                } => {
                    tracing::warn!(
                        project = %self.id,
                        peer = ?handle,
                        addr = ?info.map(|i| i.addr),
                        error = %e,
                        "a document does not fit in one message to this client; it is not \
                         sent to it until the client has more of it from elsewhere or \
                         reconnects ({})",
                        too_large_hint(
                            doc,
                            *size,
                            *max,
                            self.max_message,
                            info.and_then(|i| i.max_incoming)
                        )
                    );
                }
                SyncError::MessageTooLarge { doc: None, .. } => {
                    tracing::warn!(
                        project = %self.id,
                        peer = ?handle,
                        addr = ?info.map(|i| i.addr),
                        error = %e,
                        "a collaborator's presence (a very long selection) does not fit in \
                         one message to this client; it is not shown to it until a smaller \
                         one fits"
                    );
                }
                _ => {
                    tracing::warn!(project = %self.id, peer = ?handle, error = %e, "skipped a message");
                }
            }
        }
    }

    /// The project directory's link failed: stop using it, reopen it later.
    fn store_failed(
        &self,
        e: &SyncError,
        store: &mut Option<PeerHandle>,
        retry_at: &mut Option<Instant>,
    ) {
        tracing::error!(project = %self.id, error = %e, "project directory failed; edits are not being saved");
        *store = None;
        *retry_at = Some(Instant::now() + STORE_RETRY);
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_status(
        &self,
        relay: &Relay,
        peers: &BTreeMap<PeerHandle, PeerInfo>,
        store: Option<PeerHandle>,
        notes: &StoreNotes,
        idle_since: Option<Instant>,
        received: u64,
        sent: u64,
    ) {
        let clients: BTreeMap<PeerHandle, Option<ClientId>> = relay.peers().into_iter().collect();
        let status = ProjectStatus {
            peers: peers
                .iter()
                .map(|(h, info)| PeerStatus {
                    client: clients
                        .get(h)
                        .cloned()
                        .flatten()
                        .map(|c| c.as_str().to_owned()),
                    token: info.token.to_string(),
                    access: info.access,
                    addr: info.addr.to_string(),
                    connected_ms: info
                        .connected
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis() as u64),
                    max_incoming: info.max_incoming.map(|n| n as u64),
                })
                .collect(),
            docs: relay.docs().doc_keys().len(),
            store_ok: store.is_some(),
            not_saved: notes.not_saved.iter().map(|d| d.to_string()).collect(),
            idle_since,
            messages_received: received,
            messages_sent: sent,
        };
        *self.status.lock().expect("status lock") = status;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn project_ids() {
        assert!(ProjectId::new("film-a_01").is_ok());
        for bad in ["", "a b", "../x", "a/b", "a.b", "é", &"x".repeat(65)] {
            assert!(ProjectId::new(bad).is_err(), "{bad:?}");
        }
    }

    /// The hub opts in to the cleanup of departed clients (spec 4.7), which
    /// `DirConfig` leaves off by default.
    #[test]
    fn the_project_directory_is_cleaned_up() {
        let config = store_dir_config();
        assert_eq!(
            config.inactive_after,
            Some(DirConfig::RECOMMENDED_INACTIVE_AFTER)
        );
        assert_eq!(DirConfig::default().inactive_after, None);
        assert!(!config.watch);
    }

    /// A log record: `len: u32 LE`, the first 8 bytes of its BLAKE3, payload
    /// (spec 4.2).
    fn record(payload: &[u8]) -> Vec<u8> {
        let mut out = (payload.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(&blake3::hash(payload).as_bytes()[..8]);
        out.extend_from_slice(payload);
        out
    }

    /// Log output of a `tracing` subscriber, for tests.
    #[derive(Clone, Default)]
    struct CapturedLog(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for CapturedLog {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl CapturedLog {
        /// Warnings and errors, written here.
        fn subscriber(&self) -> impl tracing::Subscriber + Send + Sync + 'static {
            let log = self.clone();
            tracing_subscriber::fmt()
                .with_writer(move || log.clone())
                .with_ansi(false)
                .with_max_level(tracing::Level::WARN)
                .finish()
        }

        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
        }
    }

    /// The hub's own log of a document is of a newer format (a newer bae-hub
    /// ran on this data directory, then this one): the load warns about it
    /// (project and document) before the project is ready, the project
    /// opens, the log is left as it is, and the status (`/v1/projects`)
    /// lists the document as not saved. Edits to it are taken and not
    /// written.
    #[test]
    fn a_newer_format_own_log_is_reported_at_load() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("film");
        let doc_dir = dir.join("updates").join("anim").join("a");
        fs::create_dir_all(&doc_dir).unwrap();
        let header = format!(
            r#"{{"format":"{}","version":{},"doc":"anim/a","client":"{HUB_CLIENT}","generation":"x"}}"#,
            bae_sync::LOG_FORMAT,
            bae_sync::LOG_VERSION + 1
        );
        let log = doc_dir.join(format!("{HUB_CLIENT}.log"));
        let bytes = [record(header.as_bytes()), b"newer records".to_vec()].concat();
        fs::write(&log, &bytes).unwrap();
        let config = HubConfig::new(tmp.path());

        // The start of the relay thread, as `Project::open` runs it before
        // the project is ready (on this thread, to capture its log).
        let id = ProjectId::new("film").unwrap();
        let (thread, _events, _status) = RelayThread::new(id.clone(), dir.clone(), &config);
        let warnings = CapturedLog::default();
        let started = tracing::subscriber::with_default(warnings.subscriber(), || thread.start());
        let started = started.unwrap().expect("loaded");
        assert_eq!(started.notes.not_saved, [DocKey::parse("anim/a").unwrap()]);
        let text = warnings.text();
        assert!(
            text.contains("WARN")
                && text.contains("project=film")
                && text.contains("anim/a")
                && text.contains("newer format version"),
            "{text}"
        );
        // Let go of the directory.
        drop(started);
        drop(thread);

        let project = Project::open(id, dir, &config).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while project.status().not_saved.is_empty() {
            assert!(Instant::now() < deadline, "not reported");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(project.status().not_saved, ["anim/a"]);
        assert!(project.status().store_ok);
        project.shutdown();
        assert_eq!(fs::read(&log).unwrap(), bytes, "left as it is");
    }

    /// The warning about a document that does not fit in one message to a
    /// client says what helps. It used to tell the admin to raise
    /// `max_message_mib` in every case: at the default (256 MiB) the hub then
    /// refuses to start, and when the client's own limit is the smaller one
    /// the hub's setting changes nothing. At the default it then said the
    /// layer could sync once "made smaller", but deleting content from it
    /// only adds to the ops the client lacks. Later it looked only at the
    /// limits, not at the message: a 300 MiB layer got "raise
    /// max_message_mib" on a 64 MiB hub, and with both limits at 64 MiB only
    /// the client's was blamed.
    #[test]
    fn the_too_large_warning_says_what_helps() {
        let check = |hub_max_mib: u64| {
            let mut c = HubConfig::new(".");
            c.apply_file(
                crate::config::FileConfig::parse(&format!(
                    "[limits]\nmax_message_mib = {hub_max_mib}"
                ))
                .unwrap(),
            );
            c.validate().is_ok()
        };
        let layer = DocKey::parse("anim/a").unwrap();
        let most = HubLimits::default().max_incoming;
        let default = HubConfig::new(".").limits.max_message;
        assert_eq!(default, most);
        assert!(!check((most >> 20) as u64 + 1));
        assert!(check((most >> 20) as u64));
        const MIB: usize = 1 << 20;

        // Over what the hub can send at all: nothing helps but new layers,
        // whatever the limits and whatever the client takes.
        for (hub_max, client) in [
            (default, Some(default)),
            (default, Some(512 * MIB)),
            (default, None),
            (64 * MIB, Some(64 * MIB)),
            (64 * MIB, Some(256 * MIB)),
        ] {
            let max = client.map_or(hub_max, |c| c.min(hub_max));
            let hint = too_large_hint(&layer, 300 * MIB, max, hub_max, client);
            assert!(
                !hint.contains("raising")
                    && !hint.contains("own setting")
                    && hint.contains("256 MiB is the most the hub sends")
                    && hint.contains("deleting content")
                    && hint.contains("history")
                    && hint.contains("new, smaller layers"),
                "{hub_max} {client:?}: {hint}"
            );
        }
        // The version history is no layer: no advice about layers.
        let hint = too_large_hint(&DocKey::History, 300 * MIB, default, default, None);
        assert!(
            hint.contains("version history") && !hint.contains("layer"),
            "{hint}"
        );

        // Only the hub's limit is below it: raise it, by enough (an older
        // client did not say what it takes, and takes what the hub sends).
        for client in [Some(most), Some(150 * MIB), None] {
            let hint = too_large_hint(&layer, 100 * MIB + 1, 64 * MIB, 64 * MIB, client);
            assert!(
                hint.contains("max_message_mib in [limits] is 64 MiB")
                    && hint.contains("raising it to 101 or more"),
                "{client:?}: {hint}"
            );
        }
        // Only the client's: the hub's setting does not help.
        for hub_max in [default, 200 * MIB] {
            let hint = too_large_hint(&layer, 100 * MIB, 64 * MIB, hub_max, Some(64 * MIB));
            assert!(
                hint.contains("takes at most 64 MiB per message") && hint.contains("does not help"),
                "{hub_max}: {hint}"
            );
        }
        // Both, also when they are equal.
        for (hub_max, client) in [(64 * MIB, 64 * MIB), (64 * MIB, 32 * MIB)] {
            let hint = too_large_hint(&layer, 100 * MIB, client, hub_max, Some(client));
            assert!(
                hint.contains("max_message_mib in [limits] is 64 MiB")
                    && hint.contains("only raising both, to 100 MiB or more"),
                "{hub_max} {client}: {hint}"
            );
        }
    }

    /// Sizes in the warning are in the largest unit they have a whole one
    /// of. A client may say it takes a few bytes (`x-bae-max-incoming`
    /// takes any positive number): in MiB that read "0.00 MiB".
    #[test]
    fn small_limits_in_the_too_large_warning_are_not_zero() {
        let hub_max = HubLimits::default().max_incoming;
        let layer = DocKey::parse("anim/a").unwrap();
        for (max, want) in [
            (100, "100 bytes"),
            (1023, "1023 bytes"),
            (1024, "1 KiB"),
            (5242, "5.12 KiB"),
            (300 << 10, "300 KiB"),
            (3 << 19, "1.50 MiB"),
            (64 << 20, "64 MiB"),
        ] {
            let hint = too_large_hint(&layer, max + 1, max, hub_max, Some(max));
            assert!(
                hint.contains(&format!("takes at most {want} per message")),
                "{max}: {hint}"
            );
            assert!(!hint.contains("0.00"), "{max}: {hint}");
        }
    }

    /// Client sessions hold back at the configured `max_backlog_mib`, not at
    /// the session default (which happens to equal the hub's default).
    #[test]
    fn client_sessions_use_the_configured_backlog() {
        let relay = Relay::new(hub_client());
        let config = client_config(&relay, 3 << 20);
        assert_eq!(config.max_backlog, Some(3 << 20));
        assert_eq!(config.client, relay.peer_config().client);
    }

    /// A link that takes messages up to `.1` bytes, as a hub link does.
    struct Limited(bae_sync::MemoryTransport, usize);

    impl Transport for Limited {
        fn send(&mut self, msg: bae_sync::Message) -> Result<(), SyncError> {
            let size = msg.encoded_len();
            if size > self.1 {
                return Err(SyncError::MessageTooLarge {
                    doc: msg.doc().cloned(),
                    size,
                    max: self.1,
                });
            }
            self.0.send(msg)
        }

        fn recv(&mut self) -> Result<Option<bae_sync::Message>, SyncError> {
            self.0.recv()
        }

        fn max_message_size(&self) -> Option<usize> {
            Some(self.1)
        }
    }

    /// A client of a project's relay thread, linked to it in memory.
    struct TestClient {
        docs: bae_sync::MemDocs,
        session: bae_sync::SyncSession,
        link: Limited,
        /// Documents of the `MessageTooLarge` errors it reported.
        too_large: Vec<DocKey>,
    }

    impl TestClient {
        /// Joins with `root` and `anim/a` as in `base`; both ends of the
        /// link take at most `max` bytes per message.
        fn join(project: &Project, name: &str, base: &[u8], max: usize) -> TestClient {
            let (ours, theirs) = bae_sync::MemoryTransport::pair();
            let info = PeerInfo {
                token: Arc::from(name),
                access: Access::Write,
                addr: SocketAddr::from(([127, 0, 0, 1], 1)),
                connected: SystemTime::now(),
                max_incoming: Some(max),
            };
            project
                .connect(Box::new(Limited(theirs, max)), info)
                .unwrap();
            let mut docs = bae_sync::MemDocs::new();
            for doc in ["root", "anim/a"] {
                docs.load(DocKey::parse(doc).unwrap(), base).unwrap();
            }
            TestClient {
                docs,
                session: bae_sync::SyncSession::new(SessionConfig::new(
                    ClientId::new(name).unwrap(),
                )),
                link: Limited(ours, max),
                too_large: Vec::new(),
            }
        }

        fn pump(&mut self) {
            let out = self.session.pump(&mut self.docs, &mut self.link).unwrap();
            for e in out.errors {
                match e {
                    SyncError::MessageTooLarge { doc: Some(doc), .. } => self.too_large.push(doc),
                    e => panic!("{e}"),
                }
            }
        }

        /// `len` pseudo-random letters under `name` in `doc`.
        fn note(&self, doc: &str, name: &str, len: usize, seed: u64) {
            let mut x = seed | 1;
            let text: String = (0..len)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    char::from(b'a' + (x % 26) as u8)
                })
                .collect();
            let d = self.docs.get(&DocKey::parse(doc).unwrap()).unwrap();
            d.get_map("m").insert(name, text).unwrap();
            d.commit();
        }

        /// Whether it has everything `from` has of `doc`.
        fn has_all_of(&self, from: &TestClient, doc: &str) -> bool {
            let doc = DocKey::parse(doc).unwrap();
            let want = from.docs.version(&doc).unwrap();
            self.docs
                .version(&doc)
                .is_some_and(|v| v.includes_vv(&want))
        }
    }

    fn pump_until(clients: &mut [TestClient], what: &str, done: impl Fn(&[TestClient]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            for c in clients.iter_mut() {
                c.pump();
            }
            if done(clients) {
                return;
            }
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Which side logs a layer that does not fit in one message (what
    /// deploy/README.md and the README tell admins). The hub warns when it
    /// holds a layer it cannot send to a client: grown in edits that each
    /// fit, a newcomer lacks all of them. A client whose own edit does not
    /// fit skips it and reports it itself: nothing reaches the hub, which
    /// logs nothing.
    #[test]
    fn the_hub_warns_only_about_layers_it_cannot_send() {
        const LIMIT: usize = 64 << 10;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("film");
        fs::create_dir_all(&dir).unwrap();
        let mut config = HubConfig::new(tmp.path());
        config.limits.max_message = LIMIT;
        config.limits.max_incoming = LIMIT;
        // The relay thread, as `Project::open` starts it, logging here.
        let log = CapturedLog::default();
        let id = ProjectId::new("film").unwrap();
        let (thread, events, status) = RelayThread::new(id.clone(), dir, &config);
        let subscriber = log.subscriber();
        let (ready_tx, ready_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            tracing::subscriber::with_default(subscriber, || thread.run(ready_tx));
        });
        ready_rx.recv().unwrap().unwrap();
        let project = Project {
            id,
            events,
            status,
            thread: Mutex::new(Some(handle)),
        };
        let base = bae_model::loro::LoroDoc::new();
        base.get_map("m").insert("base", "base").unwrap();
        base.commit();
        let base = base.export(bae_model::loro::ExportMode::Snapshot).unwrap();
        let mut clients = vec![
            TestClient::join(&project, "editor", &base, LIMIT),
            TestClient::join(&project, "early", &base, LIMIT),
        ];
        pump_until(&mut clients, "hello", |c| {
            c.iter().all(|c| c.session.remote_versions().is_some())
        });

        // The editor's own edit does not fit: it skips it and says so.
        clients[0].note("root", "big", 100 << 10, 1);
        pump_until(&mut clients, "the editor's report", |c| {
            !c[0].too_large.is_empty()
        });
        clients[0].note("anim/a", "small", 100, 2);
        pump_until(&mut clients, "the small edit", |c| {
            c[1].has_all_of(&c[0], "anim/a")
        });
        assert_eq!(clients[0].too_large, [DocKey::parse("root").unwrap()]);
        assert!(!clients[1].has_all_of(&clients[0], "root"));
        assert_eq!(log.text(), "", "the hub logged a layer it never got");

        // `anim/a` grows in edits that each fit; a newcomer lacks them all.
        for i in 0..4 {
            clients[0].note("anim/a", &format!("note{i}"), 20 << 10, 3 + i);
            pump_until(&mut clients, "a note", |c| c[1].has_all_of(&c[0], "anim/a"));
        }
        clients.push(TestClient::join(&project, "late", &base, LIMIT));
        let deadline = Instant::now() + Duration::from_secs(20);
        while !log.text().contains("does not fit") {
            assert!(Instant::now() < deadline, "no warning");
            for c in &mut clients {
                c.pump();
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        project.shutdown();
        let text = log.text();
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(
            text.contains("WARN")
                && text.contains("does not fit in one message to this client")
                && text.contains("for document anim/a"),
            "{text}"
        );
        // The hint knows what this client said it takes (`PeerInfo`): as
        // little as the hub sends, so raising the hub's alone does not help.
        assert!(text.contains("only raising both"), "{text}");
        assert!(!clients[2].has_all_of(&clients[0], "anim/a"));
        assert!(clients.iter().all(|c| c.too_large.len() <= 1));
    }
}
