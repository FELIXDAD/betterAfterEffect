//! One open project: a [`Relay`] on its own thread, with the project's directory
//! attached as a peer for persistence.
//!
//! The directory uses the shared-folder format of `bae-sync` (`docs/spec/sync.md`
//! §4): the hub is client `hub` and appends to `updates/<doc>/hub.log`, compacted
//! into a snapshot when it grows. On start the relay reads it back through the
//! same transport, so loading, saving and crash repair are `bae-sync`'s code.
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
    ClientId, DirConfig, DirTransport, DocSet, PeerHandle, Relay, SyncError, ThreadedTransport,
    Transport,
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
/// The relay sends edits to the project directory at most this often. While a
/// client drags a value, the edits of 250 ms become one append to the log
/// instead of one per message: fewer disk writes. Shutdown drops the throttle
/// and saves what is still held back.
const STORE_SEND_INTERVAL: Duration = Duration::from_millis(250);
/// Most pumps on shutdown, until the project directory has been sent every
/// edit. One is enough unless clients are still sending: what a pump imports
/// in its last round reaches the directory on the next one.
const FINAL_PUMPS: usize = 8;

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
}

/// Snapshot of a project's state, refreshed after every pump.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectStatus {
    pub peers: Vec<PeerStatus>,
    pub docs: usize,
    /// The project directory is attached (edits are being saved).
    pub store_ok: bool,
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
        let (events, rx) = mpsc::channel();
        let waker = Waker {
            events: events.clone(),
            pending: Arc::new(AtomicBool::new(false)),
        };
        let status = Arc::new(Mutex::new(ProjectStatus::default()));
        let (ready_tx, ready_rx) = mpsc::channel();
        let ctx = RelayThread {
            id: id.clone(),
            dir,
            ticks: Ticks {
                active: config.active_tick,
                quiet: config.quiet_tick.max(config.active_tick),
                empty: config.empty_tick,
            },
            rx,
            waker,
            status: status.clone(),
        };
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
    rx: mpsc::Receiver<Event>,
    waker: Waker,
    status: Arc<Mutex<ProjectStatus>>,
}

fn hub_client() -> ClientId {
    ClientId::new(HUB_CLIENT).expect("valid client ID")
}

fn open_store(dir: &Path, waker: &Waker) -> Result<ThreadedTransport, SyncError> {
    let config = DirConfig {
        poll_interval: STORE_POLL,
        watch: false,
        ..DirConfig::default()
    };
    let store = DirTransport::open(dir, hub_client(), config)?;
    let waker = waker.clone();
    Ok(ThreadedTransport::spawn(store, STORE_TICK, move || {
        waker.wake()
    }))
}

/// Attach the project directory to the relay, with its send throttle and
/// without backpressure.
///
/// Backpressure (`SessionConfig::max_backlog`) keeps a slow client from
/// queuing more than one large message at a time; what waits stays in the
/// relay's documents. The directory is not a slow link: its queue is in-process
/// memory in front of the local disk, which takes it within milliseconds.
/// Holding it back would only leave edits unsaved for longer, and those still
/// held back when the project closes would be lost.
fn connect_store(relay: &mut Relay, store: ThreadedTransport) -> PeerHandle {
    let mut config = relay.session_config();
    config.min_send_interval = STORE_SEND_INTERVAL;
    config.max_backlog = usize::MAX;
    relay.connect_with(Box::new(store), config)
}

enum Load {
    Done,
    TimedOut,
    StoreFailed(SyncError),
    Shutdown,
}

impl RelayThread {
    fn run(self, ready: mpsc::Sender<Result<(), HubError>>) {
        let mut relay = Relay::new(hub_client());
        let store = match open_store(&self.dir, &self.waker) {
            Ok(s) => s,
            Err(e) => {
                let _ = ready.send(Err(e.into()));
                return;
            }
        };
        let store_handle = connect_store(&mut relay, store);
        let started = Instant::now();
        match self.load(&mut relay, store_handle) {
            Load::Done => tracing::info!(
                project = %self.id,
                docs = relay.docs().doc_keys().len(),
                ms = started.elapsed().as_millis() as u64,
                "project loaded"
            ),
            Load::TimedOut => tracing::warn!(
                project = %self.id,
                "project still loading; accepting connections anyway"
            ),
            Load::StoreFailed(e) => {
                let _ = ready.send(Err(e.into()));
                return;
            }
            Load::Shutdown => return,
        }
        if ready.send(Ok(())).is_err() {
            return;
        }
        self.serve(relay, Some(store_handle));
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

    fn serve(self, mut relay: Relay, mut store: Option<PeerHandle>) {
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
                        let handle = relay.connect(transport);
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
            for (handle, e) in out.disconnected {
                if Some(handle) == store {
                    tracing::error!(project = %self.id, error = %e, "project directory failed; edits are not being saved");
                    store = None;
                    retry_store_at = Some(Instant::now() + STORE_RETRY);
                } else if let Some(info) = peers.remove(&handle) {
                    tracing::info!(project = %self.id, peer = ?handle, addr = %info.addr, reason = %e, "client disconnected");
                }
            }
            for (handle, e) in out.errors {
                tracing::warn!(project = %self.id, peer = ?handle, error = %e, "skipped a message");
            }
            if store.is_none() && retry_store_at.is_some_and(|t| Instant::now() >= t) {
                match open_store(&self.dir, &self.waker) {
                    Ok(t) => {
                        store = Some(connect_store(&mut relay, t));
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
            self.publish_status(&relay, &peers, store, idle_since, received, sent);
            if shutdown {
                break;
            }
        }
        // Final pumps: the store gets every edit, without its throttle (and
        // without backpressure, set again in case it was changed). Dropping
        // the relay closes every connection and drops the store transport,
        // which writes everything still queued before its thread ends.
        if let Some(config) = store.and_then(|h| relay.peer_config_mut(h)) {
            config.min_send_interval = Duration::ZERO;
            config.max_backlog = usize::MAX;
        }
        for _ in 0..FINAL_PUMPS {
            relay.pump();
            // Done once the store has been sent everything (or is gone).
            if store
                .and_then(|h| relay.unsynced(h))
                .is_none_or(|docs| docs.is_empty())
            {
                break;
            }
        }
        drop(relay);
        tracing::info!(project = %self.id, "project closed");
    }

    fn publish_status(
        &self,
        relay: &Relay,
        peers: &BTreeMap<PeerHandle, PeerInfo>,
        store: Option<PeerHandle>,
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
                })
                .collect(),
            docs: relay.docs().doc_keys().len(),
            store_ok: store.is_some(),
            idle_since,
            messages_received: received,
            messages_sent: sent,
        };
        *self.status.lock().expect("status lock") = status;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_ids() {
        assert!(ProjectId::new("film-a_01").is_ok());
        for bad in ["", "a b", "../x", "a/b", "a.b", "é", &"x".repeat(65)] {
            assert!(ProjectId::new(bad).is_err(), "{bad:?}");
        }
    }
}
