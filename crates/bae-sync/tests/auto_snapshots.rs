//! Automatic snapshots: when they are made, retention, several collaborators,
//! older clients and clock skew. All times are fake; nothing sleeps.

mod common;

use std::collections::BTreeSet;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bae_model::loro::{LoroMap, LoroValue, ValueOrContainer, VersionVector};
use bae_model::{AuthorId, Op, Stage, SublayerId, Value};
use bae_sync::{
    AutoSnapshots, Checkpoint, CheckpointId, CheckpointKind, ClientId, History, MemoryTransport,
    ProjectDocs, ProjectVersion, Relay, Retention, RetentionTier, SessionConfig, SyncSession,
};
use common::*;

const MIN: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(3600);
const DAY: Duration = Duration::from_secs(24 * 3600);
/// 2026-11-27 00:00 UTC.
const T0_MS: i64 = 1_795_737_600_000;

/// Fake wall and monotonic clocks that advance together.
#[derive(Clone, Copy, Debug)]
struct Clock {
    wall: SystemTime,
    mono: Instant,
}

impl Clock {
    fn new() -> Self {
        Clock {
            wall: UNIX_EPOCH + Duration::from_millis(T0_MS as u64),
            mono: Instant::now(),
        }
    }

    fn advance(&mut self, d: Duration) {
        self.wall += d;
        self.mono += d;
    }

    fn wall_ms(&self) -> i64 {
        self.wall.duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
    }
}

/// A change to the project: a new opacity on the title layer.
fn touch(stage: &mut Stage, who: &AuthorId, n: u32) {
    stage.set_edit_target(&SublayerId::root()).unwrap();
    stage
        .transact(who, "edit", |tx| {
            tx.apply(Op::SetValue {
                path: p("/comps/main/layers/title"),
                attr: "opacity".into(),
                value: Some(Value::Float(f64::from(n % 100))),
            })
        })
        .unwrap();
}

fn tick(
    auto: &mut AutoSnapshots,
    history: &mut History,
    stage: &Stage,
    who: &AuthorId,
    c: &Clock,
) -> Option<CheckpointId> {
    auto.tick(history, stage, who, c.wall, c.mono).unwrap()
}

/// The label of the latest commit to the history document.
fn last_label(history: &History) -> String {
    let id = history.doc().oplog_frontiers().iter().next().unwrap();
    let msg = history.doc().get_change(id).unwrap().message.unwrap();
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
    v["label"].as_str().unwrap().to_owned()
}

#[test]
fn a_snapshot_is_made_after_the_interval_when_the_project_changed() {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let alice = author("user:alice");
    let mut auto = AutoSnapshots::new(10 * MIN);
    auto.utc_offset_minutes = 8 * 60;
    let mut c = Clock::new();

    // The timer starts at the first tick.
    assert_eq!(tick(&mut auto, &mut history, &stage, &alice, &c), None);
    touch(&mut stage, &alice, 1);
    c.advance(5 * MIN);
    assert_eq!(tick(&mut auto, &mut history, &stage, &alice, &c), None);
    c.advance(5 * MIN);
    let undo = stage.undo_label(&alice).map(str::to_owned);
    let project = ProjectVersion::current(&stage);
    let first = tick(&mut auto, &mut history, &stage, &alice, &c).expect("due");

    let cp = history.checkpoint(&first).unwrap();
    assert_eq!(cp.kind, CheckpointKind::Auto);
    assert!(cp.is_auto());
    assert_eq!(cp.name, "自动快照 2026-11-27 08:10");
    assert_eq!(cp.created_ms, c.wall_ms());
    assert_eq!((cp.author.clone(), cp.note.as_str()), (alice.clone(), ""));
    assert_eq!(cp.version, project);
    assert_eq!(cp.layer_stack, stage.layer_stack());
    assert_eq!(last_label(&history), "checkpoint.auto");
    // Only the history document was written.
    assert_eq!(ProjectVersion::current(&stage), project);
    assert_eq!(stage.undo_label(&alice).map(str::to_owned), undo);
    assert_eq!(auto.due_at(), Some(c.mono + 10 * MIN));

    // Not again before the interval, even with changes.
    c.advance(MIN);
    assert_eq!(tick(&mut auto, &mut history, &stage, &alice, &c), None);
    touch(&mut stage, &alice, 2);
    c.advance(8 * MIN);
    assert_eq!(tick(&mut auto, &mut history, &stage, &alice, &c), None);
    c.advance(MIN);
    let second = tick(&mut auto, &mut history, &stage, &alice, &c).expect("due");

    // Due but unchanged: nothing; the next change is snapshotted at once.
    c.advance(15 * MIN);
    assert_eq!(tick(&mut auto, &mut history, &stage, &alice, &c), None);
    touch(&mut stage, &alice, 3);
    c.advance(MIN);
    let third = tick(&mut auto, &mut history, &stage, &alice, &c).expect("overdue");

    let autos: Vec<CheckpointId> = history.auto_snapshots().into_iter().map(|c| c.id).collect();
    assert_eq!(autos, [first, second, third]);
    assert_eq!(history.checkpoints().len(), 3);
    assert!(history.named_checkpoints().is_empty());
    assert_eq!(
        history.checkpoints_where(|c| c.author == alice).len(),
        3,
        "filter helper"
    );
    assert_eq!(auto.stats().created, 3);
}

#[test]
fn idle_ticks_read_neither_the_history_nor_the_project() {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let alice = author("user:alice");
    let mut auto = AutoSnapshots::new(10 * MIN);
    let mut c = Clock::new();
    tick(&mut auto, &mut history, &stage, &alice, &c);
    touch(&mut stage, &alice, 1);
    c.advance(10 * MIN);
    tick(&mut auto, &mut history, &stage, &alice, &c).expect("due");
    let before = auto.stats();
    let version = history.version();

    // A day of ticks every 6 seconds with no change.
    for _ in 0..14_400 {
        c.advance(Duration::from_secs(6));
        assert_eq!(tick(&mut auto, &mut history, &stage, &alice, &c), None);
    }
    let after = auto.stats();
    assert_eq!(after.ticks - before.ticks, 14_400);
    assert_eq!(after.scans, before.scans, "the history was not read again");
    assert_eq!(
        after.version_checks - before.version_checks,
        1,
        "the project version was computed once, when the next snapshot was due"
    );
    assert_eq!(history.version(), version, "nothing was written");
    assert_eq!(history.auto_snapshots().len(), 1);

    // Disabled: nothing at all.
    auto.enabled = false;
    touch(&mut stage, &alice, 2);
    c.advance(HOUR);
    assert_eq!(tick(&mut auto, &mut history, &stage, &alice, &c), None);
    assert_eq!(auto.stats().ticks, after.ticks);
    assert_eq!(auto.due_at(), None);
    auto.enabled = true;
    assert!(tick(&mut auto, &mut history, &stage, &alice, &c).is_some());
}

#[test]
fn a_named_version_counts_as_a_snapshot() {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let alice = author("user:alice");
    let mut auto = AutoSnapshots::new(10 * MIN);
    let mut c = Clock::new();
    tick(&mut auto, &mut history, &stage, &alice, &c);
    touch(&mut stage, &alice, 1);
    c.advance(9 * MIN);
    history.create_checkpoint(&stage, &alice, "v1", "").unwrap();
    c.advance(MIN);
    assert_eq!(
        tick(&mut auto, &mut history, &stage, &alice, &c),
        None,
        "nothing changed since v1"
    );
    touch(&mut stage, &alice, 2);
    c.advance(MIN);
    let id = tick(&mut auto, &mut history, &stage, &alice, &c).expect("changed since v1");
    let kinds: Vec<CheckpointKind> = history.checkpoints().into_iter().map(|c| c.kind).collect();
    assert!(kinds.contains(&CheckpointKind::Named) && kinds.contains(&CheckpointKind::Auto));
    assert_eq!(history.named_checkpoints()[0].name, "v1");
    assert_eq!(history.auto_snapshots()[0].id, id);

    // A version a collaborator made with more ops than arrived here also
    // covers everything this client has.
    touch(&mut stage, &alice, 3);
    let mut ahead = client_stage(&base_snapshots());
    for layer in stage.sublayers() {
        let bytes = layer.export_updates(&VersionVector::new()).unwrap();
        ahead.import_updates(layer.id(), &bytes).unwrap();
    }
    touch(&mut ahead, &alice, 4);
    history
        .create_checkpoint(&ahead, &alice, "from a collaborator", "")
        .unwrap();
    c.advance(HOUR);
    assert_eq!(tick(&mut auto, &mut history, &stage, &alice, &c), None);
}

/// The periods of length `every` the snapshots fall in; panics if one holds
/// two.
fn one_per(snaps: &[&Checkpoint], every: Duration) -> BTreeSet<i64> {
    let mut buckets = BTreeSet::new();
    for s in snaps {
        let b = s.created_ms.div_euclid(every.as_millis() as i64);
        assert!(buckets.insert(b), "two snapshots in one {every:?} period");
    }
    buckets
}

#[test]
fn retention_keeps_recent_hourly_and_daily_snapshots_and_never_named_ones() {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let alice = author("user:alice");
    let mut auto = AutoSnapshots::new(10 * MIN);
    let mut c = Clock::new();
    touch(&mut stage, &alice, 0);
    let named = history
        .create_checkpoint(&stage, &alice, "给客户的第 1 版", "")
        .unwrap();
    tick(&mut auto, &mut history, &stage, &alice, &c);
    let mut kept = None;
    let steps = 32 * 24 * 6;
    for step in 1..=steps {
        touch(&mut stage, &alice, step);
        c.advance(10 * MIN);
        tick(&mut auto, &mut history, &stage, &alice, &c).expect("every interval");
        if step == 24 * 6 {
            // Renaming an automatic snapshot keeps it.
            let id = history.auto_snapshots().last().unwrap().id.clone();
            history
                .rename_checkpoint(&id, &alice, "留着", None)
                .unwrap();
            kept = Some(id);
        }
    }
    assert_eq!(auto.stats().created, u64::from(steps));
    let now = c.wall_ms();
    assert_eq!(
        history.checkpoint(&named.id).unwrap().kind,
        CheckpointKind::Named
    );
    let kept = history
        .checkpoint(&kept.unwrap())
        .expect("renamed one kept");
    assert_eq!(
        (kept.kind, kept.name.as_str()),
        (CheckpointKind::Named, "留着")
    );

    let autos = history.auto_snapshots();
    let age = |s: &Checkpoint| Duration::from_millis((now - s.created_ms) as u64);
    let within = |from: Duration, to: Duration| -> Vec<&Checkpoint> {
        autos
            .iter()
            .filter(|s| age(s) >= from && age(s) < to)
            .collect()
    };
    assert_eq!(
        within(Duration::ZERO, HOUR).len(),
        6,
        "all of the last hour"
    );
    assert_eq!(one_per(&within(HOUR, DAY), HOUR).len(), 23, "one per hour");
    let days = one_per(&within(DAY, 30 * DAY), DAY);
    assert!(days.len() >= 29, "one per day: {}", days.len());
    assert!(within(30 * DAY, 100 * DAY).is_empty(), "expired");
    assert!(autos.len() <= 6 + 24 + 30, "{}", autos.len());
    assert!(auto.stats().pruned > 4000);
}

/// Clients connected through a hub.
struct Net {
    relay: Relay,
    stages: Vec<Stage>,
    histories: Vec<History>,
    sessions: Vec<SyncSession>,
    links: Vec<MemoryTransport>,
    autos: Vec<AutoSnapshots>,
    who: Vec<AuthorId>,
}

impl Net {
    fn new(n: usize) -> Self {
        let base = base_snapshots();
        let mut relay = Relay::new(ClientId::new("hub").unwrap());
        let links = (0..n)
            .map(|_| {
                let (a, b) = MemoryTransport::pair();
                relay.connect(Box::new(b));
                a
            })
            .collect();
        Net {
            relay,
            stages: (0..n).map(|_| client_stage(&base)).collect(),
            histories: (0..n).map(|_| History::new()).collect(),
            sessions: (0..n)
                .map(|i| {
                    SyncSession::new(SessionConfig::new(ClientId::new(&format!("c{i}")).unwrap()))
                })
                .collect(),
            links,
            autos: (0..n).map(|_| AutoSnapshots::new(10 * MIN)).collect(),
            who: (0..n).map(|i| author(&format!("user:u{i}"))).collect(),
        }
    }

    /// Exchange everything between the clients in `online`.
    fn sync(&mut self, online: &[usize]) {
        for _ in 0..4 {
            for &i in online {
                let mut docs = ProjectDocs::new(&mut self.stages[i], &mut self.histories[i]);
                self.sessions[i]
                    .pump(&mut docs, &mut self.links[i])
                    .unwrap();
            }
            self.relay.pump();
        }
    }

    fn tick(&mut self, i: usize, c: &Clock) -> Option<CheckpointId> {
        self.autos[i]
            .tick(
                &mut self.histories[i],
                &self.stages[i],
                &self.who[i],
                c.wall,
                c.mono,
            )
            .unwrap()
    }

    fn ids(&self, i: usize) -> Vec<CheckpointId> {
        self.histories[i]
            .checkpoints()
            .into_iter()
            .map(|c| c.id)
            .collect()
    }
}

#[test]
fn collaborators_make_one_snapshot_per_interval_and_agree_on_pruning() {
    let mut net = Net::new(2);
    let mut c = Clock::new();
    let both = [0, 1];
    for minute in 0..=180u32 {
        // Both tick at the same moment, before hearing from each other.
        for i in both {
            net.tick(i, &c);
        }
        net.sync(&both);
        let i = (minute % 2) as usize;
        let who = net.who[i].clone();
        touch(&mut net.stages[i], &who, minute);
        net.sync(&both);
        c.advance(MIN);
    }
    // Both made the first one at minute 10 and the duplicate was dropped; from
    // then on only the maker of the latest one made the next, every 10 minutes.
    let stats = [net.autos[0].stats(), net.autos[1].stats()];
    assert_eq!(stats[0].created + stats[1].created, 2 + 17);
    assert_eq!(net.ids(0), net.ids(1));
    let autos = net.histories[0].auto_snapshots();
    let makers: BTreeSet<&AuthorId> = autos.iter().map(|s| &s.author).collect();
    assert_eq!(makers.len(), 1, "one client kept making them");
    let maker = net.who.iter().position(|w| *w == autos[0].author).unwrap();
    let other = 1 - maker;
    assert_eq!(stats[maker].created, 18);
    assert_eq!(stats[other].pruned, 0, "only the maker pruned");
    // At minute 180: every snapshot of the last hour, then the newest of each
    // hour (UTC).
    let times: Vec<i64> = autos
        .iter()
        .map(|s| (s.created_ms - T0_MS) / 60_000)
        .collect();
    assert_eq!(times, [50, 110, 130, 140, 150, 160, 170, 180]);

    // The maker leaves; the other takes over half an interval later.
    let last = autos.last().unwrap().created_ms;
    let mut taken_over = None;
    for minute in 0..60u32 {
        let who = net.who[other].clone();
        touch(&mut net.stages[other], &who, minute);
        if let Some(id) = net.tick(other, &c) {
            taken_over.get_or_insert(id);
        }
        net.sync(&[other]);
        c.advance(MIN);
    }
    let first = net.histories[other]
        .checkpoint(&taken_over.unwrap())
        .unwrap();
    assert_eq!((first.created_ms - last) / 60_000, 15);
    let after: Vec<i64> = net.histories[other]
        .auto_snapshots()
        .iter()
        .filter(|s| s.created_ms > last)
        .map(|s| (s.created_ms - first.created_ms) / 60_000)
        .collect();
    assert_eq!(after, [0, 10, 20, 30, 40]);

    // Back together, both prune with the same clock: the same deletions.
    net.sync(&both);
    assert_eq!(net.ids(0), net.ids(1));
    let later = c.wall + 2 * DAY;
    let mut copies: Vec<History> = net
        .histories
        .iter()
        .map(|h| History::from_snapshot(&h.export_snapshot().unwrap()).unwrap())
        .collect();
    let plans: Vec<Vec<CheckpointId>> = (0..2)
        .map(|i| {
            let who = net.who[i].clone();
            net.autos[i].prune(&mut copies[i], &who, later).unwrap()
        })
        .collect();
    assert!(!plans[0].is_empty());
    assert_eq!(plans[0], plans[1]);
    // Two days later: the newest of each UTC day.
    let left: Vec<i64> = copies[0]
        .auto_snapshots()
        .iter()
        .map(|s| (s.created_ms - T0_MS) / 60_000)
        .collect();
    assert_eq!(left, [(195 + 40) as i64]);
}

#[test]
fn concurrent_snapshots_are_deduplicated_but_never_both_dropped() {
    // Three clients that never made a snapshot open the same project: all due
    // at the same moment, they each make one before hearing from the others.
    let mut net = Net::new(3);
    let all = [0, 1, 2];
    let mut c = Clock::new();
    for i in all {
        net.tick(i, &c);
    }
    let who = net.who[0].clone();
    touch(&mut net.stages[0], &who, 1);
    net.sync(&all);
    c.advance(10 * MIN);
    let made: Vec<CheckpointId> = all.iter().filter_map(|&i| net.tick(i, &c)).collect();
    assert_eq!(made.len(), 3);
    net.sync(&all);
    // Whoever made the latest prunes the others at its next tick.
    c.advance(MIN);
    for i in all {
        net.tick(i, &c);
    }
    net.sync(&all);
    let left: Vec<Vec<CheckpointId>> = all.iter().map(|&i| net.ids(i)).collect();
    assert_eq!(left[0].len(), 1);
    assert!(left.iter().all(|l| *l == left[0]));
    assert!(made.contains(&left[0][0]));

    // Every client pruning on its own at once still leaves one.
    let mut net = Net::new(3);
    for i in all {
        net.tick(i, &c);
    }
    let who = net.who[0].clone();
    touch(&mut net.stages[0], &who, 1);
    net.sync(&all);
    c.advance(10 * MIN);
    for i in all {
        net.tick(i, &c).expect("due");
    }
    net.sync(&all);
    for i in all {
        let who = net.who[i].clone();
        net.autos[i]
            .prune(&mut net.histories[i], &who, c.wall)
            .unwrap();
    }
    net.sync(&all);
    assert_eq!(net.histories[0].auto_snapshots().len(), 1);
    assert!(all.iter().all(|&i| net.ids(i) == net.ids(0)));

    // Snapshots of different states (an edit was still on its way) are both
    // kept.
    let mut net = Net::new(2);
    for i in [0, 1] {
        net.tick(i, &c);
    }
    c.advance(10 * MIN);
    let who = net.who[0].clone();
    touch(&mut net.stages[0], &who, 1);
    let who = net.who[1].clone();
    touch(&mut net.stages[1], &who, 2);
    for i in [0, 1] {
        net.tick(i, &c).expect("due");
    }
    net.sync(&[0, 1]);
    for i in [0, 1] {
        let who = net.who[i].clone();
        net.autos[i]
            .prune(&mut net.histories[i], &who, c.wall)
            .unwrap();
    }
    net.sync(&[0, 1]);
    assert_eq!(net.histories[0].auto_snapshots().len(), 2);
}

/// What a client from before checkpoint kinds reads (its `parse_checkpoint`).
fn old_client_reads(history: &History) -> Vec<(String, String, i64, usize)> {
    let LoroValue::Map(all) = history.doc().get_map("checkpoints").get_deep_value() else {
        panic!("no checkpoints");
    };
    let mut out = Vec::new();
    for v in all.values() {
        let LoroValue::Map(m) = v else { continue };
        let string = |k: &str| match m.get(k) {
            Some(LoroValue::String(s)) => s.to_string(),
            other => panic!("{k}: {other:?}"),
        };
        let Some(LoroValue::I64(created)) = m.get("created") else {
            panic!("created")
        };
        let stack: Vec<String> = serde_json::from_str(&string("stack")).unwrap();
        let Some(LoroValue::Map(versions)) = m.get("versions") else {
            panic!("versions")
        };
        for vv in versions.values() {
            let LoroValue::Binary(b) = vv else { panic!() };
            VersionVector::decode(b).unwrap();
        }
        AuthorId::new(&string("author")).unwrap();
        out.push((string("name"), string("note"), *created, stack.len()));
    }
    out.sort();
    out
}

fn raw_checkpoint(history: &History, id: &CheckpointId) -> LoroMap {
    let Some(ValueOrContainer::Container(c)) = history.doc().get_map("checkpoints").get(&id.0)
    else {
        panic!("no {id}")
    };
    c.into_map().unwrap()
}

#[test]
fn older_clients_see_automatic_snapshots_as_named_versions() {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let alice = author("user:alice");
    let mut auto = AutoSnapshots::new(10 * MIN);
    auto.retention = Retention {
        tiers: vec![RetentionTier {
            until: MIN,
            every: None,
        }],
    };
    let mut c = Clock::new();
    tick(&mut auto, &mut history, &stage, &alice, &c);
    touch(&mut stage, &alice, 1);
    c.advance(10 * MIN);
    let first = tick(&mut auto, &mut history, &stage, &alice, &c).unwrap();
    touch(&mut stage, &alice, 2);
    c.advance(10 * MIN);
    let second = tick(&mut auto, &mut history, &stage, &alice, &c).unwrap();
    assert!(
        history.checkpoint(&first).is_none(),
        "expired after a minute"
    );

    // An older client gets the document and reads an ordinary named version.
    let old = History::from_snapshot(&history.export_snapshot().unwrap()).unwrap();
    assert_eq!(
        old_client_reads(&old),
        [(
            "自动快照 2026-11-27 00:20".to_owned(),
            String::new(),
            T0_MS + 20 * 60_000,
            2
        )]
    );
    // It renames it the only way it knows (name only), and writes a named
    // version without a kind; a newer client writes a kind nobody knows yet.
    let doc = old.doc();
    raw_checkpoint(&old, &second)
        .insert("name", "客户看过的版本")
        .unwrap();
    let v = doc
        .get_map("checkpoints")
        .insert_container("0000old", LoroMap::new())
        .unwrap();
    for (k, val) in [("name", "旧客户端"), ("note", ""), ("author", "user:bob")] {
        v.insert(k, val).unwrap();
    }
    v.insert("created", T0_MS).unwrap();
    v.insert("stack", "[\"root\"]").unwrap();
    v.insert_container("versions", LoroMap::new()).unwrap();
    let future = doc
        .get_map("checkpoints")
        .insert_container("0000future", LoroMap::new())
        .unwrap();
    future.insert("name", "合并").unwrap();
    future.insert("kind", "merge").unwrap();
    future.insert("created", T0_MS).unwrap();
    doc.commit();
    history
        .import(&old.export_updates(&history.version()).unwrap())
        .unwrap();

    let kind = |id: &str| history.checkpoint(&CheckpointId(id.into())).unwrap().kind;
    assert_eq!(kind(&second.0), CheckpointKind::Named, "renamed: kept");
    assert_eq!(kind("0000old"), CheckpointKind::Named);
    assert_eq!(kind("0000future"), CheckpointKind::Other("merge".into()));
    assert!(history.auto_snapshots().is_empty());
    // None of them is ever pruned, however old.
    auto.prune(&mut history, &alice, c.wall + 365 * DAY)
        .unwrap();
    assert_eq!(history.checkpoints().len(), 3);
    assert_eq!(history.named_checkpoints().len(), 3);
}

#[test]
fn a_wrong_clock_neither_stops_nor_floods_snapshots() {
    for skew in [365 * DAY, DAY] {
        for ahead in [true, false] {
            let mut net = Net::new(2);
            let mut c = Clock::new();
            let skewed = |c: &Clock| Clock {
                wall: if ahead { c.wall + skew } else { c.wall - skew },
                mono: c.mono,
            };
            // Client 1's wall clock is wrong; it makes the first snapshot and
            // leaves.
            net.tick(0, &c);
            net.tick(1, &skewed(&c));
            let who = net.who[1].clone();
            touch(&mut net.stages[1], &who, 1);
            c.advance(10 * MIN);
            net.tick(1, &skewed(&c)).expect("due");
            net.sync(&[0, 1]);

            // Client 0 keeps editing for two hours.
            let mut made = Vec::new();
            for minute in 0..120u32 {
                let who = net.who[0].clone();
                touch(&mut net.stages[0], &who, minute);
                if net.tick(0, &c).is_some() {
                    made.push(minute);
                }
                c.advance(MIN);
            }
            assert!(!made.is_empty(), "skew {skew:?} ahead {ahead}: blocked");
            assert!(
                made.len() <= 13,
                "skew {skew:?} ahead {ahead}: {} snapshots",
                made.len()
            );
            for w in made.windows(2) {
                assert_eq!(w[1] - w[0], 10, "skew {skew:?} ahead {ahead}: {made:?}");
            }
            // Ahead, the wrong clock's snapshot is the newest by time and is
            // kept; a year behind, it has expired.
            let wrong = net.histories[0]
                .auto_snapshots()
                .iter()
                .any(|s| s.author == net.who[1]);
            assert_eq!(wrong, ahead || skew == DAY, "skew {skew:?} ahead {ahead}");
        }
    }
}
