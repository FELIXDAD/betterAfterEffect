//! Randomized simulation of automatic snapshots (`docs/spec/sync.md` §5.5)
//! between 2–4 collaborators that sync the stage and the history through a
//! `Relay` over a `SimNet` (random delay up to 20 ticks, sometimes 5% loss and
//! 10% duplicates). Clients have their own intervals, retentions and wall
//! clocks (up to three days off), and at random edit, change their layer
//! stacks, create named versions, keep (rename) and delete snapshots, prune by
//! hand, go offline, and close and reopen the project. After the busy steps
//! everyone comes back and idles online.
//!
//! Checked on every client after every step (`BAE_AUTO_SIM_CHECK_EVERY`):
//! named versions and kept snapshots are never deleted, and no snapshot it has
//! seen made disappears before it is the shortest last retention tier old (in
//! real time) while no checkpoint includes its version (unless someone deleted
//! a snapshot that included it). Checked on every tick:
//! nothing is written unless a snapshot was made or something arrived; no
//! snapshot of a state a present checkpoint holds; no second snapshot within
//! the interval unless a checkpoint was deleted; no named version or newest
//! snapshot is pruned, and nothing expires early by the pruner's settings. At
//! the end: nobody writes in the second half of the idle period, and all
//! histories are the same.
//!
//! Seeds are deterministic (fixed Loro peer IDs, distinct clock skews). About
//! 5–20 s per seed in debug and 2–7 s in release, so the run is ignored by
//! default; the default 64 seeds take about a minute in release on 4 threads
//! (3–4 minutes in debug):
//!
//! ```text
//! cargo test --release -p bae-sync --test auto_snapshots_sim -- --ignored --nocapture
//! ```
//!
//! The pruning rules before "sync: thin snapshots only by later ones and age
//! them with successors" fail seed 59.
//!
//! Environment (all optional):
//! - `BAE_AUTO_SIM_SEEDS` (64), `BAE_AUTO_SIM_START` (0): the seeds to run.
//! - `BAE_AUTO_SIM_THREADS` (available parallelism): seeds run in parallel.
//! - `BAE_AUTO_SIM_STEPS` (600): busy steps per seed.
//! - `BAE_AUTO_SIM_MODE`: `storm` (4 clients, 1–2 minute intervals, 15 s
//!   steps, many renames) or `long` (10–60 minute intervals, default-like
//!   retentions, 10 minute steps; use `BAE_AUTO_SIM_CHECK_EVERY=6`).
//! - `BAE_AUTO_SIM_CHECK_EVERY` (1): check every n-th busy step.
//! - `BAE_AUTO_SIM_NOSKEW`: all clocks right (a millisecond apart).
//! - `BAE_AUTO_SIM_IDLE_MANUAL`: also prune by hand while idle.
//! - `BAE_AUTO_SIM_VERBOSE`: print every seed's settings and counts, not only
//!   failing ones.
//! - `BAE_AUTO_SIM_TRACE`: print snapshots, deletions and manual prunes as
//!   they happen; with `BAE_AUTO_SIM_TRACE_TICK=<client>` also that client's
//!   ticks.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bae_model::loro::VersionVector;
use bae_model::{AuthorId, Op, PrimType, Specifier, Stage, Sublayer, SublayerId};
use bae_sync::{
    AutoSnapshots, Checkpoint, CheckpointId, CheckpointKind, ClientId, DocKey, History, PeerHandle,
    ProjectDocs, ProjectVersion, Relay, Retention, RetentionTier, SessionConfig, SyncSession,
};
use common::*;

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;
const T0_MS: i64 = 1_795_737_600_000;

fn dur(ms: i64) -> Duration {
    Duration::from_millis(ms as u64)
}

/// Run settings from the `BAE_AUTO_SIM_*` environment (see the module doc).
#[derive(Clone, Debug, Default)]
struct Config {
    storm: bool,
    long: bool,
    noskew: bool,
    check_every: u64,
    idle_manual: bool,
    verbose: bool,
    trace: bool,
    trace_tick: Option<usize>,
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(format!("BAE_AUTO_SIM_{name}")).ok()
}

fn env_num<T: std::str::FromStr>(name: &str, default: T) -> T {
    env_var(name)
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

impl Config {
    fn from_env() -> Self {
        let mode = env_var("MODE").unwrap_or_default();
        Config {
            storm: mode == "storm",
            long: mode == "long",
            noskew: env_var("NOSKEW").is_some(),
            check_every: env_num("CHECK_EVERY", 1).max(1),
            idle_manual: env_var("IDLE_MANUAL").is_some(),
            verbose: env_var("VERBOSE").is_some(),
            trace: env_var("TRACE").is_some(),
            trace_tick: env_var("TRACE_TICK").and_then(|s| s.parse().ok()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Net {
    Online,
    Offline(i64),
    Closed(i64),
}

struct Client {
    stage: Stage,
    history: History,
    session: SyncSession,
    end: Option<SimEnd>,
    handle: Option<PeerHandle>,
    auto: AutoSnapshots,
    interval_ms: i64,
    retention: Retention,
    who: AuthorId,
    skew_ms: i64,
    net: Net,
    known: BTreeSet<SublayerId>,
    n: u32,
    nsub: u32,
    last_tick_vv: Option<VersionVector>,
    last_made: Option<i64>,
    deleted_since_made: bool,
    tick_ids: BTreeSet<CheckpointId>,
    disconnected_at: Option<i64>,
    /// Covers of checkpoints this client saw disappear (for re-creation counts).
    gone_seen: Vec<(ProjectVersion, Vec<SublayerId>)>,
    present_ids: BTreeSet<CheckpointId>,
    present_covers: BTreeMap<CheckpointId, (ProjectVersion, Vec<SublayerId>)>,
}

struct AutoRec {
    id: CheckpointId,
    author: usize,
    t: i64,
    created_ms: i64,
    version: ProjectVersion,
    vv: VersionVector,
}

struct NamedRec {
    id: CheckpointId,
    vv: VersionVector,
}

struct Restore {
    client: usize,
    t: i64,
    v_pre: ProjectVersion,
    s_pre: usize,
}

#[derive(Default, Debug, Clone)]
struct Stats {
    created: u64,
    pruned: u64,
    renames: u64,
    named: u64,
    user_deletes: u64,
    offline: u64,
    closes: u64,
    recreated: u64,
    fast: u64,
    idle_writes_late: u64,
    restore_lost_unexplained: u64,
    restore_lost_skew: u64,
    max_present: usize,
}

struct Sim {
    seed: u64,
    rng: Rng,
    net: SimNet,
    relay: Relay,
    base: Instant,
    t: i64,
    step_ms: i64,
    c: Vec<Client>,
    autos: Vec<AutoRec>,
    auto_idx: BTreeMap<CheckpointId, usize>,
    named: Vec<NamedRec>,
    user_deleted: Vec<usize>,
    restores: Vec<Restore>,
    restore_reported: BTreeSet<(usize, usize)>,
    min_until: Option<i64>,
    periods: Vec<i64>,
    max_window: i64,
    allow_user_delete: bool,
    violations: Vec<String>,
    stats: Stats,
    p_edit: u32,
    nstep: u64,
    idle: bool,
    peers: u64,
    check_every: u64,
    p_rename: u32,
    p_offline: u32,
    idle_manual: bool,
    trace: bool,
    trace_tick: Option<usize>,
}

fn key(c: &Checkpoint) -> (i64, &CheckpointId) {
    (c.created_ms, &c.id)
}

fn random_retention(rng: &mut Rng) -> Retention {
    match rng.below(100) {
        0..=19 => Retention::default(),
        20..=29 => Retention::keep_all(),
        _ => {
            let n = 1 + rng.below(3);
            let mut tiers = Vec::new();
            let mut until = [5 * MIN, 15 * MIN, HOUR, 3 * HOUR][rng.below(4)];
            let periods = [MIN, 5 * MIN, 10 * MIN, HOUR, DAY];
            let mut min_p = 0;
            for _ in 0..n {
                let every = if rng.chance(40) {
                    None
                } else {
                    let choices: Vec<usize> = (min_p..periods.len()).collect();
                    let k = choices[rng.below(choices.len())];
                    min_p = k;
                    Some(dur(periods[k]))
                };
                tiers.push(RetentionTier {
                    until: dur(until),
                    every,
                });
                until *= 2 + rng.below(7) as i64;
            }
            Retention { tiers }
        }
    }
}

impl Sim {
    fn new(seed: u64, cfg: &Config) -> Self {
        let mut rng = Rng::new(seed);
        let (storm, long) = (cfg.storm, cfg.long);
        let n = if storm { 4 } else { 2 + rng.below(3) };
        let conditions = Conditions {
            drop_percent: if rng.chance(25) { 5 } else { 0 },
            duplicate_percent: if rng.chance(25) { 10 } else { 0 },
            max_delay: [0, 2, 6, 20][rng.below(4)],
        };
        let net = SimNet::new(seed ^ 0xabcdef, conditions);
        let mut relay = Relay::new(ClientId::new("hub").unwrap());
        let base_snap = base_snapshots();
        let base = Instant::now();
        let mut c = Vec::new();
        let mut peers_used: u64 = 0;
        let mut next_peer = || {
            peers_used += 1;
            (seed << 24) | peers_used
        };
        for i in 0..n {
            let interval_ms = if storm {
                [MIN, 2 * MIN][rng.below(2)]
            } else if long {
                [10 * MIN, 30 * MIN, HOUR][rng.below(3)]
            } else {
                [MIN, 3 * MIN, 10 * MIN, 30 * MIN][rng.below(4)]
            };
            let retention = if long && rng.chance(70) {
                let mut r = Retention::default();
                if rng.chance(50) {
                    r.tiers[2].until = dur([2 * DAY, 5 * DAY, 10 * DAY][rng.below(3)]);
                }
                r
            } else if storm && rng.chance(50) {
                Retention {
                    tiers: vec![
                        RetentionTier {
                            until: dur(5 * MIN),
                            every: Some(dur(MIN)),
                        },
                        RetentionTier {
                            until: dur(HOUR),
                            every: Some(dur(10 * MIN)),
                        },
                    ],
                }
            } else {
                random_retention(&mut rng)
            };
            let skew_ms = if cfg.noskew {
                rng.below(10);
                0
            } else {
                match rng.below(10) {
                    0..=3 => 0,
                    4..=6 => rng.below(2 * HOUR as usize) as i64 - HOUR,
                    _ => rng.below(6 * DAY as usize) as i64 - 3 * DAY,
                }
            } + i as i64;
            let mut auto = AutoSnapshots::new(dur(interval_ms));
            auto.retention = retention.clone();
            let (a, b) = net.link();
            let handle = relay.connect(Box::new(b));
            let stage = client_stage(&base_snap);
            for layer in stage.sublayers() {
                layer.doc().set_peer_id(next_peer()).unwrap();
            }
            let history = History::new();
            history.doc().set_peer_id(next_peer()).unwrap();
            c.push(Client {
                stage,
                history,
                session: SyncSession::new(SessionConfig::new(
                    ClientId::new(&format!("c{i}")).unwrap(),
                )),
                end: Some(a),
                handle: Some(handle),
                auto,
                interval_ms,
                retention,
                who: author(&format!("user:u{i}")),
                skew_ms,
                net: Net::Online,
                known: [SublayerId::root(), sl("anim/a")].into_iter().collect(),
                n: 0,
                nsub: 0,
                last_tick_vv: None,
                last_made: None,
                deleted_since_made: false,
                tick_ids: BTreeSet::new(),
                disconnected_at: None,
                gone_seen: Vec::new(),
                present_ids: BTreeSet::new(),
                present_covers: BTreeMap::new(),
            });
        }
        let min_until = c
            .iter()
            .filter_map(|c| c.retention.tiers.last().map(|t| t.until.as_millis() as i64))
            .min();
        let mut periods: Vec<i64> = c
            .iter()
            .flat_map(|c| c.retention.tiers.iter().filter_map(|t| t.every))
            .map(|d| d.as_millis() as i64)
            .collect();
        periods.sort();
        periods.dedup();
        let max_window = c.iter().map(|c| c.interval_ms / 2).max().unwrap();
        let step_ms = if storm {
            15_000
        } else if long {
            10 * MIN
        } else {
            [15_000, MIN, 3 * MIN][rng.below(3)]
        };
        let allow_user_delete = rng.chance(30);
        let p_edit = if storm { 60 } else { [5, 20, 50][rng.below(3)] };
        Sim {
            seed,
            rng,
            net,
            relay,
            base,
            t: 0,
            step_ms,
            c,
            autos: Vec::new(),
            auto_idx: BTreeMap::new(),
            named: Vec::new(),
            user_deleted: Vec::new(),
            restores: Vec::new(),
            restore_reported: BTreeSet::new(),
            min_until,
            periods,
            max_window,
            allow_user_delete,
            violations: Vec::new(),
            stats: Stats::default(),
            p_edit,
            nstep: 0,
            idle: false,
            peers: peers_used,
            check_every: cfg.check_every,
            p_rename: if storm {
                6
            } else if long {
                1
            } else {
                2
            },
            p_offline: if storm { 3 } else { 1 },
            idle_manual: cfg.idle_manual,
            trace: cfg.trace,
            trace_tick: cfg.trace_tick,
        }
    }

    fn next_peer(&mut self) -> u64 {
        self.peers += 1;
        (self.seed << 24) | self.peers
    }

    fn describe(&self) -> String {
        let mut s = format!(
            "seed {} n={} step={}s user_delete={} min_until={:?}\n",
            self.seed,
            self.c.len(),
            self.step_ms / 1000,
            self.allow_user_delete,
            self.min_until.map(|u| u / MIN)
        );
        for (i, c) in self.c.iter().enumerate() {
            s.push_str(&format!(
                "  c{i}: interval={}m skew={}m retention={:?}\n",
                c.interval_ms / MIN,
                c.skew_ms / MIN,
                c.retention
                    .tiers
                    .iter()
                    .map(|t| (t.until.as_secs() / 60, t.every.map(|e| e.as_secs() / 60)))
                    .collect::<Vec<_>>()
            ));
        }
        s
    }

    fn wall(&self, i: usize) -> SystemTime {
        UNIX_EPOCH + dur(T0_MS + self.t + self.c[i].skew_ms)
    }

    fn mono(&self) -> Instant {
        self.base + dur(self.t)
    }

    fn violation(&mut self, msg: String) {
        if self.violations.len() < 20 {
            self.violations.push(format!("t={}m: {msg}", self.t / MIN));
        }
    }

    fn disconnect(&mut self, i: usize) {
        if let Some(h) = self.c[i].handle.take() {
            self.relay.disconnect(h);
        }
        self.c[i].end = None;
        self.c[i].disconnected_at.get_or_insert(self.t);
    }

    fn reconnect(&mut self, i: usize) {
        let (a, b) = self.net.link();
        let h = self.relay.connect(Box::new(b));
        let c = &mut self.c[i];
        c.end = Some(a);
        c.handle = Some(h);
        c.session.reset();
        c.net = Net::Online;
        if let Some(since) = c.disconnected_at.take() {
            // Restore point: the newest snapshot this client made while away.
            let s_pre = self
                .autos
                .iter()
                .enumerate()
                .filter(|(_, a)| a.author == i && a.t >= since)
                .filter(|(_, a)| c.present_ids.contains(&a.id))
                .max_by(|(_, a), (_, b)| (a.created_ms, &a.id).cmp(&(b.created_ms, &b.id)))
                .map(|(k, _)| k);
            if let Some(s_pre) = s_pre {
                let mut v_pre = ProjectVersion::current(&c.stage);
                for (id, vv) in &self.autos[s_pre].version.sublayers {
                    v_pre.sublayers.entry(id.clone()).or_default().merge(vv);
                }
                self.restores.push(Restore {
                    client: i,
                    t: self.t,
                    v_pre,
                    s_pre,
                });
            }
        }
    }

    fn reopen(&mut self, i: usize) {
        let c = &mut self.c[i];
        let mut auto = AutoSnapshots::new(dur(c.interval_ms));
        auto.retention = c.retention.clone();
        c.auto = auto;
        c.history = History::from_snapshot(&c.history.export_snapshot().unwrap()).unwrap();
        c.last_tick_vv = None;
        let peer = self.next_peer();
        self.c[i].history.doc().set_peer_id(peer).unwrap();
        self.reconnect(i);
    }

    fn net_transitions(&mut self, idle: bool) {
        for i in 0..self.c.len() {
            match self.c[i].net {
                Net::Online if !idle => {
                    if self.rng.chance(self.p_offline) && self.rng.chance(60) {
                        let len = [10 * MIN, HOUR, 3 * HOUR, DAY, 4 * DAY][self.rng.below(5)];
                        let back = self.t + 1 + self.rng.below(len as usize) as i64;
                        self.disconnect(i);
                        self.stats.offline += 1;
                        self.c[i].net = Net::Offline(back);
                    } else if self.rng.chance(1) && self.rng.chance(30) {
                        let len = [10 * MIN, HOUR, 6 * HOUR][self.rng.below(3)];
                        let back = self.t + 1 + self.rng.below(len as usize) as i64;
                        self.disconnect(i);
                        self.stats.closes += 1;
                        self.c[i].net = Net::Closed(back);
                    }
                }
                Net::Offline(back) if idle || self.t >= back => self.reconnect(i),
                Net::Closed(back) if idle || self.t >= back => self.reopen(i),
                _ => {}
            }
        }
    }

    fn edit(&mut self, i: usize) {
        let rng = &mut self.rng;
        let c = &mut self.c[i];
        let stack = c.stage.layer_stack().to_vec();
        let target = stack[rng.below(stack.len())].clone();
        let tag = format!("c{i}");
        if target == SublayerId::root() || target == sl("anim/a") {
            random_edit(&mut c.stage, &c.who, rng, &tag, &mut c.n);
        } else {
            c.stage.set_edit_target(&target).unwrap();
            c.n += 1;
            let path = p(&format!("/q{tag}_{}", c.n));
            let _ = c.stage.transact(&c.who, "edit", |tx| {
                tx.apply(Op::CreatePrim {
                    path,
                    specifier: Specifier::Def,
                    prim_type: Some(PrimType::new("Folder")),
                    index: None,
                })
            });
        }
    }

    fn stack_change(&mut self, i: usize) {
        let rng = &mut self.rng;
        let c = &mut self.c[i];
        let stack = c.stage.layer_stack().to_vec();
        let roll = rng.below(100);
        if roll < 55 {
            let id = stack[rng.below(stack.len())].clone();
            let _ = c.stage.move_sublayer(&id, rng.below(stack.len()));
        } else if roll < 80 {
            c.nsub += 1;
            let id = sl(&format!("x/c{i}s{}", c.nsub));
            c.known.insert(id.clone());
            c.stage
                .create_sublayer(id.clone(), rng.below(stack.len() + 1))
                .unwrap();
            self.peers += 1;
            let peer = (self.seed << 24) | self.peers;
            c.stage
                .sublayer(&id)
                .unwrap()
                .doc()
                .set_peer_id(peer)
                .unwrap();
        } else {
            let extra: Vec<SublayerId> = stack
                .iter()
                .filter(|s| **s != SublayerId::root() && **s != sl("anim/a"))
                .cloned()
                .collect();
            if !extra.is_empty() {
                let id = extra[rng.below(extra.len())].clone();
                c.stage.remove_sublayer(&id).unwrap();
            }
        }
    }

    fn user_actions(&mut self, i: usize) {
        if self.rng.chance(self.p_edit) {
            self.edit(i);
        }
        if self.rng.chance(2) {
            self.stack_change(i);
        }
        if self.rng.chance(1) {
            let c = &mut self.c[i];
            let cp = c
                .history
                .create_checkpoint(&c.stage, &c.who, "named", "")
                .unwrap();
            let vv = c.history.version();
            self.named.push(NamedRec { id: cp.id, vv });
            self.stats.named += 1;
        }
        if self.rng.chance(self.p_rename) {
            let autos = self.c[i].history.auto_snapshots();
            if !autos.is_empty() {
                let pick = autos[self.rng.below(autos.len())].id.clone();
                let c = &mut self.c[i];
                let note = if self.rng.chance(50) { Some("n") } else { None };
                let id = c
                    .history
                    .rename_checkpoint(&pick, &c.who, "kept", note)
                    .unwrap();
                let vv = c.history.version();
                self.named.push(NamedRec { id, vv });
                self.stats.renames += 1;
            }
        }
        if self.allow_user_delete && self.rng.chance(1) {
            let autos = self.c[i].history.auto_snapshots();
            if !autos.is_empty() {
                let pick = autos[self.rng.below(autos.len())].id.clone();
                let c = &mut self.c[i];
                c.history.delete_checkpoint(&pick, &c.who).unwrap();
                if let Some(&k) = self.auto_idx.get(&pick) {
                    self.user_deleted.push(k);
                }
                self.stats.user_deletes += 1;
            }
        }
    }

    fn tick(&mut self, i: usize) {
        let wall = self.wall(i);
        let mono = self.mono();
        let t = self.t;
        let c = &mut self.c[i];
        let before_vv = c.history.version();
        let arrived = c.last_tick_vv.as_ref() != Some(&before_vv);
        let before = c.history.checkpoints();
        {
            let now_ids: BTreeSet<&CheckpointId> = before.iter().map(|x| &x.id).collect();
            if c.present_ids.iter().any(|id| !now_ids.contains(id))
                || c.tick_ids.iter().any(|id| !now_ids.contains(id))
            {
                c.deleted_since_made = true;
            }
        }
        let cur_v = ProjectVersion::current(&c.stage);
        let cur_s = c.stage.layer_stack().to_vec();
        let covered_present = before
            .iter()
            .any(|x| x.layer_stack == cur_s && x.version.includes(&cur_v));
        let recreation = c
            .gone_seen
            .iter()
            .any(|(v, s)| *s == cur_s && v.includes(&cur_v));
        if self.trace && self.trace_tick == Some(i) {
            let st = c.auto.stats();
            eprintln!(
                "t={}m c{i} tick: rev={} covered_present={covered_present} due_at={:?} stats={st:?} cur={:?}",
                t / MIN,
                c.stage.revision(),
                c.auto
                    .due_at()
                    .map(|d| d.saturating_duration_since(self.base).as_secs() / 60),
                cur_v
                    .sublayers
                    .iter()
                    .map(|(k, v)| (k.as_str().to_owned(), v.values().sum::<i32>()))
                    .collect::<Vec<_>>()
            );
        }
        let pruned_before = c.auto.stats().pruned;
        let manual = self.rng.chance(3) && (!self.idle || self.idle_manual);
        if manual && self.trace {
            eprintln!("t={}m c{i} manual prune", self.t / MIN);
        }
        let made = if manual {
            c.auto.prune(&mut c.history, &c.who, wall, mono).unwrap();
            None
        } else {
            c.auto
                .tick(&mut c.history, &c.stage, &c.who, wall, mono)
                .unwrap()
        };
        self.stats.pruned += c.auto.stats().pruned - pruned_before;
        let after_vv = c.history.version();
        let wrote = after_vv != before_vv;
        c.last_tick_vv = Some(after_vv.clone());
        c.tick_ids = c.history.checkpoints().into_iter().map(|x| x.id).collect();
        let mut msgs = Vec::new();
        if wrote && made.is_none() && !arrived && !manual {
            msgs.push(format!(
                "c{i} wrote to the history in a tick with no arrival and no snapshot"
            ));
        }
        let mut new_cp = None;
        if let Some(id) = &made {
            self.stats.created += 1;
            let deleted_now = {
                let after: BTreeSet<CheckpointId> =
                    c.history.checkpoints().into_iter().map(|x| x.id).collect();
                before.iter().any(|x| !after.contains(&x.id))
            };
            if c.last_made.is_some_and(|m| t - m < c.interval_ms) {
                self.stats.fast += 1;
                if !c.deleted_since_made && !deleted_now {
                    msgs.push(format!(
                        "c{i} made a snapshot {}s after its previous one (interval {}s) with no deletion in between",
                        (t - c.last_made.unwrap()) / 1000,
                        c.interval_ms / 1000
                    ));
                }
            }
            c.last_made = Some(t);
            c.deleted_since_made = false;
            if recreation {
                self.stats.recreated += 1;
            }
            if covered_present {
                msgs.push(format!(
                    "c{i} made snapshot {id} of a state a present checkpoint already holds"
                ));
            }
            let cp = c.history.checkpoint(id).expect("just made");
            if self.trace {
                eprintln!(
                    "t={}m c{i} made {} created={} prev? stack={:?}",
                    t / MIN,
                    &id.0[..12],
                    (cp.created_ms - T0_MS) / MIN,
                    cp.layer_stack
                );
            }
            self.auto_idx.insert(id.clone(), self.autos.len());
            self.autos.push(AutoRec {
                id: id.clone(),
                author: i,
                t,
                created_ms: cp.created_ms,
                version: cp.version.clone(),
                vv: after_vv.clone(),
            });
            new_cp = Some(cp);
        }
        if wrote {
            let after: BTreeSet<CheckpointId> =
                c.history.checkpoints().into_iter().map(|x| x.id).collect();
            let mut pre_autos: Vec<&Checkpoint> = before.iter().filter(|x| x.is_auto()).collect();
            if let Some(cp) = &new_cp {
                pre_autos.push(cp);
            }
            let newest = pre_autos
                .iter()
                .max_by(|a, b| key(a).cmp(&key(b)))
                .map(|x| x.id.clone());
            let last_until = c.retention.tiers.last().map(|t| t.until.as_millis() as i64);
            for x in before.iter().filter(|x| !after.contains(&x.id)) {
                if self.trace {
                    eprintln!("t={}m c{i} deleted {}", t / MIN, &x.id.0[..12]);
                }
                if !x.is_auto() {
                    msgs.push(format!(
                        "c{i} pruned named checkpoint {} ({:?})",
                        x.id, x.kind
                    ));
                    continue;
                }
                if Some(&x.id) == newest.as_ref() {
                    msgs.push(format!("c{i} pruned the newest snapshot {}", x.id));
                }
                let covered = pre_autos
                    .iter()
                    .any(|y| key(y) > key(x) && y.version.includes(&x.version));
                if covered {
                    continue;
                }
                let Some(&k) = self.auto_idx.get(&x.id) else {
                    msgs.push(format!("c{i} deleted unknown snapshot {}", x.id));
                    continue;
                };
                let real_age = t - self.autos[k].t;
                if !last_until.is_some_and(|u| real_age >= u) {
                    msgs.push(format!(
                        "c{i} expired snapshot {} (by c{}, real age {}m, created_ms {}m) early: its last tier ends at {:?}m",
                        x.id,
                        self.autos[k].author,
                        real_age / MIN,
                        (x.created_ms - T0_MS) / MIN,
                        last_until.map(|u| u / MIN)
                    ));
                }
            }
        }
        for m in msgs {
            self.violation(m);
        }
    }

    fn sync_round(&mut self, all: bool) {
        let mono = self.mono();
        for i in 0..self.c.len() {
            if self.c[i].net != Net::Online || !(all || self.rng.chance(70)) {
                continue;
            }
            let c = &mut self.c[i];
            let Some(end) = c.end.as_mut() else { continue };
            let mut docs = ProjectDocs::new(&mut c.stage, &mut c.history);
            let out = c.session.pump_at(&mut docs, end, mono).unwrap();
            for doc in out.unknown_docs {
                if let DocKey::Sublayer(id) = doc
                    && !c.known.contains(&id)
                    && (all || self.rng.chance(40))
                {
                    let len = c.stage.layer_stack().len();
                    let at = self.rng.below(len + 1);
                    c.known.insert(id.clone());
                    if self.trace {
                        eprintln!("t={}m c{i} inserted unknown {id}", self.t / MIN);
                    }
                    let layer = Sublayer::new(id);
                    self.peers += 1;
                    layer
                        .doc()
                        .set_peer_id((self.seed << 24) | self.peers)
                        .unwrap();
                    c.stage.insert_sublayer(layer, at).unwrap();
                }
            }
        }
        self.relay.pump_at(mono);
        self.net.tick();
    }

    /// Invariants on every client's current view.
    fn check(&mut self) {
        let mut msgs = Vec::new();
        let skews: Vec<i64> = self.c.iter().map(|c| c.skew_ms).collect();
        for (k, c) in self.c.iter_mut().enumerate() {
            let vv = c.history.version();
            let present = c.history.checkpoints();
            self.stats.max_present = self.stats.max_present.max(present.len());
            let ids: BTreeSet<CheckpointId> = present.iter().map(|x| x.id.clone()).collect();
            // Track covers that disappeared.
            for (id, cover) in &c.present_covers {
                if !ids.contains(id) {
                    c.gone_seen.push(cover.clone());
                    c.deleted_since_made = true;
                }
            }
            c.present_covers = present
                .iter()
                .map(|x| (x.id.clone(), (x.version.clone(), x.layer_stack.clone())))
                .collect();
            c.present_ids = ids.clone();
            // (1) named versions are never deleted.
            for n in &self.named {
                if vv.includes_vv(&n.vv) {
                    match present.iter().find(|x| x.id == n.id) {
                        None => msgs.push(format!("c{k}: named/kept {} was deleted", n.id)),
                        Some(x) if x.kind != CheckpointKind::Named => {
                            msgs.push(format!("c{k}: named {} has kind {:?}", n.id, x.kind))
                        }
                        _ => {}
                    }
                }
            }
            // (2) every snapshot stays covered until it expires.
            let mut newest_first: Vec<&Checkpoint> = present.iter().collect();
            newest_first.sort_by(|a, b| key(b).cmp(&key(a)));
            for a in &self.autos {
                if ids.contains(&a.id) || !vv.includes_vv(&a.vv) {
                    continue;
                }
                if self.min_until.is_some_and(|u| self.t - a.t >= u) {
                    continue;
                }
                if newest_first.iter().any(|x| x.version.includes(&a.version)) {
                    continue;
                }
                if self
                    .user_deleted
                    .iter()
                    .any(|&u| self.autos[u].version.includes(&a.version))
                {
                    continue;
                }
                // Explained by a newer-by-key, older-in-real-time snapshot that
                // includes it and has expired?
                let why = self
                    .autos
                    .iter()
                    .filter(|e| {
                        (e.created_ms, &e.id) > (a.created_ms, &a.id)
                            && e.t < a.t
                            && e.version.includes(&a.version)
                            && self.min_until.is_some_and(|u| self.t - e.t >= u)
                    })
                    .map(|e| {
                        format!(
                            "c{} made at t={}m created_ms {}m",
                            e.author,
                            e.t / MIN,
                            (e.created_ms - T0_MS) / MIN
                        )
                    })
                    .next();
                msgs.push(format!(
                    "c{k}: snapshot {} by c{} (made at t={}m, created_ms {}m) deleted into nothing; cross-clock: {:?}",
                    a.id,
                    a.author,
                    a.t / MIN,
                    (a.created_ms - T0_MS) / MIN,
                    why
                ));
            }
            // (3, informational) restore points of offline work.
            for (ri, r) in self.restores.iter().enumerate() {
                let s = &self.autos[r.s_pre];
                if !vv.includes_vv(&s.vv) || self.restore_reported.contains(&(ri, k)) {
                    continue;
                }
                if self.min_until.is_some_and(|u| self.t - s.t >= u) {
                    continue;
                }
                let pure = present
                    .iter()
                    .any(|x| x.version.includes(&s.version) && r.v_pre.includes(&x.version));
                if pure {
                    continue;
                }
                // Excused: a merged snapshot in the same period, or a duplicate.
                let mut same_period_real = false;
                let mut same_period_skew = false;
                for y in &self.autos {
                    if (y.created_ms, &y.id) <= (s.created_ms, &s.id)
                        || !y.version.includes(&s.version)
                    {
                        continue;
                    }
                    let dup = y.created_ms - s.created_ms < self.max_window;
                    let period = self
                        .periods
                        .iter()
                        .any(|&p| y.created_ms.div_euclid(p) == s.created_ms.div_euclid(p));
                    if dup || period {
                        // Was the merge in that period of s's author's clock?
                        let merge_ms = T0_MS + r.t + skews[r.client];
                        let merge_in = self
                            .periods
                            .iter()
                            .any(|&p| merge_ms.div_euclid(p) == s.created_ms.div_euclid(p))
                            || merge_ms - s.created_ms < self.max_window;
                        if merge_in {
                            same_period_real = true;
                        } else {
                            same_period_skew = true;
                        }
                    }
                }
                let user = self.user_deleted.iter().any(|&u| {
                    let x = &self.autos[u];
                    x.version.includes(&s.version) && r.v_pre.includes(&x.version)
                });
                if same_period_real || user {
                    continue;
                }
                self.restore_reported.insert((ri, k));
                if same_period_skew {
                    self.stats.restore_lost_skew += 1;
                } else {
                    self.stats.restore_lost_unexplained += 1;
                    if self.trace {
                        eprintln!(
                            "restore point lost: c{k} sees no pure checkpoint for c{}'s offline snapshot {} (reconnected t={}m)",
                            r.client,
                            s.id,
                            r.t / MIN
                        );
                    }
                }
            }
        }
        for m in msgs {
            self.violation(m);
        }
    }

    fn step(&mut self, idle: bool) {
        self.idle = idle;
        self.t += self.step_ms;
        self.net_transitions(idle);
        let mut order: Vec<usize> = (0..self.c.len()).collect();
        for k in (1..order.len()).rev() {
            order.swap(k, self.rng.below(k + 1));
        }
        for &i in &order {
            if matches!(self.c[i].net, Net::Closed(_)) {
                continue;
            }
            if !idle {
                self.user_actions(i);
            }
            if idle || self.rng.chance(90) {
                self.tick(i);
            }
        }
        let rounds = if idle { 3 } else { 1 + self.rng.below(3) };
        for _ in 0..rounds {
            self.sync_round(idle);
        }
        self.nstep += 1;
        if idle || self.nstep.is_multiple_of(self.check_every) {
            self.check();
        }
    }

    fn run(&mut self, steps: usize) {
        for _ in 0..steps {
            self.step(false);
            if self.violations.len() >= 3 {
                return;
            }
        }
        // Everyone back and idle.
        self.net.set_conditions(Conditions::PERFECT);
        let idle_hours = if self.step_ms >= 10 * MIN { 24 } else { 4 };
        let idle_steps = (idle_hours * HOUR / self.step_ms) as usize;
        let mut late = 0;
        for s in 0..idle_steps {
            let before: Vec<VersionVector> = self.c.iter().map(|c| c.history.version()).collect();
            self.step(true);
            if s >= idle_steps / 2 {
                // Writes by this client (its own peer's ops) in the second half.
                let mut msgs = Vec::new();
                for (i, c) in self.c.iter().enumerate() {
                    let me = c.history.doc().peer_id();
                    let a = before[i].get(&me).copied().unwrap_or(0);
                    let b = c.history.version().get(&me).copied().unwrap_or(0);
                    if b != a {
                        late += 1;
                        if late <= 3 {
                            msgs.push(format!("c{i} still writing when idle ({a} -> {b})"));
                        }
                    }
                }
                for m in msgs {
                    self.violation(m);
                }
            }
        }
        self.stats.idle_writes_late = late;
        for _ in 0..20 {
            self.sync_round(true);
        }
        // (4) convergence.
        let sets: Vec<Vec<Checkpoint>> = self.c.iter().map(|c| c.history.checkpoints()).collect();
        for (i, s) in sets.iter().enumerate().skip(1) {
            if *s != sets[0] {
                let a: Vec<&CheckpointId> = sets[0].iter().map(|x| &x.id).collect();
                let b: Vec<&CheckpointId> = s.iter().map(|x| &x.id).collect();
                self.violation(format!("c0 and c{i} differ: {a:?} vs {b:?}"));
            }
        }
        self.check();
    }
}

fn run_seed(seed: u64, steps: usize, cfg: &Config) -> (Vec<String>, Stats, String) {
    let mut sim = Sim::new(seed, cfg);
    sim.run(steps);
    (sim.violations.clone(), sim.stats.clone(), sim.describe())
}

/// The `BAE_AUTO_SIM_*` seeds (64 from 0 by default), in parallel. Prints the
/// settings, counts and violations of every failing seed, then the totals.
#[test]
#[ignore = "a few minutes; run with --ignored --nocapture (see the module doc)"]
fn simulated_collaborators_keep_snapshots_until_they_expire() {
    let cfg = Config::from_env();
    let start: u64 = env_num("START", 0);
    let count: u64 = env_num("SEEDS", 64);
    let steps: usize = env_num("STEPS", 600);
    let parallel = std::thread::available_parallelism().map_or(4, |n| n.get() as u64);
    let threads: u64 = env_num("THREADS", parallel).clamp(1, count.max(1));
    let results = std::sync::Mutex::new(Vec::new());
    let total = std::sync::Mutex::new(Stats::default());
    std::thread::scope(|s| {
        for th in 0..threads {
            let (results, total, cfg) = (&results, &total, &cfg);
            s.spawn(move || {
                let mut seed = start + th;
                while seed < start + count {
                    let (v, st, desc) = run_seed(seed, steps, cfg);
                    {
                        let mut t = total.lock().unwrap();
                        t.created += st.created;
                        t.pruned += st.pruned;
                        t.renames += st.renames;
                        t.named += st.named;
                        t.user_deletes += st.user_deletes;
                        t.offline += st.offline;
                        t.closes += st.closes;
                        t.recreated += st.recreated;
                        t.fast += st.fast;
                        t.idle_writes_late += st.idle_writes_late;
                        t.restore_lost_unexplained += st.restore_lost_unexplained;
                        t.restore_lost_skew += st.restore_lost_skew;
                        t.max_present = t.max_present.max(st.max_present);
                    }
                    if !v.is_empty() || st.restore_lost_unexplained > 0 || cfg.verbose {
                        eprintln!("{desc}{st:?}\n  {}", v.join("\n  "));
                    }
                    if !v.is_empty() {
                        results.lock().unwrap().push((seed, v));
                    }
                    seed += threads;
                }
            });
        }
    });
    eprintln!("totals: {:?}", total.lock().unwrap());
    let r = results.lock().unwrap();
    let mut seeds: Vec<u64> = r.iter().map(|(s, _)| *s).collect();
    seeds.sort_unstable();
    assert!(r.is_empty(), "{} failing seeds: {seeds:?}", r.len());
}
