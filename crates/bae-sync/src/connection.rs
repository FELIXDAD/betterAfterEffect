//! [`Connection`]: share or join a project from the thread that owns the
//! stage, without ever blocking it (`docs/spec/sync.md` §7).
//!
//! Everything that waits for the disk or the network happens on background
//! threads: opening the shared folder (a scan of every log on the NAS),
//! the hub's TCP connect and WebSocket handshake, dropping a dead link, and
//! closing (which waits until the hub confirmed it read our last messages).
//! The stage thread calls [`Connection::pump`] every frame or tick: it adopts a
//! link once a worker has one ready, then pumps the [`SyncSession`].
//!
//! When a link fails the connection goes offline, keeps the local edits (the
//! documents are never touched by a disconnect) and retries with exponential
//! backoff; a fresh link plus [`SyncSession::reset`] makes both sides
//! exchange `Hello`s and send each other what is missing. Failures that
//! retrying does not fix (bad token, no access, no such project, client ID in
//! use) wait for [`Connection::retry`], or a slow retry.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bae_model::{PrimPath, StageEvent};

use crate::client::ClientId;
use crate::dir::{DirConfig, DirTransport};
use crate::docs::DocSet;
use crate::error::SyncError;
#[cfg(feature = "hub")]
use crate::hub::{HubLimits, HubTransport};
use crate::presence::{PeerPresence, PresenceState};
use crate::protocol::DocKey;
use crate::session::{SessionConfig, SyncSession};
use crate::threaded::ThreadedTransport;
use crate::transport::Transport;

/// What this client may do with the project.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Access {
    /// Not known: never connected yet, or a hub that does not say. Edits are
    /// sent (the remote decides).
    #[default]
    Unknown,
    /// The remote does not take our edits (a read-only hub token, a shared
    /// folder this user cannot write). Grey out editing; local edits are not
    /// sent.
    Read,
    /// Edits are accepted.
    Write,
}

impl Access {
    /// `unknown`, `read` or `write`.
    pub fn as_str(self) -> &'static str {
        match self {
            Access::Unknown => "unknown",
            Access::Read => "read",
            Access::Write => "write",
        }
    }

    /// `read` / `write` (any letter case); anything else is
    /// [`Access::Unknown`].
    pub fn parse(s: &str) -> Access {
        if s.trim().eq_ignore_ascii_case("read") {
            Access::Read
        } else if s.trim().eq_ignore_ascii_case("write") {
            Access::Write
        } else {
            Access::Unknown
        }
    }

    /// Whether local edits are sent: everything but [`Access::Read`].
    pub fn can_write(self) -> bool {
        self != Access::Read
    }
}

/// `min_send_interval` of shared-folder sessions (spec 4.5): a drag becomes a
/// few appends per second on the NAS instead of dozens.
pub const FOLDER_SEND_INTERVAL: Duration = Duration::from_millis(300);
/// How often the I/O thread of a shared-folder link looks for news (the folder
/// itself is scanned every [`DirConfig::poll_interval`]).
const FOLDER_TICK: Duration = Duration::from_millis(10);
/// A folder's lock held by our own client ID may be a link of this process
/// that was just dropped or closed and is still letting go (on its own
/// thread): [`Failure::ClientIdInUse`] only once it lasted this long.
const CLIENT_ID_GRACE: Duration = Duration::from_secs(2);
/// How often the I/O thread of a hub link reads the socket.
#[cfg(feature = "hub")]
const HUB_TICK: Duration = Duration::from_millis(2);

/// Where the project is shared.
#[derive(Clone, PartialEq, Eq)]
pub enum Target {
    /// A shared folder: the `.bae` project directory, usually on a NAS share
    /// (spec §4).
    Folder(PathBuf),
    /// A hub (feature `hub`, spec §3.1): `ws://host:port/projects/<id>` and the
    /// team token, sent as `Authorization: Bearer <token>`.
    #[cfg(feature = "hub")]
    Hub { url: String, token: Option<String> },
}

impl std::fmt::Debug for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::Folder(path) => f.debug_tuple("Folder").field(path).finish(),
            #[cfg(feature = "hub")]
            Target::Hub { url, token } => f
                .debug_struct("Hub")
                .field("url", url)
                // Never print the secret.
                .field("token", &token.as_ref().map(|_| "…"))
                .finish(),
        }
    }
}

impl Target {
    /// Recommended session settings for this target: shared folders send at
    /// most every [`FOLDER_SEND_INTERVAL`] and do not repeat `Hello`s (the
    /// folder never loses data, spec 4.5); hubs send at once and say `Hello`
    /// every 5 seconds (the hub drops connections silent for 60 seconds).
    pub fn session_config(&self, client: ClientId) -> SessionConfig {
        let mut config = SessionConfig::new(client);
        match self {
            Target::Folder(_) => {
                config.hello_interval = None;
                config.min_send_interval = FOLDER_SEND_INTERVAL;
            }
            #[cfg(feature = "hub")]
            Target::Hub { .. } => {}
        }
        config
    }

    /// Whether the target can be used at all (a hub URL's scheme and host),
    /// without touching the disk or the network.
    fn check(&self) -> Result<(), SyncError> {
        match self {
            Target::Folder(_) => Ok(()),
            #[cfg(feature = "hub")]
            Target::Hub { url, token } => crate::hub::check_url(url, token.as_deref()),
        }
    }
}

/// When to retry after a link failed.
#[derive(Clone, Debug, PartialEq)]
pub struct Backoff {
    /// Wait before the first retry; doubles after every failed attempt...
    pub first: Duration,
    /// ...up to this.
    pub max: Duration,
    /// Up to this fraction (0 to 1) of each wait is taken off at random, so
    /// clients that lost the same hub do not all come back at the same moment.
    pub jitter: f64,
    /// A link that stayed up this long counts as stable: after it drops, the
    /// retries start again from `first`.
    pub stable_after: Duration,
    /// Retry failures that need attention ([`Failure`]: bad token, no access,
    /// client ID in use) this often anyway, e.g. in case the other session
    /// with our client ID quit. `None`: only on [`Connection::retry`].
    pub failed_retry: Option<Duration>,
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff {
            first: Duration::from_millis(500),
            max: Duration::from_secs(30),
            jitter: 0.25,
            stable_after: Duration::from_secs(10),
            failed_retry: Some(Duration::from_secs(60)),
        }
    }
}

impl Backoff {
    /// Wait after `failures` failed attempts in a row (0: the first retry),
    /// before jitter.
    pub fn delay(&self, failures: u32) -> Duration {
        let factor = 1u32 << failures.min(24);
        self.first.saturating_mul(factor).min(self.max)
    }
}

/// Settings of a [`Connection`]. Start from [`ConnectionConfig::new`].
#[derive(Clone)]
pub struct ConnectionConfig {
    /// This client (spec §1: one per installation for the app, one per agent).
    pub client: ClientId,
    /// Presence to publish as soon as the link is up (and again after every
    /// reconnect); change it later with [`Connection::set_presence`].
    pub presence: Option<PresenceState>,
    /// Called from background threads whenever the connection wants a
    /// [`Connection::pump`]: something arrived, a connect attempt finished, a
    /// retry or keep-alive is due, a close finished. Hook it to "request a
    /// repaint". It must not block.
    pub wake: Arc<dyn Fn() + Send + Sync>,
    /// Session settings; `None`: [`Target::session_config`]. `client` is
    /// replaced by the one above, `read_only` follows [`Connection::access`].
    pub session: Option<SessionConfig>,
    pub backoff: Backoff,
    /// Shared-folder settings.
    pub folder: DirConfig,
    /// How often a link's I/O thread looks for incoming messages; `None`: 2 ms
    /// for hubs, 10 ms for shared folders.
    pub io_tick: Option<Duration>,
    /// Hubs: timeout of each connect step (TCP connect, handshake).
    #[cfg(feature = "hub")]
    pub hub_timeout: Duration,
    /// Hubs: message size and stall limits.
    #[cfg(feature = "hub")]
    pub hub_limits: HubLimits,
}

impl ConnectionConfig {
    pub fn new(client: ClientId, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        ConnectionConfig {
            client,
            presence: None,
            wake,
            session: None,
            backoff: Backoff::default(),
            folder: DirConfig::default(),
            io_tick: None,
            #[cfg(feature = "hub")]
            hub_timeout: Duration::from_secs(10),
            #[cfg(feature = "hub")]
            hub_limits: HubLimits::default(),
        }
    }
}

impl std::fmt::Debug for ConnectionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionConfig")
            .field("client", &self.client)
            .field("presence", &self.presence)
            .field("session", &self.session)
            .field("backoff", &self.backoff)
            .field("folder", &self.folder)
            .finish_non_exhaustive()
    }
}

/// Why a connection stopped retrying on its own schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Failure {
    /// The hub refused the token (HTTP 401): ask for another one.
    BadToken,
    /// The token may not open this project (HTTP 403).
    NoAccess,
    /// No such project on the hub, and the token may not create it (HTTP 404,
    /// read-only tokens).
    NoSuchProject,
    /// Another session with the same client ID is attached to this folder (on
    /// this machine, or another one where the lock works over the share).
    ClientIdInUse,
    /// Another refusal that retrying does not fix: the HTTP status (e.g. 400,
    /// invalid project ID).
    Refused(u16),
    /// The target itself is unusable: an invalid or unsupported URL.
    BadTarget,
}

/// Where a connection stands.
#[derive(Clone, Debug, PartialEq)]
pub enum ConnectionState {
    /// A connect attempt is running, or the link is up and the remote has not
    /// said what it has yet.
    Connecting,
    /// In sync: edits go out and come in as they happen.
    Online { since: Instant },
    /// The link failed; edits stay local. Retries in `retry_in` (counting down
    /// while nothing else changes; [`ConnectionStatus::generation`] does not
    /// change with it).
    Offline { retry_in: Duration, error: String },
    /// Needs attention: retried only by [`Connection::retry`] or every
    /// [`Backoff::failed_retry`].
    Failed { failure: Failure, reason: String },
}

/// What the status bar shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionStatus {
    pub state: ConnectionState,
    /// The last access level the remote granted ([`Access::Unknown`] before
    /// the first link).
    pub access: Access,
    /// Some local edits are not known to have reached the remote: not sent
    /// yet (offline, read-only, held back by `min_send_interval`), or not
    /// confirmed since the last reconnect.
    pub unsynced: bool,
    /// Others online (0 unless online).
    pub peers: usize,
    /// Increases whenever any of the above changes (the countdown in
    /// `Offline::retry_in` aside) or someone's presence changes: redraw when
    /// it differs from the one last drawn.
    pub generation: u64,
}

/// What one [`Connection::pump`] did.
#[derive(Debug, Default)]
pub struct ConnectionOutcome {
    /// Events of remote imports into the documents, in order (also broadcast
    /// through `Stage::subscribe`; never in an undo stack).
    pub events: Vec<StageEvent>,
    /// Documents the remote has that the local set lacks, e.g. a sublayer a
    /// collaborator created (spec 2.5): add it to the stack, the next pumps
    /// fetch its contents. Reported again with every remote `Hello` until
    /// added.
    pub unknown_docs: Vec<DocKey>,
    /// Someone came online, went offline or changed their presence (also
    /// when the link went up or down).
    pub presence_changed: bool,
    /// [`ConnectionStatus`] changed (its `generation` increased).
    pub status_changed: bool,
    /// Messages that were skipped (corrupt payloads, unknown types).
    pub errors: Vec<SyncError>,
}

/// A link a worker made, ready to be adopted on the stage thread.
struct Link {
    transport: ThreadedTransport,
    access: Access,
}

/// What a connect worker needs (everything but the target).
struct Worker {
    client: ClientId,
    folder: DirConfig,
    io_tick: Option<Duration>,
    #[cfg(feature = "hub")]
    hub_timeout: Duration,
    #[cfg(feature = "hub")]
    hub_limits: HubLimits,
    wake: Arc<dyn Fn() + Send + Sync>,
}

/// I/O errors that mean "you may not write here".
fn read_only_error(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
    )
}

impl Worker {
    /// The blocking part: open the folder or connect to the hub, find out the
    /// access level, start the link's I/O thread.
    fn connect(&self, target: &Target) -> Result<Link, SyncError> {
        let wake = self.wake.clone();
        let wake = move || wake();
        match target {
            Target::Folder(path) => {
                let client = self.client.clone();
                // Write access = we can create our lock file in `updates/`.
                let (dir, access) =
                    match DirTransport::open(path, client.clone(), self.folder.clone()) {
                        Ok(dir) => (dir, Access::Write),
                        Err(SyncError::Io { source, .. }) if read_only_error(&source) => (
                            DirTransport::open_read_only(path, client, self.folder.clone())?,
                            Access::Read,
                        ),
                        Err(e) => return Err(e),
                    };
                let tick = self.io_tick.unwrap_or(FOLDER_TICK);
                Ok(Link {
                    transport: ThreadedTransport::spawn(dir, tick, wake),
                    access,
                })
            }
            #[cfg(feature = "hub")]
            Target::Hub { url, token } => {
                let hub = HubTransport::connect_with(
                    url,
                    token.as_deref(),
                    self.hub_timeout,
                    self.hub_limits,
                )?;
                let access = hub.access();
                let tick = self.io_tick.unwrap_or(HUB_TICK);
                Ok(Link {
                    transport: ThreadedTransport::spawn(hub, tick, wake),
                    access,
                })
            }
        }
    }
}

enum Phase {
    /// A worker is connecting; its result arrives here.
    Connecting {
        rx: Receiver<Result<Link, SyncError>>,
    },
    /// A link is up. `since`: when the remote first said what it has.
    Linked {
        transport: ThreadedTransport,
        adopted: Instant,
        since: Option<Instant>,
    },
    Offline {
        retry_at: Instant,
        error: String,
    },
    Failed {
        failure: Failure,
        reason: String,
        retry_at: Option<Instant>,
    },
    /// Only while closing or dropping.
    Closed,
}

/// Everything [`ConnectionStatus::generation`] tracks.
#[derive(Clone, Debug, PartialEq)]
struct StatusKey {
    state: u8,
    since: Option<Instant>,
    retry_at: Option<Instant>,
    text: String,
    failure: Option<Failure>,
    access: Access,
    unsynced: bool,
    peers: usize,
}

/// Calls `wake` at the earliest deadline it was given: pumps that are due
/// while the app is idle (a retry, edits held back by `min_send_interval`,
/// `Hello`s and presence heartbeats, which keep a hub from dropping us).
struct Alarm {
    tx: Sender<Instant>,
    armed: Option<Instant>,
}

impl Alarm {
    fn spawn(wake: Arc<dyn Fn() + Send + Sync>) -> Alarm {
        let (tx, rx) = mpsc::channel::<Instant>();
        // Without the thread (none could be started), wakes only come from
        // the links and `set` fails quietly: the app's own frames still pump.
        let _ = std::thread::Builder::new()
            .name("bae-sync-alarm".into())
            .spawn(move || {
                let mut next: Option<Instant> = None;
                loop {
                    let got = match next {
                        None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                        Some(t) => rx.recv_timeout(t.saturating_duration_since(Instant::now())),
                    };
                    match got {
                        Ok(t) => next = Some(next.map_or(t, |n| n.min(t))),
                        Err(RecvTimeoutError::Timeout) => {
                            next = None;
                            wake();
                        }
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            });
        Alarm { tx, armed: None }
    }

    /// Wake at `at` (or earlier, if an earlier wake is armed already).
    fn set(&mut self, at: Instant, now: Instant) {
        if self.armed.is_some_and(|t| t > now && t <= at) {
            return;
        }
        if self.tx.send(at).is_ok() {
            self.armed = Some(at);
        }
    }
}

/// Shares or joins a project over a shared folder or a hub, driven from the
/// thread that owns the documents (spec §7).
///
/// - [`start`](Self::start) returns at once; a background thread connects.
/// - [`pump`](Self::pump) every frame or tick (and whenever `wake` is called):
///   never blocks. It adopts a finished connect attempt, exchanges messages,
///   and on a link failure goes offline and schedules a retry.
/// - [`status`](Self::status) for the status bar, [`access`](Self::access) to
///   grey out editing, presence passthrough ([`set_presence`](Self::set_presence),
///   [`presence`](Self::presence), [`editing_by_others`](Self::editing_by_others)).
/// - [`close`](Self::close) for a clean end: the last edits and a "left"
///   go out, the link closes on a background thread, [`Closing`] reports how
///   it went. Dropping without `close` just drops the link (on a background
///   thread).
///
/// Remote changes are imported with `Stage::import_updates` and never enter
/// an undo stack (spec §1).
pub struct Connection {
    target: Target,
    config: ConnectionConfig,
    session: SyncSession,
    phase: Phase,
    access: Access,
    /// Failed attempts in a row since the last stable link.
    failures: u32,
    /// First of the current run of "client ID in use" failures.
    in_use_since: Option<Instant>,
    attempts: u64,
    unsynced: bool,
    generation: u64,
    shown: Option<StatusKey>,
    /// The thread dropping the last dead link; the next connect waits for it
    /// (a folder's lock must be released before the new link takes it).
    retired: Option<JoinHandle<()>>,
    alarm: Alarm,
    /// Jitter source (xorshift64).
    rng: u64,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("target", &self.target)
            .field("client", &self.config.client)
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl Connection {
    /// Start connecting to `target` in the background. Returns at once.
    pub fn start(target: Target, config: ConnectionConfig) -> Connection {
        let mut session_config = config
            .session
            .clone()
            .unwrap_or_else(|| target.session_config(config.client.clone()));
        session_config.client = config.client.clone();
        session_config.read_only = false;
        let mut session = SyncSession::new(session_config);
        if let Some(p) = config.presence.clone() {
            session.set_presence(p);
        }
        let alarm = Alarm::spawn(config.wake.clone());
        let rng = RandomState::new().hash_one(config.client.as_str()) | 1;
        let mut c = Connection {
            target,
            config,
            session,
            phase: Phase::Closed,
            access: Access::Unknown,
            failures: 0,
            in_use_since: None,
            attempts: 0,
            unsynced: false,
            generation: 0,
            shown: None,
            retired: None,
            alarm,
            rng,
        };
        let now = Instant::now();
        match c.target.check() {
            Ok(()) => c.connect_now(now),
            Err(e) => {
                c.phase = Phase::Failed {
                    failure: Failure::BadTarget,
                    reason: e.to_string(),
                    retry_at: None,
                }
            }
        }
        c.refresh(false);
        c
    }

    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn client(&self) -> &ClientId {
        &self.config.client
    }

    /// The session (statistics, what the remote said); read-only.
    pub fn session(&self) -> &SyncSession {
        &self.session
    }

    /// Connect attempts started so far (diagnostics, tests).
    pub fn connect_attempts(&self) -> u64 {
        self.attempts
    }

    /// The last access level the remote granted ([`Access::Unknown`] before
    /// the first link). While [`Access::Read`], local edits are not sent.
    pub fn access(&self) -> Access {
        self.access
    }

    /// Online: the link is up and the remote said what it has.
    pub fn is_online(&self) -> bool {
        matches!(self.phase, Phase::Linked { since: Some(_), .. })
    }

    /// Increases whenever [`status`](Self::status) changes.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn status(&self) -> ConnectionStatus {
        let state = match &self.phase {
            Phase::Linked {
                since: Some(since), ..
            } => ConnectionState::Online { since: *since },
            Phase::Offline { retry_at, error } => ConnectionState::Offline {
                retry_in: retry_at.saturating_duration_since(Instant::now()),
                error: error.clone(),
            },
            Phase::Failed {
                failure, reason, ..
            } => ConnectionState::Failed {
                failure: *failure,
                reason: reason.clone(),
            },
            Phase::Connecting { .. } | Phase::Linked { since: None, .. } | Phase::Closed => {
                ConnectionState::Connecting
            }
        };
        ConnectionStatus {
            state,
            access: self.access,
            unsynced: self.unsynced,
            peers: self.peer_count(),
            generation: self.generation,
        }
    }

    /// Exchange messages without blocking. Call it every frame or tick, and
    /// whenever `wake` was called. Adopts a link a worker finished, imports
    /// what arrived, sends local edits the remote lacks (not while
    /// [`Access::Read`]); on a link failure goes offline and schedules a retry.
    pub fn pump(&mut self, docs: &mut dyn DocSet) -> ConnectionOutcome {
        let now = Instant::now();
        let mut out = ConnectionOutcome::default();
        self.advance(now);
        let mut failed = None;
        if let Phase::Linked {
            transport, since, ..
        } = &mut self.phase
        {
            match self.session.pump(docs, transport) {
                Ok(o) => {
                    if since.is_none() && self.session.is_connected() {
                        *since = Some(now);
                    }
                    if o.received >= self.session.config().max_messages_per_pump {
                        // More may be waiting; the link only wakes on news.
                        (self.config.wake)();
                    }
                    out.events = o.events;
                    out.unknown_docs = o.unknown_docs;
                    out.errors = o.errors;
                    out.presence_changed = o.presence_changed;
                }
                Err(e) => failed = Some(e),
            }
        }
        if let Some(e) = failed {
            self.lost(e, now);
        }
        self.unsynced = !self.session.unsynced(docs).is_empty();
        let (changed, presence) = self.refresh(out.presence_changed);
        out.status_changed = changed;
        out.presence_changed |= presence;
        self.schedule(now);
        out
    }

    /// Try again now: after a [`Failure`] (e.g. the token was fixed on the
    /// hub, the other session with our client ID quit), or to skip an offline
    /// wait. Does nothing while connecting or online.
    pub fn retry(&mut self) {
        let now = Instant::now();
        match self.phase {
            Phase::Failed { .. } => {
                self.failures = 0;
                self.connect_now(now);
            }
            Phase::Offline { .. } => self.connect_now(now),
            _ => return,
        }
        self.refresh(false);
    }

    /// Publish our presence (sent on the next pump while online, again after
    /// every reconnect, then as a heartbeat).
    pub fn set_presence(&mut self, state: PresenceState) {
        self.session.set_presence(state);
    }

    /// Tell the others we left (sent on the next pump).
    pub fn clear_presence(&mut self) {
        self.session.clear_presence();
    }

    /// Our current presence.
    pub fn local_presence(&self) -> Option<&PresenceState> {
        self.session.local_presence()
    }

    /// Everyone else who is online (empty unless online).
    pub fn presence(&self) -> Vec<PeerPresence> {
        if self.is_online() {
            self.session.presence()
        } else {
            Vec::new()
        }
    }

    /// Others who declared they are editing `path`, an ancestor or a
    /// descendant of it (empty unless online): show a hint, do not block.
    pub fn editing_by_others(&self, path: &PrimPath) -> Vec<PeerPresence> {
        if self.is_online() {
            self.session.editing_by_others(path)
        } else {
            Vec::new()
        }
    }

    /// End the connection cleanly without blocking: while online, say we
    /// left, import what arrived, send every local edit the remote lacks
    /// (`flush`, ignoring `min_send_interval`), then close the link on a
    /// background thread, which may take until the hub confirmed it read
    /// everything (up to `HubLimits::stall_timeout` without progress).
    /// [`Closing`] tells which documents had edits that could not be sent,
    /// and later whether the link delivered the rest.
    pub fn close(mut self, docs: &mut dyn DocSet) -> Closing {
        let phase = std::mem::replace(&mut self.phase, Phase::Closed);
        let Phase::Linked { mut transport, .. } = phase else {
            // Connecting: the worker drops its link when it finds nobody
            // waiting. Offline or failed: nothing to close.
            return Closing {
                unsynced: self.session.unsynced(docs),
                rx: None,
                result: Some(Ok(())),
            };
        };
        self.session.clear_presence();
        let sent = self
            .session
            .pump(docs, &mut transport)
            .and_then(|_| self.session.flush(docs, &mut transport))
            .map(|_| ());
        let unsynced = self.session.unsynced(docs);
        let (tx, rx) = mpsc::channel();
        let wake = self.config.wake.clone();
        let spawned = std::thread::Builder::new()
            .name("bae-sync-close".into())
            .spawn(move || {
                let closed = transport.close();
                // The first error: a failed last pump means the link was
                // already gone.
                let _ = tx.send(sent.and(closed));
                wake();
            });
        match spawned {
            Ok(_) => Closing {
                unsynced,
                rx: Some(rx),
                result: None,
            },
            Err(e) => Closing {
                unsynced,
                rx: None,
                result: Some(Err(SyncError::Io {
                    path: PathBuf::new(),
                    source: e,
                })),
            },
        }
    }

    // ---- state machine ----

    /// Adopt a finished connect attempt, or start one that is due.
    fn advance(&mut self, now: Instant) {
        let finished = match &self.phase {
            Phase::Connecting { rx } => match rx.try_recv() {
                Ok(r) => Some(r),
                Err(TryRecvError::Empty) => None,
                // The worker panicked.
                Err(TryRecvError::Disconnected) => Some(Err(SyncError::Closed)),
            },
            Phase::Offline { retry_at, .. } if now >= *retry_at => {
                self.connect_now(now);
                None
            }
            Phase::Failed {
                retry_at: Some(t), ..
            } if now >= *t => {
                self.connect_now(now);
                None
            }
            _ => None,
        };
        match finished {
            Some(Ok(link)) => {
                // A new remote: both sides say what they have, then send
                // each other what is missing. Our presence goes out again.
                self.session.reset();
                self.session.config_mut().read_only = !link.access.can_write();
                self.access = link.access;
                self.in_use_since = None;
                self.phase = Phase::Linked {
                    transport: link.transport,
                    adopted: now,
                    since: None,
                };
            }
            Some(Err(e)) => self.attempt_failed(e, now),
            None => {}
        }
    }

    fn connect_now(&mut self, now: Instant) {
        self.attempts += 1;
        let worker = Worker {
            client: self.config.client.clone(),
            folder: self.config.folder.clone(),
            io_tick: self.config.io_tick,
            #[cfg(feature = "hub")]
            hub_timeout: self.config.hub_timeout,
            #[cfg(feature = "hub")]
            hub_limits: self.config.hub_limits,
            wake: self.config.wake.clone(),
        };
        let target = self.target.clone();
        let retired = self.retired.take();
        let (tx, rx) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("bae-sync-connect".into())
            .spawn(move || {
                if let Some(old) = retired {
                    let _ = old.join();
                }
                let result = worker.connect(&target);
                // Nobody waiting (closed meanwhile): the link drops here.
                if tx.send(result).is_ok() {
                    (worker.wake)();
                }
            });
        match spawned {
            Ok(_) => self.phase = Phase::Connecting { rx },
            Err(e) => self.attempt_failed(
                SyncError::Io {
                    path: PathBuf::new(),
                    source: e,
                },
                now,
            ),
        }
    }

    /// A connect attempt or a link failed: offline with backoff, or failed
    /// if retrying will not help.
    fn attempt_failed(&mut self, e: SyncError, now: Instant) {
        let reason = e.to_string();
        let mut failure = classify(&e);
        if failure == Some(Failure::ClientIdInUse) {
            let since = *self.in_use_since.get_or_insert(now);
            if now.saturating_duration_since(since) < CLIENT_ID_GRACE {
                failure = None;
            }
        } else {
            self.in_use_since = None;
        }
        self.phase = match failure {
            Some(failure) => Phase::Failed {
                failure,
                reason,
                retry_at: self.config.backoff.failed_retry.map(|d| now + d),
            },
            None => {
                let delay = self.next_delay();
                Phase::Offline {
                    retry_at: now + delay,
                    error: reason,
                }
            }
        };
    }

    /// The link failed: drop it on a background thread, go offline.
    fn lost(&mut self, e: SyncError, now: Instant) {
        let Phase::Linked {
            transport, adopted, ..
        } = std::mem::replace(&mut self.phase, Phase::Closed)
        else {
            return;
        };
        if now.saturating_duration_since(adopted) >= self.config.backoff.stable_after {
            self.failures = 0;
        }
        self.retire(transport);
        self.attempt_failed(e, now);
    }

    /// Drop a link off this thread (`ThreadedTransport` joins its I/O thread,
    /// which may be in the middle of a write to the NAS).
    fn retire(&mut self, transport: ThreadedTransport) {
        let previous = self.retired.take();
        self.retired = std::thread::Builder::new()
            .name("bae-sync-drop".into())
            .spawn(move || {
                if let Some(p) = previous {
                    let _ = p.join();
                }
                drop(transport);
            })
            .ok();
    }

    /// The next retry delay, with jitter; counts the failure.
    fn next_delay(&mut self) -> Duration {
        let backoff = &self.config.backoff;
        let base = backoff.delay(self.failures);
        self.failures = self.failures.saturating_add(1);
        // xorshift64
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        let r = (self.rng >> 11) as f64 / (1u64 << 53) as f64;
        let jitter = if backoff.jitter.is_finite() {
            backoff.jitter.clamp(0.0, 1.0)
        } else {
            0.0
        };
        base.mul_f64(1.0 - jitter * r)
    }

    fn peer_count(&self) -> usize {
        if self.is_online() {
            self.session.presence().len()
        } else {
            0
        }
    }

    fn key(&self) -> StatusKey {
        let (state, since, retry_at, text, failure) = match &self.phase {
            Phase::Linked { since: Some(s), .. } => (1, Some(*s), None, String::new(), None),
            Phase::Offline { retry_at, error } => (2, None, Some(*retry_at), error.clone(), None),
            Phase::Failed {
                failure,
                reason,
                retry_at,
            } => (3, None, *retry_at, reason.clone(), Some(*failure)),
            Phase::Connecting { .. } | Phase::Linked { since: None, .. } | Phase::Closed => {
                (0, None, None, String::new(), None)
            }
        };
        StatusKey {
            state,
            since,
            retry_at,
            text,
            failure,
            access: self.access,
            unsynced: self.unsynced,
            peers: self.peer_count(),
        }
    }

    /// Bump the generation if the status changed (or `presence_changed`).
    /// Returns (status changed, online-ness changed).
    fn refresh(&mut self, presence_changed: bool) -> (bool, bool) {
        let key = self.key();
        let was_online = self.shown.as_ref().is_some_and(|k| k.state == 1);
        let online_changed = was_online != (key.state == 1);
        let changed = self.shown.as_ref() != Some(&key);
        if changed || presence_changed {
            self.generation += 1;
            self.shown = Some(key);
        }
        (changed || presence_changed, online_changed)
    }

    /// Arm the alarm for the next pump that is due on its own.
    fn schedule(&mut self, now: Instant) {
        let at = match &self.phase {
            Phase::Linked { .. } => {
                let c = self.session.config();
                if self.unsynced
                    && !c.read_only
                    && self.session.is_connected()
                    && !c.min_send_interval.is_zero()
                {
                    // Edits held back by `min_send_interval`.
                    Some(now + c.min_send_interval)
                } else {
                    // `Hello`s and heartbeats (and others' presence expiry).
                    let every = c
                        .hello_interval
                        .map_or(c.presence_heartbeat, |h| h.min(c.presence_heartbeat));
                    Some(now + every)
                }
            }
            Phase::Offline { retry_at, .. } => Some(*retry_at),
            Phase::Failed { retry_at, .. } => *retry_at,
            Phase::Connecting { .. } | Phase::Closed => None,
        };
        if let Some(at) = at {
            self.alarm.set(at, now);
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Phase::Linked { transport, .. } = std::mem::replace(&mut self.phase, Phase::Closed) {
            self.retire(transport);
        }
    }
}

/// Failures that retrying on a schedule does not fix.
fn classify(e: &SyncError) -> Option<Failure> {
    match e {
        SyncError::ClientIdInUse(_) => Some(Failure::ClientIdInUse),
        SyncError::HubRefused { status, .. } => match *status {
            401 => Some(Failure::BadToken),
            403 => Some(Failure::NoAccess),
            404 => Some(Failure::NoSuchProject),
            // Timeout, rate limit: try again.
            408 | 429 => None,
            s @ 400..=499 => Some(Failure::Refused(s)),
            _ => None,
        },
        _ => None,
    }
}

/// A connection being closed ([`Connection::close`]).
#[derive(Debug)]
pub struct Closing {
    unsynced: Vec<DocKey>,
    rx: Option<Receiver<Result<(), SyncError>>>,
    result: Option<Result<(), SyncError>>,
}

impl Closing {
    /// Documents with local edits that were not handed to the link: closed
    /// while offline or connecting, read-only access, or the remote had not
    /// said what it has yet. Known at once. Warn before quitting when this is
    /// not empty or the link reports an error.
    pub fn unsynced(&self) -> &[DocKey] {
        &self.unsynced
    }

    /// How closing the link went, once it is done (does not block). An error
    /// means the last messages may not have arrived (the hub did not confirm,
    /// a shared-folder write failed).
    pub fn try_result(&mut self) -> Option<&Result<(), SyncError>> {
        self.wait(Duration::ZERO)
    }

    /// [`try_result`](Self::try_result), waiting up to `timeout` (at app
    /// exit). `None`: still closing.
    pub fn wait(&mut self, timeout: Duration) -> Option<&Result<(), SyncError>> {
        if self.result.is_none()
            && let Some(rx) = &self.rx
        {
            match rx.recv_timeout(timeout) {
                Ok(r) => self.result = Some(r),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => self.result = Some(Err(SyncError::Closed)),
            }
        }
        self.result.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn backoff_doubles_up_to_the_cap() {
        let b = Backoff::default();
        let d: Vec<u128> = (0..9).map(|n| b.delay(n).as_millis()).collect();
        assert_eq!(d, [500, 1000, 2000, 4000, 8000, 16000, 30000, 30000, 30000]);
        assert_eq!(b.delay(u32::MAX), b.max);
    }

    #[test]
    fn jitter_only_shortens_and_stays_in_range() {
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        let mut c = Connection::start(
            Target::Folder(PathBuf::from("/nonexistent/bae/connection/test")),
            ConnectionConfig::new(ClientId::new("jitter").unwrap(), wake),
        );
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            c.failures = 0;
            let d = c.next_delay();
            assert!(d <= Duration::from_millis(500), "{d:?}");
            assert!(d >= Duration::from_millis(375), "{d:?}");
            seen.insert(d);
        }
        assert!(seen.len() > 10, "jitter varies");
        c.config.backoff.jitter = 0.0;
        c.failures = 2;
        assert_eq!(c.next_delay(), Duration::from_secs(2));
        assert_eq!(c.failures, 3);
    }

    #[test]
    fn permanent_failures() {
        let refused = |status| SyncError::HubRefused {
            status,
            message: String::new(),
        };
        assert_eq!(classify(&refused(401)), Some(Failure::BadToken));
        assert_eq!(classify(&refused(403)), Some(Failure::NoAccess));
        assert_eq!(classify(&refused(404)), Some(Failure::NoSuchProject));
        assert_eq!(classify(&refused(400)), Some(Failure::Refused(400)));
        for transient in [429, 408, 500, 503] {
            assert_eq!(classify(&refused(transient)), None, "{transient}");
        }
        assert_eq!(
            classify(&SyncError::ClientIdInUse(ClientId::new("a").unwrap())),
            Some(Failure::ClientIdInUse)
        );
        assert_eq!(classify(&SyncError::Closed), None);
        assert_eq!(classify(&SyncError::Hub("reset".into())), None);
    }

    #[test]
    fn access_parsing() {
        assert_eq!(Access::parse("read"), Access::Read);
        assert_eq!(Access::parse(" WRITE "), Access::Write);
        assert_eq!(Access::parse("admin"), Access::Unknown);
        assert!(Access::Unknown.can_write() && Access::Write.can_write());
        assert!(!Access::Read.can_write());
        for a in [Access::Read, Access::Write] {
            assert_eq!(Access::parse(a.as_str()), a);
        }
    }

    #[test]
    fn the_alarm_wakes_at_the_earliest_deadline() {
        let woken = Arc::new(AtomicUsize::new(0));
        let w = woken.clone();
        let mut alarm = Alarm::spawn(Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        }));
        let now = Instant::now();
        alarm.set(now + Duration::from_secs(3600), now);
        // Earlier wins; a later one while armed changes nothing.
        alarm.set(now + Duration::from_millis(20), now);
        alarm.set(now + Duration::from_secs(7200), now);
        let deadline = Instant::now() + Duration::from_secs(10);
        while woken.load(Ordering::SeqCst) == 0 {
            assert!(Instant::now() < deadline, "the alarm never fired");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(woken.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_token_is_not_printed() {
        let t = Target::Folder(PathBuf::from("x"));
        assert!(format!("{t:?}").contains('x'));
        #[cfg(feature = "hub")]
        {
            let t = Target::Hub {
                url: "ws://nas/projects/a".into(),
                token: Some("very-secret-token".into()),
            };
            assert!(!format!("{t:?}").contains("very-secret"));
        }
    }

    #[cfg(feature = "hub")]
    #[test]
    fn an_unusable_url_fails_without_retrying() {
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        for url in ["wss://nas/projects/a", "http://nas/projects/a", "not a url"] {
            let c = Connection::start(
                Target::Hub {
                    url: url.into(),
                    token: None,
                },
                ConnectionConfig::new(ClientId::new("u").unwrap(), wake.clone()),
            );
            assert_eq!(c.connect_attempts(), 0, "{url}");
            assert!(
                matches!(
                    c.status().state,
                    ConnectionState::Failed {
                        failure: Failure::BadTarget,
                        ..
                    }
                ),
                "{url}: {:?}",
                c.status()
            );
        }
    }
}
