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
//! When a link fails (an error, or a hub that went silent) the connection
//! goes offline, keeps the local edits (the documents are never touched by a
//! disconnect) and retries with exponential backoff; a fresh link plus
//! [`SyncSession::reset`] makes both sides exchange `Hello`s and send each
//! other what is missing. Failures that retrying does not fix (bad token, no
//! access, no such project, client ID in use) wait for [`Connection::retry`],
//! or a slow retry.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
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
use crate::transport::{Access, Transport};

/// `min_send_interval` of shared-folder sessions (spec 4.5): a drag becomes a
/// few appends per second on the NAS instead of dozens.
pub const FOLDER_SEND_INTERVAL: Duration = Duration::from_millis(300);
/// How often the I/O thread of a shared-folder link looks for news (the folder
/// itself is scanned every [`DirConfig::poll_interval`]).
const FOLDER_TICK: Duration = Duration::from_millis(10);
/// A folder's lock held by our own client ID may be one that is just being
/// let go (another process of ours that is quitting; links of this process
/// are waited for, see [`Releasing`]): [`Failure::ClientIdInUse`] only once it
/// lasted this long.
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
                // Never print the secret, also not the `?token=` form.
                .field("url", &redact_token(url))
                .field("token", &token.as_ref().map(|_| "…"))
                .finish(),
        }
    }
}

/// `url` with the value of a `token` query parameter (the hub's alternative to
/// the header) replaced by `…`.
#[cfg(feature = "hub")]
fn redact_token(url: &str) -> std::borrow::Cow<'_, str> {
    let Some((base, rest)) = url.split_once('?') else {
        return url.into();
    };
    let (query, fragment) = match rest.split_once('#') {
        Some((q, f)) => (q, Some(f)),
        None => (rest, None),
    };
    let is_token = |pair: &str| {
        let key = pair.split_once('=').map_or(pair, |(k, _)| k);
        key.eq_ignore_ascii_case("token")
    };
    if !query.split('&').any(is_token) {
        return url.into();
    }
    let query: Vec<&str> = query
        .split('&')
        .map(|pair| if is_token(pair) { "token=…" } else { pair })
        .collect();
    let mut out = format!("{base}?{}", query.join("&"));
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    out.into()
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
    /// Called from background threads (never from inside a call on the
    /// [`Connection`]) whenever the connection wants a [`Connection::pump`]:
    /// something arrived, a connect attempt finished, a retry or keep-alive is
    /// due, a close finished, a pump could not take everything that arrived.
    /// Hook it to "request a repaint". It must not block.
    pub wake: Arc<dyn Fn() + Send + Sync>,
    /// Session settings; `None`: [`Target::session_config`]. `client` is
    /// replaced by the one above. [`SessionConfig::send_data`] is turned off
    /// while the remote grants [`Access::Read`] ([`Connection::access`]);
    /// `false` here keeps it off whatever the access (a viewer).
    pub session: Option<SessionConfig>,
    pub backoff: Backoff,
    /// Shared-folder settings.
    pub folder: DirConfig,
    /// How often a link's I/O thread looks for incoming messages; `None`: 2 ms
    /// for hubs, 10 ms for shared folders.
    pub io_tick: Option<Duration>,
    /// Edits handed to a link whose I/O thread has been stuck this long in
    /// one read or write (a NAS that does not answer) while they wait count
    /// as unsynced ([`ConnectionStatus::unsynced`]).
    pub io_stall: Duration,
    /// Hubs: timeout of each connect step (TCP connect, handshake).
    #[cfg(feature = "hub")]
    pub hub_timeout: Duration,
    /// Hubs: message size, write-buffer and stall limits of the link
    /// ([`HubLimits::default`]: 256 MiB messages both ways). The session's
    /// flow control is its own `max_backlog`, counted over the link's whole
    /// backlog (its I/O thread's queue and the WebSocket's write buffer).
    #[cfg(feature = "hub")]
    pub hub_limits: HubLimits,
    /// Hubs: a link on which not a byte arrived for this long is lost (the hub
    /// says `Hello` every 5 seconds; a hub that lost power or a dropped VPN
    /// sends no reset, and TCP may take many minutes to notice). `None`:
    /// never.
    #[cfg(feature = "hub")]
    pub hub_idle_timeout: Option<Duration>,
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
            io_stall: Duration::from_secs(5),
            #[cfg(feature = "hub")]
            hub_timeout: Duration::from_secs(10),
            #[cfg(feature = "hub")]
            hub_limits: HubLimits::default(),
            #[cfg(feature = "hub")]
            hub_idle_timeout: Some(Duration::from_secs(20)),
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
    /// yet (offline, read-only, held back by `min_send_interval`), not
    /// confirmed since the last reconnect, handed to a link that failed
    /// before the remote confirmed them, or waiting on a link whose I/O is
    /// stuck ([`ConnectionConfig::io_stall`]).
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
    /// Until adopted, nobody may be going to use the link (the connection was
    /// closed or dropped meanwhile): a folder link counts as being let go.
    /// Dropped after `transport` (field order).
    _releasing: Releasing,
}

/// Folder links of this process that are being let go on a background
/// thread (dropped, closed, or made by a connect attempt nobody waits for any
/// more), by folder and client ID. Until one is gone it may still hold
/// `updates/<client>.lock`, and dropping it may wait for a slow NAS: a connect
/// to the same folder as the same client waits for it (it is not another
/// session, spec 7.4).
static RELEASING: Mutex<Vec<(PathBuf, ClientId)>> = Mutex::new(Vec::new());
static RELEASED: Condvar = Condvar::new();

fn releasing() -> MutexGuard<'static, Vec<(PathBuf, ClientId)>> {
    RELEASING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One entry of [`RELEASING`] while it lives (nothing for hub targets).
struct Releasing(Option<(PathBuf, ClientId)>);

impl Releasing {
    fn new(target: &Target, client: &ClientId) -> Releasing {
        match target {
            Target::Folder(path) => {
                let key = (path.clone(), client.clone());
                releasing().push(key.clone());
                Releasing(Some(key))
            }
            #[cfg(feature = "hub")]
            Target::Hub { .. } => Releasing(None),
        }
    }

    /// Block until no link of this process to `path` as `client` is being
    /// let go (connect workers only).
    fn wait(path: &Path, client: &ClientId) {
        let mut list = releasing();
        while list.iter().any(|(p, c)| p == path && c == client) {
            list = RELEASED.wait(list).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

impl Drop for Releasing {
    fn drop(&mut self) {
        let Some(key) = self.0.take() else {
            return;
        };
        let mut list = releasing();
        if let Some(i) = list.iter().position(|k| *k == key) {
            list.swap_remove(i);
        }
        drop(list);
        RELEASED.notify_all();
    }
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
    #[cfg(feature = "hub")]
    hub_idle_timeout: Option<Duration>,
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
                // Our own links to this folder let go of the lock first.
                Releasing::wait(path, &client);
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
                    _releasing: Releasing::new(target, &self.client),
                })
            }
            #[cfg(feature = "hub")]
            Target::Hub { url, token } => {
                let mut hub = HubTransport::connect_with(
                    url,
                    token.as_deref(),
                    self.hub_timeout,
                    self.hub_limits,
                )?;
                hub.set_idle_timeout(self.hub_idle_timeout);
                let access = hub.access();
                let tick = self.io_tick.unwrap_or(HUB_TICK);
                Ok(Link {
                    transport: ThreadedTransport::spawn(hub, tick, wake),
                    access,
                    _releasing: Releasing(None),
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
///
/// It only keeps the earliest deadline: every wake leads to a pump, and every
/// pump arms the next one.
struct Alarm {
    tx: Sender<Instant>,
    /// The deadline the thread will wake at, until it did.
    armed: Arc<Mutex<Option<Instant>>>,
}

fn lock_armed(armed: &Mutex<Option<Instant>>) -> MutexGuard<'_, Option<Instant>> {
    armed.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Alarm {
    fn spawn(wake: Arc<dyn Fn() + Send + Sync>) -> Alarm {
        let (tx, rx) = mpsc::channel::<Instant>();
        let armed = Arc::new(Mutex::new(None));
        let fired = armed.clone();
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
                            if let Some(t) = next.take() {
                                let mut armed = lock_armed(&fired);
                                match *armed {
                                    // From now on `set` arms again, whatever
                                    // it skipped because this deadline was
                                    // earlier.
                                    Some(a) if a <= t => *armed = None,
                                    // Armed later after all (a `set` came
                                    // between, merged into this deadline):
                                    // still due, or it stays armed for good.
                                    Some(a) => next = Some(a),
                                    None => {}
                                }
                            }
                            wake();
                        }
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            });
        Alarm { tx, armed }
    }

    /// Wake at `at`, or earlier if an earlier wake is armed already (the pump
    /// after it arms `at` again).
    fn set(&self, at: Instant) {
        let mut armed = lock_armed(&self.armed);
        if armed.is_some_and(|t| t <= at) {
            return;
        }
        if self.tx.send(at).is_ok() {
            *armed = Some(at);
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
///   thread, also one a connect attempt is still making).
///
/// Remote changes are imported with `Stage::import_updates` and never enter
/// an undo stack (spec §1).
pub struct Connection {
    target: Target,
    config: ConnectionConfig,
    session: SyncSession,
    /// [`SessionConfig::send_data`] as configured: the session sends data
    /// while this is set and the remote grants more than [`Access::Read`].
    send_data: bool,
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
    /// The thread dropping the last dead link; the next connect waits for it.
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
        let send_data = session_config.send_data;
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
            send_data,
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
        c.connect_now(Instant::now());
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
        let mut stuck = false;
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
                        // Woken from the alarm thread, not from in here.
                        self.alarm.set(now);
                    }
                    out.events = o.events;
                    out.unknown_docs = o.unknown_docs;
                    out.errors = o.errors;
                    out.presence_changed = o.presence_changed;
                    stuck = transport.stalled() >= self.config.io_stall;
                }
                Err(e) => failed = Some(e),
            }
        }
        if let Some(e) = failed {
            self.lost(e, now);
        }
        self.unsynced = stuck || !self.session.unsynced(docs).is_empty();
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
        let mut transport = match phase {
            Phase::Linked { transport, .. } => transport,
            other => {
                // Connecting: a link the worker may still deliver is dropped
                // on a background thread. Offline or failed: nothing to
                // close (what the dead link did not confirm is unsynced).
                if let Phase::Connecting { rx } = other {
                    self.abandon(rx);
                }
                return Closing {
                    unsynced: self.session.unsynced(docs),
                    rx: None,
                    result: Some(Ok(())),
                };
            }
        };
        self.session.clear_presence();
        let sent = self
            .session
            .pump(docs, &mut transport)
            .and_then(|_| self.session.flush(docs, &mut transport))
            .map(|_| ());
        if sent.is_err() {
            // The link is gone: what it did not confirm may not have arrived.
            self.session.forget_unconfirmed();
        }
        let unsynced = self.session.unsynced(docs);
        let (tx, rx) = mpsc::channel();
        let wake = self.config.wake.clone();
        let releasing = Releasing::new(&self.target, &self.config.client);
        let spawned = std::thread::Builder::new()
            .name("bae-sync-close".into())
            .spawn(move || {
                let closed = transport.close();
                drop(transport);
                drop(releasing);
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
                self.session.config_mut().send_data = self.send_data && link.access.can_write();
                self.access = link.access;
                self.in_use_since = None;
                // Ours now: no longer counts as being let go.
                let Link {
                    transport,
                    _releasing: adopted,
                    ..
                } = link;
                drop(adopted);
                self.phase = Phase::Linked {
                    transport,
                    adopted: now,
                    since: None,
                };
            }
            Some(Err(e)) => self.attempt_failed(e, now),
            None => {}
        }
    }

    fn connect_now(&mut self, now: Instant) {
        // An unusable URL stays unusable: no attempt, also not on `retry`.
        if let Err(e) = self.target.check() {
            self.phase = Phase::Failed {
                failure: Failure::BadTarget,
                reason: e.to_string(),
                retry_at: None,
            };
            return;
        }
        self.attempts += 1;
        let worker = Worker {
            client: self.config.client.clone(),
            folder: self.config.folder.clone(),
            io_tick: self.config.io_tick,
            #[cfg(feature = "hub")]
            hub_timeout: self.config.hub_timeout,
            #[cfg(feature = "hub")]
            hub_limits: self.config.hub_limits,
            #[cfg(feature = "hub")]
            hub_idle_timeout: self.config.hub_idle_timeout,
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
                // The receiver is only dropped once it took the link (or on
                // a background thread after the connection was dropped).
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

    /// The link failed: drop it on a background thread, go offline. What
    /// was handed to it but not confirmed by the remote counts as unsynced
    /// again (it may never have arrived).
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
        self.session.forget_unconfirmed();
        self.retire(transport);
        self.attempt_failed(e, now);
    }

    /// Drop a link off this thread (`ThreadedTransport` joins its I/O thread,
    /// which may be in the middle of a read or write of the NAS).
    fn retire(&mut self, transport: ThreadedTransport) {
        let releasing = Releasing::new(&self.target, &self.config.client);
        self.on_retire_thread(move || {
            drop(transport);
            drop(releasing);
        });
    }

    /// The connection no longer waits for a connect attempt: the link it may
    /// still deliver (or already did, unadopted) is dropped on a background
    /// thread, never here.
    fn abandon(&mut self, rx: Receiver<Result<Link, SyncError>>) {
        self.on_retire_thread(move || drop(rx.recv()));
    }

    /// Run `f` on a background thread after the one before it.
    fn on_retire_thread(&mut self, f: impl FnOnce() + Send + 'static) {
        let previous = self.retired.take();
        self.retired = std::thread::Builder::new()
            .name("bae-sync-drop".into())
            .spawn(move || {
                if let Some(p) = previous {
                    let _ = p.join();
                }
                f();
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
    fn schedule(&self, now: Instant) {
        let at = match &self.phase {
            Phase::Linked { transport, .. } => {
                let c = self.session.config();
                let mut at = if self.unsynced
                    && c.send_data
                    && self.session.is_connected()
                    && !c.min_send_interval.is_zero()
                {
                    // Edits held back by `min_send_interval`.
                    now + c.min_send_interval
                } else {
                    // `Hello`s and heartbeats (and others' presence expiry).
                    let every = c
                        .hello_interval
                        .map_or(c.presence_heartbeat, |h| h.min(c.presence_heartbeat));
                    now + every
                };
                let stalled = transport.stalled();
                if !stalled.is_zero() && stalled < self.config.io_stall {
                    // Sent edits wait for the link's I/O: unsynced if it
                    // stays stuck.
                    at = at.min(now + (self.config.io_stall - stalled));
                }
                Some(at)
            }
            Phase::Offline { retry_at, .. } => Some(*retry_at),
            Phase::Failed { retry_at, .. } => *retry_at,
            Phase::Connecting { .. } | Phase::Closed => None,
        };
        if let Some(at) = at {
            self.alarm.set(at);
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        match std::mem::replace(&mut self.phase, Phase::Closed) {
            Phase::Linked { transport, .. } => self.retire(transport),
            Phase::Connecting { rx } => self.abandon(rx),
            Phase::Offline { .. } | Phase::Failed { .. } | Phase::Closed => {}
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
    /// said what it has yet. Also those handed to a link that failed without
    /// the remote confirming them; a shared folder confirms only with its
    /// `Hello`s and what it reads of others, not our writes, so after a lost
    /// folder link this is every document edited on it (spec 7.4). Known at
    /// once. Warn before quitting when this is not empty or the link reports
    /// an error.
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
    use crate::dir::LOCK_WAIT;
    use crate::docs::{DocSet, MemDocs};
    use crate::protocol::Message;
    use crate::transport::MemoryTransport;
    use bae_model::SublayerId;
    use bae_model::loro::{ExportMode, LoroDoc};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn quiet() -> Arc<dyn Fn() + Send + Sync> {
        Arc::new(|| {})
    }

    /// A connection with no link of its own, for the test to put one in
    /// (its first connect attempt finds no folder and is dropped).
    fn bare(id: &str, wake: Arc<dyn Fn() + Send + Sync>) -> Connection {
        let mut c = Connection::start(
            Target::Folder(PathBuf::from("/nonexistent/bae/connection/test")),
            ConnectionConfig::new(ClientId::new(id).unwrap(), wake),
        );
        c.phase = Phase::Closed;
        c
    }

    fn linked(c: &mut Connection, transport: ThreadedTransport) {
        c.session.reset();
        c.phase = Phase::Linked {
            transport,
            adopted: Instant::now(),
            since: None,
        };
    }

    /// The inner transport of a link whose I/O thread gets stuck in `recv` (a
    /// NAS read that hangs) until `release` is sent to or dropped. `entered`
    /// hears when it is stuck; the first `recv` returns `first`. `_hold` lives
    /// as long as the transport (e.g. a folder lock).
    struct Stuck {
        first: Option<Message>,
        entered: mpsc::Sender<()>,
        release: Receiver<()>,
        _hold: Option<DirTransport>,
    }

    impl Transport for Stuck {
        fn send(&mut self, _: Message) -> Result<(), SyncError> {
            Ok(())
        }

        fn recv(&mut self) -> Result<Option<Message>, SyncError> {
            if let Some(m) = self.first.take() {
                return Ok(Some(m));
            }
            let _ = self.entered.send(());
            let _ = self.release.recv();
            Ok(None)
        }
    }

    /// A link stuck in a read, and the sender that lets it go.
    fn stuck_link(
        first: Option<Message>,
        hold: Option<DirTransport>,
    ) -> (ThreadedTransport, mpsc::Sender<()>) {
        let (entered_tx, entered) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let stuck = Stuck {
            first,
            entered: entered_tx,
            release: release_rx,
            _hold: hold,
        };
        let transport = ThreadedTransport::spawn(stuck, Duration::from_millis(1), || {});
        entered
            .recv_timeout(Duration::from_secs(10))
            .expect("the I/O thread got stuck");
        (transport, release)
    }

    fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// A local edit of `doc` (ops of a new peer).
    fn edit(docs: &mut MemDocs, doc: &DocKey, value: i64) {
        let d = LoroDoc::new();
        d.get_map("m").insert("v", value).unwrap();
        d.commit();
        docs.import(doc, &d.export(ExportMode::Snapshot).unwrap())
            .unwrap();
    }

    /// Closing or dropping the connection while a connect attempt's link
    /// waits to be adopted: the link is dropped on a background thread, which
    /// may wait for its I/O thread (here stuck in a NAS read), never on the
    /// calling thread.
    #[test]
    fn a_link_nobody_adopted_is_dropped_off_the_calling_thread() {
        for close in [false, true] {
            let (transport, release) = stuck_link(None, None);
            // The worker is done: its link waits in the channel.
            let (tx, rx) = mpsc::sync_channel(1);
            tx.send(Ok(Link {
                transport,
                access: Access::Write,
                _releasing: Releasing(None),
            }))
            .ok()
            .unwrap();
            let mut c = bare("drop", quiet());
            c.phase = Phase::Connecting { rx };
            // If dropping waits for the stuck read, let it go after 2 s.
            let (done, wait_done) = mpsc::channel::<()>();
            let releaser = std::thread::spawn(move || {
                let _ = wait_done.recv_timeout(Duration::from_secs(2));
                drop(release);
            });
            let started = Instant::now();
            if close {
                let mut closing = c.close(&mut MemDocs::new());
                assert!(matches!(closing.try_result(), Some(Ok(()))));
            } else {
                drop(c);
            }
            let took = started.elapsed();
            drop(done);
            releaser.join().unwrap();
            assert!(
                took < Duration::from_secs(1),
                "close={close}: the calling thread waited {took:?}"
            );
        }
    }

    /// Stop sharing a folder and share it again at once while the old link
    /// is still letting go (its I/O stuck on the NAS for longer than
    /// [`CLIENT_ID_GRACE`] and the lock wait of [`DirTransport::open`]): the
    /// new connection waits for it instead of trying the lock, so no attempt
    /// fails, and our own lock is never reported as another session's.
    #[test]
    fn sharing_again_waits_until_our_old_link_let_go() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_owned();
        let client = ClientId::new("alice").unwrap();
        let folder = DirConfig {
            poll_interval: Duration::from_millis(20),
            watch: false,
            ..DirConfig::default()
        };
        // The old link holds the folder's lock; its I/O is stuck.
        let lock = DirTransport::open(&dir, client.clone(), folder.clone()).unwrap();
        let (transport, release) = stuck_link(None, Some(lock));
        let mut old = bare("alice", quiet());
        old.target = Target::Folder(dir.clone());
        linked(&mut old, transport);
        let started = Instant::now();
        drop(old);
        assert!(started.elapsed() < Duration::from_secs(1));

        let mut config = ConnectionConfig::new(client, quiet());
        config.folder = folder;
        config.backoff = Backoff {
            first: Duration::from_millis(100),
            jitter: 0.0,
            ..Backoff::default()
        };
        let mut new = Connection::start(Target::Folder(dir), config);
        let mut docs = MemDocs::new();
        // An attempt that tried the lock would go offline once the lock wait
        // ran out, and fail once the grace did: watch past both. The attempt
        // waits for the old link before it tries the lock, so it is still
        // connecting all along.
        let until = Instant::now() + CLIENT_ID_GRACE.max(LOCK_WAIT) + Duration::from_millis(1500);
        while Instant::now() < until {
            new.pump(&mut docs);
            let status = new.status();
            assert!(
                matches!(status.state, ConnectionState::Connecting),
                "{status:?}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(release);
        wait_for("online once the old link let go", || {
            new.pump(&mut docs);
            new.is_online()
        });
    }

    /// Edits handed to a link whose I/O thread is stuck (a NAS read that
    /// hangs) are not on their way: unsynced, although the session counts
    /// them as sent.
    #[test]
    fn edits_waiting_on_a_stuck_link_are_unsynced() {
        let hello = Message::hello(ClientId::new("folder").unwrap(), Vec::new());
        let (transport, release) = stuck_link(Some(hello), None);
        let mut c = bare("stuck", quiet());
        c.config.io_stall = Duration::from_millis(200);
        linked(&mut c, transport);
        let mut docs = MemDocs::new();
        wait_for("online", || {
            c.pump(&mut docs);
            c.is_online()
        });
        edit(&mut docs, &DocKey::sublayer(&SublayerId::root()), 1);
        c.pump(&mut docs);
        assert!(c.session.unsynced(&docs).is_empty(), "handed to the link");
        wait_for("unsynced", || {
            c.pump(&mut docs);
            c.status().unsynced
        });
        assert!(c.is_online());
        // Unstuck: the edit is on its way.
        drop(release);
        wait_for("synced", || {
            c.pump(&mut docs);
            !c.status().unsynced
        });
    }

    /// When one pump could not take everything that arrived, the next pump
    /// is requested from another thread, not by calling `wake` from inside
    /// `pump` (an agent's `wake` may take a lock the pumping thread holds).
    #[test]
    fn a_full_pump_wakes_from_another_thread() {
        let pumping = std::thread::current().id();
        let inline = Arc::new(AtomicUsize::new(0));
        let woken = Arc::new(AtomicUsize::new(0));
        let (i, w) = (inline.clone(), woken.clone());
        let mut c = bare(
            "full",
            Arc::new(move || {
                if std::thread::current().id() == pumping {
                    i.fetch_add(1, Ordering::SeqCst);
                } else {
                    w.fetch_add(1, Ordering::SeqCst);
                }
            }),
        );
        c.session.config_mut().max_messages_per_pump = 2;
        let (mut remote, near) = MemoryTransport::pair();
        linked(
            &mut c,
            ThreadedTransport::spawn(near, Duration::from_millis(1), || {}),
        );
        for _ in 0..500 {
            remote
                .send(Message::hello(ClientId::new("hub").unwrap(), Vec::new()))
                .unwrap();
        }
        let mut docs = MemDocs::new();
        let mut before = 0;
        wait_for("a pump that takes the maximum", || {
            before = woken.load(Ordering::SeqCst);
            let received = c.session.stats().messages_received;
            c.pump(&mut docs);
            c.session.stats().messages_received - received == 2
        });
        assert_eq!(inline.load(Ordering::SeqCst), 0, "woken from inside pump");
        wait_for("a wake", || woken.load(Ordering::SeqCst) > before);
    }

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
        let alarm = Alarm::spawn(Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        }));
        let now = Instant::now();
        alarm.set(now + Duration::from_secs(3600));
        // Earlier wins; a later one while armed changes nothing.
        alarm.set(now + Duration::from_millis(20));
        alarm.set(now + Duration::from_secs(7200));
        let deadline = Instant::now() + Duration::from_secs(10);
        while woken.load(Ordering::SeqCst) == 0 {
            assert!(Instant::now() < deadline, "the alarm never fired");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(woken.load(Ordering::SeqCst), 1);
    }

    /// One pump asks for a wake at once (more arrived than it took), then
    /// arms the next keep-alive. After the wake, the pump it leads to arms
    /// the keep-alive again: it must fire, not be taken as armed already.
    #[test]
    fn the_alarm_arms_again_after_an_earlier_wake() {
        let woken = Arc::new(AtomicUsize::new(0));
        let w = woken.clone();
        let alarm = Alarm::spawn(Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        }));
        for round in 1..=20 {
            let now = Instant::now();
            alarm.set(now);
            alarm.set(now + Duration::from_millis(30));
            wait_for("the wake at once", || {
                woken.load(Ordering::SeqCst) >= 2 * round - 1
            });
            // The pump after the wake.
            alarm.set(Instant::now() + Duration::from_millis(30));
            wait_for("the keep-alive", || {
                woken.load(Ordering::SeqCst) >= 2 * round
            });
        }
    }

    /// A pump with a stale clock asks for a wake at once just as the thread
    /// times out on the armed deadline (after the timeout, before the thread
    /// takes the lock), and the next pump arms a later deadline, which the
    /// thread merges into the one at once. The later one must still fire,
    /// not stay armed with nothing to fire it: every later `set` would be
    /// dropped as armed already.
    #[test]
    fn the_alarm_keeps_a_later_deadline_merged_into_an_earlier_one() {
        let woken = Arc::new(AtomicUsize::new(0));
        let (entered_tx, entered) = mpsc::channel::<()>();
        let (release, gate) = mpsc::channel::<()>();
        let gate = Mutex::new(gate);
        let w = woken.clone();
        let alarm = Alarm::spawn(Arc::new(move || {
            if w.fetch_add(1, Ordering::SeqCst) == 0 {
                let _ = entered_tx.send(());
                let _ = gate.lock().unwrap().recv();
            }
        }));
        let start = Instant::now();
        alarm.set(start + Duration::from_millis(20));
        {
            // The thread times out and waits for the lock; meanwhile a
            // `set(start)` (exactly what it does).
            let mut armed = lock_armed(&alarm.armed);
            std::thread::sleep(Duration::from_millis(100));
            alarm.tx.send(start).unwrap();
            *armed = Some(start);
        }
        // The thread let go of `armed` and wakes; the next pump arms later.
        entered.recv_timeout(Duration::from_secs(20)).unwrap();
        let later = Instant::now() + Duration::from_millis(50);
        alarm.set(later);
        release.send(()).unwrap();
        wait_for("the wake at once", || woken.load(Ordering::SeqCst) >= 2);
        std::thread::sleep(
            (later + Duration::from_millis(20)).saturating_duration_since(Instant::now()),
        );
        // Nothing stays armed: a new deadline (past the later one) fires.
        let before = woken.load(Ordering::SeqCst);
        alarm.set(Instant::now() + Duration::from_millis(1));
        wait_for("a new deadline", || woken.load(Ordering::SeqCst) > before);
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
            // The query parameter form (spec 3.1), also in a connection.
            for url in [
                "ws://nas/projects/a?token=very-secret-token",
                "ws://nas/projects/a?x=1&TOKEN=very-secret-token#top",
                "ws://nas/projects/a?token",
            ] {
                let t = Target::Hub {
                    url: url.into(),
                    token: None,
                };
                let shown = format!("{t:?}");
                assert!(!shown.contains("very-secret"), "{shown}");
                assert!(shown.contains("token=…"), "{shown}");
            }
            assert!(
                format!("{:?}", redact_token("ws://nas/projects/a?x=1&y=2#f"))
                    .contains("x=1&y=2#f")
            );
            let c = Connection::start(
                Target::Hub {
                    url: "ws://127.0.0.1:9/projects/a?token=very-secret-token&x=1".into(),
                    token: None,
                },
                ConnectionConfig::new(ClientId::new("u").unwrap(), quiet()),
            );
            let shown = format!("{c:?}");
            assert!(
                !shown.contains("very-secret") && shown.contains("x=1"),
                "{shown}"
            );
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

    /// Pressing Retry on an unusable URL does not start a retry loop.
    #[cfg(feature = "hub")]
    #[test]
    fn retry_does_not_connect_to_an_unusable_url() {
        let mut config = ConnectionConfig::new(ClientId::new("u").unwrap(), quiet());
        config.backoff.first = Duration::from_millis(10);
        let mut c = Connection::start(
            Target::Hub {
                url: "wss://nas/projects/a".into(),
                token: None,
            },
            config,
        );
        c.retry();
        let mut docs = MemDocs::new();
        for _ in 0..50 {
            c.pump(&mut docs);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(c.connect_attempts(), 0);
        assert!(
            matches!(
                c.status().state,
                ConnectionState::Failed {
                    failure: Failure::BadTarget,
                    ..
                }
            ),
            "{:?}",
            c.status()
        );
    }
}
