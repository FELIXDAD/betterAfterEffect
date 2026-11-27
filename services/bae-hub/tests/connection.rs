//! The app's connection manager (`bae_sync::Connection`, spec §7) against a
//! running hub: share and converge, the hub going away and coming back on the
//! same port, refused tokens, read-only tokens, presence across reconnects,
//! and a clean close.

mod common;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bae_hub::{Access as TokenAccess, HubConfig, RunningHub};
use bae_model::{AuthorId, Op, PrimType, Specifier, Stage, StageEvent, SublayerId, Value};
use bae_sync::{
    Access, Backoff, ClientId, ConflictConfig, ConflictTracker, Connection, ConnectionConfig,
    ConnectionOutcome, ConnectionState, DocKey, DocSet, ElementValue, Failure, PresenceState,
    PrimChange, SessionConfig, Target,
};
use common::*;

const PROJECT: &str = "film";
const LONG: Duration = Duration::from_secs(30);

fn conn_config(id: &str) -> ConnectionConfig {
    let mut c = ConnectionConfig::new(ClientId::new(id).unwrap(), Arc::new(|| {}));
    c.backoff = Backoff {
        first: Duration::from_millis(100),
        max: Duration::from_secs(2),
        jitter: 0.0,
        ..Backoff::default()
    };
    c.hub_timeout = Duration::from_secs(5);
    c
}

fn state(name: &str) -> PresenceState {
    PresenceState {
        name: name.to_owned(),
        ..PresenceState::default()
    }
}

/// An app instance: a stage shared through a [`Connection`].
struct App {
    stage: Stage,
    conn: Connection,
    who: AuthorId,
}

impl App {
    fn join_with(
        addr: SocketAddr,
        project: &str,
        token: Option<&str>,
        config: ConnectionConfig,
        base: &[(SublayerId, Vec<u8>)],
    ) -> App {
        let who = author(&format!("user:{}", config.client));
        let target = Target::Hub {
            url: url(addr, project),
            token: token.map(str::to_owned),
        };
        App {
            stage: client_stage(base),
            conn: Connection::start(target, config),
            who,
        }
    }

    fn join(addr: SocketAddr, token: &str, id: &str, base: &[(SublayerId, Vec<u8>)]) -> App {
        Self::join_with(addr, PROJECT, Some(token), conn_config(id), base)
    }

    fn pump(&mut self) -> ConnectionOutcome {
        let out = self.conn.pump(&mut self.stage);
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        out
    }

    /// Returns the local event (for a `ConflictTracker`).
    fn set_opacity(&mut self, value: f64) -> StageEvent {
        self.stage.set_edit_target(&SublayerId::root()).unwrap();
        let ((), event) = self
            .stage
            .transact(&self.who, "edit", |tx| {
                tx.apply(Op::SetValue {
                    path: p("/comps/main/layers/title"),
                    attr: "opacity".into(),
                    value: Some(Value::Float(value)),
                })
            })
            .unwrap();
        event
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

    fn failure(&self) -> Option<Failure> {
        match self.conn.status().state {
            ConnectionState::Failed { failure, .. } => Some(failure),
            _ => None,
        }
    }
}

fn wait(what: &str, timeout: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn pump_for(apps: &mut [&mut App], d: Duration) {
    let start = Instant::now();
    while start.elapsed() < d {
        for a in apps.iter_mut() {
            a.pump();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Start a hub on `addr` (the port of one that just stopped). Retried: some
/// systems keep the port busy for a moment.
fn restart(data: &Path, addr: SocketAddr) -> RunningHub {
    let mut cfg: HubConfig = config(data);
    cfg.listen = addr;
    let deadline = Instant::now() + LONG;
    loop {
        match bae_hub::start(cfg.clone()) {
            Ok(hub) => return hub,
            Err(e) => {
                assert!(
                    Instant::now() < deadline,
                    "cannot listen on {addr} again: {e}"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

#[test]
fn two_apps_share_a_project_through_the_hub() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(config(data.path()));
    let base = base_snapshots();
    let mut a = App::join(hub.local_addr(), WRITE, "alice", &base);
    let mut b = App::join(hub.local_addr(), WRITE, "bob", &base);
    a.conn.set_presence(state("alice"));
    b.conn.set_presence(state("bob"));
    wait("both online", LONG, || {
        a.pump();
        b.pump();
        a.conn.is_online() && b.conn.is_online()
    });
    assert_eq!(a.conn.access(), Access::Write);
    assert_eq!(b.conn.status().access, Access::Write);

    a.set_opacity(7.0);
    b.create_layer("from_bob");
    let mut events = 0;
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
    assert!(events > 0);
    assert_eq!(b.conn.presence()[0].state.name, "alice");
}

/// The hub stops (backoff while it is away, edits go on offline), then a new
/// hub starts on the same port and data directory: both apps reconnect on
/// their own, everything converges, and presence is back at once although
/// heartbeats are a minute apart and presence lasts three: it is
/// re-published on reconnect (a newcomer sees both at once), and who was
/// online before is forgotten (carol, who quit while the hub was away, is
/// not shown).
#[test]
fn the_hub_goes_away_and_comes_back_on_the_same_port() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(config(data.path()));
    let addr = hub.local_addr();
    let base = base_snapshots();
    let config = |id: &str| {
        let mut c = conn_config(id);
        let mut s = SessionConfig::new(c.client.clone());
        s.presence_heartbeat = Duration::from_secs(60);
        s.presence_ttl = Duration::from_secs(180);
        c.session = Some(s);
        c.presence = Some(state(id));
        c
    };
    let mut a = App::join_with(addr, PROJECT, Some(WRITE), config("alice"), &base);
    let mut b = App::join_with(addr, PROJECT, Some(WRITE), config("bob"), &base);
    let mut c = App::join_with(addr, PROJECT, Some(WRITE), config("carol"), &base);
    wait("all online and visible", LONG, || {
        a.pump();
        b.pump();
        c.pump();
        [&a, &b, &c].iter().all(|x| x.conn.status().peers == 2)
    });
    a.set_opacity(1.0);
    wait("first edit", LONG, || {
        a.pump();
        b.pump();
        b.opacity() == Some(Value::Float(1.0))
    });

    hub.shutdown();
    wait("all offline", LONG, || {
        a.pump();
        b.pump();
        c.pump();
        [&a, &b, &c]
            .iter()
            .all(|x| matches!(x.conn.status().state, ConnectionState::Offline { .. }))
    });
    // Carol quits while the hub is away: her "left" never goes out.
    drop(c);
    let status = a.conn.status();
    assert_eq!(status.peers, 0);
    assert!(a.conn.presence().is_empty());
    assert_eq!(status.access, Access::Write, "kept while offline");

    // Retries back off: each wait about twice the one before.
    let mut seen: Vec<(u64, Duration)> = Vec::new();
    wait("three more failed attempts", LONG, || {
        a.pump();
        if let ConnectionState::Offline { retry_in, error } = a.conn.status().state {
            assert!(!error.is_empty());
            let attempt = a.conn.connect_attempts();
            if seen.last().is_none_or(|(n, _)| *n != attempt) {
                seen.push((attempt, retry_in));
            }
        }
        seen.len() >= 4
    });
    let delays: Vec<Duration> = seen.iter().skip(1).map(|(_, d)| *d).collect();
    eprintln!("hub away: retry waits as first seen {delays:?}");
    for pair in delays.windows(2) {
        assert!(pair[1] * 2 >= pair[0] * 3, "{delays:?}");
    }

    // Offline edits.
    a.set_opacity(2.0);
    b.create_layer("offline_bob");
    a.pump();
    assert!(a.conn.status().unsynced);

    let hub = restart(data.path(), addr);
    wait("reconnected and converged", LONG, || {
        a.pump();
        b.pump();
        let (sa, sb) = (a.conn.status(), b.conn.status());
        a.conn.is_online()
            && b.conn.is_online()
            && fingerprint(&a.stage) == fingerprint(&b.stage)
            && !sa.unsynced
            && !sb.unsynced
            && sa.peers == 1
            && sb.peers == 1
    });
    assert_eq!(b.opacity(), Some(Value::Float(2.0)));
    assert!(a.has_layer("offline_bob"));
    assert_eq!(b.conn.presence()[0].state.name, "alice");
    assert_eq!(a.conn.presence()[0].state.name, "bob");
    // A newcomer sees both at once: they re-published on reconnect.
    let mut d = App::join_with(addr, PROJECT, Some(WRITE), config("dave"), &base);
    wait("the newcomer sees both", Duration::from_secs(10), || {
        a.pump();
        b.pump();
        d.pump();
        d.conn.status().peers == 2
    });
    drop(hub);
}

/// The offline merge summary (`ConflictTracker`, spec 6.4) on a connection,
/// wired as the spec says: `connection_lost` when the state leaves `Online`,
/// `caught_up` at the first pump that is online again and `is_caught_up`.
/// While the hub is away, Alice sets the opacity, and Bob sets it after edits
/// of his own (so his op wins by Loro order) and adds a layer. Bob is back
/// first, and the hub has his edits before Alice reconnects: her summary
/// lists her edit as overridden by Bob and his changes as merged, and she
/// gets no online notice for it. While she is offline, `is_caught_up` still
/// answers for the old link, which is why the app asks `is_online` first.
#[test]
fn the_offline_summary_comes_once_the_new_link_caught_up() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(config(data.path()));
    let addr = hub.local_addr();
    let base = base_snapshots();
    let mut a = App::join(addr, WRITE, "alice", &base);
    let mut b = App::join(addr, WRITE, "bob", &base);
    wait("both online", LONG, || {
        a.pump();
        b.pump();
        a.conn.is_online() && b.conn.is_online()
    });
    let mut tracker = ConflictTracker::new(&a.stage, ConflictConfig::default());

    hub.shutdown();
    let mut lost = false;
    wait("both offline", LONG, || {
        a.pump();
        b.pump();
        if !lost && !a.conn.is_online() {
            tracker.connection_lost(&a.stage);
            lost = true;
        }
        lost && !b.conn.is_online()
    });
    assert!(
        a.conn.session().is_caught_up(&a.stage),
        "answers for the old link"
    );
    let event = a.set_opacity(2.0);
    assert!(tracker.observe(&a.stage, &event).is_empty());
    for v in [3.0, 4.0, 5.0] {
        b.set_opacity(v);
    }
    b.create_layer("offline_bob");

    let hub = restart(data.path(), addr);
    // The hub has Bob's edits once it says so (its next `Hello`).
    let docs = [
        DocKey::sublayer(&SublayerId::root()),
        DocKey::sublayer(&sl("anim/a")),
    ];
    wait("the hub confirmed Bob's edits", LONG, || {
        b.pump();
        let confirmed = b.conn.session().remote_versions();
        b.conn.is_online()
            && docs.iter().all(|doc| {
                confirmed
                    .and_then(|c| c.get(doc))
                    .is_some_and(|theirs| theirs.includes_vv(&b.stage.version(doc).unwrap()))
            })
    });
    let mut summary = None;
    wait("Alice caught up", LONG, || {
        a.pump();
        let notices = tracker.check(&a.stage);
        assert!(notices.is_empty(), "{notices:?}");
        if a.conn.is_online() && a.conn.session().is_caught_up(&a.stage) {
            summary = Some(tracker.caught_up(&a.stage).unwrap());
            true
        } else {
            false
        }
    });
    let caught = summary.unwrap().expect("connection_lost was called");
    assert!(caught.notices.is_empty(), "{:?}", caught.notices);
    let summary = caught.summary;
    assert_eq!(a.opacity(), Some(Value::Float(5.0)));
    assert!(a.has_layer("offline_bob"));
    assert_eq!(summary.overridden.len(), 1, "{summary:#?}");
    let edit = &summary.overridden[0];
    assert_eq!(edit.attr, "opacity");
    assert_eq!(edit.ours, Some(ElementValue::Value(Value::Float(2.0))));
    assert_eq!(edit.result, Some(ElementValue::Value(Value::Float(5.0))));
    assert_eq!(edit.winner, Some(author("user:bob")));
    assert!(summary.kept.is_empty(), "{summary:#?}");
    let merged: Vec<_> = summary
        .merged
        .iter()
        .map(|d| (d.path.to_string(), d.change.clone()))
        .collect();
    assert!(
        merged.contains(&(format!("{LAYERS}/offline_bob"), PrimChange::Added)),
        "{merged:?}"
    );
    assert!(
        merged.contains(&(format!("{LAYERS}/title"), PrimChange::Modified)),
        "{merged:?}"
    );
    // Once: the next reconnect needs `connection_lost` again.
    assert!(tracker.caught_up(&a.stage).unwrap().is_none());
    drop(hub);
}

/// A link that stayed up longer than `stable_after` and then dropped is
/// retried after `first` again, however many attempts failed before it.
#[test]
fn backoff_starts_over_after_a_stable_link() {
    // A port nobody listens on yet.
    let addr = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let data = tempfile::tempdir().unwrap();
    let base = base_snapshots();
    let mut cfg = conn_config("alice");
    cfg.backoff.first = Duration::from_millis(50);
    cfg.backoff.stable_after = Duration::from_millis(300);
    let mut a = App::join_with(addr, PROJECT, Some(WRITE), cfg, &base);
    // Three failed attempts: the next wait would be 400 ms.
    wait("failed attempts", LONG, || {
        a.pump();
        a.conn.connect_attempts() >= 4
    });
    let hub = restart(data.path(), addr);
    wait("online", LONG, || {
        a.pump();
        a.conn.is_online()
    });
    pump_for(&mut [&mut a], Duration::from_millis(500));
    hub.shutdown();
    let mut first_wait = None;
    wait("offline", LONG, || {
        a.pump();
        if let ConnectionState::Offline { retry_in, .. } = a.conn.status().state {
            first_wait = Some(retry_in);
        }
        first_wait.is_some()
    });
    let first_wait = first_wait.unwrap();
    assert!(first_wait <= Duration::from_millis(50), "{first_wait:?}");
}

/// A link without traffic stays up: the hub's `Hello`s every 5 seconds keep
/// it alive against the client's idle timeout.
#[test]
fn an_idle_link_to_the_hub_stays_up() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(config(data.path()));
    let base = base_snapshots();
    let mut cfg = conn_config("alice");
    cfg.hub_idle_timeout = Some(Duration::from_secs(9));
    let mut a = App::join_with(hub.local_addr(), PROJECT, Some(WRITE), cfg, &base);
    wait("online", LONG, || {
        a.pump();
        a.conn.is_online()
    });
    pump_for(&mut [&mut a], Duration::from_secs(11));
    assert!(a.conn.is_online(), "{:?}", a.conn.status());
    assert_eq!(a.conn.connect_attempts(), 1);
}

/// Refusals that retrying does not fix stop the retries: one attempt, then
/// only `retry()`.
#[test]
fn refused_connections_need_attention_not_a_retry_loop() {
    let data = tempfile::tempdir().unwrap();
    let mut cfg = config(data.path());
    cfg.tokens.push(token(
        "other",
        "other-token-0123456789",
        TokenAccess::Write,
        Some(&["elsewhere"]),
    ));
    let hub = start(cfg);
    let base = base_snapshots();
    let cases = [
        (Some("wrong-token-000000"), PROJECT, Failure::BadToken),
        (None, PROJECT, Failure::BadToken),
        (Some("other-token-0123456789"), PROJECT, Failure::NoAccess),
        // Read-only tokens cannot create projects.
        (Some(READ), "nothing-here", Failure::NoSuchProject),
        (Some(WRITE), "bad.name", Failure::Refused(400)),
    ];
    for (token, project, expected) in cases {
        let what = format!("{token:?} on {project}");
        let mut app = App::join_with(hub.local_addr(), project, token, conn_config("x"), &base);
        wait(&what, LONG, || {
            app.pump();
            app.failure().is_some()
        });
        assert_eq!(app.failure(), Some(expected), "{what}");
        pump_for(&mut [&mut app], Duration::from_millis(300));
        assert_eq!(app.conn.connect_attempts(), 1, "{what}: retried on its own");
        app.conn.retry();
        assert_eq!(app.conn.connect_attempts(), 2, "{what}");
        wait(&what, LONG, || {
            app.pump();
            app.failure().is_some()
        });
        assert_eq!(app.failure(), Some(expected), "{what}");
    }
}

/// A read-only token: the hub says so in the handshake, the app gets
/// everything but sends none of its own edits (and says they are unsynced);
/// its presence still goes out. The hub takes only small messages from
/// read-only connections (`read_only_max_incoming`): local edits larger than
/// that do not get the reader dropped, since it never sends them.
#[test]
fn a_read_only_token_reads_but_sends_no_edits() {
    let data = tempfile::tempdir().unwrap();
    let mut cfg = config(data.path());
    cfg.read_only_max_incoming = 64 << 10;
    let hub = start(cfg);
    let base = base_snapshots();
    let mut w = App::join(hub.local_addr(), WRITE, "writer", &base);
    w.set_opacity(5.0);
    // The writer creates the project (a read-only token gets 404 before).
    wait("writer online", LONG, || {
        w.pump();
        w.conn.is_online() && !w.conn.status().unsynced
    });
    let mut r = App::join(hub.local_addr(), READ, "reader", &base);
    r.conn.set_presence(state("reader"));
    wait("reader online with the writer's edit", LONG, || {
        w.pump();
        r.pump();
        r.conn.is_online() && r.opacity() == Some(Value::Float(5.0))
    });
    assert_eq!(r.conn.access(), Access::Read);
    assert_eq!(r.conn.status().access, Access::Read);
    assert_eq!(w.conn.access(), Access::Write);

    // The app greys out editing; edits made anyway stay local, also one over
    // the hub's limit for read-only connections.
    r.create_layer("reader_only");
    let big: String = (0..256 << 10)
        .map(|i: u32| char::from(b'a' + (i.wrapping_mul(2_654_435_761) >> 27) as u8 % 26))
        .collect();
    r.stage.set_edit_target(&SublayerId::root()).unwrap();
    r.stage
        .transact(&r.who, "note", |tx| {
            tx.apply(Op::SetValue {
                path: p("/comps/main"),
                attr: "note".into(),
                value: Some(Value::String(big)),
            })
        })
        .unwrap();
    pump_for(&mut [&mut w, &mut r], Duration::from_millis(500));
    assert!(!w.has_layer("reader_only"));
    assert!(r.conn.status().unsynced);
    assert_eq!(r.conn.session().stats().bytes_sent, 0);
    assert!(!r.conn.session().config().send_data);
    assert!(r.conn.is_online());
    assert_eq!(r.conn.connect_attempts(), 1, "never dropped");

    // It keeps receiving, and is seen.
    w.set_opacity(6.0);
    wait("the next edit and the reader's presence", LONG, || {
        w.pump();
        r.pump();
        r.opacity() == Some(Value::Float(6.0)) && w.conn.status().peers == 1
    });
    assert_eq!(w.conn.presence()[0].state.name, "reader");
}

/// Closing returns at once; the last edits (held back by a `min_send_interval`,
/// so only the final flush sends them) and the "left" reach the others, and
/// the hub keeps the edits. That the hub confirms the close before the
/// socket closes is tested in `crates/bae-sync/tests/hub.rs` (a plain drop
/// delivers just as well over loopback here).
#[test]
fn closing_delivers_the_last_edits() {
    let data = tempfile::tempdir().unwrap();
    let hub = start(config(data.path()));
    let base = base_snapshots();
    let mut cfg = conn_config("alice");
    let mut session = SessionConfig::new(cfg.client.clone());
    session.min_send_interval = Duration::from_secs(1);
    cfg.session = Some(session);
    let mut a = App::join_with(hub.local_addr(), PROJECT, Some(WRITE), cfg, &base);
    let mut b = App::join(hub.local_addr(), WRITE, "bob", &base);
    a.conn.set_presence(state("alice"));
    wait("both online", LONG, || {
        a.pump();
        b.pump();
        a.conn.is_online() && b.conn.status().peers == 1
    });
    a.set_opacity(10.0);
    a.pump();
    a.set_opacity(11.0);
    a.create_layer("last");
    let started = Instant::now();
    let mut closing = a.conn.close(&mut a.stage);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(closing.unsynced().is_empty(), "{:?}", closing.unsynced());
    let result = closing.wait(LONG).expect("closed in time");
    assert!(result.is_ok(), "{result:?}");
    wait("the last edits and the leave", LONG, || {
        b.pump();
        b.opacity() == Some(Value::Float(11.0))
            && b.has_layer("last")
            && b.conn.presence().is_empty()
    });
    // The hub has it for newcomers.
    let mut c = App::join(hub.local_addr(), WRITE, "carol", &base);
    wait("newcomer", LONG, || {
        c.pump();
        c.opacity() == Some(Value::Float(11.0)) && c.has_layer("last")
    });
}
