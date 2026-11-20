//! bae-hub: the collaboration hub (ARCHITECTURE §7 "Hub 模式", workstream 10).
//!
//! A small server for a NAS or any always-on machine on the team's LAN:
//!
//! - **Real-time sync.** `GET /projects/<id>` with a WebSocket upgrade carries
//!   the `bae-sync` protocol, one message per binary frame. The handshake and
//!   framing are `bae_sync::HubTransport`, the same code the clients use; each
//!   project is one [`bae_sync::Relay`] and every connection a transport on it.
//! - **Project logs.** Each project's documents are kept in
//!   `<data>/projects/<id>/` in the shared-folder log format of `bae-sync`, so
//!   the hub survives restarts and crashes with nothing lost but the last
//!   unwritten moment (which clients upload again on reconnect).
//! - **Team tokens.** Read or write access, optionally limited to some projects
//!   (prototype of permissions; see [`auth`]).
//!
//! Plain threads, no async runtime: one per loaded project (plus one writing its
//! directory) and a short-lived one per incoming connection. The HTTP API and
//! data layout are described in `services/bae-hub/README.md`; deployment
//! (Docker, NAS) in `deploy/`.

pub mod auth;
pub mod config;
mod error;
mod link;
pub mod probe;
mod project;
mod registry;
mod server;

use std::fs::{File, OpenOptions};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub use config::{Access, Anonymous, HubConfig, TokenConfig};
pub use error::HubError;
pub use project::{HUB_CLIENT, PeerStatus, ProjectId};
pub use registry::ProjectSummary;
pub use server::SYNC_PREFIX;

use crate::auth::Auth;
use crate::registry::Registry;
use crate::server::Shared;

/// Lock file in the data directory: one hub per data directory.
const LOCK_FILE: &str = "hub.lock";
/// How long shutdown waits for connections that are still being set up.
const PENDING_GRACE: Duration = Duration::from_secs(5);

/// A hub that is accepting connections. Dropping it shuts it down.
pub struct RunningHub {
    addr: SocketAddr,
    shared: Arc<Shared>,
    acceptor: Option<JoinHandle<()>>,
    janitor: Option<(JoinHandle<()>, mpsc::Sender<()>)>,
    _lock: File,
}

impl std::fmt::Debug for RunningHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningHub")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl RunningHub {
    /// The address it listens on (useful with port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Close every connection, save every project and stop. Blocks until done.
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        if self.shared.stopping.swap(true, Ordering::AcqRel) {
            return;
        }
        // Wake the blocking accept loop with one last connection.
        let _ = TcpStream::connect_timeout(&wake_addr(self.addr), Duration::from_secs(1));
        if let Some(t) = self.acceptor.take() {
            let _ = t.join();
        }
        if let Some((t, stop)) = self.janitor.take() {
            drop(stop);
            let _ = t.join();
        }
        let deadline = Instant::now() + PENDING_GRACE;
        while self.shared.pending.load(Ordering::Acquire) > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.shared.registry.shutdown();
        tracing::info!("bae-hub stopped");
    }
}

impl Drop for RunningHub {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Where to connect to reach a listener bound to `addr`.
fn wake_addr(addr: SocketAddr) -> SocketAddr {
    let ip = match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    SocketAddr::new(ip, addr.port())
}

/// Bind `config.listen` and start serving on background threads.
pub fn start(config: HubConfig) -> Result<RunningHub, HubError> {
    config.validate()?;
    std::fs::create_dir_all(&config.data_dir).map_err(HubError::io(&config.data_dir))?;
    let lock = lock_data_dir(&config)?;
    let listener = TcpListener::bind(config.listen).map_err(HubError::Network)?;
    let addr = listener.local_addr().map_err(HubError::Network)?;
    let config = Arc::new(config);
    let shared = Arc::new(Shared {
        auth: Auth::new(&config.tokens, config.anonymous),
        registry: Registry::new(config.clone())?,
        stopping: AtomicBool::new(false),
        started: Instant::now(),
        pending: AtomicUsize::new(0),
        config: config.clone(),
    });
    tracing::info!(
        %addr,
        data = %config.data_dir.display(),
        tokens = config.tokens.len(),
        anonymous = ?config.anonymous,
        "bae-hub {} listening",
        env!("CARGO_PKG_VERSION")
    );
    let acceptor = std::thread::Builder::new()
        .name("hub-accept".into())
        .spawn({
            let shared = shared.clone();
            move || server::accept_loop(listener, shared)
        })
        .map_err(HubError::Network)?;
    let janitor = match config.idle_unload {
        Some(idle) => {
            let (stop, rx) = mpsc::channel::<()>();
            let shared = shared.clone();
            let every = (idle / 4).clamp(Duration::from_millis(50), Duration::from_secs(30));
            let t = std::thread::Builder::new()
                .name("hub-janitor".into())
                .spawn(move || {
                    while let Err(mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(every) {
                        shared.registry.unload_idle(idle);
                    }
                })
                .map_err(HubError::Network)?;
            Some((t, stop))
        }
        None => None,
    };
    Ok(RunningHub {
        addr,
        shared,
        acceptor: Some(acceptor),
        janitor,
        _lock: lock,
    })
}

fn lock_data_dir(config: &HubConfig) -> Result<File, HubError> {
    let path = config.data_dir.join(LOCK_FILE);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(HubError::io(&path))?;
    match file.try_lock() {
        // File systems without locks are accepted as they are.
        Ok(()) | Err(std::fs::TryLockError::Error(_)) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => {
            Err(HubError::DataDirInUse(config.data_dir.clone()))
        }
    }
}
