//! The connection manager (`Connection`, spec §7) with shared-folder targets,
//! and hub targets that never answer, go silent or hang up (a fake hub).
//! Hub targets against the real hub are tested in
//! `services/bae-hub/tests/connection.rs`.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bae_model::project::Project;
use bae_model::{AuthorId, Op, PrimType, Specifier, Stage, SublayerId, Value};
use bae_sync::{
    Access, Backoff, ClientId, Connection, ConnectionConfig, ConnectionOutcome, ConnectionState,
    DirConfig, Failure, PresenceState, Target,
};
use common::*;

/// A project directory with the base project saved in it.
fn project_dir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Film.bae");
    Project::from_stage(client_stage(&base_snapshots()))
        .save_as(&dir)
        .unwrap();
    (tmp, dir)
}

fn config(id: &str) -> ConnectionConfig {
    let mut c = ConnectionConfig::new(ClientId::new(id).unwrap(), Arc::new(|| {}));
    c.folder = DirConfig {
        poll_interval: Duration::from_millis(20),
        watch: false,
        ..DirConfig::default()
    };
    c.backoff = Backoff {
        first: Duration::from_millis(100),
        max: Duration::from_secs(2),
        jitter: 0.0,
        ..Backoff::default()
    };
    c
}

fn state(name: &str) -> PresenceState {
    PresenceState {
        name: name.to_owned(),
        ..PresenceState::default()
    }
}

/// An app instance: a stage and its connection.
struct App {
    stage: Stage,
    conn: Connection,
    who: AuthorId,
}

impl App {
    fn join(dir: &Path, config: ConnectionConfig) -> App {
        let who = author(&format!("user:{}", config.client));
        App {
            stage: Project::open(dir).unwrap().stage,
            conn: Connection::start(Target::Folder(dir.to_owned()), config),
            who,
        }
    }

    fn pump(&mut self) -> ConnectionOutcome {
        let out = self.conn.pump(&mut self.stage);
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        out
    }

    fn set_opacity(&mut self, value: f64) {
        self.stage.set_edit_target(&SublayerId::root()).unwrap();
        self.stage
            .transact(&self.who, "edit", |tx| {
                tx.apply(Op::SetValue {
                    path: p("/comps/main/layers/title"),
                    attr: "opacity".into(),
                    value: Some(Value::Float(value)),
                })
            })
            .unwrap();
    }

    fn opacity(&self) -> Option<Value> {
        self.stage
            .prim(&p("/comps/main/layers/title"))
            .and_then(|m| m.value("opacity").cloned())
    }

    fn create_layer(&mut self, name: &str) {
        self.stage.set_edit_target(&sl("anim/a")).unwrap();
        self.stage
            .transact(&self.who, "edit", |tx| {
                tx.apply(Op::CreatePrim {
                    path: p(&format!("{LAYERS}/{name}")),
                    specifier: Specifier::Def,
                    prim_type: Some(PrimType::new("Layer")),
                    index: None,
                })
            })
            .unwrap();
    }

    fn has_layer(&self, name: &str) -> bool {
        self.stage.prim(&p(&format!("{LAYERS}/{name}"))).is_some()
    }
}

fn wait(what: &str, timeout: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

const LONG: Duration = Duration::from_secs(30);

#[test]
fn two_apps_converge_through_a_shared_folder() {
    let (_tmp, dir) = project_dir();
    let mut a = App::join(&dir, config("alice"));
    let mut b = App::join(&dir, config("bob"));
    a.conn.set_presence(state("alice"));
    b.conn.set_presence(state("bob"));
    let mut events = 0;
    wait("both online", LONG, || {
        a.pump();
        b.pump();
        a.conn.is_online() && b.conn.is_online()
    });
    for app in [&a, &b] {
        assert_eq!(app.conn.access(), Access::Write);
        assert!(matches!(
            app.conn.status().state,
            ConnectionState::Online { .. }
        ));
    }

    a.set_opacity(7.0);
    b.create_layer("from_bob");
    wait("convergence", LONG, || {
        a.pump();
        events += b.pump().events.len();
        let (sa, sb) = (a.conn.status(), b.conn.status());
        fingerprint(&a.stage) == fingerprint(&b.stage)
            && !sa.unsynced
            && !sb.unsynced
            && sa.peers == 1
            && sb.peers == 1
    });
    assert_eq!(b.opacity(), Some(Value::Float(7.0)));
    assert!(a.has_layer("from_bob"));
    assert!(events > 0, "remote imports are reported");
    assert_eq!(a.conn.presence()[0].state.name, "bob");

    // Nothing changes: the status generation stays put (no redraws).
    let (ga, gb) = (a.conn.generation(), b.conn.generation());
    for _ in 0..20 {
        assert!(!a.pump().status_changed);
        assert!(!b.pump().status_changed);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!((a.conn.generation(), b.conn.generation()), (ga, gb));
}

#[test]
fn closing_writes_the_last_edits_to_the_folder() {
    let (_tmp, dir) = project_dir();
    let mut a = App::join(&dir, config("alice"));
    let mut b = App::join(&dir, config("bob"));
    a.conn.set_presence(state("alice"));
    wait("both online", LONG, || {
        a.pump();
        b.pump();
        a.conn.is_online() && b.conn.status().peers == 1
    });
    // The second edit would wait for `min_send_interval` (300 ms) in a pump.
    a.set_opacity(1.0);
    a.pump();
    a.set_opacity(2.0);
    let started = Instant::now();
    let mut closing = a.conn.close(&mut a.stage);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(closing.unsynced().is_empty(), "{:?}", closing.unsynced());
    let result = closing.wait(LONG).expect("closed in time");
    assert!(result.is_ok(), "{result:?}");
    wait("the last edit and the leave", LONG, || {
        b.pump();
        b.opacity() == Some(Value::Float(2.0)) && b.conn.presence().is_empty()
    });
}

#[test]
fn a_client_id_in_use_needs_attention_and_no_retry_loop() {
    let (_tmp, dir) = project_dir();
    let mut a = App::join(&dir, config("same"));
    wait("first online", LONG, || {
        a.pump();
        a.conn.is_online()
    });
    let mut cfg = config("same");
    cfg.backoff.failed_retry = None;
    let mut b = App::join(&dir, cfg);
    wait("the second one fails", LONG, || {
        b.pump();
        matches!(
            b.conn.status().state,
            ConnectionState::Failed {
                failure: Failure::ClientIdInUse,
                ..
            }
        )
    });
    // A few quick retries first (it might have been our own link letting
    // go), then none on their own.
    let attempts = b.conn.connect_attempts();
    assert!((2..10).contains(&attempts), "{attempts}");
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(300) {
        b.pump();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        b.conn.connect_attempts(),
        attempts,
        "no retries on their own"
    );

    // The other session quits; an explicit retry gets through.
    let result = a.conn.close(&mut a.stage).wait(LONG).map(|r| r.is_ok());
    assert_eq!(result, Some(true));
    b.conn.retry();
    assert_eq!(b.conn.connect_attempts(), attempts + 1);
    wait("online after retry", LONG, || {
        b.pump();
        b.conn.is_online()
    });
}

/// Sharing the same folder again right after dropping or closing the
/// connection: the old link may still hold the folder's lock for our client
/// ID on its own thread for a moment. That is no conflict.
#[test]
fn sharing_the_same_folder_again_at_once_is_no_conflict() {
    let (_tmp, dir) = project_dir();
    let mut a = App::join(&dir, config("alice"));
    let online = |a: &mut App| {
        wait("online", LONG, || {
            a.pump();
            assert!(
                !matches!(a.conn.status().state, ConnectionState::Failed { .. }),
                "{:?}",
                a.conn.status()
            );
            a.conn.is_online()
        })
    };
    online(&mut a);
    for round in 0..6 {
        let old = std::mem::replace(
            &mut a.conn,
            Connection::start(Target::Folder(dir.clone()), config("alice")),
        );
        if round % 2 == 0 {
            drop(old);
        } else {
            let _closing = old.close(&mut a.stage);
        }
        online(&mut a);
    }
}

/// First wait the connection chose after each of the next `n` failed
/// attempts (the countdown as first seen, a little under the real wait).
fn observe_backoff(app: &mut App, n: usize) -> Vec<Duration> {
    let mut seen: Vec<(u64, Duration)> = Vec::new();
    wait("failed attempts", LONG, || {
        app.pump();
        if let ConnectionState::Offline { retry_in, .. } = app.conn.status().state {
            let attempt = app.conn.connect_attempts();
            if seen.last().is_none_or(|(a, _)| *a != attempt) {
                seen.push((attempt, retry_in));
            }
        }
        seen.len() > n
    });
    // The first one may have been seen late (it started before we looked).
    seen.into_iter().skip(1).map(|(_, d)| d).collect()
}

#[test]
fn a_folder_that_is_not_there_yet_is_retried_with_backoff() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Film.bae");
    let base = base_snapshots();
    let mut a = App {
        stage: client_stage(&base),
        conn: Connection::start(Target::Folder(dir.clone()), config("alice")),
        who: author("user:alice"),
    };
    let delays = observe_backoff(&mut a, 3);
    for pair in delays.windows(2) {
        assert!(pair[1] * 2 >= pair[0] * 3, "{delays:?}");
    }
    assert!(
        delays.iter().all(|d| *d <= Duration::from_secs(2)),
        "{delays:?}"
    );
    match a.conn.status().state {
        ConnectionState::Offline { error, .. } => {
            assert!(error.contains("not a project directory"), "{error}")
        }
        other => panic!("{other:?}"),
    }
    a.set_opacity(5.0);
    a.pump();
    assert!(a.conn.status().unsynced);

    // The share comes back: the connection joins it and writes what it has.
    Project::from_stage(client_stage(&base))
        .save_as(&dir)
        .unwrap();
    a.conn.retry();
    wait("online and synced", LONG, || {
        a.pump();
        a.conn.is_online() && !a.conn.status().unsynced
    });
    let mut b = App::join(&dir, config("bob"));
    wait("the other app reads it", LONG, || {
        b.pump();
        b.opacity() == Some(Value::Float(5.0))
    });
}

/// An idle app pumps only when woken. The alarm wakes it when a pump is due
/// on its own: a retry while the folder is not there, and an edit held back
/// by `min_send_interval` (the end of a drag).
#[test]
fn an_app_that_pumps_only_when_woken_retries_and_sends_held_back_edits() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Film.bae");
    let base = base_snapshots();
    let woken = Arc::new(AtomicUsize::new(0));
    let w = woken.clone();
    let mut cfg = config("alice");
    cfg.wake = Arc::new(move || {
        w.fetch_add(1, Ordering::SeqCst);
    });
    let mut a = App {
        stage: client_stage(&base),
        conn: Connection::start(Target::Folder(dir.clone()), cfg),
        who: author("user:alice"),
    };
    let pump_if_woken = |a: &mut App| {
        if woken.swap(0, Ordering::SeqCst) > 0 {
            a.pump();
        }
    };
    wait("retries while the folder is not there", LONG, || {
        pump_if_woken(&mut a);
        a.conn.connect_attempts() >= 3
    });
    Project::from_stage(client_stage(&base))
        .save_as(&dir)
        .unwrap();
    wait("online once it is there", LONG, || {
        pump_if_woken(&mut a);
        a.conn.is_online()
    });

    let mut b = App::join(&dir, config("bob"));
    wait("bob online", LONG, || {
        b.pump();
        b.conn.is_online()
    });
    // A drag: the last step is held back by `min_send_interval`.
    for i in 1..=5 {
        a.set_opacity(f64::from(i));
        a.pump();
    }
    wait("the last step of the drag", LONG, || {
        pump_if_woken(&mut a);
        b.pump();
        b.opacity() == Some(Value::Float(5.0))
    });
}

#[test]
fn wake_is_called_when_a_connect_attempt_finishes() {
    let (_tmp, dir) = project_dir();
    let woken = Arc::new(AtomicUsize::new(0));
    let w = woken.clone();
    let mut cfg = config("alice");
    cfg.wake = Arc::new(move || {
        w.fetch_add(1, Ordering::SeqCst);
    });
    let mut a = App::join(&dir, cfg);
    wait("a wake", LONG, || woken.load(Ordering::SeqCst) > 0);
    let before = a.conn.generation();
    // Pump only when woken, as the app does when idle.
    wait("online", LONG, || {
        if woken.swap(0, Ordering::SeqCst) > 0 {
            a.pump();
        }
        a.conn.is_online()
    });
    assert!(a.conn.generation() > before);
}

/// A shared folder this user may not write: read access, nothing written.
#[cfg(unix)]
#[test]
fn a_folder_we_cannot_write_is_joined_read_only() {
    use std::os::unix::fs::PermissionsExt;

    struct Restore(PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }

    let (_tmp, dir) = project_dir();
    let mut w = App::join(&dir, config("writer"));
    wait("writer online", LONG, || {
        w.pump();
        w.conn.is_online()
    });
    w.set_opacity(3.0);
    let r = w.conn.close(&mut w.stage).wait(LONG).map(|r| r.is_ok());
    assert_eq!(r, Some(true));

    let updates = dir.join("updates");
    std::fs::set_permissions(&updates, std::fs::Permissions::from_mode(0o555)).unwrap();
    let _restore = Restore(updates.clone());
    let probe = updates.join("probe");
    if std::fs::write(&probe, b"").is_ok() {
        // Permissions do not apply (running as root): nothing to test here.
        let _ = std::fs::remove_file(&probe);
        return;
    }

    let mut reader = App::join(&dir, config("reader"));
    reader.conn.set_presence(state("reader"));
    wait("reader online with the edit", LONG, || {
        reader.pump();
        reader.conn.is_online() && reader.opacity() == Some(Value::Float(3.0))
    });
    assert_eq!(reader.conn.access(), Access::Read);
    assert_eq!(reader.conn.status().access, Access::Read);
    assert!(reader.conn.session().config().read_only);

    reader.set_opacity(9.0);
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(500) {
        reader.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(reader.conn.status().unsynced, "the edit is reported unsent");
    assert_eq!(reader.conn.session().stats().bytes_sent, 0);
    assert!(!updates.join("root/reader.log").exists());
    assert!(!dir.join("presence/reader.presence").exists());
}

/// Connecting to a hub that never answers (the handshake hangs until the
/// timeout) never holds up the thread that pumps, nor closing.
#[cfg(feature = "hub")]
#[test]
fn pump_never_waits_for_a_hanging_connect() {
    // Accepted by the OS (backlog), never by anyone: the handshake hangs.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/projects/film", listener.local_addr().unwrap());
    let mut cfg = config("waiting");
    cfg.hub_timeout = Duration::from_secs(5);
    let mut conn = Connection::start(Target::Hub { url, token: None }, cfg);
    let mut stage = client_stage(&base_snapshots());
    let started = Instant::now();
    let mut slowest = Duration::ZERO;
    while started.elapsed() < Duration::from_millis(500) {
        let t = Instant::now();
        conn.pump(&mut stage);
        slowest = slowest.max(t.elapsed());
        assert_eq!(conn.status().state, ConnectionState::Connecting);
        std::thread::sleep(Duration::from_millis(5));
    }
    eprintln!("slowest pump while the connect hangs: {slowest:?}");
    assert!(slowest < Duration::from_millis(250), "{slowest:?}");
    assert_eq!(conn.connect_attempts(), 1);

    let t = Instant::now();
    let mut closing = conn.close(&mut stage);
    assert!(
        t.elapsed() < Duration::from_millis(250),
        "{:?}",
        t.elapsed()
    );
    assert!(matches!(closing.try_result(), Some(Ok(()))));
    // Never connected: every document counts as not sent.
    assert!(!closing.unsynced().is_empty());
    drop(listener);
}

/// A hub stand-in for links that fail in ways a real hub does not show on
/// demand: it accepts one client, says it has the same base project, takes
/// the client's `Hello`, then reads and sends nothing more (a hub that lost
/// power, a dropped VPN) until [`FakeHub::hang_up`] drops the connection.
#[cfg(feature = "hub")]
struct FakeHub {
    url: String,
    ready: std::sync::mpsc::Receiver<()>,
    hang_up: Option<std::sync::mpsc::Sender<()>>,
}

#[cfg(feature = "hub")]
impl FakeHub {
    fn start() -> FakeHub {
        use bae_sync::{HubLimits, HubTransport, SessionConfig, SyncSession};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}/projects/film", listener.local_addr().unwrap());
        let (ready_tx, ready) = std::sync::mpsc::channel();
        let (hang_up, hung_up) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut link =
                HubTransport::accept_granting(stream, None, HubLimits::default(), Access::Write)
                    .unwrap();
            let mut stage = client_stage(&base_snapshots());
            let mut config = SessionConfig::new(ClientId::new("hub").unwrap());
            config.hello_interval = None;
            let mut session = SyncSession::new(config);
            let deadline = Instant::now() + LONG;
            while !session.is_connected() {
                assert!(Instant::now() < deadline, "the client never said hello");
                session.pump(&mut stage, &mut link).unwrap();
                std::thread::sleep(Duration::from_millis(2));
            }
            let _ = ready_tx.send(());
            // Silent; later connect attempts hang in the listen backlog.
            let _ = hung_up.recv();
            drop(link);
            let _ = hung_up.recv();
            drop(listener);
        });
        FakeHub {
            url,
            ready,
            hang_up: Some(hang_up),
        }
    }

    /// Wait until the hub took the client's `Hello`: from now on it reads
    /// nothing.
    fn wait_ready(&self) {
        self.ready
            .recv_timeout(LONG)
            .expect("the hub took our Hello");
    }

    /// Drop the connection without reading what the client sent since.
    fn hang_up(&self) {
        let _ = self.hang_up.as_ref().unwrap().send(());
    }

    fn join(&self, id: &str, config: ConnectionConfig) -> App {
        App {
            stage: client_stage(&base_snapshots()),
            conn: Connection::start(
                Target::Hub {
                    url: self.url.clone(),
                    token: None,
                },
                config,
            ),
            who: author(&format!("user:{id}")),
        }
    }
}

/// The first `Offline` error while pumping.
#[cfg(feature = "hub")]
fn wait_offline(app: &mut App) -> String {
    let mut error = None;
    wait("offline", LONG, || {
        app.pump();
        if let ConnectionState::Offline { error: e, .. } = app.conn.status().state {
            error = Some(e);
        }
        error.is_some()
    });
    error.unwrap()
}

/// A hub that goes silent without closing the connection (power loss, a
/// dropped VPN: no reset arrives, TCP takes many minutes to notice): the
/// connection is lost after `hub_idle_timeout`, and the edit sent into the
/// silence counts as unsynced.
#[cfg(feature = "hub")]
#[test]
fn a_hub_that_goes_silent_is_lost() {
    let hub = FakeHub::start();
    let mut cfg = config("alice");
    cfg.hub_idle_timeout = Some(Duration::from_millis(500));
    let mut a = hub.join("alice", cfg);
    wait("online", LONG, || {
        a.pump();
        a.conn.is_online()
    });
    hub.wait_ready();
    assert!(!a.conn.status().unsynced);
    a.set_opacity(3.0);
    a.pump();
    let error = wait_offline(&mut a);
    assert!(error.contains("nothing received"), "{error}");
    assert!(a.conn.status().unsynced);
    let closing = a.conn.close(&mut a.stage);
    let root = bae_sync::DocKey::sublayer(&SublayerId::root());
    assert!(
        closing.unsynced().contains(&root),
        "{:?}",
        closing.unsynced()
    );
}

/// An edit handed to the link just before the hub went away (a crash, a
/// reset connection) never arrived: offline, it counts as unsynced, and
/// closing reports it, so the app warns before quitting. Also when the app
/// closes before a pump noticed the loss.
#[cfg(feature = "hub")]
#[test]
fn edits_a_lost_link_did_not_confirm_stay_unsynced() {
    let root = bae_sync::DocKey::sublayer(&SublayerId::root());
    for noticed in [true, false] {
        let hub = FakeHub::start();
        let woken = Arc::new(AtomicUsize::new(0));
        let w = woken.clone();
        let mut cfg = config("alice");
        cfg.wake = Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        });
        let mut a = hub.join("alice", cfg);
        wait("online", LONG, || {
            a.pump();
            a.conn.is_online()
        });
        hub.wait_ready();
        a.set_opacity(4.0);
        a.pump();
        assert!(!a.conn.status().unsynced, "handed to the link");
        let before = woken.load(Ordering::SeqCst);
        hub.hang_up();
        if noticed {
            wait_offline(&mut a);
            assert!(a.conn.status().unsynced);
        } else {
            // The link's I/O thread saw it (and woke us), no pump did yet.
            wait("the link noticed", LONG, || {
                woken.load(Ordering::SeqCst) > before
            });
        }
        let mut closing = a.conn.close(&mut a.stage);
        assert!(
            closing.unsynced().contains(&root),
            "noticed={noticed}: {:?}",
            closing.unsynced()
        );
        if !noticed {
            assert!(matches!(closing.wait(LONG), Some(Err(_))));
        }
    }
}

/// A silent hub is reported once: the link's I/O thread gives up instead of
/// reporting it again and waking the app on every tick until the next pump
/// (an agent busy with a long job, an app that only pumps when woken).
#[cfg(feature = "hub")]
#[test]
fn a_silent_hub_wakes_the_app_once() {
    let hub = FakeHub::start();
    let woken = Arc::new(AtomicUsize::new(0));
    let w = woken.clone();
    let mut cfg = config("alice");
    cfg.hub_idle_timeout = Some(Duration::from_millis(300));
    cfg.wake = Arc::new(move || {
        w.fetch_add(1, Ordering::SeqCst);
    });
    let mut a = hub.join("alice", cfg);
    wait("online", LONG, || {
        a.pump();
        a.conn.is_online()
    });
    hub.wait_ready();
    a.pump();
    let before = woken.load(Ordering::SeqCst);
    // No pump until well past the idle timeout (the next keep-alive is 5 s
    // away).
    std::thread::sleep(Duration::from_millis(1300));
    let wakes = woken.load(Ordering::SeqCst) - before;
    assert!((1..=4).contains(&wakes), "{wakes} wakes");
    let error = wait_offline(&mut a);
    assert!(error.contains("nothing received"), "{error}");
}
