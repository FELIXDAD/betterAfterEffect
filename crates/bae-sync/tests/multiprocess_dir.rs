//! Shared-folder mode with every client in its own OS process (spec 4.6):
//! real file handles and locks held by different processes, processes killed
//! while they edit and write (their locks released only by the OS) and
//! restarted with the same `ClientId`. A kill lands in the middle of a write
//! only by chance, so what that leaves (a torn append, a half-written
//! compaction) is put in place by the test after a kill.
//!
//! The clients are this test binary started again (`common/procs.rs`): the
//! `mp_child` entry runs a client when `BAE_MP_ROLE` is set and does nothing
//! otherwise. Each client opens the saved project, attaches a `DirTransport`
//! (in a `ThreadedTransport`, as the app does) and edits when told to.
//!
//! The in-process tests in `shared_dir.rs` cover the same folder with clients
//! as threads; message loss, duplication and reordering are covered by the
//! simulated network in `convergence.rs`.

mod common;
#[path = "common/procs.rs"]
mod procs;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bae_model::project::Project;
use bae_model::{AuthorId, Op, Stage, SublayerId, Value};
use bae_sync::{
    ClientId, DirConfig, DirTransport, Message, SessionConfig, SyncError, SyncSession,
    ThreadedTransport, Transport,
};
use common::*;
use procs::*;

/// The test that runs the clients (see the module docs).
const ENTRY: &str = "mp_child";

/// Logs are compacted early so that compaction happens many times in a short
/// run, across restarts too.
const COMPACT_MIN: u64 = 16 * 1024;
const COMPACT_RATIO: u64 = 2;

/// Generous: debug builds on a busy CI machine.
const START: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(30);
const SETTLE: Duration = Duration::from_secs(90);

// ---- the client process ----

#[test]
fn mp_child() {
    match role().as_deref() {
        None => {}
        Some("dir") => dir_client(),
        Some(other) => panic!("unknown role {other}"),
    }
}

fn dir_config() -> DirConfig {
    DirConfig {
        poll_interval: Duration::from_millis(20),
        compact_min_bytes: COMPACT_MIN,
        compact_ratio: COMPACT_RATIO,
        // Polling only: the same on every platform.
        watch: false,
        ..DirConfig::default()
    }
}

/// Counts the snapshots the session hands the folder: each one rewrites (compacts)
/// this client's log for that document.
struct Counted {
    inner: DirTransport,
    compactions: Arc<AtomicU64>,
}

impl Transport for Counted {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let snapshot = matches!(msg, Message::Snapshot { .. });
        self.inner.send(msg)?;
        if snapshot {
            self.compactions.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        self.inner.recv()
    }

    fn close(&mut self) -> Result<(), SyncError> {
        self.inner.close()
    }
}

/// Mostly drag steps (one value set again and again: a snapshot of them stays
/// small, so compaction keeps the logs small), some structural edits.
fn edit(stage: &mut Stage, who: &AuthorId, rng: &mut Rng, tag: &str, n: &mut u32) {
    if rng.chance(70) {
        stage.set_edit_target(&SublayerId::root()).unwrap();
        let value = Value::Float(rng.below(1000) as f64);
        stage
            .transact(who, "drag", |tx| {
                tx.apply(Op::SetValue {
                    path: p("/comps/main"),
                    attr: "bgColor".into(),
                    value: Some(value),
                })
            })
            .unwrap();
    } else {
        random_edit(stage, who, rng, tag, n);
    }
}

/// A client: `BAE_MP_DIR` (project), `BAE_MP_ID` (client ID), `BAE_MP_TAG`
/// (names of the layers it creates), `BAE_MP_SEED`, `BAE_MP_LOCK_WAIT_MS` (how
/// long to wait for the client ID's lock, e.g. while the OS has not yet released
/// the lock of a killed process with that ID).
///
/// Answers `waiting` when it first finds the lock held and will wait for it,
/// then `ready lock_retries=N` (or `refused <error>`), then takes commands:
/// `edit <count> <ms between>` (answers `edited vv=…` when done), `stop`
/// (`stopped vv=…`), `state` (`fp= vv= unsynced= compactions= sent= errors=`:
/// snapshots and bytes written, failed pumps), `size` (`snapshot=`: bytes
/// of a snapshot of every sublayer now) and `quit` (flush, close the
/// transport, `bye vv= close=ok|<error>`).
fn dir_client() {
    let dir = PathBuf::from(env("BAE_MP_DIR"));
    let id = ClientId::new(&env("BAE_MP_ID")).unwrap();
    let tag = env("BAE_MP_TAG");
    let mut rng = Rng::new(env("BAE_MP_SEED").parse().unwrap());
    let lock_wait = Duration::from_millis(env("BAE_MP_LOCK_WAIT_MS").parse().unwrap());
    let who = author(&format!("user:{tag}"));

    let mut stage = Project::open(&dir).unwrap().stage;
    let started = Instant::now();
    let mut lock_retries = 0u32;
    let folder = loop {
        match DirTransport::open(&dir, id.clone(), dir_config()) {
            Ok(t) => break t,
            Err(SyncError::ClientIdInUse(_)) if started.elapsed() < lock_wait => {
                if lock_retries == 0 {
                    reply("waiting", "");
                }
                lock_retries += 1;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                reply("refused", &e.to_string().replace(' ', "_"));
                return;
            }
        }
    };
    let compactions = Arc::new(AtomicU64::new(0));
    let (wake, input) = input();
    let mut link = ThreadedTransport::spawn(
        Counted {
            inner: folder,
            compactions: compactions.clone(),
        },
        Duration::from_millis(5),
        move || {
            let _ = wake.send(Input::Wake);
        },
    );
    let mut config = SessionConfig::new(id);
    // Spec 4.5: no periodic hello for a folder; sends batched (shorter than the
    // app's few hundred milliseconds, so a short test writes many records).
    config.hello_interval = None;
    config.min_send_interval = Duration::from_millis(20);
    let mut session = SyncSession::new(config);
    reply("ready", &format!("lock_retries={lock_retries}"));

    let (mut schedule, mut n, mut errors) = (Schedule::default(), 0, 0u64);
    loop {
        let due = schedule.due(Instant::now());
        for _ in 0..due {
            edit(&mut stage, &who, &mut rng, &tag, &mut n);
        }
        if due > 0 && schedule.left() == 0 {
            reply("edited", &format!("vv={}", versions(&stage)));
        }
        // A write that failed (e.g. a file another process had open at the
        // wrong moment) is repaired by the transport (spec 4.3); keep going.
        // `quit` reports a log that is still behind.
        if let Err(e) = session.pump(&mut stage, &mut link) {
            errors += 1;
            eprintln!("{tag}: shared folder: {e}");
        }
        let (lines, eof) = commands(&input, schedule.wait());
        for line in lines {
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.as_slice() {
                ["edit", count, ms] => schedule.start(
                    count.parse().unwrap(),
                    Duration::from_millis(ms.parse().unwrap()),
                ),
                ["stop"] => {
                    schedule.stop();
                    reply("stopped", &format!("vv={}", versions(&stage)));
                }
                ["state"] => reply(
                    "state",
                    &format!(
                        "fp={} vv={} unsynced={} compactions={} sent={} errors={errors}",
                        digest(&fingerprint(&stage)),
                        versions(&stage),
                        session.unsynced(&stage).len(),
                        compactions.load(Ordering::Relaxed),
                        session.stats().bytes_sent,
                    ),
                ),
                ["size"] => reply(
                    "size",
                    &format!(
                        "snapshot={}",
                        stage
                            .sublayers()
                            .map(|s| s.export_snapshot().unwrap().len())
                            .sum::<usize>()
                    ),
                ),
                ["quit"] => {
                    let pumped = session.pump(&mut stage, &mut link);
                    let flushed = session.flush(&stage, &mut link);
                    let closed = link.close();
                    let result = pumped.and(flushed).and(closed);
                    reply(
                        "bye",
                        &format!(
                            "vv={} close={}",
                            versions(&stage),
                            result
                                .map_or_else(|e| e.to_string().replace(' ', "_"), |_| "ok".into())
                        ),
                    );
                    return;
                }
                other => panic!("unknown command {other:?}"),
            }
        }
        if eof {
            return;
        }
    }
}

// ---- the parent ----

/// How long a restarted client waits for its client ID's lock: Windows
/// releases the locks of a killed process some time after it is gone.
const LOCK_WAIT: Duration = Duration::from_secs(30);

/// A project directory with the base project saved in it (`Project::save_as`,
/// what the app does before others join).
fn project_dir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Film.bae");
    let mut project = Project::from_stage(client_stage(&base_snapshots()));
    project.save_as(&dir).unwrap();
    (tmp, dir)
}

/// One client process as the parent sees it.
struct Client {
    id: String,
    tag: String,
    proc: Proc,
    /// What `ready` said.
    lock_retries: u64,
    /// Started again after a process with its client ID was killed.
    restarted: bool,
}

/// What a client reported.
#[derive(Debug, Clone)]
struct State {
    fp: String,
    vv: String,
    unsynced: u64,
    compactions: u64,
    sent: u64,
    errors: u64,
}

impl Client {
    /// Start client `id` (layers it creates are named after `tag`); waits for
    /// the client ID's lock up to `lock_wait`.
    fn start(dir: &Path, id: &str, tag: &str, seed: u64, lock_wait: Duration) -> Client {
        Client::ready(spawn(dir, id, tag, seed, lock_wait), id, tag)
    }

    /// Wait until `proc` (started by [`spawn`]) is ready.
    fn ready(mut proc: Proc, id: &str, tag: &str) -> Client {
        let ready = proc.wait_for("ready", START);
        Client {
            id: id.to_owned(),
            tag: tag.to_owned(),
            proc,
            lock_retries: field_u64(&ready, "lock_retries"),
            restarted: false,
        }
    }

    fn state(&mut self) -> State {
        let s = self.proc.ask("state", "state", ANSWER);
        State {
            fp: field(&s, "fp").to_owned(),
            vv: field(&s, "vv").to_owned(),
            unsynced: field_u64(&s, "unsynced"),
            compactions: field_u64(&s, "compactions"),
            sent: field_u64(&s, "sent"),
            errors: field_u64(&s, "errors"),
        }
    }

    /// Tell it to quit cleanly; its final versions.
    fn quit(mut self) -> String {
        let bye = self.proc.ask("quit", "bye", ANSWER);
        assert_eq!(field(&bye, "close"), "ok", "{}: {bye}", self.tag);
        self.proc.wait_exit(ANSWER);
        field(&bye, "vv").to_owned()
    }
}

fn spawn(dir: &Path, id: &str, tag: &str, seed: u64, lock_wait: Duration) -> Proc {
    Proc::spawn(
        ENTRY,
        tag,
        "dir",
        &[
            ("BAE_MP_DIR", dir.display().to_string()),
            ("BAE_MP_ID", id.to_owned()),
            ("BAE_MP_TAG", tag.to_owned()),
            ("BAE_MP_SEED", seed.to_string()),
            ("BAE_MP_LOCK_WAIT_MS", lock_wait.as_millis().to_string()),
        ],
    )
}

/// Wait until every client has the same documents, all written to the folder;
/// returns the common state.
fn settle(clients: &mut [Client]) -> State {
    let deadline = Instant::now() + SETTLE;
    loop {
        let states = clients.iter_mut().map(Client::state).collect::<Vec<_>>();
        let first = &states[0];
        if states
            .iter()
            .all(|s| s.unsynced == 0 && s.fp == first.fp && same_versions(&s.vv, &first.vv))
        {
            return first.clone();
        }
        assert!(
            Instant::now() < deadline,
            "no convergence within {SETTLE:?}: {:?}",
            states
                .iter()
                .map(|s| (&s.fp, s.unsynced))
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Wait until `reader` has read everything `state` says its writer had: by
/// then it is in the writer's log on disk.
fn wait_read(reader: &mut Client, state: &State) {
    let want = parse_versions(&state.vv);
    wait_until("another process to read the writes", SETTLE, || {
        includes(&parse_versions(&reader.state().vv), &want)
    });
}

/// Kill client `i` while it edits and start a new process with the same
/// client ID, which goes on with `edits`. First another client reads what it
/// wrote so far: that must survive (the returned state). `at_once`: the new
/// process is started first and waits for the client ID's lock, the old one
/// is killed, and the new one gets the lock once the OS released it with the
/// dead process. Otherwise the new process starts after the old one is gone,
/// which left what a crash in the middle of a write leaves: a torn append
/// and a half-written compaction; the new process cuts the torn tail.
fn kill_and_restart(
    dir: &Path,
    clients: &mut [Client],
    i: usize,
    at_once: bool,
    edits: &str,
    seed: u64,
) -> State {
    let before = clients[i].state();
    wait_read(&mut clients[(i + 1) % clients.len()], &before);
    let id = clients[i].id.clone();
    let tag = format!("{id}r{seed}");
    let mut new = if at_once {
        let mut proc = spawn(dir, &id, &tag, seed, LOCK_WAIT);
        proc.wait_for("waiting", START);
        clients[i].proc.kill();
        let new = Client::ready(proc, &id, &tag);
        assert!(new.lock_retries > 0, "{tag} did not wait for the lock");
        new
    } else {
        clients[i].proc.kill_and_wait();
        let log = dir.join(format!("updates/root/{id}.log"));
        let torn = tear(&log);
        let tmp = dir.join(format!("updates/root/{id}.tmp-{seed:016x}"));
        fs::write(tmp, b"half a compaction").unwrap();
        let new = Client::start(dir, &id, &tag, seed, LOCK_WAIT);
        if torn {
            // Checked before it edits: a compaction would rewrite the log
            // whether or not the tail was cut.
            wait_until("the restarted process to cut the torn tail", SETTLE, || {
                fs::read(&log).is_ok_and(|b| log_records(&b).1 == b.len())
            });
        }
        new
    };
    new.restarted = true;
    new.proc.send(edits);
    let mut old = std::mem::replace(&mut clients[i], new);
    old.proc.kill_and_wait();
    before
}

/// A crash in the middle of an append: the first half of a copy of the last
/// record is left at the end of `log`, unless the kill already tore it.
/// Whether the log ends in a torn record now (`false`: it has no records).
fn tear(log: &Path) -> bool {
    let Ok(bytes) = fs::read(log) else {
        return false;
    };
    let (recs, end) = log_records(&bytes);
    if end != bytes.len() {
        return true;
    }
    let Some(&(at, len)) = recs.last() else {
        return false;
    };
    let half = &bytes[at..at + (RECORD_HEADER + len) / 2];
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(log)
        .unwrap()
        .write_all(half)
        .unwrap();
    true
}

/// The shape of `updates/` after everyone quit cleanly: every log intact (torn
/// tails were cut by their owners on restart), no temporary files left by
/// compactions that were killed, and every log within its compaction threshold
/// (plus what can be appended while the snapshot is on its way). Returns the
/// total size.
fn check_logs(dir: &Path) -> u64 {
    let mut all = Vec::new();
    files_under(&dir.join("updates"), &mut all);
    let mut total = 0;
    for path in &all {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(!name.contains(".tmp-"), "left behind: {}", path.display());
        assert!(
            !name.contains(".corrupt-"),
            "corrupt log: {}",
            path.display()
        );
        if !name.ends_with(".log") {
            continue;
        }
        let bytes = fs::read(path).unwrap();
        total += bytes.len() as u64;
        let (recs, end) = log_records(&bytes);
        assert_eq!(end, bytes.len(), "torn tail left in {}", path.display());
        // Header, then a snapshot if the log was ever compacted.
        let compacted = recs.get(1).and_then(|&(at, len)| {
            let payload = &bytes[at + RECORD_HEADER..at + RECORD_HEADER + len];
            matches!(Message::decode(payload), Ok(Message::Snapshot { .. }))
                .then_some((at + RECORD_HEADER + len) as u64)
        });
        let threshold = COMPACT_MIN.max(compacted.unwrap_or(0) * COMPACT_RATIO);
        assert!(
            (bytes.len() as u64) < threshold + 16 * 1024,
            "{}: {} bytes, compaction threshold {threshold}",
            path.display(),
            bytes.len()
        );
    }
    total
}

/// The end of a run: the clients stopped editing (`finals`: their versions
/// then). They settle, a newcomer gets the same project from the folder
/// alone, everyone quits cleanly; nothing in `finals` or `durable` (what had
/// reached the folder before a kill) is lost, and `updates/` stays bounded.
/// `sent`: bytes the killed processes had written. Returns how many times each
/// restarted client compacted its logs since its restart.
fn finish(
    dir: &Path,
    mut clients: Vec<Client>,
    durable: &[String],
    finals: &[String],
    sent: u64,
) -> Vec<(String, u64)> {
    let started = Instant::now();
    let common = settle(&mut clients);
    let settled = started.elapsed();
    let mut fresh = Client::start(dir, "fresh", "fresh", 1, Duration::ZERO);
    wait_until("the newcomer to catch up", SETTLE, || {
        fresh.state().fp == common.fp
    });
    let snapshot = field_u64(&fresh.proc.ask("size", "size", ANSWER), "snapshot");
    clients.push(fresh);

    let states: Vec<State> = clients.iter_mut().map(Client::state).collect();
    let restarted: Vec<(String, u64, u64)> = clients
        .iter()
        .zip(&states)
        .filter(|(c, _)| c.restarted)
        .map(|(c, s)| (c.tag.clone(), c.lock_retries, s.compactions))
        .collect();
    let compactions = restarted.iter().map(|(t, _, n)| (t.clone(), *n)).collect();
    let sent = sent + states.iter().map(|s| s.sent).sum::<u64>();
    let errors: u64 = states.iter().map(|s| s.errors).sum();
    let ids = clients.len() as u64 - 1;
    let mut last = String::new();
    for c in clients {
        last = c.quit();
    }
    let have = parse_versions(&last);
    for vv in durable.iter().chain(finals) {
        assert!(includes(&have, &parse_versions(vv)), "lost edits: {vv}");
    }
    let total = check_logs(dir);
    println!(
        "{ids} client IDs; restarted (tag, lock retries, compactions since): {restarted:?}; \
         settled in {settled:.1?}; {sent} bytes written ({errors} failed pumps), updates/ \
         {total} bytes, project snapshot {snapshot} bytes"
    );
    // Spec 4.3: about "clients x snapshot size", however much was written.
    let bound = ids * (COMPACT_RATIO * snapshot + 2 * (COMPACT_MIN + 16 * 1024));
    assert!(total < bound, "updates/ is {total} bytes, bound {bound}");
    compactions
}

/// Four processes edit one folder. Two are killed while editing and started
/// again with the same `ClientId`: one after it is gone (with a torn append,
/// which its successor cuts, and a half-written compaction left in its place),
/// one while its successor already waits for the lock (which the OS releases
/// with the dead process). A second process with a client ID in use is
/// refused. In the end every process and a fresh one agree, nothing that
/// reached the folder before a kill is lost, and the logs stay within their
/// compaction thresholds.
#[test]
fn processes_share_a_folder_through_kills_and_restarts() {
    let (_tmp, dir) = project_dir();
    let mut clients: Vec<Client> = (0..4)
        .map(|i| Client::start(&dir, &format!("c{i}"), &format!("c{i}"), 10 + i, LOCK_WAIT))
        .collect();
    // c1 and c2 are still editing when they are killed, however slow the
    // machine; their successors make the rest.
    for c in &mut clients {
        let edits = if ["c1", "c2"].contains(&c.id.as_str()) {
            "edit 1000000 2"
        } else {
            "edit 1200 2"
        };
        c.proc.send(edits);
    }
    let started = Instant::now();
    let (mut durable, mut sent) = (Vec::new(), 0);

    wait_until("c1 to write", SETTLE, || clients[1].state().sent > 4096);
    // The lock keeps a second process off a client ID that is in use.
    let mut dup = spawn(&dir, "c1", "dup", 99, Duration::ZERO);
    let refused = dup.wait_for("refused", START);
    assert!(refused.contains("already_in_use"), "{refused}");
    dup.wait_exit(ANSWER);
    let before = kill_and_restart(&dir, &mut clients, 1, false, "edit 900 2", 21);
    durable.push(before.vv);
    sent += before.sent;

    wait_until("c2 to write", SETTLE, || clients[2].state().sent > 4096);
    let before = kill_and_restart(&dir, &mut clients, 2, true, "edit 900 2", 22);
    durable.push(before.vv);
    sent += before.sent;

    let finals: Vec<String> = clients
        .iter_mut()
        .map(|c| field(&c.proc.wait_for("edited", SETTLE), "vv").to_owned())
        .collect();
    println!("edits took {:.1?}", started.elapsed());
    // Compaction goes on after a restart (both wrote enough to need it).
    for (tag, compactions) in finish(&dir, clients, &durable, &finals, sent) {
        assert!(compactions > 0, "{tag} never compacted after its restart");
    }
}

/// Soak: four processes edit without pause (each every 10 ms, which a debug
/// build keeps up with as the documents grow) while a random one is killed and
/// restarted (either way of [`kill_and_restart`]) every 0.2–1.5 s, for
/// `BAE_MP_SOAK_SECS` seconds (default 60). Run with
///
/// ```sh
/// BAE_MP_SOAK_SECS=600 cargo test -p bae-sync --features hub,watch \
///     --test multiprocess_dir -- --ignored --nocapture
/// ```
#[test]
#[ignore = "soak test (a minute or more): run with --ignored, see its doc comment"]
fn soak_random_kills_and_restarts() {
    const EDITS: &str = "edit 4000000000 10";
    let secs = std::env::var("BAE_MP_SOAK_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let (_tmp, dir) = project_dir();
    let mut clients: Vec<Client> = (0..4)
        .map(|i| Client::start(&dir, &format!("c{i}"), &format!("c{i}"), 10 + i, LOCK_WAIT))
        .collect();
    for c in &mut clients {
        c.proc.send(EDITS);
    }
    let mut rng = Rng::new(secs);
    let (mut durable, mut sent, mut kills) = (Vec::new(), 0, 0u64);
    // Compactions by processes started after a kill, over the whole run: each
    // is killed again a few seconds later, and late in a run (large documents,
    // so a compaction threshold far above the minimum) most do not live long
    // enough to reach it.
    let mut compactions = 0;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(Duration::from_millis(200 + rng.below(1300) as u64));
        let i = rng.below(clients.len());
        let at_once = rng.chance(50);
        let restarted = clients[i].restarted;
        let before = kill_and_restart(&dir, &mut clients, i, at_once, EDITS, 100 + kills);
        if restarted {
            compactions += before.compactions;
        }
        durable.push(before.vv);
        sent += before.sent;
        kills += 1;
    }
    let finals: Vec<String> = clients
        .iter_mut()
        .map(|c| field(&c.proc.ask("stop", "stopped", ANSWER), "vv").to_owned())
        .collect();
    println!("{kills} kills in {secs} s");
    let last = finish(&dir, clients, &durable, &finals, sent);
    compactions += last.iter().map(|(_, n)| n).sum::<u64>();
    println!("{compactions} compactions by restarted processes");
    assert!(compactions > 0, "no process compacted after a restart");
}
