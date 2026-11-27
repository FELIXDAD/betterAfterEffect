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
use crate::protocol::{DocKey, Message};
use crate::session::{SessionConfig, SyncSession};
use crate::threaded::ThreadedTransport;
use crate::transport::{Access, LinkNotes, Transport};

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
/// Connect attempts in a row that may fail on our own lock file in a shared
/// folder we can write ([`DirTransport::open`] denied, the write probe not)
/// before the next one joins it read-only after all. A lock file that is
/// being deleted (spec 7.5) is gone by then (each attempt already waits
/// [`crate::dir::LOCK_WAIT`] for it); one that stays unopenable (a read-only
/// copy, another user's file) would keep the connection offline for good.
const LOCK_DENIED_ATTEMPTS: u32 = 3;
/// How often the closing thread looks whether the link took what waits
/// ([`send_paced`]).
const CLOSE_POLL: Duration = Duration::from_millis(2);
/// How often the I/O thread of a hub link reads the socket.
#[cfg(feature = "hub")]
const HUB_TICK: Duration = Duration::from_millis(2);
/// Hubs: what a connect attempt may take beyond its own timeouts (starting
/// its thread, looking up the name) before it counts as unresponsive.
#[cfg(feature = "hub")]
const HUB_CONNECT_SLACK: Duration = Duration::from_secs(1);

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
    /// with our client ID quit. `None`: only on [`Connection::retry`]. Also
    /// how often a shared-folder link that is read-only checks whether the
    /// folder can be written now (and if so connects again, writable).
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
    /// It must not block. Hook it to something that runs a pump even when the
    /// window draws no frames (minimized, covered): an event-loop wake-up the
    /// shell handles without a repaint, not only egui's `request_repaint`. A
    /// hub drops a client it hears nothing from for 60 s, and `Hello`s and
    /// heartbeats only go out in `pump` (`docs/spec/sync.md` 7.9).
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
    /// A connect attempt still not online after this long is reported as
    /// [`ConnectionState::Unresponsive`] (it keeps waiting). Hubs: not before
    /// `2 × hub_timeout` plus a second either, because until then the
    /// attempt ends on its own timeouts (a hub that does not answer is
    /// reported [`ConnectionState::Offline`] with the reason, not "not
    /// responding" for a moment before that).
    pub unresponsive_after: Duration,
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
            unresponsive_after: Duration::from_secs(10),
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
    /// Still connecting after [`ConnectionConfig::unresponsive_after`] (since
    /// `since`): the shared folder or the hub does not answer, e.g. a NAS
    /// that hangs while the folder is opened or scanned, or while our
    /// previous link to it lets go (for hubs only past the connect's own
    /// timeouts, see there). Show "not responding"; the attempt keeps
    /// waiting (nothing is lost, local edits stay unsynced), never on the
    /// thread that pumps, and the state moves on once it is done.
    /// [`Connection::retry`] does nothing meanwhile (another attempt would
    /// wait for the same NAS).
    Unresponsive { since: Instant },
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
    /// before the remote confirmed them, waiting on a link whose I/O is
    /// stuck ([`ConnectionConfig::io_stall`]), or taken by a link that does
    /// not write them ([`Connection::link_notes`]).
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
    /// Shared folders: join read-only when our lock file is denied, even if
    /// `updates/` can be written (it was denied [`LOCK_DENIED_ATTEMPTS`]
    /// times in a row).
    read_only_if_denied: bool,
    io_tick: Option<Duration>,
    #[cfg(feature = "hub")]
    hub_timeout: Duration,
    #[cfg(feature = "hub")]
    hub_limits: HubLimits,
    #[cfg(feature = "hub")]
    hub_idle_timeout: Option<Duration>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

/// I/O errors that may mean "you may not write here" (or a lock file that
/// is being deleted, see [`folder_writable`]).
fn read_only_error(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
    )
}

/// Open the shared folder for writing (blocks).
fn open_folder(
    path: &Path,
    client: &ClientId,
    config: &DirConfig,
) -> Result<DirTransport, SyncError> {
    #[cfg(test)]
    if fault::deny_open(path) {
        let lock = path.join("updates").join(format!("{client}.lock"));
        return Err(SyncError::io(&lock)(
            std::io::ErrorKind::PermissionDenied.into(),
        ));
    }
    DirTransport::open(path, client.clone(), config.clone())
}

/// Whether `client` may write in the folder: a file of its own can be created
/// in `updates/` (blocks). [`DirTransport::open`] failing with access denied
/// does not tell: on Windows and SMB shares served by it, opening a lock file
/// that is "delete pending" (another client's cleanup deleted it while
/// holding it open, spec 4.7) fails that way too, also after `open`'s wait.
fn folder_writable(path: &Path, client: &ClientId) -> std::io::Result<bool> {
    #[cfg(test)]
    if fault::deny_probe(path) {
        return Ok(false);
    }
    DirTransport::probe_writable(path, client)
}

/// Whether a folder linked read-only can be linked writable now (blocks):
/// a file of our own can be created in `updates/`, and our lock file, if
/// there is one, can be opened for writing. The second is what failed when
/// the folder was joined read-only after [`LOCK_DENIED_ATTEMPTS`]; without
/// it such a link would be found writable, connect again, fail on the lock
/// and scan the folder again every [`Backoff::failed_retry`].
fn folder_writable_again(path: &Path, client: &ClientId) -> bool {
    if !matches!(folder_writable(path, client), Ok(true)) {
        return false;
    }
    #[cfg(test)]
    if fault::deny_lock(path) {
        return false;
    }
    matches!(DirTransport::probe_lock(path, client), Ok(true))
}

/// Test faults of shared folders, by path: [`DirTransport::open`] failing
/// with access denied (a lock file that stays delete pending, or cannot be
/// opened at all), creating any file in `updates/` failing too (a folder
/// this user cannot write), and our lock file failing to open for writing
/// when the write check tries it.
#[cfg(test)]
mod fault {
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard, PoisonError};

    #[derive(Clone, Copy, Debug, Default)]
    pub(super) struct Denied {
        /// This many more `open`s fail (`u32::MAX`: every one).
        pub open: u32,
        /// The write probe fails.
        pub probe: bool,
        /// The write check finds our lock file cannot be opened.
        pub lock: bool,
    }

    static FOLDERS: Mutex<Vec<(PathBuf, Denied)>> = Mutex::new(Vec::new());

    fn folders() -> MutexGuard<'static, Vec<(PathBuf, Denied)>> {
        FOLDERS.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn set(path: &Path, denied: Denied) {
        let mut list = folders();
        list.retain(|(p, _)| p != path);
        list.push((path.to_owned(), denied));
    }

    pub(super) fn deny_open(path: &Path) -> bool {
        let mut list = folders();
        let Some((_, d)) = list.iter_mut().find(|(p, _)| p == path) else {
            return false;
        };
        if d.open == 0 {
            return false;
        }
        if d.open != u32::MAX {
            d.open -= 1;
        }
        true
    }

    pub(super) fn deny_probe(path: &Path) -> bool {
        folders().iter().any(|(p, d)| p == path && d.probe)
    }

    pub(super) fn deny_lock(path: &Path) -> bool {
        folders().iter().any(|(p, d)| p == path && d.lock)
    }
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
                // Denied: read-only only if we cannot create another file
                // there either; otherwise it was the lock file (one being
                // deleted), and a later attempt gets it. Unless it was
                // denied a few times in a row already: then it stays so.
                let (dir, access) = match open_folder(path, &client, &self.folder) {
                    Ok(dir) => (dir, Access::Write),
                    Err(e) => {
                        let denied =
                            matches!(&e, SyncError::Io { source, .. } if read_only_error(source));
                        let read_only = denied
                            && (self.read_only_if_denied
                                || matches!(folder_writable(path, &client), Ok(false)));
                        if !read_only {
                            return Err(e);
                        }
                        let dir = DirTransport::open_read_only(path, client, self.folder.clone())?;
                        (dir, Access::Read)
                    }
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
                let mut access = hub.access();
                if access == Access::Unknown {
                    // A hub that does not say in the handshake (bae-hub
                    // 0.0.1): ask its status endpoint, still on this thread.
                    access = crate::hub::status_access(url, token.as_deref(), self.hub_timeout);
                }
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

/// Whether a shared folder linked read-only can be written now (spec 7.5).
enum WriteCheck {
    /// Not linked read-only to a folder, or no checks (`failed_retry` is
    /// `None`).
    Idle,
    /// Check at this time.
    At(Instant),
    /// A background thread is checking.
    Running(Receiver<bool>),
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
    /// Changes of [`Connection::link_notes`].
    notes: u64,
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
    /// A shared folder linked read-only: whether it can be written now is
    /// checked every [`Backoff::failed_retry`].
    write_check: WriteCheck,
    /// When the current connect attempt started.
    attempt_started: Instant,
    /// Still connecting after `unresponsive_after` (as of the last pump).
    unresponsive: bool,
    phase: Phase,
    access: Access,
    /// Failed attempts in a row since the last stable link.
    failures: u32,
    /// First of the current run of "client ID in use" failures.
    in_use_since: Option<Instant>,
    /// Connect attempts in a row that failed on our own lock file in a
    /// shared folder we can write ([`LOCK_DENIED_ATTEMPTS`]).
    lock_denied: u32,
    attempts: u64,
    unsynced: bool,
    /// The current (or last) link's [`Transport::notes`], copied when its
    /// `notes_seq` moved (`notes_seen`); `notes_changes` counts the copies.
    link_notes: LinkNotes,
    notes_seen: u64,
    notes_changes: u64,
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
            write_check: WriteCheck::Idle,
            attempt_started: Instant::now(),
            unresponsive: false,
            phase: Phase::Closed,
            access: Access::Unknown,
            failures: 0,
            in_use_since: None,
            lock_denied: 0,
            attempts: 0,
            unsynced: false,
            link_notes: LinkNotes::default(),
            notes_seen: 0,
            notes_changes: 0,
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
    /// the first link). While [`Access::Read`], local edits are not sent; a
    /// shared folder linked read-only is checked every
    /// [`Backoff::failed_retry`] and linked again, writable, once it can be
    /// written.
    pub fn access(&self) -> Access {
        self.access
    }

    /// Online: the link is up and the remote said what it has.
    pub fn is_online(&self) -> bool {
        matches!(self.phase, Phase::Linked { since: Some(_), .. })
    }

    /// Increases whenever [`status`](Self::status) or
    /// [`link_notes`](Self::link_notes) change.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// What the current link works around without failing (the last link's
    /// while offline; empty before the first): its last skipped problem
    /// (`last_error`, e.g. a shared-folder log it could not read), and the
    /// documents it does not write (`not_written`: our shared-folder log of
    /// them is of a newer format version, after a downgrade; spec 4.3).
    /// Edits to those count as unsynced ([`ConnectionStatus::unsynced`],
    /// [`Closing::unsynced`]) and closing the link fails. Show them, e.g.
    /// "changes to these layers are not saved to the shared folder: open the
    /// project with the newer BAE". A change bumps
    /// [`generation`](Self::generation).
    pub fn link_notes(&self) -> &LinkNotes {
        &self.link_notes
    }

    /// Copy the link's notes if they changed.
    fn take_notes(&mut self) {
        let fresh = match &self.phase {
            Phase::Linked { transport, .. } => {
                let seq = transport.notes_seq();
                (seq != self.notes_seen).then(|| (seq, transport.notes()))
            }
            _ => None,
        };
        if let Some((seq, notes)) = fresh {
            self.notes_seen = seq;
            if notes != self.link_notes {
                self.link_notes = notes;
                self.notes_changes += 1;
            }
        }
    }

    /// The session's unsynced documents and those of
    /// [`not_written`](Self::not_written).
    fn unsynced_docs(&self, docs: &dyn DocSet) -> Vec<DocKey> {
        let mut out = self.session.unsynced(docs);
        for doc in self.not_written(docs) {
            if !out.contains(&doc) {
                out.push(doc);
            }
        }
        out
    }

    /// Documents of [`LinkNotes::not_written`] with local ops the remote did
    /// not show it has: the session counted them as delivered when it sent
    /// them, but they were not written.
    fn not_written(&self, docs: &dyn DocSet) -> Vec<DocKey> {
        let confirmed = self.session.remote_versions();
        self.link_notes
            .not_written
            .iter()
            .filter(|doc| {
                docs.version(doc).is_some_and(|local| {
                    !local.is_empty()
                        && !confirmed
                            .and_then(|c| c.get(*doc))
                            .is_some_and(|theirs| theirs.includes_vv(&local))
                })
            })
            .cloned()
            .collect()
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
            Phase::Connecting { .. } | Phase::Linked { since: None, .. } if self.unresponsive => {
                ConnectionState::Unresponsive {
                    since: self.attempt_started,
                }
            }
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
        // Before the pump: notes copied before a message arrived come with
        // it (the shared folder's first scan, with its `Hello`).
        self.take_notes();
        if let Phase::Linked {
            transport, since, ..
        } = &mut self.phase
        {
            match self.session.pump(docs, transport) {
                Ok(o) => {
                    if since.is_none() && self.session.is_connected() {
                        *since = Some(now);
                    }
                    if o.received >= self.session.config().max_messages_per_pump
                        || self.session.pending_snapshot_requests() > 0
                    {
                        // More may be waiting (messages, or snapshot requests
                        // left for later pumps); the link only wakes on news.
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
        self.check_writable(now);
        self.unresponsive = self
            .connecting_since()
            .is_some_and(|t| now.saturating_duration_since(t) >= self.unresponsive_after());
        self.unsynced =
            stuck || !self.session.unsynced(docs).is_empty() || !self.not_written(docs).is_empty();
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
    /// left, import what arrived, and hand every local edit the remote lacks
    /// to a background thread (`flush`, ignoring `min_send_interval`; the
    /// data is exported here). That thread sends it with the session's flow
    /// control ([`SessionConfig::max_backlog`]), as the link takes it: all
    /// at once, a large upload held back by flow control could go over a hub
    /// link's `max_unsent` and fail although the hub was reading. Then it
    /// closes the link, which may take until the hub confirmed it read
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
                    unsynced: self.unsynced_docs(docs),
                    rx: None,
                    result: Some(Ok(())),
                };
            }
        };
        self.session.clear_presence();
        // The last pump sends as usual (flow control included); what it held
        // back is collected here and sent by the closing thread.
        let mut rest = Collect::default();
        let sent = self
            .session
            .pump(docs, &mut transport)
            .and_then(|_| self.session.flush(docs, &mut rest))
            .map(|_| ());
        if sent.is_err() {
            // The link is gone: what it did not confirm may not have arrived.
            self.session.forget_unconfirmed();
        }
        self.take_notes();
        let unsynced = self.unsynced_docs(docs);
        let pacing = Pacing {
            max_backlog: self.session.config().max_backlog,
            stall: self.close_stall(),
        };
        let (tx, rx) = mpsc::channel();
        let wake = self.config.wake.clone();
        let releasing = Releasing::new(&self.target, &self.config.client);
        let spawned = std::thread::Builder::new()
            .name("bae-sync-close".into())
            .spawn(move || {
                let paced = sent.and_then(|()| send_paced(&mut transport, rest.0, pacing));
                let closed = transport.close();
                drop(transport);
                drop(releasing);
                // The first error: a failed last pump means the link was
                // already gone.
                let _ = tx.send(paced.and(closed));
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
                self.write_check = match (&self.target, link.access) {
                    (Target::Folder(_), Access::Read) => self.next_write_check(now),
                    _ => WriteCheck::Idle,
                };
                self.in_use_since = None;
                self.lock_denied = 0;
                // Ours now: no longer counts as being let go.
                let Link {
                    transport,
                    _releasing: adopted,
                    ..
                } = link;
                drop(adopted);
                // The old link's notes go with it.
                if !self.link_notes.is_empty() {
                    self.link_notes = LinkNotes::default();
                    self.notes_changes += 1;
                }
                self.notes_seen = 0;
                self.phase = Phase::Linked {
                    transport,
                    adopted: now,
                    since: None,
                };
            }
            Some(Err(e)) => {
                self.lock_denied = if self.lock_was_denied(&e) {
                    self.lock_denied.saturating_add(1)
                } else {
                    0
                };
                self.attempt_failed(e, now);
            }
            None => {}
        }
    }

    /// A connect attempt to a shared folder failed on our own lock file
    /// although `updates/` can be written: the worker reports access denied
    /// only then (otherwise it joins read-only).
    fn lock_was_denied(&self, e: &SyncError) -> bool {
        matches!(self.target, Target::Folder(_))
            && matches!(e, SyncError::Io { source, .. } if read_only_error(source))
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
        self.attempt_started = now;
        self.unresponsive = false;
        let worker = Worker {
            client: self.config.client.clone(),
            folder: self.config.folder.clone(),
            read_only_if_denied: self.lock_denied >= LOCK_DENIED_ATTEMPTS,
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

    /// While linked read-only to a shared folder: every
    /// [`Backoff::failed_retry`], check on a background thread whether the
    /// folder can be written now (its permissions changed, our lock file can
    /// be opened again, or the link was made read-only when it should not
    /// have been; [`folder_writable_again`]); if so, connect again, writable.
    /// The edits made meanwhile then go out.
    fn check_writable(&mut self, now: Instant) {
        let path = match &self.target {
            Target::Folder(path) => path.clone(),
            #[cfg(feature = "hub")]
            Target::Hub { .. } => return,
        };
        if !matches!(self.phase, Phase::Linked { .. }) || self.access != Access::Read {
            self.write_check = WriteCheck::Idle;
            return;
        }
        self.write_check = match std::mem::replace(&mut self.write_check, WriteCheck::Idle) {
            WriteCheck::Idle => WriteCheck::Idle,
            WriteCheck::At(t) if now < t => WriteCheck::At(t),
            WriteCheck::At(_) => {
                let (tx, rx) = mpsc::channel();
                let client = self.config.client.clone();
                let wake = self.config.wake.clone();
                let spawned = std::thread::Builder::new()
                    .name("bae-sync-probe".into())
                    .spawn(move || {
                        let writable = folder_writable_again(&path, &client);
                        if tx.send(writable).is_ok() {
                            wake();
                        }
                    });
                match spawned {
                    Ok(_) => WriteCheck::Running(rx),
                    Err(_) => self.next_write_check(now),
                }
            }
            WriteCheck::Running(rx) => match rx.try_recv() {
                Ok(true) => {
                    self.reconnect(now);
                    WriteCheck::Idle
                }
                Ok(false) | Err(TryRecvError::Disconnected) => self.next_write_check(now),
                Err(TryRecvError::Empty) => WriteCheck::Running(rx),
            },
        };
    }

    fn next_write_check(&self, now: Instant) -> WriteCheck {
        self.config
            .backoff
            .failed_retry
            .map_or(WriteCheck::Idle, |d| WriteCheck::At(now + d))
    }

    /// Drop the link (off this thread) and connect again at once. Not a
    /// failure: the backoff stays as it is.
    fn reconnect(&mut self, now: Instant) {
        let Phase::Linked { transport, .. } = std::mem::replace(&mut self.phase, Phase::Closed)
        else {
            return;
        };
        self.session.forget_unconfirmed();
        self.retire(transport);
        self.connect_now(now);
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
            Phase::Connecting { .. } | Phase::Linked { since: None, .. } if self.unresponsive => {
                (4, Some(self.attempt_started), None, String::new(), None)
            }
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
            notes: self.notes_changes,
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

    /// Since when we have been connecting (the attempt runs, or the link is
    /// up and the remote has not said what it has); `None` otherwise.
    fn connecting_since(&self) -> Option<Instant> {
        match self.phase {
            Phase::Connecting { .. } | Phase::Linked { since: None, .. } => {
                Some(self.attempt_started)
            }
            _ => None,
        }
    }

    /// How long an attempt may go on before it counts as unresponsive
    /// ([`ConnectionConfig::unresponsive_after`]). A hub's attempt ends on
    /// its own timeouts (the TCP connect within `hub_timeout`, then each read
    /// and write of the handshake), so it counts only once it took longer
    /// than those allow; before, a hub that dropped our packets showed "not
    /// responding" for a few milliseconds right before every "offline".
    fn unresponsive_after(&self) -> Duration {
        match &self.target {
            Target::Folder(_) => self.config.unresponsive_after,
            #[cfg(feature = "hub")]
            Target::Hub { .. } => self.config.unresponsive_after.max(
                self.config
                    .hub_timeout
                    .saturating_mul(2)
                    .saturating_add(HUB_CONNECT_SLACK),
            ),
        }
    }

    /// When a connection that is still connecting becomes unresponsive, if
    /// that is still to come.
    fn unresponsive_at(&self, now: Instant) -> Option<Instant> {
        let at = self
            .connecting_since()?
            .checked_add(self.unresponsive_after())?;
        (at > now).then_some(at)
    }

    /// How long the closing thread waits for a backlog that does not go down
    /// ([`send_paced`]): a hub link's `stall_timeout` (its own check only
    /// counts what waits beyond its `max_backlog`, which may be larger than
    /// the session's); shared folders: as long as it takes, like their
    /// `close`.
    fn close_stall(&self) -> Option<Duration> {
        match &self.target {
            Target::Folder(_) => None,
            #[cfg(feature = "hub")]
            Target::Hub { .. } => Some(self.config.hub_limits.stall_timeout),
        }
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
                if let WriteCheck::At(t) = self.write_check {
                    at = at.min(t);
                }
                if let Some(t) = self.unresponsive_at(now) {
                    at = at.min(t);
                }
                Some(at)
            }
            Phase::Offline { retry_at, .. } => Some(*retry_at),
            Phase::Failed { retry_at, .. } => *retry_at,
            // The attempt wakes us when it is done, or when it is reported
            // as unresponsive.
            Phase::Connecting { .. } => self.unresponsive_at(now),
            Phase::Closed => None,
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

/// What a session sends, kept instead of sent: [`Connection::close`] flushes
/// into it on the thread that owns the documents, the closing thread sends it.
#[derive(Default)]
struct Collect(Vec<Message>);

impl Transport for Collect {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        self.0.push(msg);
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        Ok(None)
    }
}

/// How the closing thread sends what [`Connection::close`] flushed.
#[derive(Clone, Copy, Debug)]
struct Pacing {
    /// The session's flow control ([`SessionConfig::max_backlog`]).
    max_backlog: Option<usize>,
    /// Give up once the backlog did not go down for this long.
    stall: Option<Duration>,
}

/// Send `msgs` as the session would: before each, wait while more than
/// `max_backlog` waits in the link ([`Transport::backlog`]), so a link that
/// caps what may wait (a hub link's `max_unsent`) takes them all as long as
/// the peer reads. What arrives meanwhile is dropped (the session is gone);
/// a link that failed says so there, and the error is returned.
fn send_paced(
    transport: &mut ThreadedTransport,
    msgs: Vec<Message>,
    pacing: Pacing,
) -> Result<(), SyncError> {
    for msg in msgs {
        if let Some(max) = pacing.max_backlog {
            let mut lowest = usize::MAX;
            let mut since = Instant::now();
            loop {
                while transport.recv()?.is_some() {}
                let backlog = transport.backlog();
                if backlog <= max {
                    break;
                }
                if backlog < lowest {
                    (lowest, since) = (backlog, Instant::now());
                } else if let Some(stall) = pacing.stall
                    && since.elapsed() >= stall
                {
                    return Err(SyncError::Hub(format!(
                        "the peer took nothing of our last edits for {:.0} s ({} KiB waiting)",
                        stall.as_secs_f32(),
                        backlog >> 10
                    )));
                }
                std::thread::sleep(CLOSE_POLL);
            }
        }
        transport.send(msg)?;
    }
    Ok(())
}

/// A connection being closed ([`Connection::close`]).
#[derive(Debug)]
pub struct Closing {
    unsynced: Vec<DocKey>,
    rx: Option<Receiver<Result<(), SyncError>>>,
    result: Option<Result<(), SyncError>>,
}

impl Closing {
    /// Documents with local edits that were not handed to the link (or to
    /// the closing thread, which sends what flow control held back): closed
    /// while offline or connecting, read-only access, or the remote had not
    /// said what it has yet. Also those handed to a link that failed without
    /// the remote confirming them; a shared folder confirms only with its
    /// `Hello`s and what it reads of others, not our writes, so after a lost
    /// folder link this is every document edited on it (spec 7.4). And
    /// documents the link took edits of but does not write
    /// ([`LinkNotes::not_written`]). Known at once. Warn before quitting when
    /// this is not empty or the link reports an error.
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

    /// A folder with `updates/`, and a connection config that retries fast.
    fn folder_fixture(id: &str) -> (tempfile::TempDir, PathBuf, ConnectionConfig) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_owned();
        std::fs::create_dir_all(dir.join("updates")).unwrap();
        let mut config = ConnectionConfig::new(ClientId::new(id).unwrap(), quiet());
        config.folder = DirConfig {
            poll_interval: Duration::from_millis(20),
            watch: false,
            ..DirConfig::default()
        };
        config.backoff = Backoff {
            first: Duration::from_millis(50),
            jitter: 0.0,
            ..Backoff::default()
        };
        (tmp, dir, config)
    }

    /// Opening a folder we may write fails with access denied once (on
    /// Windows/SMB: our lock file is "delete pending" for longer than the
    /// lock wait, after another client's cleanup deleted it): a file of our
    /// own can be created in `updates/`, so it is not taken for a read-only
    /// folder; the attempt is retried and the link is writable. The probe
    /// leaves nothing behind: checked while the denied attempt is offline,
    /// since the retry that holds the lock removes our temporary files
    /// anyway (`DirTransport::remove_leftovers`).
    #[test]
    fn a_denied_lock_in_a_writable_folder_is_retried_not_read_only() {
        let (_tmp, dir, config) = folder_fixture("lock");
        fault::set(
            &dir,
            fault::Denied {
                open: 1,
                ..fault::Denied::default()
            },
        );
        let mut c = Connection::start(Target::Folder(dir.clone()), config);
        let mut docs = MemDocs::new();
        let mut offline = false;
        wait_for("online", || {
            c.pump(&mut docs);
            assert_ne!(c.access(), Access::Read, "{:?}", c.status());
            if matches!(c.status().state, ConnectionState::Offline { .. }) && !offline {
                offline = true;
                // The denied attempt probed before it reported; the retry
                // only starts on a later `pump`, once the backoff ran out.
                assert_eq!(c.connect_attempts(), 1);
                let left: Vec<_> = std::fs::read_dir(dir.join("updates"))
                    .unwrap()
                    .flatten()
                    .map(|e| e.file_name())
                    .filter(|n| n.to_string_lossy().contains(".tmp-"))
                    .collect();
                assert!(left.is_empty(), "the probe left {left:?}");
            }
            c.is_online()
        });
        assert!(offline, "the denied attempt went offline");
        assert_eq!(c.access(), Access::Write);
        assert_eq!(c.connect_attempts(), 2);
    }

    /// A folder linked read-only checks every `failed_retry` whether it can
    /// be written now; once it can, the connection connects again, writable,
    /// and the edit made meanwhile goes out.
    #[test]
    fn a_read_only_folder_link_becomes_writable_once_it_can_write() {
        let (_tmp, dir, mut config) = folder_fixture("upgrade");
        config.backoff.failed_retry = Some(Duration::from_millis(100));
        fault::set(
            &dir,
            fault::Denied {
                open: u32::MAX,
                probe: true,
                lock: false,
            },
        );
        let mut c = Connection::start(Target::Folder(dir.clone()), config);
        let mut docs = MemDocs::new();
        wait_for("online read-only", || {
            c.pump(&mut docs);
            c.is_online()
        });
        assert_eq!(c.access(), Access::Read);
        assert!(!c.session.config().send_data);
        let root = DocKey::sublayer(&SublayerId::root());
        edit(&mut docs, &root, 1);
        // Still not writable: checked a few times, the link stays.
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(400) {
            c.pump(&mut docs);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(c.is_online());
        assert_eq!(c.access(), Access::Read);
        assert!(c.status().unsynced);
        assert_eq!(c.connect_attempts(), 1);
        // Writable now.
        fault::set(&dir, fault::Denied::default());
        wait_for("online writable, the edit written", || {
            c.pump(&mut docs);
            c.is_online() && c.access() == Access::Write && !c.status().unsynced
        });
        assert_eq!(c.connect_attempts(), 2);
        wait_for("the edit in our log", || {
            c.pump(&mut docs);
            dir.join("updates/root/upgrade.log").is_file()
        });
    }

    /// Our own lock file cannot be opened although `updates/` can be written
    /// (a lock file copied read-only or made by another user, a Windows
    /// folder marked read-only in Explorer): after [`LOCK_DENIED_ATTEMPTS`]
    /// offline attempts the folder is joined read-only instead of staying
    /// offline for good. The write check tries the lock file too, so the
    /// link stays as long as it cannot be opened (no reconnect and rescan
    /// every check), and becomes writable once it can.
    #[test]
    fn a_lock_file_denied_for_good_joins_read_only() {
        let (_tmp, dir, mut config) = folder_fixture("locked");
        config.backoff.failed_retry = Some(Duration::from_millis(100));
        fault::set(
            &dir,
            fault::Denied {
                open: u32::MAX,
                probe: false,
                lock: true,
            },
        );
        let mut c = Connection::start(Target::Folder(dir.clone()), config);
        let mut docs = MemDocs::new();
        let mut offline = 0;
        let mut was_offline = false;
        wait_for("online read-only", || {
            c.pump(&mut docs);
            let now_offline = matches!(c.status().state, ConnectionState::Offline { .. });
            offline += u32::from(now_offline && !was_offline);
            was_offline = now_offline;
            c.is_online()
        });
        assert_eq!(c.access(), Access::Read);
        assert_eq!(offline, LOCK_DENIED_ATTEMPTS);
        let attempts = u64::from(LOCK_DENIED_ATTEMPTS) + 1;
        assert_eq!(c.connect_attempts(), attempts);
        // Checked a few times, still denied: the link stays.
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(400) {
            c.pump(&mut docs);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(c.is_online());
        assert_eq!(c.access(), Access::Read);
        assert_eq!(c.connect_attempts(), attempts);
        // The lock file can be opened again.
        fault::set(&dir, fault::Denied::default());
        wait_for("online writable", || {
            c.pump(&mut docs);
            c.is_online() && c.access() == Access::Write
        });
        assert_eq!(c.connect_attempts(), attempts + 1);
    }

    /// `n` pseudo-random letters (Loro does not get them much smaller).
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

    /// A link that lets at most `cap` bytes wait, like a hub link's
    /// `max_unsent`: a send that would make more wait fails, and so does
    /// every later one. `drain` bytes leave per `recv` (a peer that reads
    /// slowly). Records the documents of the data it took.
    struct Capped {
        hello: Option<Message>,
        unsent: usize,
        cap: usize,
        drain: usize,
        over: bool,
        took: Arc<Mutex<Vec<String>>>,
    }

    impl Transport for Capped {
        fn send(&mut self, msg: Message) -> Result<(), SyncError> {
            let len = msg.approx_len();
            if self.over || (self.unsent > 0 && self.unsent + len > self.cap) {
                self.over = true;
                return Err(SyncError::Hub(format!(
                    "{} bytes would wait, over {}",
                    self.unsent + len,
                    self.cap
                )));
            }
            self.unsent += len;
            if let Message::Updates { doc, .. } = msg {
                self.took.lock().unwrap().push(doc.to_string());
            }
            Ok(())
        }

        fn recv(&mut self) -> Result<Option<Message>, SyncError> {
            self.unsent = self.unsent.saturating_sub(self.drain);
            Ok(self.hello.take())
        }

        fn backlog(&self) -> usize {
            self.unsent
        }
    }

    /// Closing while flow control holds back most of an upload (a first
    /// share of large sublayers over a slow link): the rest goes out on the
    /// closing thread with the session's flow control, as the link takes it,
    /// and everything arrives. It used to go out all at once, over the link's
    /// cap (a hub link's `max_unsent`): the link failed and closing reported
    /// an error although the peer kept reading.
    #[test]
    fn closing_sends_what_flow_control_held_back_as_the_link_takes_it() {
        let mut docs = MemDocs::new();
        let keys: Vec<DocKey> = (0..8)
            .map(|i| DocKey::parse(&format!("layer{i}")).unwrap())
            .collect();
        for (i, key) in keys.iter().enumerate() {
            let d = LoroDoc::new();
            d.get_map("m")
                .insert("v", text(i as u64 + 1, 4000))
                .unwrap();
            d.commit();
            docs.load(key.clone(), &d.export(ExportMode::Snapshot).unwrap())
                .unwrap();
        }
        // As the hub link's default is made (`HubLimits::max_unsent_for`):
        // two of the largest messages and `max_backlog`.
        let max_backlog = 1000;
        let largest = keys
            .iter()
            .map(|k| docs.export_updates(k, &Default::default()).unwrap().len() + 64)
            .max()
            .unwrap();
        let took = Arc::new(Mutex::new(Vec::new()));
        let link = Capped {
            hello: Some(Message::hello(ClientId::new("hub").unwrap(), Vec::new())),
            unsent: 0,
            cap: 2 * largest + max_backlog,
            drain: 500,
            over: false,
            took: took.clone(),
        };
        let mut c = bare("closing", quiet());
        c.session.config_mut().max_backlog = Some(max_backlog);
        linked(
            &mut c,
            ThreadedTransport::spawn(link, Duration::from_millis(1), || {}),
        );
        wait_for("online", || {
            c.pump(&mut docs);
            c.is_online()
        });
        assert!(c.session.stats().held_back > 0, "{:?}", c.session.stats());
        let mut closing = c.close(&mut docs);
        assert!(closing.unsynced().is_empty(), "{:?}", closing.unsynced());
        let result = closing
            .wait(Duration::from_secs(20))
            .map(|r| r.as_ref().map_err(ToString::to_string).copied());
        assert_eq!(result, Some(Ok(())));
        let mut got = took.lock().unwrap().clone();
        got.sort();
        got.dedup();
        let want: Vec<String> = keys.iter().map(ToString::to_string).collect();
        assert_eq!(got, want);
    }

    /// A connect attempt that hangs on the NAS (here: it waits for our old
    /// link to the folder to let go, whose I/O is stuck in a read) is
    /// reported as unresponsive after `unresponsive_after`, also to an app
    /// that pumps only when woken; pumps stay fast; the attempt goes on and
    /// links once the NAS answers.
    #[test]
    fn a_connect_that_hangs_on_the_nas_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_owned();
        let client = ClientId::new("hang").unwrap();
        let folder = DirConfig {
            poll_interval: Duration::from_millis(20),
            watch: false,
            ..DirConfig::default()
        };
        let lock = DirTransport::open(&dir, client.clone(), folder.clone()).unwrap();
        let (transport, release) = stuck_link(None, Some(lock));
        let mut old = bare("hang", quiet());
        old.target = Target::Folder(dir.clone());
        linked(&mut old, transport);
        drop(old);

        let woken = Arc::new(AtomicUsize::new(0));
        let w = woken.clone();
        let mut config = ConnectionConfig::new(
            client,
            Arc::new(move || {
                w.fetch_add(1, Ordering::SeqCst);
            }),
        );
        config.folder = folder;
        config.unresponsive_after = Duration::from_millis(300);
        let started = Instant::now();
        let mut c = Connection::start(Target::Folder(dir), config);
        let mut docs = MemDocs::new();
        c.pump(&mut docs);
        assert_eq!(c.status().state, ConnectionState::Connecting);
        let generation = c.generation();
        // Pumped only when woken: the alarm wakes us when it is due.
        wait_for("a wake", || woken.load(Ordering::SeqCst) > 0);
        let t = Instant::now();
        let out = c.pump(&mut docs);
        assert!(
            t.elapsed() < Duration::from_millis(100),
            "{:?}",
            t.elapsed()
        );
        assert!(started.elapsed() >= Duration::from_millis(300));
        assert!(out.status_changed);
        let state = c.status().state;
        assert!(
            matches!(state, ConnectionState::Unresponsive { since } if since >= started),
            "{state:?}"
        );
        // It stays so without new generations while the NAS hangs.
        let generation_unresponsive = c.generation();
        assert!(generation_unresponsive > generation);
        for _ in 0..20 {
            c.pump(&mut docs);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(c.generation(), generation_unresponsive);
        assert!(matches!(
            c.status().state,
            ConnectionState::Unresponsive { .. }
        ));
        assert_eq!(c.connect_attempts(), 1);
        // The NAS answers: online.
        drop(release);
        wait_for("online", || {
            c.pump(&mut docs);
            c.is_online()
        });
        assert_eq!(c.connect_attempts(), 1);
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

    /// Snapshot requests left for later pumps (`max_snapshots_per_pump`)
    /// bring the next pump soon, like a full pump: an app that pumps only
    /// when woken answers them all.
    #[test]
    fn snapshot_requests_left_for_later_wake_the_next_pump() {
        let woken = Arc::new(AtomicUsize::new(0));
        let w = woken.clone();
        let mut c = bare(
            "snapshots",
            Arc::new(move || {
                w.fetch_add(1, Ordering::SeqCst);
            }),
        );
        // No other reason to wake for an hour (`Hello`s, heartbeats).
        let config = c.session.config_mut();
        config.hello_interval = Some(Duration::from_secs(3600));
        config.presence_heartbeat = Duration::from_secs(3600);
        let (mut remote, near) = MemoryTransport::pair();
        linked(
            &mut c,
            ThreadedTransport::spawn(near, Duration::from_millis(1), || {}),
        );
        let mut docs = MemDocs::new();
        let names = ["a", "b", "c", "d", "e"];
        for doc in names {
            let d = bae_model::loro::LoroDoc::new();
            d.get_map("m").insert("v", doc).unwrap();
            d.commit();
            let snapshot = d.export(bae_model::loro::ExportMode::Snapshot).unwrap();
            docs.load(DocKey::parse(doc).unwrap(), &snapshot).unwrap();
        }
        remote
            .send(Message::hello(ClientId::new("folder").unwrap(), Vec::new()))
            .unwrap();
        for doc in names {
            remote
                .send(Message::SnapshotRequest {
                    doc: DocKey::parse(doc).unwrap(),
                })
                .unwrap();
        }
        // Wakes counted before each pump: the alarm may fire before the pump
        // returns.
        let mut before = 0;
        wait_for("the requests to arrive", || {
            before = woken.load(Ordering::SeqCst);
            c.pump(&mut docs);
            c.session.stats().messages_received >= 6
        });
        let mut answered = 0;
        let mut collect = |remote: &mut MemoryTransport| {
            while let Ok(Some(m)) = remote.recv() {
                answered += usize::from(matches!(m, Message::Snapshot { .. }));
            }
            answered
        };
        // Pump only when woken.
        while c.session.pending_snapshot_requests() > 0 {
            wait_for("a wake", || woken.load(Ordering::SeqCst) > before);
            before = woken.load(Ordering::SeqCst);
            c.pump(&mut docs);
        }
        wait_for("every answer", || collect(&mut remote) == names.len());
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

    /// A hub's connect attempt is bounded by its own timeouts: it counts as
    /// unresponsive only once it took longer than they allow (two
    /// `hub_timeout`s and [`HUB_CONNECT_SLACK`]), however short
    /// `unresponsive_after` is; a folder's at `unresponsive_after`. The same
    /// for a link that is up while the remote has not said hello.
    #[cfg(feature = "hub")]
    #[test]
    fn a_hub_attempt_is_unresponsive_only_past_its_own_timeouts() {
        let mut config = ConnectionConfig::new(ClientId::new("u").unwrap(), quiet());
        config.hub_timeout = Duration::from_millis(200);
        config.unresponsive_after = Duration::from_millis(50);
        let bound = Duration::from_millis(400) + HUB_CONNECT_SLACK;
        let mut c = Connection::start(
            Target::Hub {
                url: "ws://127.0.0.1:9/projects/a".into(),
                token: None,
            },
            config,
        );
        assert_eq!(c.unresponsive_after(), bound);
        // An attempt that never finishes.
        let (_worker, rx) = mpsc::sync_channel(1);
        c.phase = Phase::Connecting { rx };
        let mut docs = MemDocs::new();
        let ago = |d: Duration| Instant::now().checked_sub(d).unwrap();
        c.attempt_started = ago(Duration::from_millis(300));
        c.pump(&mut docs);
        assert_eq!(c.status().state, ConnectionState::Connecting);
        c.attempt_started = ago(bound + Duration::from_millis(100));
        c.pump(&mut docs);
        assert!(
            matches!(c.status().state, ConnectionState::Unresponsive { .. }),
            "{:?}",
            c.status()
        );
        // Linked, no `Hello` yet.
        let (_remote, near) = MemoryTransport::pair();
        linked(
            &mut c,
            ThreadedTransport::spawn(near, Duration::from_millis(1), || {}),
        );
        c.attempt_started = ago(Duration::from_millis(300));
        c.pump(&mut docs);
        assert_eq!(c.status().state, ConnectionState::Connecting);
        // A folder: at `unresponsive_after`.
        c.target = Target::Folder(PathBuf::from("/nonexistent/bae/connection/test"));
        assert_eq!(c.unresponsive_after(), Duration::from_millis(50));
        c.pump(&mut docs);
        assert!(
            matches!(c.status().state, ConnectionState::Unresponsive { .. }),
            "{:?}",
            c.status()
        );
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
