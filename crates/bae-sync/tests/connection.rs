//! The connection manager (`Connection`, spec §7) with shared-folder targets,
//! and a hub target that never answers. Hub targets against the real hub are
//! tested in `services/bae-hub/tests/connection.rs`.

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
