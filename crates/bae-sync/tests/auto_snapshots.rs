//! Automatic snapshots: when they are made, retention, several collaborators,
//! older clients and clock skew. All times are fake; nothing sleeps.

mod common;

use std::collections::BTreeSet;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bae_model::loro::{LoroMap, LoroValue, ValueOrContainer, VersionVector};
use bae_model::{AuthorId, Op, Stage, SublayerId, Value};
use bae_sync::{
    AutoSnapshots, Checkpoint, CheckpointId, CheckpointKind, ClientId, History, MemoryTransport,
    ProjectDocs, ProjectVersion, Relay, Retention, RetentionTier, RevertScope, SessionConfig,
    SyncSession,
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
            // Renaming an automatic snapshot keeps it, as a named version with
            // a new ID.
            let auto = history.auto_snapshots().last().unwrap().clone();
            let id = history
                .rename_checkpoint(&auto.id, &alice, "留着", None)
                .unwrap();
            assert_ne!(id, auto.id);
            assert!(history.checkpoint(&auto.id).is_none());
            let named = history.checkpoint(&id).unwrap();
            assert_eq!(
                (named.created_ms, &named.version, &named.author),
                (auto.created_ms, &auto.version, &auto.author)
            );
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

    // Back together, both see each other's snapshots and prune with the same
    // clock: the same deletions.
    net.sync(&both);
    assert_eq!(net.ids(0), net.ids(1));
    for i in both {
        net.tick(i, &c);
    }
    let later = c.wall + 2 * DAY;
    let mut copies: Vec<History> = net
        .histories
        .iter()
        .map(|h| History::from_snapshot(&h.export_snapshot().unwrap()).unwrap())
        .collect();
    let plans: Vec<Vec<CheckpointId>> = (0..2)
        .map(|i| {
            let who = net.who[i].clone();
            net.autos[i]
                .prune(&mut copies[i], &who, later, c.mono + 2 * DAY)
                .unwrap()
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
            .prune(&mut net.histories[i], &who, c.wall, c.mono)
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
            .prune(&mut net.histories[i], &who, c.wall, c.mono)
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
    auto.prune(&mut history, &alice, c.wall + 365 * DAY, c.mono + 365 * DAY)
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
            // The wrong clock's snapshot is kept: ahead, it is the newest by
            // time; behind, client 0 takes it to be only as old as the time
            // since it saw it arrive, two hours.
            let wrong = net.histories[0]
                .auto_snapshots()
                .iter()
                .any(|s| s.author == net.who[1]);
            assert!(wrong, "skew {skew:?} ahead {ahead}");
        }
    }
}

/// Exchange everything between two histories.
fn merge(a: &mut History, b: &mut History) {
    let (va, vb) = (a.version(), b.version());
    a.import(&b.export_updates(&va).unwrap()).unwrap();
    b.import(&a.export_updates(&vb).unwrap()).unwrap();
}

#[test]
fn keeping_a_snapshot_survives_a_concurrent_prune() {
    let mut stage = client_stage(&base_snapshots());
    let mut alices = History::new();
    let (alice, bob) = (author("user:alice"), author("user:bob"));
    let mut auto = AutoSnapshots::new(10 * MIN);
    let mut c = Clock::new();
    tick(&mut auto, &mut alices, &stage, &alice, &c);
    let mut made = Vec::new();
    for minute in 1..=55u32 {
        touch(&mut stage, &alice, minute);
        c.advance(MIN);
        made.extend(tick(&mut auto, &mut alices, &stage, &alice, &c));
    }
    assert_eq!(made.len(), 5);

    // Bob has the history and, offline, keeps the second snapshot.
    let mut bobs = History::from_snapshot(&alices.export_snapshot().unwrap()).unwrap();
    let before = bobs.checkpoint(&made[1]).unwrap();
    let kept = bobs
        .rename_checkpoint(&made[1], &bob, "客户确认的版本", Some("keep"))
        .unwrap();
    assert_eq!(bobs.checkpoint(&kept).unwrap().kind, CheckpointKind::Named);
    assert!(bobs.checkpoint(&made[1]).is_none());
    assert_eq!(last_label(&bobs), "checkpoint.rename");

    // Meanwhile Alice keeps working; her snapshots thin out that hour, which
    // deletes the one Bob kept.
    for minute in 56..=200u32 {
        touch(&mut stage, &alice, minute);
        c.advance(MIN);
        tick(&mut auto, &mut alices, &stage, &alice, &c);
    }
    assert!(alices.checkpoint(&made[1]).is_none(), "pruned by Alice");

    merge(&mut alices, &mut bobs);
    tick(&mut auto, &mut alices, &stage, &alice, &c);
    auto.prune(&mut alices, &alice, c.wall + 365 * DAY, c.mono + 365 * DAY)
        .unwrap();
    merge(&mut alices, &mut bobs);
    for h in [&alices, &bobs] {
        let cp = h.checkpoint(&kept).expect("the kept version survives");
        assert_eq!(
            (cp.kind, cp.name.as_str(), cp.note.as_str()),
            (CheckpointKind::Named, "客户确认的版本", "keep")
        );
        assert_eq!(
            (cp.created_ms, &cp.version, &cp.layer_stack, &cp.author),
            (
                before.created_ms,
                &before.version,
                &before.layer_stack,
                &alice
            )
        );
        assert!(h.checkpoint(&made[1]).is_none());
    }
}

#[test]
fn a_clock_that_runs_ahead_does_not_prune_other_peoples_snapshots_early() {
    for skew in [30 * MIN, 3 * HOUR, 8 * HOUR, 40 * DAY, 365 * DAY] {
        let mut net = Net::new(2);
        let mut c = Clock::new();
        let bobs_clock = |c: &Clock| Clock {
            wall: c.wall + skew,
            mono: c.mono,
        };
        // Alice (right clock) works alone for 55 minutes.
        net.tick(0, &c);
        let mut alices = Vec::new();
        for minute in 1..=55u32 {
            let who = net.who[0].clone();
            touch(&mut net.stages[0], &who, minute);
            c.advance(MIN);
            alices.extend(net.tick(0, &c));
        }
        assert_eq!(alices.len(), 5);

        // Bob joins and edits. His clock makes her snapshots look old, so he
        // makes one at once, and prunes.
        net.sync(&[0, 1]);
        let who = net.who[1].clone();
        touch(&mut net.stages[1], &who, 100);
        net.sync(&[0, 1]);
        assert!(net.tick(1, &bobs_clock(&c)).is_some(), "skew {skew:?}");
        net.sync(&[0, 1]);
        let left = |net: &Net, ids: &[CheckpointId]| -> Vec<CheckpointId> {
            ids.iter()
                .filter(|id| net.histories[0].checkpoint(id).is_some())
                .cloned()
                .collect()
        };
        assert_eq!(
            left(&net, &alices),
            alices,
            "skew {skew:?}: Bob deleted Alice's snapshots of the last hour"
        );

        // They work together for 2.5 hours; Bob keeps making the snapshots.
        for minute in 0..150u32 {
            let who = net.who[0].clone();
            touch(&mut net.stages[0], &who, minute);
            net.sync(&[0, 1]);
            c.advance(MIN);
            net.tick(0, &c);
            net.tick(1, &bobs_clock(&c));
            net.sync(&[0, 1]);
        }
        assert_eq!(net.ids(0), net.ids(1));
        assert_eq!(net.autos[0].stats().created, 5, "skew {skew:?}");
        // Her snapshots age by the time that really passed: two hours later,
        // only the newest of that hour is left.
        assert_eq!(left(&net, &alices), [alices[4].clone()], "skew {skew:?}");
    }
}

#[test]
fn thinning_keeps_each_persons_offline_work() {
    let mut net = Net::new(2);
    let mut c = Clock::new();
    let both = [0, 1];
    // Two people work apart for three hours; Alice's snapshots come two
    // minutes after Bob's.
    net.tick(1, &c);
    c.advance(2 * MIN);
    net.tick(0, &c);
    let mut offline: [Vec<ProjectVersion>; 2] = Default::default();
    for minute in 0..180u32 {
        for i in both {
            let who = net.who[i].clone();
            touch(&mut net.stages[i], &who, minute + 100 * i as u32);
            if let Some(id) = net.tick(i, &c) {
                let cp = net.histories[i].checkpoint(&id).unwrap();
                offline[i].push(cp.version);
            }
        }
        c.advance(MIN);
    }
    assert!(offline.iter().all(|v| v.len() >= 17));

    // Back together for an hour and a half.
    for minute in 0..90u32 {
        net.sync(&both);
        let i = (minute % 2) as usize;
        let who = net.who[i].clone();
        touch(&mut net.stages[i], &who, minute);
        for i in both {
            net.tick(i, &c);
        }
        c.advance(MIN);
    }
    net.sync(&both);
    assert_eq!(net.ids(0), net.ids(1));
    let kept = net.histories[0].auto_snapshots();
    for (i, versions) in offline.iter().enumerate() {
        // Each hour apart keeps the newest state of each person's own work,
        // which no snapshot of the other person's includes.
        let restore_points = kept
            .iter()
            .filter(|s| versions.contains(&s.version))
            .count();
        assert_eq!(restore_points, 3, "client {i}");
    }
}

#[test]
fn deleting_the_newest_snapshot_does_not_bring_it_back() {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let alice = author("user:alice");
    let mut auto = AutoSnapshots::new(10 * MIN);
    let mut c = Clock::new();
    tick(&mut auto, &mut history, &stage, &alice, &c);
    touch(&mut stage, &alice, 1);
    c.advance(10 * MIN);
    let first = tick(&mut auto, &mut history, &stage, &alice, &c).expect("due");
    touch(&mut stage, &alice, 2);
    c.advance(10 * MIN);
    let second = tick(&mut auto, &mut history, &stage, &alice, &c).expect("due");

    assert!(history.delete_checkpoint(&second, &alice).unwrap());
    for _ in 0..60 {
        c.advance(Duration::from_secs(5));
        assert_eq!(tick(&mut auto, &mut history, &stage, &alice, &c), None);
    }
    let ids: Vec<CheckpointId> = history.checkpoints().into_iter().map(|c| c.id).collect();
    assert_eq!(ids, [first]);
    // A change is snapshotted at once (the remaining one is old enough).
    touch(&mut stage, &alice, 3);
    c.advance(Duration::from_secs(5));
    assert!(tick(&mut auto, &mut history, &stage, &alice, &c).is_some());

    // Deleted by a collaborator: neither client makes it again.
    let mut net = Net::new(2);
    let mut c = Clock::new();
    net.tick(0, &c);
    net.tick(1, &c);
    let who = net.who[0].clone();
    touch(&mut net.stages[0], &who, 1);
    net.sync(&[0, 1]);
    c.advance(10 * MIN);
    let made = net.tick(0, &c).expect("due");
    net.sync(&[0, 1]);
    assert_eq!(net.tick(1, &c), None, "client 1 has it");
    c.advance(MIN);
    let who = net.who[1].clone();
    assert!(net.histories[1].delete_checkpoint(&made, &who).unwrap());
    for _ in 0..60 {
        net.sync(&[0, 1]);
        c.advance(MIN);
        assert_eq!((net.tick(0, &c), net.tick(1, &c)), (None, None));
    }
    assert!(net.histories[0].checkpoints().is_empty());
}

#[test]
fn layer_stack_changes_are_snapshotted() {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let alice = author("user:alice");
    let mut auto = AutoSnapshots::new(10 * MIN);
    let mut c = Clock::new();
    tick(&mut auto, &mut history, &stage, &alice, &c);
    touch(&mut stage, &alice, 1);
    c.advance(10 * MIN);
    tick(&mut auto, &mut history, &stage, &alice, &c).expect("due");
    let original = stage.layer_stack().to_vec();
    let mut snapshot = |stage: &Stage, c: &mut Clock| {
        c.advance(10 * MIN);
        let id = tick(&mut auto, &mut history, stage, &alice, c)?;
        Some(history.checkpoint(&id).unwrap().layer_stack)
    };

    stage.move_sublayer(&sl("anim/a"), 1).unwrap();
    let reordered = stage.layer_stack().to_vec();
    assert_ne!(reordered, original);
    assert_eq!(snapshot(&stage, &mut c), Some(reordered));
    // Back to an arrangement a checkpoint already has: nothing new.
    stage.move_sublayer(&sl("anim/a"), 0).unwrap();
    assert_eq!(snapshot(&stage, &mut c), None);

    stage.remove_sublayer(&sl("anim/a")).unwrap();
    assert_eq!(snapshot(&stage, &mut c), Some(vec![SublayerId::root()]));
    // A sublayer added (for example a collaborator's, once the app puts it in
    // the stack) is a change too.
    stage.create_sublayer(sl("anim/b"), 0).unwrap();
    assert_eq!(
        snapshot(&stage, &mut c),
        Some(vec![sl("anim/b"), SublayerId::root()])
    );
}

#[test]
fn a_project_nobody_changes_gets_no_snapshot() {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let alice = author("user:alice");
    let mut auto = AutoSnapshots::new(10 * MIN);
    let mut c = Clock::new();
    for _ in 0..360 {
        assert_eq!(tick(&mut auto, &mut history, &stage, &alice, &c), None);
        c.advance(MIN);
    }
    assert!(history.checkpoints().is_empty());
    assert!(history.version().is_empty(), "nothing was written");
    touch(&mut stage, &alice, 1);
    assert!(tick(&mut auto, &mut history, &stage, &alice, &c).is_some());
}

#[test]
fn a_note_from_an_older_client_keeps_a_snapshot() {
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
    let noted = tick(&mut auto, &mut history, &stage, &alice, &c).unwrap();

    // An older client gives it a note and keeps the name.
    let old = History::from_snapshot(&history.export_snapshot().unwrap()).unwrap();
    raw_checkpoint(&old, &noted)
        .insert("note", "客户确认")
        .unwrap();
    old.doc().commit();
    history
        .import(&old.export_updates(&history.version()).unwrap())
        .unwrap();
    let cp = history.checkpoint(&noted).unwrap();
    assert_eq!(
        (cp.kind, cp.note.as_str()),
        (CheckpointKind::Named, "客户确认")
    );

    touch(&mut stage, &alice, 2);
    c.advance(10 * MIN);
    let newer = tick(&mut auto, &mut history, &stage, &alice, &c).unwrap();
    touch(&mut stage, &alice, 3);
    c.advance(10 * MIN);
    tick(&mut auto, &mut history, &stage, &alice, &c).unwrap();
    assert!(history.checkpoint(&newer).is_none(), "expired");
    auto.prune(&mut history, &alice, c.wall + 365 * DAY, c.mono + 365 * DAY)
        .unwrap();
    assert!(history.checkpoint(&noted).is_some(), "never pruned");
}

#[test]
fn versions_include_sublayers_they_do_not_list_only_when_empty() {
    let vv = |n: i32| {
        let mut v = VersionVector::new();
        if n > 0 {
            v.insert(7, n);
        }
        v
    };
    let version = |layers: &[(&str, i32)]| ProjectVersion {
        sublayers: layers.iter().map(|(id, n)| (sl(id), vv(*n))).collect(),
    };
    let checkpoint = version(&[("root", 3)]);
    // A collaborator's new sublayer with ops: not included.
    assert!(!checkpoint.includes(&version(&[("root", 3), ("anim/b", 1)])));
    assert!(version(&[("root", 3), ("anim/b", 1)]).includes(&checkpoint));
    // An empty one is.
    assert!(checkpoint.includes(&version(&[("root", 3), ("anim/b", 0)])));
    assert!(!checkpoint.includes(&version(&[("root", 4)])));
}

#[test]
fn reverting_to_an_automatic_snapshot_names_it_in_the_undo_label() {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let alice = author("user:alice");
    let mut auto = AutoSnapshots::new(10 * MIN);
    auto.utc_offset_minutes = 8 * 60;
    let mut c = Clock::new();
    tick(&mut auto, &mut history, &stage, &alice, &c);
    touch(&mut stage, &alice, 1);
    c.advance(10 * MIN);
    let id = tick(&mut auto, &mut history, &stage, &alice, &c).unwrap();
    touch(&mut stage, &alice, 2);
    history
        .revert_to(&mut stage, &alice, &id, &RevertScope::Project)
        .unwrap();
    assert_eq!(
        stage.undo_label(&alice),
        Some("revert to automatic snapshot 2026-11-27 00:10 UTC")
    );
    let named = history
        .create_checkpoint(&stage, &alice, "给客户的第 1 版", "")
        .unwrap();
    assert_eq!(named.revert_label(), "revert to 给客户的第 1 版");
}

/// A snapshot thins the older ones of its period in the commit that makes it.
#[test]
fn a_new_snapshot_thins_its_period_at_once() {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let alice = author("user:alice");
    let mut auto = AutoSnapshots::new(10 * MIN);
    auto.retention = Retention {
        tiers: vec![
            RetentionTier {
                until: MIN,
                every: None,
            },
            RetentionTier {
                until: DAY,
                every: Some(HOUR),
            },
        ],
    };
    let mut c = Clock::new();
    tick(&mut auto, &mut history, &stage, &alice, &c);
    for minute in [10, 20, 30] {
        touch(&mut stage, &alice, minute);
        c.advance(10 * MIN);
        let id = tick(&mut auto, &mut history, &stage, &alice, &c).expect("due");
        let left: Vec<CheckpointId> = history.checkpoints().into_iter().map(|c| c.id).collect();
        assert_eq!(left, [id], "minute {minute}");
        assert_eq!(last_label(&history), "checkpoint.auto");
    }
}

/// Bob's clock is right, Alice's runs two hours ahead, Carol's a day ahead.
/// Alice snapshots a state at minute 20; Bob, who has not received her
/// snapshot, snapshots the same state at minute 120. Hers looks newer (her
/// clock) but was made 100 minutes earlier, so it expires first: it must not
/// replace his, because Carol's snapshot, the newest by its time, does not
/// include that state.
#[test]
fn a_snapshot_that_only_looks_newer_does_not_replace_a_later_one() {
    let short = Retention {
        tiers: vec![
            RetentionTier {
                until: HOUR,
                every: None,
            },
            RetentionTier {
                until: DAY,
                every: Some(DAY),
            },
        ],
    };
    for (retention, until) in [(short, DAY), (Retention::default(), 30 * DAY)] {
        let start = Clock::new();
        let at = |t: Duration, skew: Duration| Clock {
            wall: start.wall + t + skew,
            mono: start.mono + t,
        };
        let who = [
            author("user:alice"),
            author("user:bob"),
            author("user:carol"),
        ];
        let skew = [2 * HOUR, Duration::ZERO, DAY];
        let (alice, bob, carol) = (0, 1, 2);
        // One project everybody has (it syncs at once here); the histories
        // do not.
        let mut stage = client_stage(&base_snapshots());
        let mut h: Vec<History> = (0..3).map(|_| History::new()).collect();
        let mut autos: Vec<AutoSnapshots> = (0..3)
            .map(|_| {
                let mut a = AutoSnapshots::new(10 * MIN);
                a.retention = retention.clone();
                a
            })
            .collect();
        let mut tick_at = |i: usize, h: &mut [History], stage: &Stage, t: Duration| {
            tick(&mut autos[i], &mut h[i], stage, &who[i], &at(t, skew[i]))
        };
        for i in 0..3 {
            assert_eq!(tick_at(i, &mut h, &stage, Duration::ZERO), None);
        }
        touch(&mut stage, &who[alice], 1);
        let z = tick_at(carol, &mut h, &stage, 10 * MIN).expect("carol's");
        touch(&mut stage, &who[alice], 2);
        let y = tick_at(alice, &mut h, &stage, 20 * MIN).expect("alice's");
        let s = tick_at(bob, &mut h, &stage, 120 * MIN).expect("bob's");
        let (a, rest) = h.split_at_mut(1);
        for _ in 0..2 {
            for other in rest.iter_mut() {
                merge(&mut a[0], other);
            }
        }
        let v2 = h[bob].checkpoint(&s).unwrap().version;
        assert!(!h[carol].checkpoint(&z).unwrap().version.includes(&v2));
        assert!(h[alice].checkpoint(&y).unwrap().version.includes(&v2));
        for i in 0..3 {
            assert_eq!(tick_at(i, &mut h, &stage, 120 * MIN), None);
        }

        // Bob prunes a little later; Alice once hers is `until` old.
        let t = 200 * MIN;
        autos[bob]
            .prune(
                &mut h[bob],
                &who[bob],
                at(t, skew[bob]).wall,
                at(t, skew[bob]).mono,
            )
            .unwrap();
        let (a, b) = h.split_at_mut(1);
        merge(&mut a[0], &mut b[0]);
        let t = 20 * MIN + until;
        let c = at(t, skew[alice]);
        let gone = autos[alice]
            .prune(&mut h[alice], &who[alice], c.wall, c.mono)
            .unwrap();
        assert_eq!(gone, std::slice::from_ref(&y), "until {until:?}");
        assert!(
            h[alice].checkpoint(&s).is_some(),
            "until {until:?}: Bob's snapshot, 100 minutes younger, was deleted"
        );
        // It goes once it is that old itself (nothing newer includes it).
        let t = 120 * MIN + until + MIN;
        let c = at(t, skew[alice]);
        let gone = autos[alice]
            .prune(&mut h[alice], &who[alice], c.wall, c.mono)
            .unwrap();
        assert_eq!(gone, [s], "until {until:?}");
        assert_eq!(h[alice].auto_snapshots().len(), 1);
    }
}

/// Bob works alone for two days, then leaves. Alice opens the project for an
/// hour once a week, each time with a new `AutoSnapshots`. Bob's snapshots
/// age with hers, which were made after them: they stay while they are
/// younger than 30 days and then go, instead of staying forever because each
/// session first sees them as new.
#[test]
fn a_departed_collaborators_snapshots_still_expire() {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let (alice, bob) = (author("user:alice"), author("user:bob"));
    let mut c = Clock::new();
    let mut auto = AutoSnapshots::new(10 * MIN);
    tick(&mut auto, &mut history, &stage, &bob, &c);
    for step in 1..=2 * 24 * 6 {
        touch(&mut stage, &bob, step);
        c.advance(10 * MIN);
        tick(&mut auto, &mut history, &stage, &bob, &c).expect("every interval");
    }
    let bobs = |h: &History| -> Vec<Checkpoint> { h.checkpoints_where(|s| s.author == bob) };
    assert!(bobs(&history).len() > 20);
    let left = c.wall_ms();

    for week in 1..=12 {
        c.advance(7 * DAY - HOUR);
        let mut auto = AutoSnapshots::new(10 * MIN);
        tick(&mut auto, &mut history, &stage, &alice, &c);
        for minute in 1..=60 {
            touch(&mut stage, &alice, minute);
            c.advance(MIN);
            tick(&mut auto, &mut history, &stage, &alice, &c);
        }
        auto.prune(&mut history, &alice, c.wall, c.mono).unwrap();
        let bob_left = bobs(&history);
        let age = Duration::from_millis((c.wall_ms() - left) as u64);
        if age < 30 * DAY {
            assert!(!bob_left.is_empty(), "week {week}: deleted early");
        }
        if week >= 6 {
            assert!(
                bob_left.is_empty(),
                "week {week}: {} of Bob's snapshots, his last {age:?} old",
                bob_left.len()
            );
        }
    }
    assert!(history.auto_snapshots().len() <= 6 + 24 + 30);
}

/// Ten snapshots that differ only in the layer stack; the stack is put back to
/// the ninth arrangement, and all but the newest expire in one prune. The
/// ninth one's cover must be among the remembered ones, or the unchanged
/// project is snapshotted again.
#[test]
fn deleting_many_snapshots_at_once_does_not_bring_the_newest_back() {
    let mut stage = client_stage(&base_snapshots());
    stage.create_sublayer(sl("x/b"), 0).unwrap();
    stage.create_sublayer(sl("x/c"), 0).unwrap();
    let ids = [SublayerId::root(), sl("anim/a"), sl("x/b"), sl("x/c")];
    let mut history = History::new();
    let alice = author("user:alice");
    let mut auto = AutoSnapshots::new(10 * MIN);
    auto.retention = Retention {
        tiers: vec![RetentionTier {
            until: 3 * HOUR,
            every: None,
        }],
    };
    let mut c = Clock::new();
    tick(&mut auto, &mut history, &stage, &alice, &c);
    let mut stacks = Vec::new();
    for i in 0..10 {
        let order = [i % 4, (i / 4 + 1 + i % 4) % 4];
        stage.move_sublayer(&ids[order[0]], 3).unwrap();
        stage.move_sublayer(&ids[order[1]], 0).unwrap();
        if stacks.contains(&stage.layer_stack().to_vec()) {
            stage.move_sublayer(&ids[i % 4], 1).unwrap();
        }
        assert!(!stacks.contains(&stage.layer_stack().to_vec()), "{i}");
        stacks.push(stage.layer_stack().to_vec());
        c.advance(10 * MIN);
        tick(&mut auto, &mut history, &stage, &alice, &c).expect("a stack change");
    }
    // Back to the ninth arrangement; four hours later all but the newest
    // expire at once.
    for (k, id) in stacks[8].iter().enumerate() {
        stage.move_sublayer(id, k).unwrap();
    }
    assert_eq!(stage.layer_stack(), stacks[8]);
    c.advance(4 * HOUR);
    let gone = auto.prune(&mut history, &alice, c.wall, c.mono).unwrap();
    assert_eq!(gone.len(), 9);
    c.advance(MIN);
    assert_eq!(
        tick(&mut auto, &mut history, &stage, &alice, &c),
        None,
        "the ninth one, deleted, holds the project as it is"
    );
}

/// Layer stacks are each client's own. Bob's has the same sublayers as
/// Alice's in another order and never changes; only Alice edits, in bursts.
/// Neither makes a snapshot of a version a checkpoint (with the other's
/// stack) already includes.
#[test]
fn another_clients_layer_stack_is_not_a_change() {
    let mut net = Net::new(2);
    for auto in &mut net.autos {
        auto.retention = Retention::keep_all();
    }
    net.stages[1].move_sublayer(&sl("anim/a"), 1).unwrap();
    assert_ne!(net.stages[0].layer_stack(), net.stages[1].layer_stack());
    let mut c = Clock::new();
    let (mut n, mut made, mut nothing_new) = (0, 0, 0);
    for _burst in 0..5 {
        for minute in 0..120 {
            if minute < 30 {
                n += 1;
                let who = net.who[0].clone();
                touch(&mut net.stages[0], &who, n);
            }
            net.sync(&[0, 1]);
            for i in [0, 1] {
                let current = ProjectVersion::current(&net.stages[i]);
                let held = net.histories[i]
                    .checkpoints()
                    .iter()
                    .any(|cp| cp.version.includes(&current));
                if net.tick(i, &c).is_some() {
                    made += 1;
                    nothing_new += u32::from(held);
                }
            }
            net.sync(&[0, 1]);
            c.advance(MIN);
        }
    }
    assert!(made >= 15, "{made}");
    assert_eq!(nothing_new, 0, "of {made}");

    // A stack change of one's own is a change.
    net.stages[1].move_sublayer(&sl("anim/a"), 0).unwrap();
    c.advance(HOUR);
    assert!(net.tick(1, &c).is_some());
}

#[test]
fn renaming_a_snapshot_an_older_client_renamed_keeps_its_id() {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let alice = author("user:alice");
    let mut auto = AutoSnapshots::new(10 * MIN);
    let mut c = Clock::new();
    tick(&mut auto, &mut history, &stage, &alice, &c);
    touch(&mut stage, &alice, 1);
    c.advance(10 * MIN);
    let id = tick(&mut auto, &mut history, &stage, &alice, &c).unwrap();
    let auto_name = history.checkpoint(&id).unwrap().name;

    // An older client renames it in place: a named version from then on.
    let old = History::from_snapshot(&history.export_snapshot().unwrap()).unwrap();
    raw_checkpoint(&old, &id).insert("name", "客户版").unwrap();
    old.doc().commit();
    history
        .import(&old.export_updates(&history.version()).unwrap())
        .unwrap();
    assert_eq!(history.checkpoint(&id).unwrap().kind, CheckpointKind::Named);

    // Two people rename it concurrently: the same ID, last writer wins.
    let mut copies: Vec<History> = (0..2)
        .map(|_| History::from_snapshot(&history.export_snapshot().unwrap()).unwrap())
        .collect();
    for (h, name) in copies.iter_mut().zip(["A", "B"]) {
        assert_eq!(h.rename_checkpoint(&id, &alice, name, None).unwrap(), id);
    }
    let (a, b) = copies.split_at_mut(1);
    merge(&mut a[0], &mut b[0]);
    for h in &copies {
        let named = h.named_checkpoints();
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].id, id);
        assert_eq!(named[0].name, copies[0].named_checkpoints()[0].name);
    }

    // Given its automatic name back, it stays a named version.
    let h = &mut copies[0];
    h.rename_checkpoint(&id, &alice, &auto_name, Some(""))
        .unwrap();
    let cp = h.checkpoint(&id).unwrap();
    assert_eq!((cp.kind, cp.name), (CheckpointKind::Named, auto_name));
}

/// Random collaborators whose clocks are from 50 minutes to a day apart, with
/// short retentions (mostly the same everywhere), going offline, and in turns
/// editing, reordering their layer stacks (four sublayers) and keeping
/// snapshots for an hour, then leaving the project alone for two while
/// pruning by hand. After every minute, on every client: no snapshot it has
/// seen made disappeared before it was the shortest retention old (in real
/// time) while no checkpoint includes its version, and no kept (renamed)
/// snapshot disappeared. Then everyone idles online: in the second half nobody
/// writes. A few seconds; `AUTO_SIM_SEEDS=<n>` (from `AUTO_SIM_START`) runs
/// more.
#[test]
fn random_collaborators_keep_every_snapshot_until_it_expires() {
    let var = |k: &str, d: u64| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let start = var("AUTO_SIM_START", 0);
    for seed in start..start + var("AUTO_SIM_SEEDS", 8) {
        random_collaborators(seed);
    }
}

fn random_collaborators(seed: u64) {
    struct Made {
        id: CheckpointId,
        by: usize,
        t: Duration,
        version: ProjectVersion,
        written: VersionVector,
    }
    let mut rng = Rng::new(seed);
    let n = 3 + rng.below(2);
    let mut net = Net::new(n);
    for stage in &mut net.stages {
        stage.create_sublayer(sl("x/b"), 0).unwrap();
        stage.create_sublayer(sl("x/c"), 0).unwrap();
    }
    // A right clock, one 50 minutes behind, one a day ahead (whose old
    // snapshots look newest), maybe one two hours ahead. Clocks differ by
    // more than anyone is offline plus a duplicate window: concurrent
    // duplicates are only made on clocks that agree (see `prune_plan`).
    const DAY_MS: i64 = 24 * 60 * 60_000;
    let mut skews: Vec<i64> = vec![0, -50 * 60_000, DAY_MS, 120 * 60_000];
    skews.truncate(n);
    for k in (1..n).rev() {
        skews.swap(k, rng.below(k + 1));
    }
    let retention = |rng: &mut Rng| {
        let last = [10, 15, 30, 60][rng.below(4)] * MIN;
        let every = [None, Some(MIN), Some(5 * MIN), Some(HOUR), Some(DAY)][rng.below(5)];
        Retention {
            tiers: vec![
                RetentionTier {
                    until: last / [2, 3][rng.below(2)],
                    every: None,
                },
                RetentionTier { until: last, every },
            ],
        }
    };
    // Mostly the same settings everywhere.
    let shared = rng.chance(70).then(|| retention(&mut rng));
    for auto in &mut net.autos {
        auto.interval = [1, 3, 10][rng.below(3)] * MIN;
        auto.retention = shared.clone().unwrap_or_else(|| retention(&mut rng));
    }
    let until = net
        .autos
        .iter()
        .map(|a| a.retention.tiers[1].until)
        .min()
        .unwrap();
    let base = Clock::new();
    let clock = |t: Duration, i: usize| Clock {
        wall: UNIX_EPOCH + Duration::from_millis((T0_MS + skews[i] + t.as_millis() as i64) as u64),
        mono: base.mono + t,
    };
    let mut offline_until = vec![Duration::ZERO; n];
    let p_edit = [5, 20, 50][rng.below(3)];
    let mut made: Vec<Made> = Vec::new();
    let mut kept: Vec<(CheckpointId, VersionVector)> = Vec::new();
    let mut edits = 0;
    let fail = |msg: String| panic!("seed {seed} (skews {skews:?}): {msg}");
    let steps = 360;
    let idle = 180;
    for step in 0..steps + idle {
        let t = Duration::from_secs(60 * step as u64);
        let busy = step < steps;
        // An hour of work, then two of quiet.
        let working = busy && step % 180 < 60;
        let online: Vec<usize> = (0..n).filter(|&i| !busy || offline_until[i] <= t).collect();
        let before: Vec<VersionVector> = net.histories.iter().map(History::version).collect();
        for i in 0..n {
            if busy {
                // The one a day ahead goes away for up to four hours, the
                // others for up to 20 minutes.
                let away = if skews[i] == DAY_MS { 240 } else { 20 };
                if offline_until[i] <= t && rng.chance(2) {
                    offline_until[i] = t + Duration::from_secs(60 * (1 + rng.below(away) as u64));
                }
            }
            if working {
                if rng.chance(p_edit) {
                    edits += 1;
                    let who = net.who[i].clone();
                    touch(&mut net.stages[i], &who, edits);
                }
                if rng.chance(5) {
                    let len = net.stages[i].layer_stack().len();
                    let id = net.stages[i].layer_stack()[rng.below(len)].clone();
                    net.stages[i].move_sublayer(&id, rng.below(len)).unwrap();
                }
                let autos = net.histories[i].auto_snapshots();
                if rng.chance(1) && !autos.is_empty() {
                    let pick = &autos[rng.below(autos.len())];
                    let who = net.who[i].clone();
                    let id = net.histories[i]
                        .rename_checkpoint(&pick.id, &who, "kept", None)
                        .unwrap();
                    kept.push((id, net.histories[i].version()));
                }
            }
            if busy && rng.chance(3) {
                let (c, who) = (clock(t, i), net.who[i].clone());
                net.autos[i]
                    .prune(&mut net.histories[i], &who, c.wall, c.mono)
                    .unwrap();
            }
            if let Some(id) = net.tick(i, &clock(t, i)) {
                let cp = net.histories[i].checkpoint(&id).unwrap();
                made.push(Made {
                    id,
                    by: i,
                    t,
                    version: cp.version,
                    written: net.histories[i].version(),
                });
            }
        }
        net.sync(&online);
        if !busy && step >= steps + idle / 2 {
            for i in 0..n {
                let me = net.histories[i].doc().peer_id();
                if before[i].get(&me) != net.histories[i].version().get(&me) {
                    fail(format!("client {i} wrote while idle at minute {step}"));
                }
            }
        }
        for (i, h) in net.histories.iter().enumerate() {
            let has = h.version();
            let present = h.checkpoints();
            for (id, vv) in &kept {
                if has.includes_vv(vv) && !present.iter().any(|c| c.id == *id) {
                    fail(format!("client {i}: kept {id} was deleted"));
                }
            }
            for a in made.iter().filter(|a| has.includes_vv(&a.written)) {
                if t - a.t < until && !present.iter().any(|c| c.version.includes(&a.version)) {
                    fail(format!(
                        "client {i} at minute {step}: {} by client {} at minute {} gone after {:?} with nothing including it",
                        a.id,
                        a.by,
                        a.t.as_secs() / 60,
                        t - a.t
                    ));
                }
            }
        }
    }
    assert!(made.len() > 5, "seed {seed}: {}", made.len());
}
