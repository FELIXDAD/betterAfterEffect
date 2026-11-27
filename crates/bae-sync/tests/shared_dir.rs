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
    ClientId, DirConfig, DirTransport, DocKey, DocSet, MemoryTransport, Message, PresenceState,
    SessionConfig, SyncError, SyncSession, ThreadedTransport, Transport,
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
    /// Whether random edits may create a name another client creates too.
    collide: bool,
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
            collide: true,
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
        random_edit_with(
            &mut self.stage,
            &self.who,
            &mut self.rng,
            &self.tag,
            &mut self.n,
            self.collide,
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

/// Our log grew while we are running, under our own header (another writer with
/// our ID, on a share that does not pass the lock on): the extra bytes are cut
/// off before the next append, which is not a failed write. The cut needs its
/// own handle on Windows, where an append handle may not change the file's
/// length.
#[test]
fn a_log_that_grew_under_our_header_is_cut_back_before_the_next_append() {
    let (_tmp, dir) = project_dir();
    let mut a = Client::open(&dir, "alice", fast(), 1);
    a.pump();
    set_opacity(&mut a, 1.0);
    a.pump();
    let log = a.dir.log_path(&DocKey::sublayer(&SublayerId::root()));
    let mut bytes = fs::read(&log).unwrap();
    bytes.extend_from_slice(&[200, 0, 0, 0, 1, 2, 3]);
    fs::write(&log, &bytes).unwrap();

    set_opacity(&mut a, 2.0);
    a.pump();
    assert_eq!((a.dir.stats().repairs, a.dir.stats().compactions), (0, 0));
    // Others read the new record (it is not stuck behind the cut-off bytes).
    let mut b = Client::open(&dir, "bob", fast(), 2);
    b.pump();
    b.pump();
    assert_eq!(fingerprint(&b.stage), fingerprint(&a.stage));
}

/// One small edit: `opacity` of the title layer in the root sublayer.
fn set_opacity(c: &mut Client, value: f64) {
    c.stage.set_edit_target(&SublayerId::root()).unwrap();
    c.stage
        .transact(&c.who, "edit", |tx| {
            tx.apply(Op::SetValue {
                path: p("/comps/main/layers/title"),
                attr: "opacity".into(),
                value: Some(Value::Float(value)),
            })
        })
        .unwrap();
}

/// One small edit in `layer`: a new layer `name`.
fn create_layer(c: &mut Client, layer: &SublayerId, name: &str) {
    c.stage.set_edit_target(layer).unwrap();
    c.stage
        .transact(&c.who, "edit", |tx| {
            tx.apply(Op::CreatePrim {
                path: p(&format!("{LAYERS}/{name}")),
                specifier: Specifier::Def,
                prim_type: Some(PrimType::new("Layer")),
                index: None,
            })
        })
        .unwrap();
}

/// The share stalls: writes to `log` fail (it is a directory for now) until
/// [`unstall`] puts the file back as it was.
fn stall(log: &Path) -> PathBuf {
    let aside = log.with_extension("aside");
    fs::rename(log, &aside).unwrap();
    fs::create_dir(log).unwrap();
    fs::write(log.join("x"), b"x").unwrap();
    aside
}

fn unstall(log: &Path, aside: &Path) {
    fs::remove_dir_all(log).unwrap();
    fs::rename(aside, log).unwrap();
}

/// A write to our own log fails, later ones would work again. Nothing may be
/// appended after the gap: a record's version covers everything before it, so
/// the folder's `Hello` would claim the lost edit and no session would ever send
/// it again (spec 4.3). The transport asks for a snapshot instead; if the session
/// goes away before answering (the hub dropping its store after the error), the
/// log claims only what it holds, and the next session on the folder writes the
/// rest.
#[test]
fn a_failed_append_leaves_no_gap_for_the_next_session() {
    let (_tmp, dir) = project_dir();
    let mut a = Client::open(&dir, "alice", fast(), 1);
    a.pump();
    set_opacity(&mut a, 1.0);
    a.pump();
    let log = a.dir.log_path(&DocKey::sublayer(&SublayerId::root()));
    let aside = stall(&log);
    set_opacity(&mut a, 2.0);
    assert!(a.session.pump(&mut a.stage, &mut a.dir).is_err());
    unstall(&log, &aside);
    // The next update was already on its way (`ThreadedTransport` queues it
    // before the session sees the error, or a final flush sends it).
    set_opacity(&mut a, 3.0);
    a.session.flush(&a.stage, &mut a.dir).unwrap();
    // Reattach: a new transport and session for the same documents.
    let Client {
        stage,
        session: old_session,
        dir: old_dir,
        who,
        rng,
        n,
        tag,
        collide,
    } = a;
    // Closing tells that the log still lacks edits.
    let mut old_dir = old_dir;
    assert!(old_dir.close().is_err());
    drop((old_session, old_dir));
    let mut a = Client {
        stage,
        session: session("alice"),
        dir: DirTransport::open(&dir, ClientId::new("alice").unwrap(), fast()).unwrap(),
        who,
        rng,
        n,
        tag,
        collide,
    };
    a.pump();
    a.pump();
    let mut b = Client::open(&dir, "bob", fast(), 2);
    b.pump();
    b.pump();
    assert_eq!(fingerprint(&b.stage), fingerprint(&a.stage));
}

/// As above, but the session stays: it answers the snapshot request and the log
/// is rewritten whole. A failed rewrite asks again.
#[test]
fn a_failed_append_is_repaired_with_a_snapshot() {
    let (_tmp, dir) = project_dir();
    let mut a = Client::open(&dir, "alice", fast(), 1);
    a.pump();
    set_opacity(&mut a, 1.0);
    a.pump();
    let log = a.dir.log_path(&DocKey::sublayer(&SublayerId::root()));
    let aside = stall(&log);
    set_opacity(&mut a, 2.0);
    assert!(a.session.pump(&mut a.stage, &mut a.dir).is_err());
    // Still stalled: the snapshot cannot be written either.
    set_opacity(&mut a, 3.0);
    assert!(a.session.pump(&mut a.stage, &mut a.dir).is_err());
    unstall(&log, &aside);
    set_opacity(&mut a, 4.0);
    // The snapshot is asked for again (with backoff), then the log is rewritten.
    let deadline = Instant::now() + Duration::from_secs(10);
    while a.dir.stats().compactions == 0 {
        assert!(Instant::now() < deadline, "the log was never rewritten");
        std::thread::sleep(Duration::from_millis(10));
        a.pump();
    }
    assert_eq!(a.dir.stats().compactions, 1);
    let mut b = Client::open(&dir, "bob", fast(), 2);
    b.pump();
    b.pump();
    assert_eq!(fingerprint(&b.stage), fingerprint(&a.stage));
    // Appending works again afterwards.
    set_opacity(&mut a, 5.0);
    a.pump();
    b.pump();
    assert_eq!(fingerprint(&b.stage), fingerprint(&a.stage));
    assert!(a.dir.close().is_ok());
}

/// A failed write to one document's log does not keep the session from
/// sending the others in the same pump: it already counts them as sent, so a
/// skipped one would leave a gap in its log that nothing repairs.
#[test]
fn a_failed_append_does_not_hold_back_other_documents() {
    let (_tmp, dir) = project_dir();
    let anim = sl("anim/a");
    let mut a = Client::open(&dir, "alice", fast(), 1);
    a.pump();
    create_layer(&mut a, &anim, "a1");
    set_opacity(&mut a, 1.0);
    a.pump();
    // `anim/a` is sent first.
    assert_eq!(a.stage.doc_keys()[0], DocKey::sublayer(&anim));
    let log = a.dir.log_path(&DocKey::sublayer(&anim));
    let aside = stall(&log);
    create_layer(&mut a, &anim, "a2");
    set_opacity(&mut a, 2.0);
    assert!(a.session.pump(&mut a.stage, &mut a.dir).is_err());
    unstall(&log, &aside);
    set_opacity(&mut a, 3.0);
    a.pump();
    a.pump();
    let mut b = Client::open(&dir, "bob", fast(), 2);
    b.pump();
    b.pump();
    assert_eq!(fingerprint(&b.stage), fingerprint(&a.stage));
}

/// Snapshot requests are answered after the pump's import. One whose answer
/// fails (its log cannot be written) does not cost the others theirs: the
/// folder asks once per compaction, so a dropped request would stop that log
/// from being compacted.
#[test]
fn a_failed_snapshot_answer_does_not_hold_back_other_documents() {
    let (_tmp, dir) = project_dir();
    let anim = sl("anim/a");
    let config = DirConfig {
        compact_min_bytes: 1,
        compact_ratio: 1,
        ..fast()
    };
    let mut a = Client::open(&dir, "alice", config, 1);
    a.pump();
    // The first write starts each log, the second asks for a snapshot of each.
    for i in 0..2 {
        create_layer(&mut a, &anim, &format!("a{i}"));
        set_opacity(&mut a, f64::from(i));
        a.pump();
    }
    assert_eq!(a.dir.stats().compactions, 0);
    // `anim/a` is asked first; its log cannot be written.
    let log = a.dir.log_path(&DocKey::sublayer(&anim));
    let aside = stall(&log);
    assert!(a.session.pump(&mut a.stage, &mut a.dir).is_err());
    assert_eq!(a.dir.stats().compactions, 1, "root was not compacted");
    unstall(&log, &aside);
    // The rest catches up: `anim/a` is asked again, everything is compacted.
    let deadline = Instant::now() + Duration::from_secs(10);
    while a.dir.stats().compactions < 3 {
        assert!(Instant::now() < deadline, "{:?}", a.dir.stats());
        std::thread::sleep(Duration::from_millis(10));
        set_opacity(&mut a, 9.0);
        a.pump();
    }
    assert!(a.dir.close().is_ok());
    let mut b = Client::open(&dir, "bob", fast(), 2);
    b.pump();
    b.pump();
    assert_eq!(fingerprint(&b.stage), fingerprint(&a.stage));
}

/// The first scan can miss our own log (a listing that failed on a network
/// hiccup). An edit before the next poll must not replace it unread: it holds
/// edits nobody saved.
#[test]
fn an_own_log_the_first_scan_missed_is_not_replaced() {
    let (tmp, dir) = project_dir();
    let root = SublayerId::root();
    let mut a = Client::open(&dir, "alice", fast(), 1);
    a.pump();
    create_layer(&mut a, &root, "unsaved1");
    a.pump();
    drop(a);
    // alice restarts from the saved project; her first scan misses the log.
    let doc_dir = dir.join("updates/root");
    let aside = tmp.path().join("aside");
    fs::rename(&doc_dir, &aside).unwrap();
    let slow = DirConfig {
        poll_interval: Duration::from_secs(3600),
        ..fast()
    };
    let mut a = Client::open(&dir, "alice", slow, 2);
    a.pump();
    fs::rename(&aside, &doc_dir).unwrap();
    create_layer(&mut a, &root, "unsaved2");
    a.pump();
    a.pump();
    drop(a);
    let mut n = Client::open(&dir, "n", fast(), 3);
    n.pump();
    n.pump();
    let layer = n.stage.sublayer(&root).unwrap();
    for name in ["unsaved1", "unsaved2"] {
        assert!(layer.has_spec(&p(&format!("{LAYERS}/{name}"))), "{name}");
    }
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

// ---- cleanup of departed clients (spec §4.7) ----

/// Clients idle for an hour count as departed; a cleanup pass on every poll,
/// done in one go (no time budget: a slow machine must not split a pass the
/// tests expect to finish); no taking over of logs nobody holds (tests that
/// want it turn it on).
fn cleaner() -> DirConfig {
    DirConfig {
        inactive_after: Some(Duration::from_secs(3600)),
        cleanup_interval: Duration::ZERO,
        absorb_departed: false,
        cleanup_budget: Duration::MAX,
        ..fast()
    }
}

const DOCS: [&str; 2] = ["root", "anim/a"];

fn layers() -> [SublayerId; 2] {
    [SublayerId::root(), sl("anim/a")]
}

fn log(dir: &Path, doc: &str, client: &str) -> PathBuf {
    dir.join(format!("updates/{doc}/{client}.log"))
}

fn has_logs(dir: &Path, client: &str) -> [bool; 2] {
    DOCS.map(|doc| log(dir, doc, client).is_file())
}

fn set_mtime(path: &Path, t: SystemTime) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

/// Make `path` look untouched for two hours.
fn age(path: &Path) {
    set_mtime(path, SystemTime::now() - Duration::from_secs(2 * 3600));
}

/// Make the logs `client` left (and its presence file, with `presence`) look two
/// hours old.
fn age_client(dir: &Path, client: &str, presence: bool) {
    for doc in DOCS {
        let l = log(dir, doc, client);
        if l.exists() {
            age(&l);
        }
    }
    let p = dir.join(format!("presence/{client}.presence"));
    if presence && p.exists() {
        age(&p);
    }
}

/// Edit and pump everyone, then settle.
fn work(clients: &mut [Client], rounds: usize) {
    for _ in 0..rounds {
        for c in clients.iter_mut() {
            c.edit();
            c.pump();
        }
    }
    settle(clients);
}

/// One edit in each sublayer.
fn edit_each(c: &mut Client) {
    for id in layers() {
        c.n += 1;
        let value = Value::String(format!("{} {}", c.tag, c.n));
        c.stage.set_edit_target(&id).unwrap();
        c.stage
            .transact(&c.who, "note", |tx| {
                tx.apply(Op::SetValue {
                    path: p("/comps/main"),
                    attr: "note".into(),
                    value: Some(value),
                })
            })
            .unwrap();
    }
}

/// Compact `c`'s logs now, as its transport does when they grow: each becomes
/// one snapshot of `c`'s whole document.
fn compact(c: &mut Client) {
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

/// What must be identical on converged clients: the project, and every op.
fn state(stage: &Stage) -> (String, Vec<VersionVector>) {
    let versions = layers()
        .iter()
        .map(|id| stage.sublayer(id).unwrap().version())
        .collect();
    (fingerprint(stage), versions)
}

/// A client joining now sees exactly `expected`, down to every op.
fn newcomer_sees(dir: &Path, id: &str, expected: &(String, Vec<VersionVector>)) {
    let mut n = Client::open(dir, id, fast(), 77);
    n.pump();
    n.pump();
    assert!(&state(&n.stage) == expected, "{id} joining now");
}

/// A record as the logs frame it (`len`, BLAKE3 prefix, payload).
fn frame(payload: &[u8]) -> Vec<u8> {
    let mut out = (payload.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&blake3::hash(payload).as_bytes()[..8]);
    out.extend_from_slice(payload);
    out
}

fn append(path: &Path, bytes: &[u8]) {
    use std::io::Write;
    fs::File::options()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}

/// `x` and `a` edit together; `x` leaves (its edits only in its logs: nobody
/// saved the project); `c` joins, edits and compacts, so its logs hold
/// everything; `a` cleans up. Returns `a`, `c` and the project they share.
fn remove_departed_x(dir: &Path) -> (Client, Client, (String, Vec<VersionVector>)) {
    let mut clients = vec![
        Client::open(dir, "x", fast(), 1),
        Client::open(dir, "a", cleaner(), 2),
    ];
    work(&mut clients, 30);
    let mut a = clients.pop().unwrap();
    drop(clients);
    let mut c = Client::open(dir, "c", fast(), 3);
    c.pump();
    c.pump();
    edit_each(&mut c);
    c.pump();
    compact(&mut c);
    let mut both = vec![a, c];
    settle(&mut both);
    let c = both.pop().unwrap();
    a = both.pop().unwrap();
    // Not yet: x was active a moment ago.
    a.pump();
    assert_eq!(has_logs(dir, "x"), [true, true]);
    assert!(a.dir.stats().cleanups > 0);

    age_client(dir, "x", true);
    a.pump();
    assert_eq!(has_logs(dir, "x"), [false, false]);
    assert_eq!(a.dir.stats().logs_removed, 2);
    let expected = state(&a.stage);
    assert_eq!(state(&c.stage), expected);
    (a, c, expected)
}

#[test]
fn a_departed_clients_logs_are_removed_once_another_log_holds_them() {
    let (_tmp, dir) = project_dir();
    let (mut a, c, expected) = remove_departed_x(&dir);
    // Nothing else was touched, and nothing went wrong.
    assert_eq!(has_logs(&dir, "a"), [true, true]);
    assert_eq!(has_logs(&dir, "c"), [true, true]);
    a.pump();
    assert_eq!(a.dir.stats().logs_removed, 2);
    // x's edits are still in the folder (in c's snapshots): a newcomer, starting
    // from the stale saved project, gets the whole project.
    drop((a, c));
    newcomer_sees(&dir, "n", &expected);
}

#[test]
fn logs_that_no_other_log_holds_are_kept() {
    let (_tmp, dir) = project_dir();
    let mut clients = vec![
        Client::open(&dir, "x", fast(), 1),
        Client::open(&dir, "a", cleaner(), 2),
    ];
    work(&mut clients, 20);
    // c's snapshots hold what x wrote so far, but x goes on editing.
    let mut c = Client::open(&dir, "c", fast(), 3);
    c.pump();
    c.pump();
    edit_each(&mut c);
    c.pump();
    compact(&mut c);
    clients.push(c);
    for _ in 0..10 {
        clients[0].edit();
        clients[0].pump();
    }
    edit_each(&mut clients[0]);
    settle(&mut clients);
    let expected = state(&clients[0].stage);
    let mut c = clients.pop().unwrap();
    let mut a = clients.pop().unwrap();
    drop(clients);
    age_client(&dir, "x", true);

    // 1. No log holds all of x's: a's are deltas, c's snapshots are too old.
    for _ in 0..3 {
        a.pump();
    }
    assert_eq!(has_logs(&dir, "x"), [true, true]);

    // 2. Now c's do, but x's logs end in records we cannot read: a record that
    // is not Loro data (its version is empty, so readers skip it) and a torn one.
    c.pump();
    compact(&mut c);
    let lens = DOCS.map(|doc| fs::metadata(log(&dir, doc, "x")).unwrap().len());
    let junk = Message::Updates {
        doc: DocKey::sublayer(&SublayerId::root()),
        version: Default::default(),
        data: b"not a Loro blob".to_vec(),
    };
    append(&log(&dir, "root", "x"), &frame(&junk.encode()));
    let torn = frame(&junk.encode());
    append(&log(&dir, "anim/a", "x"), &torn[..torn.len() / 2]);
    age_client(&dir, "x", true);
    for _ in 0..3 {
        a.pump();
    }
    assert_eq!(has_logs(&dir, "x"), [true, true]);
    assert_eq!(a.dir.stats().logs_removed, 0);

    // 3. Without them, c's snapshots are enough.
    for (doc, len) in DOCS.iter().zip(lens) {
        let l = log(&dir, doc, "x");
        fs::File::options()
            .write(true)
            .open(&l)
            .unwrap()
            .set_len(len)
            .unwrap();
        age(&l);
    }
    a.pump();
    assert_eq!(has_logs(&dir, "x"), [false, false]);
    assert_eq!(a.dir.stats().logs_removed, 2);
    assert_eq!(state(&a.stage), expected);
    drop((a, c));
    newcomer_sees(&dir, "n", &expected);
}

#[test]
fn logs_of_clients_that_may_still_be_running_are_kept() {
    let (_tmp, dir) = project_dir();
    let mut clients = vec![
        Client::open(&dir, "x", fast(), 1),
        Client::open(&dir, "a", cleaner(), 2),
        Client::open(&dir, "c", fast(), 3),
    ];
    work(&mut clients, 20);
    // c's logs hold everything x wrote.
    edit_each(&mut clients[2]);
    clients[2].pump();
    compact(&mut clients[2]);
    let c = clients.pop().unwrap();
    let mut a = clients.pop().unwrap();
    let mut x = clients.pop().unwrap();
    let keep = |a: &mut Client| {
        for _ in 0..3 {
            a.pump();
        }
        assert_eq!(has_logs(&dir, "x"), [true, true]);
        assert_eq!(a.dir.stats().logs_removed, 0);
    };

    // 1. x still runs (holds its lock) but has been quiet for hours.
    age_client(&dir, "x", true);
    keep(&mut a);

    // 2. x quit a moment ago: its heartbeat file is fresh.
    x.session.set_presence(PresenceState {
        name: "x".into(),
        ..PresenceState::default()
    });
    x.pump();
    drop(x);
    assert!(dir.join("presence/x.presence").is_file());
    age_client(&dir, "x", false);
    keep(&mut a);

    // 3. One of x's logs was written a moment ago: that keeps the other too.
    age_client(&dir, "x", true);
    set_mtime(&log(&dir, "root", "x"), SystemTime::now());
    keep(&mut a);

    // 3b. A sign of life spelled `X`: on case-insensitive file systems (macOS,
    // Windows, SMB shares) that is x's own presence file as a listing may spell
    // it, so the cleanup ignores case everywhere.
    age_client(&dir, "x", true);
    let other_case = dir.join("presence/X.presence");
    fs::write(&other_case, b"").unwrap();
    keep(&mut a);
    fs::remove_file(&other_case).unwrap();

    // 4. All quiet: gone.
    age_client(&dir, "x", true);
    a.pump();
    assert_eq!(has_logs(&dir, "x"), [false, false]);
    assert_eq!(a.dir.stats().logs_removed, 2);
    let expected = state(&c.stage);
    assert_eq!(state(&a.stage), expected);
    drop((a, c));
    newcomer_sees(&dir, "n", &expected);
}

#[test]
fn of_two_logs_holding_the_same_only_one_is_removed() {
    let (_tmp, dir) = project_dir();
    let mut w = Client::open(&dir, "w", fast(), 1);
    for _ in 0..20 {
        w.edit();
        w.pump();
    }
    edit_each(&mut w);
    w.pump();
    let expected = state(&w.stage);
    // x and y both compacted to the same state, then left; so did w.
    for id in ["x", "y"] {
        let mut t = DirTransport::open(&dir, ClientId::new(id).unwrap(), fast()).unwrap();
        for layer in layers() {
            let l = w.stage.sublayer(&layer).unwrap();
            t.send(Message::Snapshot {
                doc: DocKey::sublayer(&layer),
                version: l.version(),
                data: l.export_snapshot().unwrap(),
            })
            .unwrap();
        }
    }
    drop(w);
    for id in ["w", "x", "y"] {
        age_client(&dir, id, true);
    }
    // Two cleaners, one after the other (whichever goes first removes them
    // all; the unit test `two_cleaners_at_once_remove_only_one_of_two_equal_logs`
    // in dir.rs interleaves two passes): they pick the same one.
    let mut cleaners = vec![
        Client::open(&dir, "a", cleaner(), 2),
        Client::open(&dir, "b", cleaner(), 3),
    ];
    for _ in 0..5 {
        for c in &mut cleaners {
            c.pump();
        }
    }
    // w's deltas go (held by either snapshot), and of x and y only x: the one
    // with the smaller ID.
    assert_eq!(has_logs(&dir, "w"), [false, false]);
    assert_eq!(has_logs(&dir, "x"), [false, false]);
    assert_eq!(has_logs(&dir, "y"), [true, true]);
    let removed: u64 = cleaners.iter().map(|c| c.dir.stats().logs_removed).sum();
    assert_eq!(removed, 4);
    for c in &cleaners {
        assert_eq!(state(&c.stage), expected);
    }
    drop(cleaners);
    newcomer_sees(&dir, "n", &expected);
}

#[test]
fn a_client_whose_logs_were_removed_comes_back() {
    let (_tmp, dir) = project_dir();
    let (a, c, expected) = remove_departed_x(&dir);
    // x returns, from the stale saved project: its old edits come from c's logs.
    let mut x = Client::open(&dir, "x", fast(), 9);
    x.pump();
    x.pump();
    assert_eq!(state(&x.stage), expected);
    let mut clients = vec![a, c, x];
    work(&mut clients, 20);
    let x = &clients[2];
    assert_eq!(has_logs(&dir, "x"), [true, true]);
    assert_eq!(x.dir.stats().repairs, 0, "x simply starts new logs");
    let expected = state(&x.stage);
    drop(clients);
    newcomer_sees(&dir, "n", &expected);
}

#[test]
fn a_running_client_repairs_its_log_deleted_underneath() {
    let (_tmp, dir) = project_dir();
    let mut clients = vec![
        Client::open(&dir, "x", fast(), 1),
        Client::open(&dir, "a", fast(), 2),
    ];
    work(&mut clients, 20);
    // A cleanup pass on a file system without locks took x for gone. x's edits
    // were only in these logs (nobody saved the project).
    for doc in DOCS {
        fs::remove_file(log(&dir, doc, "x")).unwrap();
    }
    // x's next append notices, rewrites its log and adds a full snapshot.
    edit_each(&mut clients[0]);
    clients[0].pump();
    assert_eq!(clients[0].dir.stats().repairs, 2);
    work(&mut clients, 10);
    assert_eq!(has_logs(&dir, "x"), [true, true]);
    let expected = state(&clients[0].stage);
    drop(clients);
    newcomer_sees(&dir, "n", &expected);
}

/// As above, but the session goes away before answering the request for a full
/// snapshot (the hub drops its store after an error) and a new one attaches to
/// the same documents. The record that noticed the deletion was not written on
/// its own: a log holding only it would claim all of x's ops, so the new
/// session would send none of the older ones and newcomers could not import it.
#[test]
fn a_repair_whose_snapshot_never_comes_claims_nothing() {
    let (_tmp, dir) = project_dir();
    let mut x = Client::open(&dir, "x", fast(), 1);
    x.pump();
    for _ in 0..10 {
        x.edit();
        x.pump();
    }
    for doc in DOCS {
        fs::remove_file(log(&dir, doc, "x")).unwrap();
    }
    edit_each(&mut x);
    x.pump();
    assert_eq!(x.dir.stats().repairs, 2);
    let Client {
        stage,
        session: _,
        dir: old,
        who,
        rng,
        n,
        tag,
        collide,
    } = x;
    drop(old);
    let mut x = Client {
        stage,
        session: session("x"),
        dir: DirTransport::open(&dir, ClientId::new("x").unwrap(), fast()).unwrap(),
        who,
        rng,
        n,
        tag,
        collide,
    };
    x.pump();
    x.pump();
    let expected = state(&x.stage);
    drop(x);
    newcomer_sees(&dir, "n", &expected);
}

#[test]
fn a_departed_log_nobody_holds_is_taken_over() {
    let (_tmp, dir) = project_dir();
    let config = DirConfig {
        absorb_departed: true,
        ..cleaner()
    };
    let mut clients = vec![
        Client::open(&dir, "x", fast(), 1),
        Client::open(&dir, "a", config, 2),
    ];
    work(&mut clients, 20);
    let expected = state(&clients[0].stage);
    let mut a = clients.pop().unwrap();
    drop(clients);
    age_client(&dir, "x", true);
    // No log holds x's (a's are deltas): a asks its session for snapshots, its
    // logs become snapshots holding x's, and then x's go.
    for _ in 0..4 {
        a.pump();
    }
    assert_eq!(has_logs(&dir, "x"), [false, false]);
    let stats = a.dir.stats().clone();
    assert_eq!(stats.absorb_requests, 2, "{stats:?}");
    assert_eq!(stats.logs_removed, 2, "{stats:?}");
    // Nothing to take over any more: no further requests.
    for _ in 0..3 {
        a.pump();
    }
    assert_eq!(a.dir.stats().absorb_requests, 2);
    assert_eq!(state(&a.stage), expected);
    drop(a);
    newcomer_sees(&dir, "n", &expected);
}

/// A client's own log can be all that holds some ops: edits nobody saved, or a
/// departed client's log that was deleted because this one held it. A client
/// that restarts from the saved project and at once takes a departed log over
/// (its first cleanup pass asks its session for a snapshot) must not replace
/// its log with a snapshot made before the session read that log back.
#[test]
fn a_restarted_client_keeps_what_only_its_own_log_holds() {
    let (_tmp, dir) = project_dir();
    let absorb = || DirConfig {
        absorb_departed: true,
        ..cleaner()
    };
    let mut clients = vec![
        Client::open(&dir, "x", fast(), 1),
        Client::open(&dir, "a", absorb(), 2),
    ];
    work(&mut clients, 20);
    let mut a = clients.pop().unwrap();
    drop(clients);
    // a goes on alone, then quits; nobody saves the project.
    for _ in 0..10 {
        a.edit();
        a.pump();
    }
    edit_each(&mut a);
    a.pump();
    let expected = state(&a.stage);
    drop(a);

    // 1. a comes back while x looks long gone and no log holds x's: its first
    // pump scans the folder, runs a cleanup pass and asks for a snapshot.
    age_client(&dir, "x", true);
    let mut a = Client::open(&dir, "a", absorb(), 3);
    for _ in 0..4 {
        a.pump();
    }
    assert_eq!(state(&a.stage), expected);
    newcomer_sees(&dir, "n1", &expected);
    let stats = a.dir.stats().clone();
    assert_eq!(has_logs(&dir, "x"), [false, false], "{stats:?}");
    assert_eq!(stats.snapshots_appended, 0, "{stats:?}");

    // 2. Now a's logs are all that hold x's edits. y joins, edits and leaves; a
    // quits and comes back once more, and takes y's logs over too.
    let mut both = vec![a, Client::open(&dir, "y", fast(), 4)];
    work(&mut both, 10);
    edit_each(&mut both[1]);
    settle(&mut both);
    let expected = state(&both[0].stage);
    drop(both);
    age_client(&dir, "y", true);
    let mut a = Client::open(&dir, "a", absorb(), 5);
    for _ in 0..4 {
        a.pump();
    }
    assert_eq!(state(&a.stage), expected);
    drop(a);
    newcomer_sees(&dir, "n2", &expected);
    assert_eq!(has_logs(&dir, "y"), [false, false]);
}

#[test]
fn leftover_temporary_and_lock_files_are_removed() {
    let (_tmp, dir) = project_dir();
    let mut a = Client::open(&dir, "a", cleaner(), 1);
    a.pump();
    // Temporary files of crashed writers, and one being written right now.
    let stale = [
        dir.join("updates/root/ghost.tmp-1"),
        dir.join("updates/anim/a/a.tmp-2"),
        dir.join("presence/ghost.tmp-3"),
    ];
    let fresh = dir.join("updates/root/busy.tmp-4");
    for f in stale.iter().chain([&fresh]) {
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, b"partial").unwrap();
    }
    for f in &stale {
        age(f);
    }
    // Lock files: of a departed client without logs (goes), of one that still
    // has a log (stays with it; this one cannot even be read as Loro data), and
    // of one still running (stays). Ours stays too, however old.
    let gone = DirTransport::open(&dir, ClientId::new("g").unwrap(), fast()).unwrap();
    drop(gone);
    let mut h = DirTransport::open(&dir, ClientId::new("h").unwrap(), fast()).unwrap();
    h.send(Message::Updates {
        doc: DocKey::sublayer(&SublayerId::root()),
        version: Default::default(),
        data: Vec::new(),
    })
    .unwrap();
    drop(h);
    let running = DirTransport::open(&dir, ClientId::new("r").unwrap(), fast()).unwrap();
    for id in ["g", "h", "r", "a"] {
        age(&dir.join(format!("updates/{id}.lock")));
    }
    age(&log(&dir, "root", "h"));
    a.pump();
    a.pump();
    for f in &stale {
        assert!(!f.exists(), "{}", f.display());
    }
    assert!(fresh.exists());
    assert!(!dir.join("updates/g.lock").exists());
    for id in ["h", "r", "a"] {
        assert!(dir.join(format!("updates/{id}.lock")).exists(), "{id}");
    }
    assert!(log(&dir, "root", "h").exists());
    assert_eq!(a.dir.stats().stale_files_removed, 4);
    assert_eq!(a.dir.stats().logs_removed, 0);
    // g can come back.
    drop(running);
    DirTransport::open(&dir, ClientId::new("g").unwrap(), fast()).unwrap();
}

/// Deleting other clients' logs is off unless asked for: the app turns it on
/// once it has been tried on real NAS shares (spec 4.8), the hub does for its
/// own directories.
#[test]
fn the_cleanup_is_off_unless_asked_for() {
    assert_eq!(DirConfig::default().inactive_after, None);
    let (_tmp, dir) = project_dir();
    let mut clients = vec![
        Client::open(&dir, "x", fast(), 1),
        Client::open(&dir, "c", fast(), 2),
    ];
    work(&mut clients, 10);
    // c's logs hold everything x wrote; x left days ago.
    edit_each(&mut clients[1]);
    clients[1].pump();
    compact(&mut clients[1]);
    drop(clients);
    let days_ago = SystemTime::now() - 2 * DirConfig::RECOMMENDED_INACTIVE_AFTER;
    for doc in DOCS {
        set_mtime(&log(&dir, doc, "x"), days_ago);
    }
    let often = DirConfig {
        cleanup_interval: Duration::ZERO,
        ..fast()
    };
    let mut a = Client::open(&dir, "a", often.clone(), 3);
    for _ in 0..3 {
        a.pump();
    }
    assert_eq!(has_logs(&dir, "x"), [true, true]);
    assert_eq!(a.dir.stats().cleanups, 0);
    drop(a);
    // Asked for with the recommended setting: they go.
    let config = DirConfig {
        inactive_after: Some(DirConfig::RECOMMENDED_INACTIVE_AFTER),
        cleanup_budget: Duration::MAX,
        ..often
    };
    let mut a = Client::open(&dir, "a", config, 4);
    for _ in 0..3 {
        a.pump();
    }
    assert_eq!(has_logs(&dir, "x"), [false, false]);
}

/// Clients join, edit, compact and leave in waves while every live one cleans
/// up: whatever got deleted, a newcomer still gets every op. (No two clients
/// create the same name here: how the mirror names such twins can depend on
/// import order, which is the doc model's business, not the folder's.)
//
// TODO(#737): back to colliding names (`collide: true`) once the doc-model
// issue reported on #737 is fixed: prims of the same name created
// concurrently end up named differently in the mirrors (`to_trees`) of two
// clients whose Loro version vectors are identical (`shared` on one,
// `shared_<peer>_<counter>` on the other), in about one run in five.
#[test]
fn clients_coming_and_going_lose_nothing() {
    let (_tmp, dir) = project_dir();
    let mut rng = Rng::new(31);
    let mut live: Vec<Client> = Vec::new();
    let mut departed: Vec<String> = Vec::new();
    let mut removed = 0;
    let mut next = 0u64;
    for _wave in 0..8 {
        // Some join (every live client also cleans up; some take logs over),
        // everyone edits, some compact, one or two leave and look long gone.
        while live.len() < 3 {
            let config = DirConfig {
                absorb_departed: next == 0 || rng.chance(50),
                ..cleaner()
            };
            let mut c = Client::open(&dir, &format!("k{next}"), config, 100 + next);
            // TODO(#737): see above.
            c.collide = false;
            live.push(c);
            next += 1;
        }
        work(&mut live, 8);
        for c in &mut live {
            if rng.chance(40) {
                compact(c);
            }
        }
        settle(&mut live);
        for _ in 0..1 + rng.below(2) {
            let c = live.remove(rng.below(live.len()));
            removed += c.dir.stats().logs_removed;
            departed.push(c.tag.clone());
        }
        for id in &departed {
            age_client(&dir, id, true);
        }
        for _ in 0..3 {
            for c in &mut live {
                c.pump();
            }
        }
    }
    // Departed logs nobody holds may remain (no live client took them over, or
    // a live one holds exactly the same and has a smaller ID). A client that
    // takes logs over and edits every document clears them all.
    let mut last = Client::open(
        &dir,
        "z",
        DirConfig {
            absorb_departed: true,
            ..cleaner()
        },
        7,
    );
    // TODO(#737): see above.
    last.collide = false;
    last.pump();
    last.pump();
    edit_each(&mut last);
    live.push(last);
    work(&mut live, 2);
    for _ in 0..3 {
        for c in &mut live {
            c.pump();
        }
    }
    let expected = state(&live[0].stage);
    removed += live.iter().map(|c| c.dir.stats().logs_removed).sum::<u64>();
    let left: usize = departed
        .iter()
        .map(|id| has_logs(&dir, id).iter().filter(|&&b| b).count())
        .sum();
    eprintln!(
        "{} departed clients: {removed} of their logs removed, {left} left",
        departed.len()
    );
    drop(live);
    newcomer_sees(&dir, "n", &expected);
    assert_eq!(left, 0);
    assert!(removed > 0);
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

/// While the folder keeps failing, the session is not asked for a snapshot on
/// every pump: it exports a whole snapshot for each request, on the thread that
/// owns the stage.
#[test]
fn a_stalled_log_is_not_snapshotted_on_every_pump() {
    let (_tmp, dir) = project_dir();
    let mut a = Client::open(&dir, "alice", fast(), 1);
    a.pump();
    set_opacity(&mut a, 1.0);
    a.pump();
    let log = a.dir.log_path(&DocKey::sublayer(&SublayerId::root()));
    let aside = stall(&log);
    set_opacity(&mut a, 2.0);
    assert!(a.session.pump(&mut a.stage, &mut a.dir).is_err());
    // No edits from here on: everything the session tries to send is a snapshot
    // answer (failed sends are not counted as messages, but their bytes are).
    let root = DocKey::sublayer(&SublayerId::root());
    let snapshot_len = DocSet::export_snapshot(&a.stage, &root).unwrap().len() as u64;
    let before = a.session.stats().bytes_sent;
    let mut pumps = 0;
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(800) {
        let _ = a.session.pump(&mut a.stage, &mut a.dir);
        pumps += 1;
        std::thread::sleep(Duration::from_millis(1));
    }
    // At once, then after 100, 200 and 400 ms (the poll interval is zero here,
    // so the backoff starts at 100 ms and doubles).
    let snapshots = (a.session.stats().bytes_sent - before) / snapshot_len;
    assert!(pumps > 80, "only {pumps} pumps");
    assert!(
        (3..=5).contains(&snapshots),
        "{snapshots} snapshots in {pumps} pumps over 0.8 s"
    );
    unstall(&log, &aside);
    let deadline = Instant::now() + Duration::from_secs(10);
    while a.dir.stats().compactions == 0 {
        assert!(Instant::now() < deadline, "the log was never rewritten");
        std::thread::sleep(Duration::from_millis(10));
        let _ = a.session.pump(&mut a.stage, &mut a.dir);
    }
    let mut b = Client::open(&dir, "bob", fast(), 2);
    b.pump();
    b.pump();
    assert_eq!(fingerprint(&b.stage), fingerprint(&a.stage));
}
