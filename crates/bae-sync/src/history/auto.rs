//! Automatic snapshots (`docs/spec/sync.md` §5.5): checkpoints of kind
//! [`CheckpointKind::Auto`](super::CheckpointKind::Auto) that
//! [`AutoSnapshots::tick`] makes while the project changes, so people can go
//! back in time without naming versions, thinned out by a deterministic
//! [`Retention`] policy.
//!
//! Every collaborator's client ticks. The history document syncs, so a client
//! only makes a snapshot when nobody has made one for an interval; the client
//! that made the latest one makes the next, the others only take over half an
//! interval later. Snapshots made concurrently anyway (within sync latency) are
//! deduplicated deterministically.
//!
//! Pruning never trusts one machine's wall clock with other people's snapshots
//! (their age is bounded by the monotonic time since they arrived), and only
//! drops a snapshot that a newer one includes, so the work of people who were
//! offline at the same time is kept per period on both sides.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bae_model::loro::VersionVector;
use bae_model::{AuthorId, Stage, SublayerId};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{CheckpointId, History, ProjectVersion};
use crate::error::SyncError;

const MINUTE: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(3600);
const DAY: Duration = Duration::from_secs(24 * 3600);

/// One step of a [`Retention`] policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetentionTier {
    /// Applies to snapshots younger than this (and at least as old as the
    /// previous tier's `until`).
    pub until: Duration,
    /// Keep only the newest snapshot of each period this long, counted in UTC
    /// from the Unix epoch so every client cuts the same periods; `None`: keep
    /// all. Each tier's period should be a multiple of the previous one's.
    pub every: Option<Duration>,
}

/// How long automatic snapshots are kept. Named versions are never pruned.
///
/// Default: every snapshot of the last hour, then the newest of each hour for
/// a day, then the newest of each day (UTC) for 30 days.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Retention {
    /// Youngest first, `until` increasing. Snapshots older than the last tier's
    /// `until` are deleted, except the newest snapshot, which is always kept.
    /// No tiers: keep everything.
    pub tiers: Vec<RetentionTier>,
}

impl Default for Retention {
    fn default() -> Self {
        Retention {
            tiers: vec![
                RetentionTier {
                    until: HOUR,
                    every: None,
                },
                RetentionTier {
                    until: DAY,
                    every: Some(HOUR),
                },
                RetentionTier {
                    until: 30 * DAY,
                    every: Some(DAY),
                },
            ],
        }
    }
}

impl Retention {
    /// Never prune by age (concurrent duplicates are still dropped).
    pub fn keep_all() -> Self {
        Retention { tiers: Vec::new() }
    }

    /// The tier for a snapshot `age_ms` old (`None`: expired).
    fn tier(&self, age_ms: i64) -> Option<RetentionTier> {
        if self.tiers.is_empty() {
            return Some(RetentionTier {
                until: Duration::MAX,
                every: None,
            });
        }
        self.tiers.iter().copied().find(|t| age_ms < ms(t.until))
    }
}

/// Counters for checking that idle ticks stay cheap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AutoSnapshotStats {
    /// Ticks while enabled.
    pub ticks: u64,
    /// Times the history document was read (only after it changed).
    pub scans: u64,
    /// Times the project's current version was computed (only when a snapshot
    /// was due and the stage had changed since the last check).
    pub version_checks: u64,
    /// Snapshots this client made.
    pub created: u64,
    /// Snapshots this client deleted.
    pub pruned: u64,
}

/// Automatic snapshots of one open project: call [`AutoSnapshots::tick`]
/// periodically (every few seconds is fine; idle ticks are cheap).
///
/// A snapshot is made when both:
/// - nobody's automatic snapshot is younger than `interval`, as this client
///   measures it (the client that made the latest one waits `interval`, others
///   half an interval longer, so with several collaborators online normally
///   only one of them makes it), and
/// - the project changed since every checkpoint of any kind (a named version
///   counts): no checkpoint has the current layer stack and a version that
///   includes the current one. Checkpoints deleted while this client watched
///   (by anyone) still count, and so does the project as this client first saw
///   it when there was no checkpoint, so nothing is written for a project
///   nobody changes.
///
/// It is one commit to the history document labelled `checkpoint.auto` (with
/// the pruning it causes) and touches neither project data nor any undo stack.
/// It takes `&Stage`, so it never runs inside an open transaction.
///
/// Keep one per open project: it remembers which snapshots this client made
/// and when it first saw the others.
#[derive(Debug)]
pub struct AutoSnapshots {
    /// Off: `tick` does nothing.
    pub enabled: bool,
    /// Time between snapshots while the project changes (preference; default
    /// 10 minutes).
    pub interval: Duration,
    pub retention: Retention,
    /// Offset from UTC, in minutes, of the time in the fallback names older
    /// clients show (the app's local offset; default 0).
    pub utc_offset_minutes: i32,
    state: State,
}

impl Default for AutoSnapshots {
    fn default() -> Self {
        Self::new(10 * MINUTE)
    }
}

/// When this client first saw an automatic snapshot, and how old it was then.
#[derive(Clone, Copy, Debug)]
struct Seen {
    at: Instant,
    age: Duration,
}

impl Seen {
    fn age(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.at) + self.age
    }
}

/// An automatic snapshot as the policy sees it.
#[derive(Clone, Debug)]
struct Snap {
    id: CheckpointId,
    author: AuthorId,
    created_ms: i64,
    prev: Option<CheckpointId>,
    version: ProjectVersion,
}

impl Snap {
    /// Total order used by every pruning rule: newer is greater.
    fn key(&self) -> (i64, &CheckpointId) {
        (self.created_ms, &self.id)
    }
}

/// What a checkpoint holds: the project at `version` with `stack`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Cover {
    version: ProjectVersion,
    stack: Vec<SublayerId>,
}

impl Cover {
    fn of(stage: &Stage) -> Self {
        Cover {
            version: ProjectVersion::current(stage),
            stack: stage.layer_stack().to_vec(),
        }
    }

    /// Whether the project at `other` shows nothing this does not.
    fn includes(&self, other: &Cover) -> bool {
        self.stack == other.stack && self.version.includes(&other.version)
    }
}

/// How many covers of deleted checkpoints are remembered.
const GONE_MAX: usize = 8;

#[derive(Debug, Default)]
struct State {
    /// Version of the history document the fields below were read at.
    read_at: Option<VersionVector>,
    /// All checkpoints (any kind), newest first.
    covers: Vec<(CheckpointId, Cover)>,
    /// What checkpoints held that disappeared while this client watched
    /// (deleted, or pruned with nothing newer that includes them), and the
    /// project at the first tick when there was no checkpoint: none of it is
    /// snapshotted again. Only ones no other includes, oldest first.
    gone: Vec<Cover>,
    /// Automatic snapshots.
    snaps: Vec<Snap>,
    seen: FxHashMap<CheckpointId, Seen>,
    /// Snapshots this client made.
    mine: FxHashSet<CheckpointId>,
    /// The automatic snapshot that is youngest as this client measures it.
    latest: Option<CheckpointId>,
    /// First enabled tick: the timer when there is no automatic snapshot.
    started: Option<Instant>,
    /// Stage revision at which the project was found unchanged since some
    /// checkpoint (cleared when the history changes).
    unchanged_at: Option<u64>,
    stats: AutoSnapshotStats,
}

impl State {
    fn covered(&self, c: &Cover) -> bool {
        self.covers.iter().map(|(_, x)| x).any(|x| x.includes(c))
            || self.gone.iter().any(|x| x.includes(c))
    }

    fn remember_gone(&mut self, c: Cover) {
        if self.covered(&c) {
            return;
        }
        self.gone.retain(|x| !c.includes(x));
        self.gone.push(c);
        if self.gone.len() > GONE_MAX {
            self.gone.remove(0);
        }
    }
}

impl AutoSnapshots {
    pub fn new(interval: Duration) -> Self {
        AutoSnapshots {
            enabled: true,
            interval,
            retention: Retention::default(),
            utc_offset_minutes: 0,
            state: State::default(),
        }
    }

    pub fn stats(&self) -> AutoSnapshotStats {
        self.state.stats
    }

    /// Earliest time the next snapshot can be made (it also needs a change);
    /// `None` before the first tick or while disabled.
    pub fn due_at(&self) -> Option<Instant> {
        if !self.enabled {
            return None;
        }
        let st = &self.state;
        match st.latest.as_ref() {
            Some(id) => {
                let seen = st.seen.get(id)?;
                Some(seen.at + self.wait(id).saturating_sub(seen.age))
            }
            None => Some(st.started? + self.interval),
        }
    }

    /// Make an automatic snapshot if one is due (see [`AutoSnapshots`]), and
    /// prune by the retention policy. Returns the new snapshot's ID.
    ///
    /// `now_wall` is stored as the snapshot's creation time and drives
    /// retention; `now` (monotonic) drives the interval, so a wrong wall clock
    /// on any machine can neither stop nor flood snapshots, and limits how old
    /// other authors' snapshots are taken to be (see [`AutoSnapshots::prune`]).
    pub fn tick(
        &mut self,
        history: &mut History,
        stage: &Stage,
        author: &AuthorId,
        now_wall: SystemTime,
        now: Instant,
    ) -> Result<Option<CheckpointId>, SyncError> {
        if !self.enabled {
            return Ok(None);
        }
        self.state.stats.ticks += 1;
        let first = self.state.started.is_none();
        let started = *self.state.started.get_or_insert(now);
        let now_ms = unix_ms(now_wall);
        let arrived = self.refresh(history, now_ms, now);
        if first && self.state.covers.is_empty() {
            // Nothing to compare with: the project as it is now counts as
            // covered, so only a change makes the first snapshot.
            self.state.stats.version_checks += 1;
            self.state.remember_gone(Cover::of(stage));
        }
        if arrived && self.made_latest() {
            // Checkpoints arrived: drop concurrent duplicates right away. Only
            // the maker of the latest snapshot does, so not every client writes
            // the same deletions.
            let plan = self.plan(author, now_ms, now);
            if !plan.is_empty() {
                self.remove(history, &plan)?;
                history.commit(author, "checkpoint.prune");
                self.refresh(history, now_ms, now);
            }
        }

        let due = match self.state.latest.as_ref() {
            Some(id) => self.state.seen[id].age(now) >= self.wait(id),
            None => now.saturating_duration_since(started) >= self.interval,
        };
        if !due || self.state.unchanged_at == Some(stage.revision()) {
            return Ok(None);
        }
        self.state.stats.version_checks += 1;
        let current = Cover::of(stage);
        if self.state.covered(&current) {
            // A stage event bumps the revision; ops that change nothing visible
            // wait for the next one.
            self.state.unchanged_at = Some(stage.revision());
            return Ok(None);
        }

        let name = auto_snapshot_name(now_ms, self.utc_offset_minutes);
        let prev = self.state.latest.clone();
        let cp = history.write_auto_snapshot(
            stage,
            author,
            now_ms,
            name,
            current.version,
            prev.as_ref(),
        )?;
        let st = &mut self.state;
        st.stats.created += 1;
        st.mine.insert(cp.id.clone());
        st.seen.insert(
            cp.id.clone(),
            Seen {
                at: now,
                age: Duration::ZERO,
            },
        );
        st.snaps.push(Snap {
            id: cp.id.clone(),
            author: cp.author,
            created_ms: cp.created_ms,
            prev,
            version: cp.version,
        });
        let mut plan = self.plan(author, now_ms, now);
        plan.retain(|id| *id != cp.id);
        self.remove(history, &plan)?;
        history.commit(author, "checkpoint.auto");
        self.refresh(history, now_ms, now);
        Ok(Some(cp.id))
    }

    /// Apply the retention policy to `history` now (`tick` does this whenever
    /// it makes a snapshot); returns the deleted snapshots.
    ///
    /// Ages come from `now_wall` only for `author`'s own snapshots. Another
    /// author's are at most as old as their newest snapshot's creation time
    /// plus the time (by `now`) since this client first saw that one, so a
    /// wall clock that runs ahead never prunes other people's snapshots early.
    /// Clients with the same settings and clock that saw the snapshots arrive
    /// at the same moments delete the same.
    pub fn prune(
        &mut self,
        history: &mut History,
        author: &AuthorId,
        now_wall: SystemTime,
        now: Instant,
    ) -> Result<Vec<CheckpointId>, SyncError> {
        let now_ms = unix_ms(now_wall);
        self.refresh(history, now_ms, now);
        let plan = self.plan(author, now_ms, now);
        if !plan.is_empty() {
            self.remove(history, &plan)?;
            history.commit(author, "checkpoint.prune");
            self.refresh(history, now_ms, now);
        }
        Ok(plan)
    }

    /// How long after the latest snapshot `latest` the next one is due.
    fn wait(&self, latest: &CheckpointId) -> Duration {
        if self.state.mine.contains(latest) {
            self.interval
        } else {
            self.interval + self.interval / 2
        }
    }

    fn made_latest(&self) -> bool {
        self.state
            .latest
            .as_ref()
            .is_some_and(|l| self.state.mine.contains(l))
    }

    /// The pruning plan for `author` (see [`AutoSnapshots::prune`]).
    fn plan(&self, author: &AuthorId, now_ms: i64, now: Instant) -> Vec<CheckpointId> {
        let st = &self.state;
        // The time each other author's clock has at least reached.
        let mut clocks: FxHashMap<&AuthorId, i64> = FxHashMap::default();
        for s in st.snaps.iter().filter(|s| s.author != *author) {
            let since = st
                .seen
                .get(&s.id)
                .map_or(0, |seen| ms(now.saturating_duration_since(seen.at)));
            let t = s.created_ms.saturating_add(since);
            clocks
                .entry(&s.author)
                .and_modify(|c| *c = (*c).max(t))
                .or_insert(t);
        }
        prune_plan(&st.snaps, &self.retention, self.interval / 2, |s| {
            clocks.get(&s.author).map_or(now_ms, |&t| t.min(now_ms))
        })
    }

    fn remove(&mut self, history: &History, plan: &[CheckpointId]) -> Result<(), SyncError> {
        let n = history.remove_checkpoints(plan)?;
        self.state.stats.pruned += n as u64;
        Ok(())
    }

    /// Read the history document again if it changed; returns whether it did.
    fn refresh(&mut self, history: &History, now_ms: i64, now: Instant) -> bool {
        let vv = history.version();
        let st = &mut self.state;
        if st.read_at.as_ref() == Some(&vv) {
            return false;
        }
        st.stats.scans += 1;
        let entries = history.entries();
        let old = std::mem::replace(
            &mut st.covers,
            entries
                .iter()
                .rev()
                .map(|(cp, _)| {
                    let cover = Cover {
                        version: cp.version.clone(),
                        stack: cp.layer_stack.clone(),
                    };
                    (cp.id.clone(), cover)
                })
                .collect(),
        );
        let present: FxHashSet<CheckpointId> = st.covers.iter().map(|(id, _)| id.clone()).collect();
        for (id, cover) in old {
            if !present.contains(&id) {
                st.remember_gone(cover);
            }
        }
        st.snaps = entries
            .into_iter()
            .filter(|(cp, _)| cp.is_auto())
            .map(|(cp, prev)| Snap {
                id: cp.id,
                author: cp.author,
                created_ms: cp.created_ms,
                prev,
                version: cp.version,
            })
            .collect();
        let present: FxHashSet<&CheckpointId> = st.snaps.iter().map(|s| &s.id).collect();
        st.seen.retain(|id, _| present.contains(id));
        st.mine.retain(|id| present.contains(id));
        for s in &st.snaps {
            // Back-dated by its wall-clock age (never into the future), so a
            // snapshot received late or found when the project is opened counts
            // from when it was made, and one from a machine whose clock runs
            // ahead counts from now.
            st.seen.entry(s.id.clone()).or_insert_with(|| Seen {
                at: now,
                age: Duration::from_millis(now_ms.saturating_sub(s.created_ms).max(0) as u64),
            });
        }
        let seen = &st.seen;
        st.latest = st
            .snaps
            .iter()
            .min_by(|a, b| {
                seen[&a.id]
                    .age(now)
                    .cmp(&seen[&b.id].age(now))
                    .then_with(|| b.key().cmp(&a.key()))
            })
            .map(|s| s.id.clone());
        st.unchanged_at = None;
        st.read_at = Some(vv);
        true
    }
}

/// The automatic snapshots to delete, oldest first; `now_ms(s)` is the time it
/// is now for snapshot `s`'s age.
///
/// Only the stored snapshots, the policy, `window` and `now_ms` matter, so
/// clients with the same settings and clock agree. A snapshot `x` is deleted
/// when it is not the newest (by creation time, then ID) and:
/// - **duplicate**: another snapshot `y` with the same `prev` (made
///   concurrently) is newer, less than `window` younger, and includes `x`'s
///   version; or
/// - **thinned**: its tier keeps one snapshot per period and a newer snapshot
///   in the same period includes `x`'s version (so concurrent snapshots of
///   different branches, such as two people's work offline, are each kept per
///   period until one including both is made); or
/// - **expired**: it is older than the last tier.
///
/// Except for expiry, a deletion always needs a newer snapshot that includes
/// it, so pruners with different views, clocks or settings can never delete
/// each other's representatives into nothing: following "deleted because of"
/// links only goes to newer snapshots that include the deleted one and ends at
/// one that is kept (or expired too).
fn prune_plan(
    snaps: &[Snap],
    retention: &Retention,
    window: Duration,
    now_ms: impl Fn(&Snap) -> i64,
) -> Vec<CheckpointId> {
    let Some(newest) = snaps.iter().max_by(|a, b| a.key().cmp(&b.key())) else {
        return Vec::new();
    };
    let periods: Vec<i64> = retention
        .tiers
        .iter()
        .filter_map(|t| t.every)
        .map(ms)
        .filter(|&p| p > 0)
        .collect();
    let mut in_period: FxHashMap<(i64, i64), Vec<&Snap>> = FxHashMap::default();
    for s in snaps {
        for &p in &periods {
            in_period
                .entry((p, s.created_ms.div_euclid(p)))
                .or_default()
                .push(s);
        }
    }
    let mut by_prev: FxHashMap<Option<&CheckpointId>, Vec<&Snap>> = FxHashMap::default();
    for s in snaps {
        by_prev.entry(s.prev.as_ref()).or_default().push(s);
    }
    let window = ms(window);
    let mut out: Vec<&Snap> = snaps
        .iter()
        .filter(|s| s.id != newest.id)
        .filter(|s| {
            let Some(tier) = retention.tier(now_ms(s).saturating_sub(s.created_ms).max(0)) else {
                return true;
            };
            let covers = |y: &&Snap| y.key() > s.key() && y.version.includes(&s.version);
            let thinned = tier.every.map(ms).filter(|&p| p > 0).is_some_and(|p| {
                in_period
                    .get(&(p, s.created_ms.div_euclid(p)))
                    .is_some_and(|ys| ys.iter().any(covers))
            });
            thinned
                || by_prev[&s.prev.as_ref()]
                    .iter()
                    .filter(|y| y.created_ms.saturating_sub(s.created_ms) < window)
                    .any(covers)
        })
        .collect();
    out.sort_by(|a, b| a.key().cmp(&b.key()));
    out.into_iter().map(|s| s.id.clone()).collect()
}

/// The name older clients show for an automatic snapshot made at
/// `created_ms`: `自动快照 2026-11-27 14:30`, in the time zone `utc_offset_minutes`
/// east of UTC. Newer clients show a localized label instead.
pub fn auto_snapshot_name(created_ms: i64, utc_offset_minutes: i32) -> String {
    format!(
        "自动快照 {}",
        format_minutes(created_ms, utc_offset_minutes)
    )
}

/// `2026-11-27 14:30`: the date and time `utc_offset_minutes` east of UTC.
pub(super) fn format_minutes(created_ms: i64, utc_offset_minutes: i32) -> String {
    let minutes = created_ms.div_euclid(60_000) + i64::from(utc_offset_minutes);
    let (y, m, d) = civil_from_days(minutes.div_euclid(24 * 60));
    let t = minutes.rem_euclid(24 * 60);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", t / 60, t % 60)
}

/// Proleptic Gregorian date of a day count since 1970-01-01 (H. Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn unix_ms(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => ms(d),
        Err(e) => -ms(e.duration()),
    }
}

fn ms(d: Duration) -> i64 {
    i64::try_from(d.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A version with `a` ops of peer 1 and `b` of peer 2.
    fn version2(a: i32, b: i32) -> ProjectVersion {
        let mut vv = VersionVector::new();
        for (peer, n) in [(1, a), (2, b)] {
            if n > 0 {
                vv.insert(peer, n);
            }
        }
        ProjectVersion {
            sublayers: [(SublayerId::root(), vv)].into_iter().collect(),
        }
    }

    fn version(counter: i32) -> ProjectVersion {
        version2(counter, 0)
    }

    fn snap(id: &str, created_ms: i64, prev: Option<&str>, counter: i32) -> Snap {
        Snap {
            id: CheckpointId(id.to_owned()),
            author: AuthorId::system(),
            created_ms,
            prev: prev.map(|p| CheckpointId(p.to_owned())),
            version: version(counter),
        }
    }

    fn ids(names: &[&str]) -> Vec<CheckpointId> {
        names
            .iter()
            .map(|n| CheckpointId((*n).to_owned()))
            .collect()
    }

    const M: i64 = 60_000;
    const H: i64 = 3_600_000;
    const D: i64 = 24 * H;
    /// 2026-11-27 00:00 UTC.
    const T0: i64 = 1_795_737_600_000;

    #[test]
    fn names_use_the_given_offset() {
        assert_eq!(auto_snapshot_name(T0, 0), "自动快照 2026-11-27 00:00");
        assert_eq!(
            auto_snapshot_name(T0 + 6 * H + 30 * 60_000, 8 * 60),
            "自动快照 2026-11-27 14:30"
        );
        assert_eq!(auto_snapshot_name(T0 - 1, 0), "自动快照 2026-11-26 23:59");
        assert_eq!(auto_snapshot_name(0, 0), "自动快照 1970-01-01 00:00");
        assert_eq!(
            auto_snapshot_name(951_782_400_000, 0),
            "自动快照 2000-02-29 00:00"
        );
    }

    #[test]
    fn duplicates_need_the_same_prev_a_newer_one_and_an_included_version() {
        let w = Duration::from_secs(300);
        let r = Retention::keep_all();
        let now = |_: &Snap| T0;
        // Concurrent (same prev), equal versions: the older goes.
        let s = [snap("a", T0, None, 5), snap("b", T0 + 1000, None, 5)];
        assert_eq!(prune_plan(&s, &r, w, now), ids(&["a"]));
        // The newer one lacks ops of the older: both stay.
        let s = [snap("a", T0, None, 6), snap("b", T0 + 1000, None, 5)];
        assert!(prune_plan(&s, &r, w, now).is_empty());
        // Made one after the other (different prev): both stay.
        let s = [snap("a", T0, None, 5), snap("b", T0 + 1000, Some("a"), 5)];
        assert!(prune_plan(&s, &r, w, now).is_empty());
        // Too far apart to be the same moment.
        let s = [snap("a", T0, None, 5), snap("b", T0 + 400_000, None, 5)];
        assert!(prune_plan(&s, &r, w, now).is_empty());
    }

    #[test]
    fn thinning_keeps_each_branch_until_a_snapshot_includes_it() {
        let w = Duration::from_secs(300);
        let r = Retention::default();
        let now = |_: &Snap| T0 + 3 * H;
        // Two people's work offline in the same hour: concurrent versions.
        let mut a = snap("a", T0 + 10 * M, None, 0);
        a.version = version2(5, 0);
        let mut b = snap("b", T0 + 20 * M, Some("x"), 0);
        b.version = version2(0, 3);
        let s = [a.clone(), b.clone()];
        assert!(prune_plan(&s, &r, w, now).is_empty());
        // A later one in that hour includes both.
        let mut c = snap("c", T0 + 30 * M, Some("b"), 0);
        c.version = version2(5, 3);
        assert_eq!(prune_plan(&[a, b, c], &r, w, now), ids(&["a", "b"]));
    }

    #[test]
    fn each_snapshot_is_aged_by_its_own_clock() {
        let w = Duration::from_secs(300);
        let r = Retention::default();
        let mut s = [
            snap("a", T0, None, 1),
            snap("b", T0 + 10 * M, Some("a"), 2),
            snap("c", T0 + 20 * M, Some("b"), 3),
        ];
        let other = AuthorId::new("user:other").unwrap();
        s[0].author = other.clone();
        s[1].author = other.clone();
        // Expired for one clock, an hour old for the other.
        let now = |x: &Snap| {
            if x.author == other {
                T0 + 30 * M
            } else {
                T0 + 40 * D
            }
        };
        assert!(prune_plan(&s, &r, w, now).is_empty());
        assert_eq!(prune_plan(&s, &r, w, |_| T0 + 40 * D), ids(&["a", "b"]));
    }

    /// Random snapshots (with concurrent versions) pruned by two clients with
    /// different clocks and settings. In the union of their deletions the
    /// newest snapshot is kept, every deleted snapshot that is not expired has
    /// a newer kept one that includes its version, and the newest snapshot of
    /// each day is kept unless it expired or is a concurrent duplicate of a
    /// newer one.
    #[test]
    fn concurrent_pruners_never_delete_into_nothing() {
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut rnd = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let (mut deleted, mut deduped) = (0, 0);
        for round in 0..300 {
            let n = 2 + rnd(80) as usize;
            let mut snaps: Vec<Snap> = Vec::new();
            for i in 0..n {
                let id = format!("{round:03}-{i:03}");
                if !snaps.is_empty() && rnd(4) == 0 {
                    // Made concurrently with another one, within two minutes.
                    let other = snaps[rnd(snaps.len() as u64) as usize].clone();
                    let mut s = other.clone();
                    s.id = CheckpointId(id);
                    s.created_ms = other.created_ms + rnd(120_000) as i64;
                    if rnd(3) == 0 {
                        s.version = version2(rnd(4) as i32, rnd(2) as i32);
                    }
                    snaps.push(s);
                    continue;
                }
                let created = T0 - rnd(40) as i64 * D + rnd(D as u64) as i64;
                let prev = (!snaps.is_empty() && rnd(3) > 0)
                    .then(|| snaps[rnd(snaps.len() as u64) as usize].id.0.clone());
                let mut s = snap(&id, created, prev.as_deref(), 0);
                s.version = version2(rnd(4) as i32, rnd(2) as i32);
                snaps.push(s);
            }
            let a_window = Duration::from_secs(60 * (1 + rnd(30)));
            let b_window = Duration::from_secs(60 * (1 + rnd(30)));
            let a_now = T0 + rnd(3 * D as u64) as i64;
            let b_now = T0 + rnd(3 * D as u64) as i64;
            let mut b_retention = Retention::default();
            if rnd(2) == 0 {
                b_retention.tiers[0].until = 3 * HOUR;
            }
            let a = prune_plan(&snaps, &Retention::default(), a_window, |_| a_now);
            let b = prune_plan(&snaps, &b_retention, b_window, |_| b_now);
            let gone: FxHashSet<&CheckpointId> = a.iter().chain(&b).collect();
            deleted += gone.len();
            deduped += prune_plan(&snaps, &Retention::keep_all(), a_window, |_| a_now).len();
            let kept: Vec<&Snap> = snaps.iter().filter(|s| !gone.contains(&s.id)).collect();
            let newest = snaps.iter().max_by(|x, y| x.key().cmp(&y.key())).unwrap();
            assert!(kept.iter().any(|s| s.id == newest.id), "round {round}");
            // The later clock expires the most.
            let expiry = a_now.max(b_now) - 30 * D;
            let max_window = ms(a_window.max(b_window));
            for s in snaps.iter().filter(|s| gone.contains(&s.id)) {
                assert!(
                    s.created_ms <= expiry
                        || kept
                            .iter()
                            .any(|k| k.key() > s.key() && k.version.includes(&s.version)),
                    "round {round}: {} deleted with nothing newer kept that includes it",
                    s.id
                );
            }
            let mut days: FxHashMap<i64, &Snap> = FxHashMap::default();
            for s in &snaps {
                let e = days.entry(s.created_ms.div_euclid(D)).or_insert(s);
                if s.key() > e.key() {
                    *e = s;
                }
            }
            for m in days.values() {
                let duplicate = snaps.iter().any(|y| {
                    y.prev == m.prev
                        && y.key() > m.key()
                        && y.created_ms - m.created_ms < max_window
                        && y.version.includes(&m.version)
                });
                assert!(
                    m.created_ms <= expiry || duplicate || !gone.contains(&m.id),
                    "round {round}: the newest of its day, {}, was thinned",
                    m.id
                );
            }
        }
        assert!(deleted > 1000 && deduped > 300, "{deleted} {deduped}");
    }
}
