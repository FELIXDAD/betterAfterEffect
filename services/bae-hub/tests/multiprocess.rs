//! Hub mode with every party in its own OS process: the real `bae-hub` binary
//! and client processes using the app's client code (`HubTransport` in a
//! `ThreadedTransport`, a `SyncSession` on a `Stage`), over real sockets on
//! loopback. Processes are killed without any cleanup (`Child::kill`:
//! `TerminateProcess` on Windows, `SIGKILL` on Unix): the hub in the middle of
//! editing, then started again on the same data directory and port; a client in
//! the middle of editing, then started again with the same `ClientId` from its
//! saved state or from scratch. A fault-injecting TCP proxy between clients and
//! hub cuts connections (sometimes in the middle of a frame) and delays traffic
//! in both directions.
//!
//! TCP never loses, duplicates or reorders bytes inside a connection, so a cut
//! connection and delays are the faults a real network produces at this level.
//! Loss, duplication and reordering of single messages are covered in-process
//! by the simulated network in `crates/bae-sync/tests/convergence.rs` (`SimNet`).
//!
//! The clients are this test binary started again (see
//! `crates/bae-sync/tests/common/procs.rs`): the `mp_child` entry runs a client
//! when `BAE_MP_ROLE` is set and does nothing otherwise.

mod common;
#[path = "../../../crates/bae-sync/tests/common/procs.rs"]
mod procs;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bae_model::Stage;
use bae_sync::{
    ClientId, HubTransport, SessionConfig, SyncError, SyncSession, ThreadedTransport, Transport,
};
use common::{Rng, WRITE, author, base_snapshots, client_stage, fingerprint, random_edit};
use procs::*;

/// The test that runs the clients (see the module docs).
const ENTRY: &str = "mp_child";
const PROJECT: &str = "film";

/// Generous: debug builds on a busy CI machine.
const START: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(30);
const SETTLE: Duration = Duration::from_secs(90);

// ---- the client process ----

#[test]
fn mp_child() {
    match role().as_deref() {
        None => {}
        Some("hub") => hub_client(),
        Some(other) => panic!("unknown role {other}"),
    }
}

/// Connect on a thread of its own (connecting to a hub that is down can take a
/// while, on Windows seconds), so the client keeps editing meanwhile.
fn connect(url: &str, wake: &Sender<Input>) -> Receiver<Result<ThreadedTransport, SyncError>> {
    let (tx, rx) = mpsc::channel();
    let (url, wake) = (url.to_owned(), wake.clone());
    std::thread::spawn(move || {
        let link = HubTransport::connect(&url, Some(WRITE), Duration::from_secs(2)).map(|hub| {
            let wake = wake.clone();
            ThreadedTransport::spawn(hub, Duration::from_millis(1), move || {
                let _ = wake.send(Input::Wake);
            })
        });
        let _ = tx.send(link);
        let _ = wake.send(Input::Wake);
    });
    rx
}

/// A client: `BAE_MP_URL`, `BAE_MP_ID` (client ID), `BAE_MP_TAG` (names of the
/// layers it creates), `BAE_MP_SEED`, `BAE_MP_BASE` (the base project's
/// snapshots), `BAE_MP_STATE` (optional: where `save` writes the stage; if it
/// exists at start, the client starts from it instead of the base).
///
/// It connects as the app does, and after a lost connection keeps editing
/// offline and reconnects (new transport, `SyncSession::reset`) every 50 ms
/// until the hub answers. Answers `ready`, then takes commands: `edit <count>
/// <ms between>` (answers `edited vv=…` when done), `stop` (`stopped vv=…`),
/// `state` (`fp= vv= connected=0|1 unsynced= made= offline= drops=
/// reconnects=`: edits made, those made while not connected, connections
/// lost, connections made after the first), `save` (`saved vv=…`) and `quit`
/// (flush, close the connection, `bye vv= close=ok|<error>`).
fn hub_client() {
    let url = env("BAE_MP_URL");
    let id = ClientId::new(&env("BAE_MP_ID")).unwrap();
    let tag = env("BAE_MP_TAG");
    let mut rng = Rng::new(env("BAE_MP_SEED").parse().unwrap());
    let saved = std::env::var_os("BAE_MP_STATE").map(PathBuf::from);
    let who = author(&format!("user:{tag}"));
    let mut stage: Stage = match &saved {
        Some(path) if path.is_file() => client_stage(&read_snapshots(path)),
        _ => client_stage(&read_snapshots(Path::new(&env("BAE_MP_BASE")))),
    };
    let mut session = SyncSession::new(SessionConfig::new(id));
    let (wake, input) = input();
    reply("ready", "");

    let mut link: Option<ThreadedTransport> = None;
    let mut pending = Some(connect(&url, &wake));
    let mut retry_at = Instant::now();
    let (mut made, mut offline, mut drops, mut reconnects, mut connected_once) =
        (0u64, 0u64, 0u64, 0u64, false);
    let (mut left, mut every, mut next_edit, mut n) = (0u32, Duration::ZERO, Instant::now(), 0);
    loop {
        let now = Instant::now();
        if link.is_none() {
            match pending.as_ref().map(Receiver::try_recv) {
                Some(Ok(Ok(t))) => {
                    link = Some(t);
                    session.reset();
                    reconnects += u64::from(connected_once);
                    connected_once = true;
                    pending = None;
                }
                Some(Ok(Err(_)) | Err(TryRecvError::Disconnected)) => {
                    pending = None;
                    retry_at = now + Duration::from_millis(50);
                }
                Some(Err(TryRecvError::Empty)) => {}
                None if now >= retry_at => pending = Some(connect(&url, &wake)),
                None => {}
            }
        }
        // Keep to the schedule where a wait lasts a whole timer tick (about
        // 16 ms on Windows): make the edits that are due at once.
        let mut burst = 0;
        while left > 0 && now >= next_edit && burst < 16 {
            random_edit(&mut stage, &who, &mut rng, &tag, &mut n);
            made += 1;
            offline += u64::from(link.is_none() || !session.is_connected());
            left -= 1;
            burst += 1;
            next_edit += every;
            if left == 0 {
                reply("edited", &format!("vv={}", versions(&stage)));
            }
        }
        if let Some(l) = link.as_mut()
            && session.pump(&mut stage, l).is_err()
        {
            // The hub went away or the connection was cut: edit offline,
            // reconnect.
            link = None;
            drops += 1;
        }
        let wait = if left > 0 {
            next_edit.saturating_duration_since(Instant::now())
        } else {
            Duration::from_millis(5)
        };
        let line = match input.recv_timeout(wait.min(Duration::from_millis(5))) {
            Ok(Input::Line(line)) => line,
            Ok(Input::Wake) | Err(RecvTimeoutError::Timeout) => continue,
            Ok(Input::Eof) | Err(RecvTimeoutError::Disconnected) => return,
        };
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["edit", count, ms] => {
                left = count.parse().unwrap();
                every = Duration::from_millis(ms.parse().unwrap());
                next_edit = Instant::now();
            }
            ["stop"] => {
                left = 0;
                reply("stopped", &format!("vv={}", versions(&stage)));
            }
            ["state"] => reply(
                "state",
                &format!(
                    "fp={} vv={} connected={} unsynced={} made={made} offline={offline} \
                     drops={drops} reconnects={reconnects}",
                    digest(&fingerprint(&stage)),
                    versions(&stage),
                    u8::from(link.is_some() && session.is_connected()),
                    session.unsynced(&stage).len(),
                ),
            ),
            ["save"] => {
                let layers: Vec<_> = stage
                    .sublayers()
                    .map(|s| (s.id().clone(), s.export_snapshot().unwrap()))
                    .collect();
                write_snapshots(saved.as_deref().expect("BAE_MP_STATE"), &layers);
                reply("saved", &format!("vv={}", versions(&stage)));
            }
            ["quit"] => {
                let result = match link.as_mut() {
                    Some(l) => session
                        .pump(&mut stage, l)
                        .and_then(|_| session.flush(&stage, l))
                        .and_then(|_| l.close()),
                    None => Err(SyncError::Closed),
                };
                reply(
                    "bye",
                    &format!(
                        "vv={} close={}",
                        versions(&stage),
                        result.map_or_else(|e| e.to_string().replace(' ', "_"), |_| "ok".into())
                    ),
                );
                return;
            }
            other => panic!("unknown command {other:?}"),
        }
    }
}

// ---- the hub process ----

/// The `bae-hub` binary, configured the way a NAS deployment is (command line
/// and environment, see `deploy/`).
struct Hub {
    child: Child,
    addr: SocketAddr,
}

impl Hub {
    /// Start a hub on `data` listening on `listen` (port 0: any free port; it
    /// logs the address it got). Tries again while the port or the data
    /// directory's lock is still held: after a kill the OS releases them with
    /// the dead process (on Windows a moment later).
    fn start(data: &Path, listen: SocketAddr) -> Hub {
        let deadline = Instant::now() + START;
        let mut tries = 0;
        loop {
            tries += 1;
            let mut child = Command::new(env!("CARGO_BIN_EXE_bae-hub"))
                .arg("serve")
                .arg("--data")
                .arg(data)
                .args(["--listen", &listen.to_string(), "--log", "info"])
                .env("BAE_HUB_TOKEN", WRITE)
                .env_remove("BAE_HUB_CONFIG")
                .env_remove("BAE_HUB_READ_TOKEN")
                .env_remove("BAE_HUB_ANONYMOUS")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .expect("start bae-hub");
            let stderr = child.stderr.take().expect("piped");
            let log = Arc::new(Mutex::new(Vec::new()));
            let (tx, rx) = mpsc::channel();
            let lines = log.clone();
            // Keeps reading after the address showed up, or the hub would block
            // on a full pipe.
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines() {
                    let Ok(line) = line else { break };
                    if line.contains("listening")
                        && let Some(addr) = line.split("addr=").nth(1)
                    {
                        let addr = addr.split_whitespace().next().unwrap_or_default();
                        let _ = tx.send(addr.parse::<SocketAddr>().ok());
                    }
                    lines.lock().unwrap().push(line);
                }
                let _ = tx.send(None);
            });
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(Some(addr)) => return Hub { child, addr },
                Ok(None) | Err(RecvTimeoutError::Disconnected) => {
                    let status = child.wait();
                    let log = log.lock().unwrap().join("\n");
                    assert!(
                        Instant::now() < deadline,
                        "bae-hub did not start after {tries} tries ({status:?}):\n{log}"
                    );
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(RecvTimeoutError::Timeout) => {
                    let _ = child.kill();
                    panic!("bae-hub did not start:\n{}", log.lock().unwrap().join("\n"));
                }
            }
        }
    }

    /// Kill it (no shutdown runs: whatever it had not written yet is lost).
    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.kill();
    }
}

// ---- a TCP proxy that cuts and delays ----

/// One proxied connection.
struct Conn {
    client: TcpStream,
    upstream: TcpStream,
    /// Cut after this (while faults are on).
    deadline: Instant,
    cut: AtomicBool,
}

impl Conn {
    fn cut(&self, cuts: &AtomicU64) {
        if !self.cut.swap(true, Ordering::AcqRel) {
            let _ = self.client.shutdown(Shutdown::Both);
            let _ = self.upstream.shutdown(Shutdown::Both);
            cuts.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Fault settings, shared with the proxy's threads.
struct Faults {
    on: AtomicBool,
    /// Connections live 50 ms plus up to this long.
    max_life_ms: u64,
    /// Each chunk waits up to this long, in both directions.
    max_delay_ms: u64,
    cuts: AtomicU64,
    stop: AtomicBool,
    conns: Mutex<Vec<Arc<Conn>>>,
}

/// Forwards `127.0.0.1:<port>` to the hub. While faults are on, every
/// connection is cut after a random time (forwarding a random part of the
/// chunk in hand first, so a frame can end half-way) and every chunk is
/// delayed by a random time.
struct Proxy {
    addr: SocketAddr,
    faults: Arc<Faults>,
    thread: Option<JoinHandle<()>>,
}

impl Proxy {
    fn start(upstream: SocketAddr, seed: u64, max_life_ms: u64, max_delay_ms: u64) -> Proxy {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let faults = Arc::new(Faults {
            on: AtomicBool::new(true),
            max_life_ms,
            max_delay_ms,
            cuts: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            conns: Mutex::new(Vec::new()),
        });
        let f = faults.clone();
        let thread = std::thread::spawn(move || {
            let mut rng = Rng::new(seed);
            while !f.stop.load(Ordering::Acquire) {
                let Ok((client, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                };
                // Accepted sockets inherit non-blocking mode on some systems.
                let _ = client.set_nonblocking(false);
                let _ = client.set_nodelay(true);
                let Ok(upstream) = TcpStream::connect_timeout(&upstream, Duration::from_secs(2))
                else {
                    continue;
                };
                let _ = upstream.set_nodelay(true);
                let life = 50 + rng.below(f.max_life_ms as usize) as u64;
                let conn = Arc::new(Conn {
                    client: client.try_clone().unwrap(),
                    upstream: upstream.try_clone().unwrap(),
                    deadline: Instant::now() + Duration::from_millis(life),
                    cut: AtomicBool::new(false),
                });
                {
                    let mut conns = f.conns.lock().unwrap();
                    // Finished connections (no pipe thread holds them) close
                    // their sockets.
                    conns.retain(|c| Arc::strong_count(c) > 1);
                    conns.push(conn.clone());
                }
                let pipes = [
                    (client, upstream.try_clone().unwrap()),
                    (upstream, conn.client.try_clone().unwrap()),
                ];
                for (from, to) in pipes {
                    let (f, conn, seed) = (f.clone(), conn.clone(), rng.next());
                    std::thread::spawn(move || pipe(from, to, &conn, &f, Rng::new(seed)));
                }
            }
        });
        Proxy {
            addr,
            faults,
            thread: Some(thread),
        }
    }

    /// No more cuts or delays.
    fn calm(&self) {
        self.faults.on.store(false, Ordering::Release);
    }

    fn cuts(&self) -> u64 {
        self.faults.cuts.load(Ordering::Relaxed)
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.faults.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        for c in self.faults.conns.lock().unwrap().drain(..) {
            c.cut(&self.faults.cuts);
        }
    }
}

/// Copy `from` to `to` until either side closes or the connection is cut.
fn pipe(mut from: TcpStream, mut to: TcpStream, conn: &Conn, f: &Faults, mut rng: Rng) {
    let _ = from.set_read_timeout(Some(Duration::from_millis(10)));
    let mut buf = vec![0u8; 64 * 1024];
    let due = |f: &Faults| f.on.load(Ordering::Acquire) && Instant::now() >= conn.deadline;
    while !conn.cut.load(Ordering::Acquire) {
        if due(f) {
            conn.cut(&f.cuts);
            break;
        }
        let n = match from.read(&mut buf) {
            Ok(0) => {
                // Pass the end of the stream on (the close handshake).
                let _ = to.shutdown(Shutdown::Write);
                break;
            }
            Ok(n) => n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => {
                conn.cut(&f.cuts);
                break;
            }
        };
        if f.on.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(
                rng.below(f.max_delay_ms as usize + 1) as u64,
            ));
        }
        if due(f) {
            let _ = to.write_all(&buf[..rng.below(n)]);
            conn.cut(&f.cuts);
            break;
        }
        if to.write_all(&buf[..n]).is_err() {
            conn.cut(&f.cuts);
            break;
        }
    }
}

// ---- the parent ----

/// One client process as the parent sees it.
struct Client {
    id: String,
    proc: Proc,
}

/// What a client reported.
#[derive(Debug, Clone)]
struct State {
    fp: String,
    vv: String,
    connected: bool,
    unsynced: u64,
    made: u64,
    offline: u64,
    drops: u64,
    reconnects: u64,
}

/// Where the parent keeps the base project and saved client states.
struct Setup {
    url: String,
    base: PathBuf,
    dir: PathBuf,
}

impl Setup {
    fn new(dir: &Path, addr: SocketAddr) -> Setup {
        let base = dir.join("base.snapshots");
        write_snapshots(&base, &base_snapshots());
        Setup {
            url: common::url(addr, PROJECT),
            base,
            dir: dir.to_owned(),
        }
    }

    /// Where client `id` saves its stage.
    fn state_file(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.snapshots"))
    }

    /// Start client `id` (layers it creates are named after `tag`), saving to
    /// and starting from its state file if `saved`.
    fn start(&self, id: &str, tag: &str, seed: u64, saved: bool) -> Client {
        let mut vars = vec![
            ("BAE_MP_URL", self.url.clone()),
            ("BAE_MP_ID", id.to_owned()),
            ("BAE_MP_TAG", tag.to_owned()),
            ("BAE_MP_SEED", seed.to_string()),
            ("BAE_MP_BASE", self.base.display().to_string()),
        ];
        if saved {
            vars.push(("BAE_MP_STATE", self.state_file(id).display().to_string()));
        }
        let mut proc = Proc::spawn(ENTRY, tag, "hub", &vars);
        proc.wait_for("ready", START);
        Client {
            id: id.to_owned(),
            proc,
        }
    }
}

impl Client {
    fn state(&mut self) -> State {
        let s = self.proc.ask("state", "state", ANSWER);
        State {
            fp: field(&s, "fp").to_owned(),
            vv: field(&s, "vv").to_owned(),
            connected: field(&s, "connected") == "1",
            unsynced: field_u64(&s, "unsynced"),
            made: field_u64(&s, "made"),
            offline: field_u64(&s, "offline"),
            drops: field_u64(&s, "drops"),
            reconnects: field_u64(&s, "reconnects"),
        }
    }

    /// Tell it to quit cleanly; its final versions.
    fn quit(mut self) -> String {
        let bye = self.proc.ask("quit", "bye", ANSWER);
        assert_eq!(field(&bye, "close"), "ok", "{}: {bye}", self.id);
        self.proc.wait_exit(ANSWER);
        field(&bye, "vv").to_owned()
    }
}

fn wait_all(clients: &mut [Client], what: &str, mut f: impl FnMut(&State) -> bool) {
    wait_until(what, SETTLE, || clients.iter_mut().all(|c| f(&c.state())));
}

/// Wait until every client is connected and has the same documents as the
/// others, with nothing left to send; returns the common state.
fn settle(clients: &mut [Client]) -> State {
    let deadline = Instant::now() + SETTLE;
    loop {
        let states: Vec<State> = clients.iter_mut().map(Client::state).collect();
        let first = &states[0];
        if states.iter().all(|s| {
            s.connected && s.unsynced == 0 && s.fp == first.fp && same_versions(&s.vv, &first.vv)
        }) {
            return first.clone();
        }
        assert!(
            Instant::now() < deadline,
            "no convergence within {SETTLE:?}: {:?}",
            states
                .iter()
                .map(|s| (&s.fp, s.connected, s.unsynced))
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Wait until `reader` has what `state` says its writer had: by then it went
/// through the hub.
fn wait_through(reader: &mut Client, state: &State) {
    let want = parse_versions(&state.vv);
    wait_until("the writes to go through the hub", SETTLE, || {
        includes(&parse_versions(&reader.state().vv), &want)
    });
}

/// The end of a run: the clients stopped editing (`finals`: their versions
/// then). They settle, a newcomer gets the same project, everyone quits
/// cleanly, and nothing in `finals` or `kept` is lost. Returns the clients'
/// states once settled.
fn finish(
    setup: &Setup,
    mut clients: Vec<Client>,
    kept: &[String],
    finals: &[String],
) -> Vec<State> {
    let common = settle(&mut clients);
    let mut fresh = setup.start("newcomer", "newcomer", 1, false);
    wait_until("the newcomer to catch up", SETTLE, || {
        let s = fresh.state();
        s.connected && s.fp == common.fp && same_versions(&s.vv, &common.vv)
    });
    clients.push(fresh);
    let states: Vec<State> = clients.iter_mut().map(Client::state).collect();
    let mut last = String::new();
    for c in clients {
        last = c.quit();
    }
    let have = parse_versions(&last);
    for vv in kept.iter().chain(finals) {
        assert!(includes(&have, &parse_versions(vv)), "lost edits: {vv}");
    }
    states
}

fn edited(clients: &mut [Client]) -> Vec<String> {
    clients
        .iter_mut()
        .map(|c| field(&c.proc.wait_for("edited", SETTLE), "vv").to_owned())
        .collect()
}

fn loopback_any_port() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 0))
}

/// Three client processes edit through the hub process and agree, with each
/// other and with a newcomer.
#[test]
fn client_processes_converge_through_the_hub_process() {
    let tmp = tempfile::tempdir().unwrap();
    let hub = Hub::start(&tmp.path().join("data"), loopback_any_port());
    let setup = Setup::new(tmp.path(), hub.addr);
    let mut clients: Vec<Client> = ["alice", "bob", "carol"]
        .iter()
        .zip(1..)
        .map(|(id, seed)| setup.start(id, id, seed, false))
        .collect();
    for c in &mut clients {
        c.proc.send("edit 150 4");
    }
    let finals = edited(&mut clients);
    let states = finish(&setup, clients, &[], &finals);
    assert!(states.iter().all(|s| s.drops == 0), "{states:?}");
    drop(hub);
}

/// The hub process is killed while three clients edit. They notice, go on
/// editing offline, and once a new hub process runs on the same data directory
/// and port they reconnect and converge: whatever the killed hub had not
/// written is uploaded again, nothing any client made is lost.
#[test]
fn the_hub_process_is_killed_and_restarted_while_clients_edit() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let mut hub = Hub::start(&data, loopback_any_port());
    let addr = hub.addr;
    let setup = Setup::new(tmp.path(), addr);
    let mut clients: Vec<Client> = ["alice", "bob", "carol"]
        .iter()
        .zip(1..)
        .map(|(id, seed)| setup.start(id, id, seed, false))
        .collect();
    wait_all(&mut clients, "the clients to connect", |s| s.connected);
    for c in &mut clients {
        c.proc.send("edit 400 4");
    }
    wait_all(&mut clients, "edits through the hub", |s| s.made >= 40);

    hub.kill();
    wait_all(&mut clients, "the clients to notice", |s| !s.connected);
    wait_all(&mut clients, "offline edits", |s| s.offline >= 20);
    let hub = Hub::start(&data, addr);

    let finals = edited(&mut clients);
    let states = finish(&setup, clients, &[], &finals);
    for s in &states[..3] {
        assert!(
            s.drops >= 1 && s.reconnects >= 1 && s.offline >= 20,
            "{s:?}"
        );
    }
    println!(
        "hub killed and restarted: {:?}",
        states[..3]
            .iter()
            .map(|s| (s.made, s.offline, s.reconnects))
            .collect::<Vec<_>>()
    );
    drop(hub);
}

/// Client processes are killed while they edit and started again with the
/// same `ClientId`: alice from the state she saved before (edits made after
/// the save and not yet through the hub die with the process), bob from
/// scratch (the base project). Both catch up and go on; nothing that had gone
/// through the hub, and nothing alice had saved, is lost.
#[test]
fn killed_client_processes_restart_from_saved_state_or_from_scratch() {
    let tmp = tempfile::tempdir().unwrap();
    let hub = Hub::start(&tmp.path().join("data"), loopback_any_port());
    let setup = Setup::new(tmp.path(), hub.addr);
    let mut clients = vec![
        setup.start("alice", "alice", 1, true),
        setup.start("bob", "bob", 2, false),
        setup.start("carol", "carol", 3, false),
    ];
    // Bob is still editing when he is killed, however slow the machine.
    for (c, edits) in clients
        .iter_mut()
        .zip(["edit 500 4", "edit 1000000 4", "edit 500 4"])
    {
        c.proc.send(edits);
    }
    let mut kept = Vec::new();

    wait_until("alice to edit", SETTLE, || clients[0].state().made >= 40);
    let saved = clients[0].proc.ask("save", "saved", ANSWER);
    kept.push(field(&saved, "vv").to_owned());
    wait_until("alice to edit on", SETTLE, || clients[0].state().made >= 80);
    let before = clients[0].state();
    wait_through(&mut clients[2], &before);
    kept.push(before.vv);
    clients[0].proc.kill_and_wait();
    clients[0] = setup.start("alice", "alice2", 11, true);
    clients[0].proc.send("edit 200 4");

    wait_until("bob to edit", SETTLE, || clients[1].state().made >= 80);
    let before = clients[1].state();
    wait_through(&mut clients[2], &before);
    kept.push(before.vv);
    clients[1].proc.kill_and_wait();
    clients[1] = setup.start("bob", "bob2", 12, false);
    clients[1].proc.send("edit 200 4");

    let finals = edited(&mut clients);
    finish(&setup, clients, &kept, &finals);
    drop(hub);
}

/// Clients reach the hub through a proxy that cuts every connection after
/// 50–500 ms (sometimes half-way through a frame) and delays every chunk by up
/// to 15 ms in both directions. They reconnect again and again; once the
/// faults stop they converge and nothing is lost.
#[test]
fn clients_reconnect_through_a_proxy_that_cuts_and_delays() {
    let tmp = tempfile::tempdir().unwrap();
    let hub = Hub::start(&tmp.path().join("data"), loopback_any_port());
    let proxy = Proxy::start(hub.addr, 5, 450, 15);
    let setup = Setup::new(tmp.path(), proxy.addr);
    let mut clients: Vec<Client> = ["alice", "bob", "carol"]
        .iter()
        .zip(1..)
        .map(|(id, seed)| setup.start(id, id, seed, false))
        .collect();
    for c in &mut clients {
        c.proc.send("edit 400 4");
    }
    let finals = edited(&mut clients);
    proxy.calm();
    let states = finish(&setup, clients, &[], &finals);
    println!(
        "{} connections cut; reconnects {:?}",
        proxy.cuts(),
        states[..3].iter().map(|s| s.reconnects).collect::<Vec<_>>()
    );
    assert!(proxy.cuts() >= 6, "{} cuts", proxy.cuts());
    for s in &states[..3] {
        assert!(s.reconnects >= 1, "{s:?}");
    }
    drop(hub);
}

/// Soak: three clients edit without pause through a faulty proxy while, every
/// 0.3–3 s, the hub is killed and restarted, a client is killed and restarted
/// (from its saved state or from scratch), or the faults are switched on or
/// off, for `BAE_MP_SOAK_SECS` seconds (default 60). Run with
///
/// ```sh
/// BAE_MP_SOAK_SECS=600 cargo test -p bae-hub --test multiprocess -- --ignored --nocapture
/// ```
#[test]
#[ignore = "soak test (a minute or more): run with --ignored, see its doc comment"]
fn soak_hub_and_client_kills_with_a_faulty_network() {
    const EDITS: &str = "edit 4000000000 5";
    let secs = std::env::var("BAE_MP_SOAK_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let mut hub = Hub::start(&data, loopback_any_port());
    let addr = hub.addr;
    let proxy = Proxy::start(addr, secs, 2000, 10);
    let setup = Setup::new(tmp.path(), proxy.addr);
    let ids = ["alice", "bob", "carol"];
    let mut clients: Vec<Client> = ids
        .iter()
        .zip(1..)
        .map(|(id, seed)| setup.start(id, id, seed, true))
        .collect();
    for c in &mut clients {
        c.proc.send(EDITS);
    }
    let mut rng = Rng::new(secs);
    let mut kept = Vec::new();
    let (mut hub_kills, mut client_kills, mut generation) = (0, 0, 100);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(Duration::from_millis(300 + rng.below(2700) as u64));
        match rng.below(3) {
            0 => {
                hub.kill();
                std::thread::sleep(Duration::from_millis(rng.below(1000) as u64));
                hub = Hub::start(&data, addr);
                hub_kills += 1;
            }
            1 => {
                let i = rng.below(clients.len());
                generation += 1;
                if rng.chance(50) {
                    let saved = clients[i].proc.ask("save", "saved", ANSWER);
                    kept.push(field(&saved, "vv").to_owned());
                }
                clients[i].proc.kill_and_wait();
                let tag = format!("{}{generation}", ids[i]);
                clients[i] = setup.start(ids[i], &tag, generation, true);
                clients[i].proc.send(EDITS);
                client_kills += 1;
            }
            _ => proxy.faults.on.store(rng.chance(60), Ordering::Release),
        }
    }
    let finals: Vec<String> = clients
        .iter_mut()
        .map(|c| field(&c.proc.ask("stop", "stopped", ANSWER), "vv").to_owned())
        .collect();
    proxy.calm();
    finish(&setup, clients, &kept, &finals);
    println!(
        "{hub_kills} hub kills, {client_kills} client kills, {} connections cut in {secs} s",
        proxy.cuts()
    );
}
