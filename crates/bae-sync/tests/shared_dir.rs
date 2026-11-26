//! Shared-folder mode: several clients on one project directory (temporary
//! directories stand in for the NAS share).

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use bae_model::loro::VersionVector;
use bae_model::project::Project;
use bae_model::{AuthorId, Op, PrimType, Specifier, Stage, SublayerId, Value};
use bae_sync::{
    ClientId, DirConfig, DirTransport, DocKey, MemoryTransport, Message, PeerHandle, PresenceState,
    Relay, SessionConfig, SyncError, SyncSession, ThreadedTransport, Transport,
};
use common::*;

/// A project directory with the base project saved in it.
fn project_dir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Film.bae");
    let base = base_snapshots();
    let mut project = Project::from_stage(client_stage(&base));
    project.save_as(&dir).unwrap();
    (tmp, dir)
}

fn fast() -> DirConfig {
    DirConfig {
        poll_interval: Duration::ZERO,
        watch: false,
        ..DirConfig::default()
    }
}

fn session(id: &str) -> SyncSession {
    let mut config = SessionConfig::new(ClientId::new(id).unwrap());
    config.hello_interval = None;
    SyncSession::new(config)
}

struct Client {
    stage: Stage,
    session: SyncSession,
    dir: DirTransport,
    who: AuthorId,
    rng: Rng,
    n: u32,
    tag: String,
}

impl Client {
    fn open(project: &Path, id: &str, config: DirConfig, seed: u64) -> Self {
        Client {
            stage: Project::open(project).unwrap().stage,
            session: session(id),
            dir: DirTransport::open(project, ClientId::new(id).unwrap(), config).unwrap(),
            who: author(&format!("user:{id}")),
            rng: Rng::new(seed),
            n: 0,
            tag: id.to_owned(),
        }
    }

    fn pump(&mut self) {
        let out = self.session.pump(&mut self.stage, &mut self.dir).unwrap();
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        assert!(
            self.dir.last_error().is_none(),
            "{:?}",
            self.dir.last_error()
        );
    }

    fn edit(&mut self) {
        random_edit(
            &mut self.stage,
            &self.who,
            &mut self.rng,
            &self.tag,
            &mut self.n,
        );
    }
}

fn all_equal(clients: &[Client]) -> bool {
    let first = fingerprint(&clients[0].stage);
    clients.iter().all(|c| fingerprint(&c.stage) == first)
}

fn settle(clients: &mut [Client]) {
    for round in 0.. {
        for c in clients.iter_mut() {
            c.pump();
        }
        if all_equal(clients)
            && clients
                .iter()
                .all(|c| c.session.unsynced(&c.stage).is_empty())
        {
            return;
        }
        assert!(round < 50, "no convergence");
    }
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    for e in fs::read_dir(dir).unwrap().flatten() {
        let m = e.metadata().unwrap();
        total += if m.is_dir() {
            dir_size(&e.path())
        } else {
            m.len()
        };
    }
    total
}

#[test]
fn clients_converge_through_a_shared_folder() {
    let (_tmp, dir) = project_dir();
    let mut clients: Vec<Client> = (0..3)
        .map(|i| Client::open(&dir, &format!("c{i}"), fast(), 40 + i))
        .collect();
    for _ in 0..120 {
        for c in &mut clients {
            if c.rng.chance(60) {
                c.edit();
            }
            c.pump();
        }
    }
    settle(&mut clients);
    // Every client wrote its own logs for both sublayers.
    for i in 0..3 {
        for doc in ["root", "anim/a"] {
            let log = dir.join(format!("updates/{doc}/c{i}.log"));
            assert!(log.is_file(), "{}", log.display());
        }
    }
    // A client that joins later (from the stale saved project) catches up.
    let mut late = Client::open(&dir, "late", fast(), 99);
    late.pump();
    late.pump();
    assert_eq!(fingerprint(&late.stage), fingerprint(&clients[0].stage));
}

#[test]
fn restart_recovers_unsaved_edits_and_repairs_a_torn_log() {
    let (_tmp, dir) = project_dir();
    let mut a = Client::open(&dir, "alice", fast(), 1);
    a.pump();
    for _ in 0..30 {
        a.edit();
        a.pump();
    }
    let expected = fingerprint(&a.stage);
    let log = a.dir.log_path(&DocKey::sublayer(&SublayerId::root()));
    drop(a);

    // Crash in the middle of an append: half a record at the end.
    let good_len = fs::metadata(&log).unwrap().len();
    let mut bytes = fs::read(&log).unwrap();
    bytes.extend_from_slice(&[200, 0, 0, 0, 1, 2, 3]);
    fs::write(&log, &bytes).unwrap();

    // The saved project never saw these edits; the logs have them.
    let mut a = Client::open(&dir, "alice", fast(), 2);
    a.pump();
    assert_eq!(fingerprint(&a.stage), expected);
    assert_eq!(fs::metadata(&log).unwrap().len(), good_len, "torn tail cut");
    assert_eq!(a.dir.stats().repairs, 1);

    // Appending after the repair produces a log others can read.
    for _ in 0..5 {
        a.edit();
        a.pump();
    }
    let b = Client::open(&dir, "bob", fast(), 3);
    let mut clients = vec![a, b];
    settle(&mut clients);
}

#[test]
fn own_log_is_compacted_and_growth_stays_bounded() {
    let (_tmp, dir) = project_dir();
    let config = DirConfig {
        compact_min_bytes: 32 * 1024,
        ..fast()
    };
    let mut a = Client::open(&dir, "alice", config.clone(), 5);
    let mut b = Client::open(&dir, "bob", config, 6);
    a.pump();
    b.pump();
    let root = DocKey::sublayer(&SublayerId::root());
    let log = a.dir.log_path(&root);
    let mut max_log = 0;
    // "An hour of editing": one change per second, mostly re-setting the same
    // attributes (drags), as an animator does.
    // Random edits never delete or move the comp itself.
    let comp = p("/comps/main");
    let mut drags = 0;
    a.stage.set_edit_target(&SublayerId::root()).unwrap();
    for i in 0..3600 {
        if i % 50 == 0 {
            a.edit();
        } else {
            a.stage.set_edit_target(&SublayerId::root()).unwrap();
            a.stage
                .transact(&a.who, "drag", |tx| {
                    tx.apply(Op::SetValue {
                        path: comp.clone(),
                        attr: "bgColor".into(),
                        value: Some(Value::Float(f64::from(i % 100))),
                    })
                })
                .unwrap();
            drags += 1;
        }
        a.pump();
        if i % 10 == 0 {
            b.pump();
        }
        max_log = max_log.max(fs::metadata(&log).map(|m| m.len()).unwrap_or(0));
    }
    assert!(drags > 3500);
    let mut clients = vec![a, b];
    settle(&mut clients);
    let a = &clients[0];
    let snapshot = a
        .stage
        .sublayer(&SublayerId::root())
        .unwrap()
        .export_snapshot()
        .unwrap();
    let compactions = a.dir.stats().compactions;
    let total = dir_size(&dir.join("updates"));
    eprintln!(
        "3600 edits: {} records, {compactions} compactions, own log peaked at {max_log} B, \
         snapshot {} B, updates/ total {total} B",
        a.dir.stats().records_written,
        snapshot.len()
    );
    assert!(compactions > 0);
    // The log never grows far past the compaction threshold.
    let threshold = (32 * 1024u64).max(4 * snapshot.len() as u64);
    assert!(max_log < threshold + 16 * 1024, "{max_log} vs {threshold}");
    // Two clients' logs over two sublayers: a few snapshots' worth, not 3600 records.
    assert!(total < 8 * threshold, "{total}");
}

#[test]
fn concurrent_compaction_does_not_lose_data() {
    let (_tmp, dir) = project_dir();
    let config = DirConfig {
        compact_min_bytes: 2 * 1024,
        compact_ratio: 2,
        ..fast()
    };
    let mut clients: Vec<Client> = (0..3)
        .map(|i| Client::open(&dir, &format!("k{i}"), config.clone(), 70 + i))
        .collect();
    for _ in 0..300 {
        for c in &mut clients {
            c.edit();
            c.pump();
        }
    }
    settle(&mut clients);
    assert!(clients.iter().all(|c| c.dir.stats().compactions > 0));
    let mut fresh = Client::open(&dir, "fresh", fast(), 1);
    fresh.pump();
    fresh.pump();
    assert_eq!(fingerprint(&fresh.stage), fingerprint(&clients[0].stage));
}

#[test]
fn a_client_id_can_only_be_used_once_at_a_time() {
    let (_tmp, dir) = project_dir();
    let id = ClientId::new("solo").unwrap();
    let first = DirTransport::open(&dir, id.clone(), fast()).unwrap();
    assert!(matches!(
        DirTransport::open(&dir, id.clone(), fast()),
        Err(SyncError::ClientIdInUse(_))
    ));
    drop(first);
    DirTransport::open(&dir, id, fast()).unwrap();
}

#[test]
fn a_sublayer_added_later_is_redelivered() {
    let (_tmp, dir) = project_dir();
    let mut a = Client::open(&dir, "a", fast(), 1);
    let mut b = Client::open(&dir, "b", fast(), 2);
    a.pump();
    b.pump();
    let fx = sl("fx");
    a.stage.create_sublayer(fx.clone(), 0).unwrap();
    a.stage.set_edit_target(&fx).unwrap();
    a.stage
        .transact(&a.who, "fx", |tx| {
            tx.apply(Op::CreatePrim {
                path: p("/fx"),
                specifier: bae_model::Specifier::Def,
                prim_type: None,
                index: None,
            })
        })
        .unwrap();
    a.pump();
    let out = b.session.pump(&mut b.stage, &mut b.dir).unwrap();
    assert!(out.unknown_docs.contains(&DocKey::sublayer(&fx)));
    b.stage
        .insert_sublayer(bae_model::Sublayer::new(fx.clone()), 0)
        .unwrap();
    b.pump();
    b.pump();
    assert!(b.stage.sublayer(&fx).unwrap().has_spec(&p("/fx")));
}

#[test]
fn threaded_transport_moves_io_off_the_caller() {
    let (tx, rx) = std::sync::mpsc::channel();
    let (inner, mut remote) = MemoryTransport::pair();
    let mut threaded = ThreadedTransport::spawn(inner, Duration::from_millis(5), move || {
        let _ = tx.send(());
    });
    // Messages pass both ways.
    threaded
        .send(bae_sync::Message::SnapshotRequest {
            doc: DocKey::History,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let got = loop {
        if let Some(m) = remote.recv().unwrap() {
            break m;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(matches!(got, bae_sync::Message::SnapshotRequest { .. }));
    remote
        .send(bae_sync::Message::SnapshotRequest {
            doc: DocKey::History,
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(threaded.recv().unwrap().is_some());
    // Closing the far side surfaces as `Closed`.
    drop(remote);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match threaded.recv() {
            Err(SyncError::Closed) => break,
            _ => assert!(Instant::now() < deadline),
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

// ---- simulated SMB latency ----

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            fs::copy(e.path(), target).unwrap();
        }
    }
}

/// Files under `updates/` named `<client>.log`, relative to `root`.
fn own_logs(root: &Path, dir: &Path, client: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            own_logs(root, &path, client, out);
        } else if path.file_name().and_then(|n| n.to_str()) == Some(&format!("{client}.log")) {
            out.push(path.strip_prefix(root).unwrap().to_owned());
        }
    }
}

struct Pending {
    at: Instant,
    view: usize,
    rel: PathBuf,
    bytes: Vec<u8>,
}

/// Every client works on its own copy ("view") of the project directory. This
/// thread copies each client's logs into the other views after a random delay
/// (per file in order, like one SMB client's cache) and sometimes shows a
/// truncated prefix first, as a reader can see a file whose append has not fully
/// arrived. Overwrites are not atomic either.
fn replicate(
    views: Vec<(PathBuf, String)>,
    delay: (Duration, Duration),
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use std::collections::HashMap;
    let mut rng = Rng::new(4242);
    let mut seen: HashMap<(usize, PathBuf), Vec<u8>> = HashMap::new();
    let mut last_at: HashMap<(usize, PathBuf), Instant> = HashMap::new();
    let mut queue: Vec<Pending> = Vec::new();
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        let now = Instant::now();
        for (i, (root, client)) in views.iter().enumerate() {
            let mut rels = Vec::new();
            own_logs(root, &root.join("updates"), client, &mut rels);
            for rel in rels {
                let Ok(bytes) = fs::read(root.join(&rel)) else {
                    continue;
                };
                if seen.get(&(i, rel.clone())) == Some(&bytes) {
                    continue;
                }
                seen.insert((i, rel.clone()), bytes.clone());
                for j in (0..views.len()).filter(|&j| j != i) {
                    let span = (delay.1 - delay.0).as_millis() as u64;
                    let wait = delay.0 + Duration::from_millis(rng.next() % (span + 1));
                    let key = (j, rel.clone());
                    let at = (now + wait).max(*last_at.get(&key).unwrap_or(&now));
                    last_at.insert(key, at);
                    if rng.chance(30) && bytes.len() > 1 {
                        let cut = 1 + rng.below(bytes.len() - 1);
                        queue.push(Pending {
                            at,
                            view: j,
                            rel: rel.clone(),
                            bytes: bytes[..cut].to_vec(),
                        });
                    }
                    queue.push(Pending {
                        at: at + Duration::from_millis(1),
                        view: j,
                        rel: rel.clone(),
                        bytes: bytes.clone(),
                    });
                }
            }
        }
        queue.sort_by_key(|p| p.at);
        while queue.first().is_some_and(|p| p.at <= now) {
            let p = queue.remove(0);
            let target = views[p.view].0.join(&p.rel);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(&target, &p.bytes).unwrap();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn smb_latency(delay: (Duration, Duration), edit_for: Duration, seed: u64) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let (_tmp, dir) = project_dir();
    let n = 3;
    let views: Vec<(PathBuf, String)> = (0..n)
        .map(|i| {
            let view = dir.with_file_name(format!("view{i}.bae"));
            copy_dir(&dir, &view);
            (view, format!("s{i}"))
        })
        .collect();
    let stop = Arc::new(AtomicBool::new(false));
    let replicator = {
        let (views, stop) = (views.clone(), stop.clone());
        std::thread::spawn(move || replicate(views, delay, stop))
    };
    struct Peer {
        stage: Stage,
        session: SyncSession,
        link: ThreadedTransport,
        who: AuthorId,
        rng: Rng,
        n: u32,
        tag: String,
    }
    let mut peers: Vec<Peer> = views
        .iter()
        .enumerate()
        .map(|(i, (view, id))| {
            let config = DirConfig {
                poll_interval: Duration::from_millis(50),
                ..fast()
            };
            let dir = DirTransport::open(view, ClientId::new(id).unwrap(), config).unwrap();
            let mut session = session(id);
            session.config_mut().min_send_interval = Duration::from_millis(100);
            Peer {
                stage: Project::open(view).unwrap().stage,
                session,
                link: ThreadedTransport::spawn(dir, Duration::from_millis(10), || {}),
                who: author(&format!("user:{id}")),
                rng: Rng::new(seed + i as u64),
                n: 0,
                tag: id.clone(),
            }
        })
        .collect();
    let pump = |peers: &mut Vec<Peer>| {
        for p in peers.iter_mut() {
            let out = p.session.pump(&mut p.stage, &mut p.link).unwrap();
            assert!(out.errors.is_empty(), "{:?}", out.errors);
        }
    };
    let start = Instant::now();
    while start.elapsed() < edit_for {
        for p in peers.iter_mut() {
            if p.rng.chance(30) {
                random_edit(&mut p.stage, &p.who, &mut p.rng, &p.tag, &mut p.n);
            }
        }
        pump(&mut peers);
        std::thread::sleep(Duration::from_millis(10));
    }
    let stopped = Instant::now();
    let settle_by = stopped + delay.1 * 10 + Duration::from_secs(5);
    loop {
        pump(&mut peers);
        let first = fingerprint(&peers[0].stage);
        if peers
            .iter()
            .all(|p| fingerprint(&p.stage) == first && p.session.unsynced(&p.stage).is_empty())
        {
            break;
        }
        assert!(
            Instant::now() < settle_by,
            "no convergence under SMB-like latency"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    eprintln!(
        "SMB latency {delay:?}: converged {:?} after edits stopped",
        stopped.elapsed()
    );
    stop.store(true, Ordering::Relaxed);
    replicator.join().unwrap();
}

/// 1–3 s of visibility lag, scaled down 10x so the suite stays fast; the logic
/// does not depend on absolute times. `smb_latency_real_time` runs it unscaled.
#[test]
fn converges_under_smb_like_latency() {
    smb_latency(
        (Duration::from_millis(100), Duration::from_millis(300)),
        Duration::from_secs(2),
        500,
    );
}

#[test]
#[ignore = "slow: real 1-3 s latency (run with --ignored)"]
fn smb_latency_real_time() {
    smb_latency(
        (Duration::from_secs(1), Duration::from_secs(3)),
        Duration::from_secs(10),
        600,
    );
}

/// With `watch`, a change wakes the reader long before its poll interval.
#[cfg(feature = "watch")]
#[test]
fn file_events_wake_the_poll() {
    let (_tmp, dir) = project_dir();
    let mut a = Client::open(&dir, "a", fast(), 1);
    let slow = DirConfig {
        poll_interval: Duration::from_secs(3600),
        watch: true,
        ..DirConfig::default()
    };
    let mut b = Client::open(&dir, "b", slow, 2);
    a.pump();
    b.pump();
    b.pump();
    // Let the watcher settle so the first scan's own events are behind us.
    std::thread::sleep(Duration::from_millis(300));
    b.pump();
    a.stage.set_edit_target(&SublayerId::root()).unwrap();
    a.stage
        .transact(&a.who, "set", |tx| {
            tx.apply(Op::SetValue {
                path: p("/comps/main"),
                attr: "note".into(),
                value: Some(Value::String("hello".into())),
            })
        })
        .unwrap();
    a.pump();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        b.pump();
        if fingerprint(&b.stage) == fingerprint(&a.stage) {
            break;
        }
        assert!(Instant::now() < deadline, "no wake-up from file events");
        std::thread::sleep(Duration::from_millis(10));
    }
}

// ---- departed clients' logs (spec §4.6) ----

const DAY: Duration = Duration::from_secs(24 * 3600);

/// Both sublayers of the test project.
fn layers() -> [SublayerId; 2] {
    [SublayerId::root(), sl("anim/a")]
}

/// `client`'s log of every sublayer.
fn logs_of(dir: &Path, client: &str) -> Vec<PathBuf> {
    layers()
        .iter()
        .map(|id| dir.join(format!("updates/{}/{client}.log", id.as_str())))
        .collect()
}

/// Pretend nobody touched `path` for `days` days.
fn age(path: &Path, days: u32) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(SystemTime::now() - DAY * days)
        .unwrap();
}

/// Create the layer `name` in both sublayers, so `c` writes a log for each and
/// the edit can be looked for later.
fn mark(c: &mut Client, name: &str) {
    for id in layers() {
        c.stage.set_edit_target(&id).unwrap();
        c.stage
            .transact(&c.who, "mark", |tx| {
                tx.apply(Op::CreatePrim {
                    path: p(&format!("{LAYERS}/{name}")),
                    specifier: Specifier::Def,
                    prim_type: Some(PrimType::new("Layer")),
                    index: None,
                })
            })
            .unwrap();
    }
    c.pump();
}

fn has_mark(stage: &Stage, name: &str) -> bool {
    layers().iter().all(|id| {
        stage
            .sublayer(id)
            .unwrap()
            .has_spec(&p(&format!("{LAYERS}/{name}")))
    })
}

/// Rewrite `c`'s logs as one snapshot each, as compaction does (the session
/// sends the same message when the folder asks for a snapshot).
fn compact_now(c: &mut Client) {
    for id in layers() {
        let layer = c.stage.sublayer(&id).unwrap();
        c.dir
            .send(Message::Snapshot {
                doc: DocKey::sublayer(&id),
                version: layer.version(),
                data: layer.export_snapshot().unwrap(),
            })
            .unwrap();
    }
}

/// `a`, `b` and `d` edit for a while and converge; `d` leaves a mark.
fn three_clients(dir: &Path) -> (Client, Client, Client) {
    let mut clients: Vec<Client> = ["a", "b", "d"]
        .iter()
        .zip(1..)
        .map(|(id, seed)| Client::open(dir, id, fast(), seed))
        .collect();
    for _ in 0..20 {
        for c in &mut clients {
            c.edit();
            c.pump();
        }
    }
    mark(&mut clients[2], "d_mark");
    settle(&mut clients);
    let d = clients.pop().unwrap();
    let b = clients.pop().unwrap();
    let a = clients.pop().unwrap();
    (a, b, d)
}

#[test]
fn departed_clients_covered_log_is_removed() {
    let (_tmp, dir) = project_dir();
    let (mut a, b, d) = three_clients(&dir);
    let d_logs = logs_of(&dir, "d");
    drop(d);
    a.session.set_presence(PresenceState {
        name: "a".into(),
        ..PresenceState::default()
    });
    a.pump();
    // a's logs start with a snapshot that holds everything d wrote.
    compact_now(&mut a);
    for log in &d_logs {
        age(log, 30);
    }
    // A newcomer's second poll runs the cleanup on its own.
    let mut janitor = Client::open(&dir, "janitor", fast(), 7);
    janitor.pump();
    janitor.pump();
    assert_eq!(janitor.dir.stats().logs_removed, 2);
    assert!(d_logs.iter().all(|l| !l.exists()));
    for log in logs_of(&dir, "a").iter().chain(&logs_of(&dir, "b")) {
        assert!(log.is_file(), "{}", log.display());
    }
    // Only logs go: not the lock file, presence files or base snapshots.
    assert!(dir.join("updates/d.lock").is_file());
    assert!(dir.join("presence/a.presence").is_file());
    assert!(dir.join("sublayers/root.loro").is_file());

    // Everyone left keeps working.
    let mut clients = vec![a, b, janitor];
    for (i, c) in clients.iter_mut().enumerate() {
        mark(c, &format!("after_{i}"));
    }
    settle(&mut clients);
    // A client joining now never sees d's log: d's edits come from a's snapshot.
    let mut late = Client::open(&dir, "late", fast(), 9);
    late.pump();
    late.pump();
    assert_eq!(fingerprint(&late.stage), fingerprint(&clients[0].stage));
    assert!(has_mark(&late.stage, "d_mark"));
    assert!(has_mark(&late.stage, "after_0"));
}

#[test]
fn log_with_ops_nobody_else_holds_is_kept() {
    let (_tmp, dir) = project_dir();
    let (mut a, mut b, mut d) = three_clients(&dir);
    compact_now(&mut a);
    // d's last edit is not in a's snapshot.
    mark(&mut d, "d_late");
    drop(d);
    // a and b read it and append edits of their own: the `version` of those
    // records includes d's edit, their data does not. b's logs do not start with
    // a snapshot, so they cover nothing anyway.
    a.pump();
    mark(&mut a, "a_after");
    b.pump();
    mark(&mut b, "b_after");
    assert!(has_mark(&a.stage, "d_late") && has_mark(&b.stage, "d_late"));
    let d_logs = logs_of(&dir, "d");
    for log in &d_logs {
        age(log, 30);
    }
    assert_eq!(b.dir.remove_departed_logs(), 0);
    assert_eq!(a.dir.remove_departed_logs(), 0);
    assert!(d_logs.iter().all(|l| l.is_file()));
    // Once a snapshot holds it, it can go.
    compact_now(&mut a);
    assert_eq!(b.dir.remove_departed_logs(), 2);
    assert!(d_logs.iter().all(|l| !l.exists()));
    let mut late = Client::open(&dir, "late", fast(), 9);
    late.pump();
    late.pump();
    assert!(has_mark(&late.stage, "d_late"));
}

#[test]
fn log_of_a_client_with_a_fresh_heartbeat_is_kept() {
    let (_tmp, dir) = project_dir();
    let (mut a, mut b, mut d) = three_clients(&dir);
    d.session.set_presence(PresenceState {
        name: "d".into(),
        ..PresenceState::default()
    });
    d.pump();
    let heartbeat = dir.join("presence/d.presence");
    assert!(heartbeat.is_file());
    drop(d);
    compact_now(&mut a);
    let d_logs = logs_of(&dir, "d");
    for log in &d_logs {
        age(log, 30);
    }
    // Old logs, but the owner was online recently.
    assert_eq!(b.dir.remove_departed_logs(), 0);
    assert!(d_logs.iter().all(|l| l.is_file()));
    // A heartbeat as old as the logs: now it goes.
    age(&heartbeat, 30);
    assert_eq!(b.dir.remove_departed_logs(), 2);
    assert!(d_logs.iter().all(|l| !l.exists()));
}

#[test]
fn of_two_equal_logs_only_the_larger_client_id_goes() {
    let (_tmp, dir) = project_dir();
    let mut clients = vec![
        Client::open(&dir, "a", fast(), 1),
        Client::open(&dir, "b", fast(), 2),
    ];
    for _ in 0..20 {
        for c in &mut clients {
            c.edit();
            c.pump();
        }
    }
    settle(&mut clients);
    for id in layers() {
        let version = |c: &Client| c.stage.sublayer(&id).unwrap().version();
        assert_eq!(version(&clients[0]), version(&clients[1]));
    }
    for c in &mut clients {
        compact_now(c);
    }
    let (a_logs, b_logs) = (logs_of(&dir, "a"), logs_of(&dir, "b"));
    // Both logs hold the same version, nobody touched them for a month and
    // neither client publishes presence: each is a candidate for the other.
    for log in a_logs.iter().chain(&b_logs) {
        age(log, 30);
    }
    let (a, b) = clients.split_at_mut(1);
    let (a, b) = (&mut a[0], &mut b[0]);
    // b's side: a's logs are covered by b's, but not by a strictly larger one.
    assert_eq!(b.dir.remove_departed_logs(), 0);
    // a's side: b's logs are equal to a's and "b" > "a", so they go.
    assert_eq!(a.dir.remove_departed_logs(), 2);
    assert!(a_logs.iter().all(|l| l.is_file()));
    assert!(b_logs.iter().all(|l| !l.exists()));
    // And again from b's side: a's logs are not covered by anything left.
    assert_eq!(b.dir.remove_departed_logs(), 0);
    assert!(a_logs.iter().all(|l| l.is_file()));
    // b is still running: it notices and writes its logs again.
    b.pump();
    assert!(b_logs.iter().all(|l| l.is_file()));
    settle(&mut clients);
}

#[test]
fn log_with_a_torn_tail_is_kept() {
    let (_tmp, dir) = project_dir();
    let (mut a, b, d) = three_clients(&dir);
    drop((b, d));
    compact_now(&mut a);
    let [root_log, anim_log] = logs_of(&dir, "d").try_into().unwrap();
    let good = fs::read(&root_log).unwrap();
    let mut torn = good.clone();
    torn.extend_from_slice(&[200, 0, 0, 0, 1, 2, 3]);
    fs::write(&root_log, &torn).unwrap();
    age(&root_log, 30);
    age(&anim_log, 30);
    // The intact log goes; the one we cannot read to the end stays.
    assert_eq!(a.dir.remove_departed_logs(), 1);
    assert!(root_log.is_file());
    assert!(!anim_log.exists());
    // Without the torn tail it goes too.
    fs::write(&root_log, &good).unwrap();
    age(&root_log, 30);
    assert_eq!(a.dir.remove_departed_logs(), 1);
    assert!(!root_log.exists());
}

#[test]
fn running_owner_writes_its_removed_log_again() {
    let (_tmp, dir) = project_dir();
    let (mut a, b, mut d) = three_clients(&dir);
    compact_now(&mut a);
    // d is still running, but idle and without presence for a month.
    let d_logs = logs_of(&dir, "d");
    for log in &d_logs {
        age(log, 30);
    }
    assert_eq!(a.dir.remove_departed_logs(), 2);
    assert!(d_logs.iter().all(|l| !l.exists()));
    // d's next poll notices and writes its full state again, self-contained,
    // before it edits anything.
    let repairs = d.dir.stats().repairs;
    d.pump();
    assert_eq!(d.dir.stats().repairs, repairs + 2);
    assert!(d_logs.iter().all(|l| l.is_file()));
    // Its later edits reach everyone.
    mark(&mut d, "d_back");
    let mut clients = vec![a, b, d];
    settle(&mut clients);
    assert!(has_mark(&clients[0].stage, "d_back"));
    // The new logs are fresh: nobody deletes them.
    assert_eq!(clients[1].dir.remove_departed_logs(), 0);
    let mut late = Client::open(&dir, "late", fast(), 9);
    late.pump();
    late.pump();
    assert_eq!(fingerprint(&late.stage), fingerprint(&clients[0].stage));
}

#[test]
fn owner_restarting_after_its_log_was_removed_loses_nothing() {
    let (_tmp, dir) = project_dir();
    let (mut a, b, d) = three_clients(&dir);
    drop(d);
    compact_now(&mut a);
    let d_logs = logs_of(&dir, "d");
    for log in &d_logs {
        age(log, 30);
    }
    assert_eq!(a.dir.remove_departed_logs(), 2);
    // d comes back from the saved project, which never had its edits: they come
    // from a's snapshot.
    let mut d = Client::open(&dir, "d", fast(), 33);
    d.pump();
    d.pump();
    assert!(has_mark(&d.stage, "d_mark"));
    mark(&mut d, "d_restarted");
    assert!(d_logs.iter().all(|l| l.is_file()));
    let mut clients = vec![a, b, d];
    settle(&mut clients);
    assert!(has_mark(&clients[0].stage, "d_restarted"));
    assert!(has_mark(&clients[1].stage, "d_restarted"));
}

#[test]
fn log_cleanup_can_be_turned_off() {
    let (_tmp, dir) = project_dir();
    let (mut a, b, d) = three_clients(&dir);
    drop((b, d));
    compact_now(&mut a);
    let d_logs = logs_of(&dir, "d");
    for log in &d_logs {
        age(log, 30);
    }
    let off = DirConfig {
        log_gc_after: None,
        ..fast()
    };
    let mut keeper = Client::open(&dir, "keeper", off, 5);
    keeper.pump();
    keeper.pump();
    assert_eq!(keeper.dir.remove_departed_logs(), 0);
    assert_eq!(keeper.dir.stats().logs_removed, 0);
    assert!(d_logs.iter().all(|l| l.is_file()));
    // The same folder with cleanup on.
    assert_eq!(a.dir.remove_departed_logs(), 2);
}

/// A log of ours that the first poll did not take over (the listing missed it,
/// or reading it failed for a moment) is read before anything is written to
/// it, not replaced: after §4.6 it can be the only copy of ops (here: our own
/// unsaved edits). It used to be renamed over with a new log.
#[test]
fn an_own_log_the_poll_missed_is_read_before_writing() {
    let (_tmp, dir) = project_dir();
    let mut d = Client::open(&dir, "d", fast(), 1);
    mark(&mut d, "d_unsaved");
    drop(d);
    // The restarted client's first poll does not see its logs.
    let logs = logs_of(&dir, "d");
    let hidden: Vec<PathBuf> = logs.iter().map(|l| l.with_extension("hidden")).collect();
    for (log, h) in logs.iter().zip(&hidden) {
        fs::rename(log, h).unwrap();
    }
    let mut d = Client::open(&dir, "d", fast(), 2);
    d.pump();
    assert!(!has_mark(&d.stage, "d_unsaved"));
    for (log, h) in logs.iter().zip(&hidden) {
        fs::rename(h, log).unwrap();
    }
    // An edit reaches the transport before the next poll.
    let root = DocKey::sublayer(&SublayerId::root());
    let before = d.stage.sublayer(&SublayerId::root()).unwrap().version();
    d.stage.set_edit_target(&SublayerId::root()).unwrap();
    d.stage
        .transact(&d.who, "edit", |tx| {
            tx.apply(Op::SetValue {
                path: p(&format!("{LAYERS}/title")),
                attr: "note".into(),
                value: Some(Value::String("after restart".into())),
            })
        })
        .unwrap();
    let layer = d.stage.sublayer(&SublayerId::root()).unwrap();
    d.dir
        .send(Message::Updates {
            doc: root,
            version: layer.version(),
            data: layer.export_updates(&before).unwrap(),
        })
        .unwrap();
    // Both the old records and the new one are in the folder.
    let mut late = Client::open(&dir, "late", fast(), 3);
    late.pump();
    late.pump();
    assert!(
        has_mark(&late.stage, "d_unsaved"),
        "the old records were lost"
    );
    let note = late
        .stage
        .prim(&p(&format!("{LAYERS}/title")))
        .and_then(|t| t.value("note").cloned());
    assert_eq!(note, Some(Value::String("after restart".into())));
    // And the restarted client got its old edits back from its log.
    d.pump();
    assert!(has_mark(&d.stage, "d_unsaved"));
}

/// One encoded log record (spec §4.2).
fn record(payload: &[u8]) -> Vec<u8> {
    let mut out = (payload.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&blake3::hash(payload).as_bytes()[..8]);
    out.extend_from_slice(payload);
    out
}

/// Write a log of `client`'s for the root sublayer in a newer version of the
/// format (e.g. the same client ID used by a newer BAE before, or the hub
/// downgraded). Returns its path and bytes.
fn newer_root_log(dir: &Path, client: &str) -> (PathBuf, Vec<u8>) {
    let log = dir.join(format!("updates/root/{client}.log"));
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    let header = format!(
        r#"{{"format":"bae.sync.log","version":99,"doc":"root","client":"{client}","generation":"g","more":1}}"#
    );
    let mut bytes = record(header.as_bytes());
    bytes.extend(record(b"a record of a newer kind"));
    fs::write(&log, &bytes).unwrap();
    (log, bytes)
}

/// Create the layer `name` in sublayer `id` only.
fn create_layer(stage: &mut Stage, who: &AuthorId, id: &SublayerId, name: &str) {
    stage.set_edit_target(id).unwrap();
    stage
        .transact(who, "create", |tx| {
            tx.apply(Op::CreatePrim {
                path: p(&format!("{LAYERS}/{name}")),
                specifier: Specifier::Def,
                prim_type: Some(PrimType::new("Layer")),
                index: None,
            })
        })
        .unwrap();
}

fn has_layer(stage: &Stage, id: &SublayerId, name: &str) -> bool {
    stage
        .sublayer(id)
        .unwrap()
        .has_spec(&p(&format!("{LAYERS}/{name}")))
}

/// Whether `e` is the refusal of `doc` because our log of it is newer.
fn refused(e: &SyncError, doc: &DocKey) -> bool {
    matches!(e, SyncError::NewerLogVersion { doc: d, version: 99, .. } if d == doc)
}

/// A log of ours written by a newer version of the format: neither renamed
/// aside nor replaced, since newer clients read it and it may hold ops nobody
/// else has. Writes to that document are refused, and only those: the error
/// names the document (the session skips it and keeps the link), the refused
/// data does not count as being in the folder, and other documents are
/// written as usual. It used to be renamed to `*.corrupt-*`, where nobody
/// reads it; then every write to it was an I/O error, which fails the link.
#[test]
fn an_own_log_from_a_newer_version_is_left_alone() {
    let (_tmp, dir) = project_dir();
    let (log, bytes) = newer_root_log(&dir, "d");
    let mut t = DirTransport::open(&dir, ClientId::new("d").unwrap(), fast()).unwrap();
    while t.recv().unwrap().is_some() {}
    let e = t.last_error().expect("the poll reports it").to_owned();
    assert!(e.contains("newer version"), "{e}");
    assert_eq!(fs::read(&log).unwrap(), bytes);

    let mut stage = Project::open(&dir).unwrap().stage;
    let who = author("user:d");
    // `(message, version)` of an edit of each sublayer: root, then anim/a.
    let mut sent = Vec::new();
    for id in layers() {
        let before = stage.sublayer(&id).unwrap().version();
        create_layer(&mut stage, &who, &id, "after");
        let layer = stage.sublayer(&id).unwrap();
        let msg = Message::Updates {
            doc: DocKey::sublayer(&id),
            version: layer.version(),
            data: layer.export_updates(&before).unwrap(),
        };
        sent.push((msg, layer.version()));
    }
    let [(root_msg, root_version), (anim_msg, anim_version)]: [(Message, VersionVector); 2] =
        sent.try_into().unwrap();
    let root = DocKey::sublayer(&SublayerId::root());
    let anim = DocKey::sublayer(&sl("anim/a"));
    for _ in 0..2 {
        let r = t.send(root_msg.clone());
        assert!(r.as_ref().is_err_and(|e| refused(e, &root)), "{r:?}");
    }
    assert_eq!(t.stats().records_refused, 2);
    assert!(t.last_error().unwrap().contains("newer version"));
    assert_eq!(fs::read(&log).unwrap(), bytes, "written over");
    let aside = fs::read_dir(log.parent().unwrap())
        .unwrap()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().contains("corrupt"));
    assert!(!aside);
    // The other document is written.
    t.send(anim_msg).unwrap();
    assert_eq!(t.stats().records_written, 1);
    assert!(dir.join("updates/anim/a/d.log").is_file());
    // The folder says it has the written data, not the refused data.
    t.send(Message::hello(ClientId::new("d").unwrap(), Vec::new()))
        .unwrap();
    let mut hello = None;
    while let Some(m) = t.recv().unwrap() {
        if let Message::Hello { versions, .. } = m {
            hello = Some(versions);
        }
    }
    let folder: std::collections::BTreeMap<DocKey, VersionVector> =
        hello.expect("the folder answers").into_iter().collect();
    assert!(folder[&anim].includes_vv(&anim_version));
    assert!(!folder[&root].includes_vv(&root_version));
}

/// A session on a folder where its own log of one document is from a newer
/// version: the pump does not fail, the refusal is reported once, that
/// document stays unsynced and is not sent again, and the other document's
/// edits keep being written. Once the newer log is moved away and the folder
/// reopened (`reset`), what was refused is written. Before, the first pump
/// after an edit of that document failed: for the hub, the whole project
/// directory was disconnected and reopened over and over.
#[test]
fn only_the_document_with_a_newer_own_log_is_not_written() {
    let (_tmp, dir) = project_dir();
    let (log, bytes) = newer_root_log(&dir, "d");
    let root = DocKey::sublayer(&SublayerId::root());
    let mut stage = Project::open(&dir).unwrap().stage;
    let mut s = session("d");
    let who = author("user:d");
    let mut folder = DirTransport::open(&dir, ClientId::new("d").unwrap(), fast()).unwrap();
    let mut errors = Vec::new();
    let mut pump = |stage: &mut Stage, s: &mut SyncSession, folder: &mut DirTransport| {
        for _ in 0..3 {
            let out = s.pump(stage, folder).expect("the link stays up");
            errors.extend(out.errors);
        }
    };
    pump(&mut stage, &mut s, &mut folder);
    for name in ["one", "two"] {
        for id in layers() {
            create_layer(&mut stage, &who, &id, name);
        }
        pump(&mut stage, &mut s, &mut folder);
        assert_eq!(s.unsynced(&stage), vec![root.clone()]);
    }
    let errors = std::mem::take(&mut errors);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(refused(&errors[0], &root), "{errors:?}");
    assert_eq!(folder.stats().records_refused, 1, "sent again");
    assert_eq!(fs::read(&log).unwrap(), bytes);
    let mut late = Client::open(&dir, "late", fast(), 1);
    late.pump();
    late.pump();
    let anim = sl("anim/a");
    assert!(has_layer(&late.stage, &anim, "one") && has_layer(&late.stage, &anim, "two"));
    assert!(!has_layer(&late.stage, &SublayerId::root(), "one"));
    drop(late);

    // Fixed, and the folder reopened: nothing was lost.
    fs::rename(&log, log.with_extension("newer")).unwrap();
    drop(folder);
    let mut folder = DirTransport::open(&dir, ClientId::new("d").unwrap(), fast()).unwrap();
    s.reset();
    let start = Instant::now();
    while !s.unsynced(&stage).is_empty() {
        let out = s.pump(&mut stage, &mut folder).expect("pump");
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        assert!(start.elapsed() < Duration::from_secs(60), "not written");
    }
    let mut late = Client::open(&dir, "late", fast(), 2);
    late.pump();
    late.pump();
    assert!(has_layer(&late.stage, &SublayerId::root(), "one"));
    assert!(has_layer(&late.stage, &SublayerId::root(), "two"));
}

/// The hub's case: the project directory is a relay peer behind a
/// `ThreadedTransport` (as in `services/bae-hub`), and the hub's own log of
/// one document is from a newer version (the hub was downgraded). The refusal
/// comes back from the background thread; only that document is skipped and
/// it is reported once. The directory stays connected and the other
/// document's edits keep being saved. Before, the refused write failed the
/// whole link: the relay disconnected the directory at the first edit of that
/// document (the hub then reopened it every 5 s and read everything again).
#[test]
fn a_newer_own_log_does_not_disconnect_the_relay_from_the_folder() {
    struct Alice {
        stage: Stage,
        session: SyncSession,
        link: MemoryTransport,
        who: AuthorId,
    }
    /// Rounds of alice and the relay, a simulated 50 ms each, until the relay
    /// has all of alice's edits and sent the folder (`store`) all but the
    /// root sublayer; panics if a connection drops. Returns the relay's
    /// per-document errors. Real time only bounds the wait for the folder's
    /// background thread.
    fn save(
        relay: &mut Relay,
        store: PeerHandle,
        a: &mut Alice,
        now: &mut Instant,
    ) -> Vec<SyncError> {
        let root = DocKey::sublayer(&SublayerId::root());
        let mut errors = Vec::new();
        let start = Instant::now();
        loop {
            *now += Duration::from_millis(50);
            let out = a
                .session
                .pump_at(&mut a.stage, &mut a.link, *now)
                .expect("alice's link");
            assert!(out.errors.is_empty(), "{:?}", out.errors);
            let out = relay.pump_at(*now);
            assert!(out.disconnected.is_empty(), "{:?}", out.disconnected);
            errors.extend(out.errors.into_iter().map(|(_, e)| e));
            if a.session.unsynced(&a.stage).is_empty()
                && relay.unsynced(store) == Some(vec![root.clone()])
            {
                return errors;
            }
            assert!(
                start.elapsed() < Duration::from_secs(120),
                "not saved: {:?}",
                relay.unsynced(store)
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    let (_tmp, dir) = project_dir();
    let (log, bytes) = newer_root_log(&dir, "hub");
    let hub = ClientId::new("hub").unwrap();
    let mut relay = Relay::new(hub.clone());
    let folder = DirTransport::open(&dir, hub, fast()).unwrap();
    let store = relay.connect(Box::new(ThreadedTransport::spawn(
        folder,
        Duration::from_millis(1),
        || {},
    )));
    let (link, hub_end) = MemoryTransport::pair();
    relay.connect(Box::new(hub_end));
    let mut alice = Alice {
        stage: Project::open(&dir).unwrap().stage,
        session: SyncSession::new(SessionConfig::new(ClientId::new("alice").unwrap())),
        link,
        who: author("user:alice"),
    };
    let mut now = Instant::now();
    let root = DocKey::sublayer(&SublayerId::root());
    let anim = sl("anim/a");
    for (round, name) in ["one", "two"].into_iter().enumerate() {
        for id in layers() {
            create_layer(&mut alice.stage, &alice.who, &id, name);
        }
        let errors = save(&mut relay, store, &mut alice, &mut now);
        if round == 0 {
            assert_eq!(errors.len(), 1, "{errors:?}");
            assert!(refused(&errors[0], &root), "{errors:?}");
        } else {
            assert!(errors.is_empty(), "reported again: {errors:?}");
        }
    }
    // Dropping the relay writes what is still queued for the folder.
    drop(relay);
    assert_eq!(fs::read(&log).unwrap(), bytes);
    let mut late = Client::open(&dir, "late", fast(), 1);
    late.pump();
    late.pump();
    assert!(has_layer(&late.stage, &anim, "one") && has_layer(&late.stage, &anim, "two"));
    assert!(!has_layer(&late.stage, &SublayerId::root(), "one"));
}

/// A departed client's log that cannot go (here: its ops are in nobody's
/// snapshot) is not read again on every cleanup pass while nothing changes;
/// a change to one document's logs makes only that document's logs be read.
#[test]
fn cleanup_reads_a_document_again_only_when_its_logs_change() {
    let (_tmp, dir) = project_dir();
    let (mut a, mut b, d) = three_clients(&dir);
    drop(d);
    let d_logs = logs_of(&dir, "d");
    for log in &d_logs {
        age(log, 30);
    }
    // No snapshot covers d's logs: nothing goes, every log is read once.
    assert_eq!(a.dir.remove_departed_logs(), 0);
    let first = a.dir.stats().logs_scanned;
    assert_eq!(first, 6, "three logs of two documents");
    assert_eq!(a.dir.remove_departed_logs(), 0);
    assert_eq!(a.dir.stats().logs_scanned, first, "read again");
    // b edits one document: only its three logs are read again.
    b.stage.set_edit_target(&SublayerId::root()).unwrap();
    b.stage
        .transact(&b.who, "edit", |tx| {
            tx.apply(Op::CreatePrim {
                path: p(&format!("{LAYERS}/b_root_only")),
                specifier: Specifier::Def,
                prim_type: Some(PrimType::new("Layer")),
                index: None,
            })
        })
        .unwrap();
    b.pump();
    assert_eq!(a.dir.remove_departed_logs(), 0);
    assert_eq!(a.dir.stats().logs_scanned, first + 3);
    // Once a snapshot covers them, they go.
    a.pump();
    compact_now(&mut a);
    assert_eq!(a.dir.remove_departed_logs(), 2);
    assert!(d_logs.iter().all(|l| !l.exists()));
}
