//! Shared-folder mode: several clients on one project directory (temporary
//! directories stand in for the NAS share).

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bae_model::project::Project;
use bae_model::{AuthorId, Op, Stage, SublayerId, Value};
use bae_sync::{
    ClientId, DirConfig, DirTransport, DocKey, MemoryTransport, SessionConfig, SyncError,
    SyncSession, ThreadedTransport, Transport,
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
