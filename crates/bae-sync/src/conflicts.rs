//! Conflict notices (`docs/spec/sync.md` §6.4): what a merge did to this
//! client's own edits. Observe only: nothing here writes project data.
//!
//! - [`ConflictTracker::observe`]: fed every [`StageEvent`]. Local edits (origin
//!   `local`, `undo`, `redo`) are remembered element by element for a short
//!   window; a remote import that touches one of them yields a
//!   [`ConcurrentEdit`]: our value was overridden, or it won over someone else's.
//! - [`ConflictTracker::connection_lost`] / [`ConflictTracker::caught_up`] (or
//!   [`merge_summary`]): after working offline, which of our edits were kept,
//!   which were overridden and what the others changed meanwhile
//!   ([`MergeSummary`]). The summary is expensive (snapshots of past
//!   versions); [`ConflictTracker::caught_up_detached`] leaves it to a worker
//!   thread ([`DetachedMerge`]).
//!
//! An *element* is the unit the CRDT merges on its own, last writer wins: an
//! attribute's static value, its expression, or one keyframe (by time). Two
//! people changing different keyframes, or one the value and the other the
//! expression, never overwrite each other and are not reported.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bae_model::loro::{
    ContainerID, Frontiers, ID, IdSpan, Lamport, LoroDoc, LoroValue, PeerID, TreeID, VersionVector,
};
use bae_model::{
    AttrSpec, AuthorId, EventOrigin, Expression, Keyframe, PrimPath, Stage, StageEvent, Sublayer,
    SublayerId, Time, Value,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::error::SyncError;
use crate::history::{self, PrimDiff, ProjectVersion};
use crate::ops::{self, Target, Walker};

/// Part of an attribute opinion that merges on its own (last writer wins).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Element {
    /// The static value.
    Value,
    Expression,
    /// The keyframe at this time.
    Keyframe(Time),
}

impl Element {
    /// This element of an attribute opinion.
    pub fn get(&self, spec: &AttrSpec) -> Option<ElementValue> {
        match self {
            Element::Value => spec.value.clone().map(ElementValue::Value),
            Element::Expression => spec.expression.clone().map(ElementValue::Expression),
            Element::Keyframe(t) => spec.keyframes.get(t).cloned().map(ElementValue::Keyframe),
        }
    }

    /// Content of a stored map value of this element: JSON of a `Value`,
    /// `Expression` or `Keyframe` (bae-model's storage layout).
    pub(crate) fn decode(&self, v: Option<&LoroValue>) -> Option<ElementValue> {
        let LoroValue::String(s) = v? else {
            return None;
        };
        let s: &str = s;
        match self {
            Element::Value => serde_json::from_str::<Value>(s)
                .ok()
                .map(ElementValue::Value),
            Element::Expression => serde_json::from_str::<Expression>(s)
                .ok()
                .map(ElementValue::Expression),
            Element::Keyframe(_) => serde_json::from_str::<Keyframe>(s)
                .ok()
                .map(ElementValue::Keyframe),
        }
    }
}

/// The content of an [`Element`].
#[derive(Clone, Debug, PartialEq)]
pub enum ElementValue {
    Value(Value),
    Expression(Expression),
    Keyframe(Keyframe),
}

/// Settings of a [`ConflictTracker`].
#[derive(Clone, Debug)]
pub struct ConflictConfig {
    /// A remote change of an element arriving within this long after our own
    /// change of it (local clock) is reported.
    pub window: Duration,
    /// Local element edits remembered at most (oldest dropped first).
    pub max_recent: usize,
    /// Local commits with more ops than this (a paste, an import) are not
    /// remembered element by element: they mostly create new prims, which
    /// nobody else can have edited.
    pub max_ops_per_edit: usize,
    /// A notice for an element and outcome that comes less than this after
    /// the previous one for it (reported or held back) is held back: two
    /// people dragging the same slider get one notice each, not one per
    /// frame. Zero reports every one.
    pub repeat_quiet: Duration,
    /// Give up waiting for [`ConflictTracker::caught_up`] this long after the
    /// catch-up began: the first check after the latest
    /// [`connection_lost`](ConflictTracker::connection_lost) that finds ops
    /// of others beyond the version the stage had at that call (only a new
    /// link brings them; a sublayer added to the stack since counts from the
    /// first check that sees it). Then the mark is dropped as by
    /// [`cancel_offline`](ConflictTracker::cancel_offline): online notices
    /// resume, and no summary comes for that reconnect. Counted from the
    /// catch-up, not from the disconnection, so a long offline period still
    /// ends in a summary, also after a catch-up the link dropped again
    /// before it completed (the next `connection_lost` restarts the count).
    /// It covers a catch-up that never completes
    /// (`SyncSession::is_caught_up` stays false while a record that failed to
    /// import, ops Loro holds pending for missing dependencies, or a document
    /// the remote cannot send in one message are listed by the remote;
    /// `SyncSession::behind` lists the documents); where nothing of the
    /// others arrives at all, call
    /// `cancel_offline` (e.g. once the connection has been online for a while
    /// without catching up). `None` (default): wait for `caught_up` or
    /// `cancel_offline`.
    pub catch_up_timeout: Option<Duration>,
}

impl Default for ConflictConfig {
    fn default() -> Self {
        ConflictConfig {
            window: Duration::from_secs(10),
            max_recent: 1024,
            max_ops_per_edit: 2048,
            repeat_quiet: Duration::from_secs(1),
            catch_up_timeout: None,
        }
    }
}

/// What happened to our edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConflictOutcome {
    /// Another author's value won; ours is gone (the history keeps it).
    Overridden,
    /// Our value won over another author's change of the same element. The
    /// other side gets `Overridden`; here it is a softer "Alice also changed
    /// this" hint.
    Kept,
    /// Another author removed the prim.
    PrimRemoved,
}

/// A remote import changed an element we had changed shortly before.
#[derive(Clone, Debug, PartialEq)]
pub struct ConcurrentEdit {
    pub sublayer: SublayerId,
    /// The prim, by tree node (stable across clients and versions).
    pub node: TreeID,
    /// For display: the prim's current path in this client's mirror (for
    /// `PrimRemoved`, the path it had when we edited it).
    pub path: PrimPath,
    pub attr: String,
    pub element: Element,
    pub outcome: ConflictOutcome,
    /// What we set (`None`: we removed it, e.g. deleted the keyframe).
    pub ours: Option<ElementValue>,
    /// The converged content (`None`: no such element, or the prim is gone).
    pub result: Option<ElementValue>,
    /// The other side's value: the winning one (`Overridden`), or the latest
    /// one that lost to ours (`Kept`); `None` for a removal.
    pub theirs: Option<ElementValue>,
    /// Author of the change that holds `result` (for `Kept`, ours). `None`:
    /// unknown (the commit carries no author).
    pub winner: Option<AuthorId>,
    /// Other authors whose changes of this element (or removal of the prim)
    /// arrived with this import.
    pub others: Vec<AuthorId>,
    /// The other change was made without knowing ours (a true race). `false`:
    /// the other author saw our value and changed it afterwards.
    pub concurrent: bool,
    /// When we made our change (local clock).
    pub edited_at: Instant,
    /// When the import arrived.
    pub at: Instant,
}

/// Work done, for diagnostics and the cost tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConflictStats {
    /// Ops of local commits decoded.
    pub local_ops: u64,
    /// Local commits skipped as bulk edits (`max_ops_per_edit`).
    pub bulk_edits: u64,
    /// Remote events observed.
    pub remote_events: u64,
    /// Sublayer checks that ended at the quick test: the document's frontiers
    /// are unchanged and nothing was remembered since its last check.
    pub quick_checks: u64,
    /// Version-vector comparisons of the other checks: one per distinct
    /// version the remembered edits were last compared with (most share one),
    /// plus one per check; never one per remembered edit.
    pub versions_compared: u64,
    /// Remembered elements an import touched and that were analysed.
    pub elements_checked: u64,
    /// Ops of remote commits decoded (only commits that wrote a candidate's map).
    pub remote_ops: u64,
    /// Notices held back as repeats ([`ConflictConfig::repeat_quiet`]).
    pub held_back: u64,
}

/// One remembered local element edit.
#[derive(Clone, Debug)]
struct Recent {
    attr: String,
    element: Element,
    /// The map and key that hold the element.
    container: ContainerID,
    key: String,
    path: PrimPath,
    ours: Option<ElementValue>,
    /// Our latest op on it.
    op: ID,
    author: Option<AuthorId>,
    edited_at: Instant,
    /// Order of remembering (breaks ties of `edited_at`).
    seq: u64,
    /// Remote ops in this version have been compared with this edit: at first
    /// what our op knew (its causal past), after a check the version checked,
    /// shared by all edits of the sublayer.
    checked: Arc<VersionVector>,
}

/// A remote op on an element.
#[derive(Clone, Debug)]
struct RemoteOp {
    id: ID,
    lamport: Lamport,
    author: Option<AuthorId>,
    value: Option<LoroValue>,
    /// Written by the automatic rollback of a failed transaction: it competes
    /// for the element like any write, but is no change anyone asked for.
    rollback: bool,
}

impl RemoteOp {
    fn order(&self) -> (Lamport, u64) {
        (self.lamport, self.id.peer)
    }
}

type ElementKey = (ContainerID, String);

/// An element and outcome, for holding back repeated notices.
type NoticeKey = (SublayerId, TreeID, String, Element, ConflictOutcome);

#[derive(Debug)]
struct Offline {
    since: ProjectVersion,
    at: Instant,
    /// The version at the latest `connection_lost` (`since` stays at the
    /// first): ops of others beyond it came over a link that is back, for
    /// [`ConflictConfig::catch_up_timeout`]. A sublayer it does not list was
    /// added to the stack since, maybe with others' history (a layer file);
    /// the first check that sees it adds it as it is then.
    lost: ProjectVersion,
    /// When a check first found ops of others beyond `lost` (the catch-up
    /// began).
    catching_up: Option<Instant>,
}

/// Where the checks of one sublayer stand.
#[derive(Debug)]
struct Seen {
    /// The document's frontiers at the last check. Unchanged, with nothing
    /// remembered since: nothing to look at.
    frontiers: Frontiers,
    /// The version of the last check, shared by the edits compared up to it.
    version: Arc<VersionVector>,
    /// Edits were remembered since the last check.
    new_edits: bool,
}

/// Turns stage events into [`ConcurrentEdit`] notices and, around a
/// disconnection, a [`MergeSummary`]. One per open stage, on the thread that
/// owns it.
///
/// Feed it every local event (`Stage::transact`, `Stage::undo`,
/// `Stage::redo`, ...) in order, and call [`check`](Self::check) after every
/// `SyncSession::pump` (remote events may also be passed to
/// [`observe`](Self::observe)). Local events need not be observed before the
/// next import: an edit is compared with every remote op its commit did not
/// know. A local event costs its own ops. A check of a sublayer where nothing
/// was committed or imported since the last one compares frontiers and stops;
/// otherwise it costs a pass over that sublayer's remembered edits plus a
/// version-vector comparison per distinct version they were last compared
/// with (usually one), finds which newly arrived ops wrote the maps of
/// remembered edits without decoding them, and decodes only around those.
/// The whole project is never scanned.
///
/// The exception is the end of a reconnect: [`caught_up`](Self::caught_up)
/// computes a [`MergeSummary`], which exports and imports snapshots of past
/// versions of every sublayer that changed. On the UI thread of a large
/// project use [`caught_up_detached`](Self::caught_up_detached) and compute
/// the summary on a worker. If it fails the mark stays: do not retry it
/// every frame.
#[derive(Debug)]
pub struct ConflictTracker {
    config: ConflictConfig,
    /// Remembered local edits by sublayer and prim.
    recent: FxHashMap<SublayerId, FxHashMap<TreeID, Vec<Recent>>>,
    len: usize,
    next_seq: u64,
    last_prune: Option<Instant>,
    /// Our op counter per sublayer at the last local event.
    local_seen: FxHashMap<SublayerId, (u64, i32)>,
    seen: FxHashMap<SublayerId, Seen>,
    /// Last notice (reported or held back) per element and outcome.
    last_notice: FxHashMap<NoticeKey, Instant>,
    offline: Option<Offline>,
    /// Notices of a [`caught_up`](Self::caught_up) whose summary failed,
    /// returned by the next check (or `caught_up`).
    carried: Vec<ConcurrentEdit>,
    stats: ConflictStats,
}

impl ConflictTracker {
    /// A tracker for `stage`, created when the project opens (it notes how far
    /// each sublayer's local history goes, so the first local event is read
    /// exactly). Make a new one when the stage is replaced.
    pub fn new(stage: &Stage, config: ConflictConfig) -> Self {
        let mut t = ConflictTracker {
            config,
            recent: FxHashMap::default(),
            len: 0,
            next_seq: 0,
            last_prune: None,
            local_seen: FxHashMap::default(),
            seen: FxHashMap::default(),
            last_notice: FxHashMap::default(),
            offline: None,
            carried: Vec::new(),
            stats: ConflictStats::default(),
        };
        t.note_sublayers(stage);
        t
    }

    /// Note where the local history of sublayers not seen yet ends.
    fn note_sublayers(&mut self, stage: &Stage) {
        for layer in stage.sublayers() {
            if !self.local_seen.contains_key(layer.id()) {
                let doc = layer.doc();
                let peer = doc.peer_id();
                let end = doc.oplog_vv().get(&peer).copied().unwrap_or(0);
                self.local_seen.insert(layer.id().clone(), (peer, end));
            }
        }
    }

    pub fn config(&self) -> &ConflictConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut ConflictConfig {
        &mut self.config
    }

    pub fn stats(&self) -> &ConflictStats {
        &self.stats
    }

    /// Local element edits currently remembered.
    pub fn remembered(&self) -> usize {
        self.len
    }

    /// [`observe_at`](Self::observe_at) with the current time.
    pub fn observe(&mut self, stage: &Stage, event: &StageEvent) -> Vec<ConcurrentEdit> {
        self.observe_at(stage, event, Instant::now())
    }

    /// Look at one event (`stage` as it is now). Local events update what is
    /// remembered: `now` is taken as the time of the edit, so pass the event
    /// within the frame it happened in. Remote ops imported in between are
    /// still compared with it. A remote import is checked like
    /// [`check_at`](Self::check_at) for its sublayers and returns the
    /// notices. While a reconnect is pending
    /// ([`connection_lost`](Self::connection_lost)), edits made after it are
    /// left to the [`MergeSummary`].
    pub fn observe_at(
        &mut self,
        stage: &Stage,
        event: &StageEvent,
        now: Instant,
    ) -> Vec<ConcurrentEdit> {
        match event.origin {
            EventOrigin::Local | EventOrigin::Undo | EventOrigin::Redo => {
                self.record_local(stage, event, now);
                Vec::new()
            }
            EventOrigin::Remote => {
                self.stats.remote_events += 1;
                self.check_sublayers(stage, &event.sublayers, now)
            }
            EventOrigin::Stack => {
                self.recent.retain(|id, _| stage.sublayer(id).is_some());
                self.local_seen.retain(|id, _| stage.sublayer(id).is_some());
                // A sublayer may have been replaced: look at everything again.
                self.seen.clear();
                self.note_sublayers(stage);
                self.recount();
                Vec::new()
            }
        }
    }

    // ---- local edits ----

    fn record_local(&mut self, stage: &Stage, event: &StageEvent, now: Instant) {
        for id in &event.sublayers {
            let Some(layer) = stage.sublayer(id) else {
                continue;
            };
            let doc = layer.doc();
            let peer = doc.peer_id();
            let end = doc.oplog_vv().get(&peer).copied().unwrap_or(0);
            // The ops since the last local event (for a sublayer not seen
            // yet, the last commit).
            let start = match self.local_seen.insert(id.clone(), (peer, end)) {
                Some((p, c)) if p == peer && c <= end => c,
                _ => ops::last_change_start(doc, peer, end),
            };
            if start >= end {
                continue;
            }
            if usize::try_from(end - start).unwrap_or(usize::MAX) > self.config.max_ops_per_edit {
                self.stats.bulk_edits += 1;
                continue;
            }
            let mut edits: Vec<(TreeID, Recent)> = Vec::new();
            let mut removed: Vec<TreeID> = Vec::new();
            let mut walker = Walker::new(doc);
            let mut at = start;
            while at < end {
                // One Loro change at a time. What our op knew is its change's
                // deps, not what has arrived by the time we observe it: Loro
                // never merges a commit made after an import into the change
                // before it.
                let (upto, checked) = match doc.get_change(ID::new(peer, at)) {
                    Some(c) => {
                        let len = i32::try_from(c.len).unwrap_or(i32::MAX);
                        let past = doc
                            .frontiers_to_vv(&c.deps)
                            .unwrap_or_else(|| doc.oplog_vv());
                        (c.id.counter.saturating_add(len).clamp(at + 1, end), past)
                    }
                    None => (end, doc.oplog_vv()),
                };
                let checked = Arc::new(checked);
                walker.walk(IdSpan::new(peer, at, upto), |op| {
                    if op.rollback {
                        return;
                    }
                    let (
                        Target::Element {
                            node,
                            attr,
                            element,
                            value,
                        },
                        Some((container, key)),
                    ) = (op.target, op.key)
                    else {
                        if let Target::Delete(n) = op.target {
                            removed.push(n);
                        }
                        return;
                    };
                    // Gone again within the same step: nothing to protect.
                    let Some(path) = layer.node_path(node) else {
                        return;
                    };
                    edits.push((
                        node,
                        Recent {
                            attr: attr.to_owned(),
                            element,
                            container: container.clone(),
                            key: key.to_owned(),
                            path: path.clone(),
                            ours: element.decode(value),
                            op: op.id,
                            author: op.author.cloned().or_else(|| event.author.clone()),
                            edited_at: now,
                            seq: 0,
                            checked: checked.clone(),
                        },
                    ));
                });
                at = upto;
            }
            self.stats.local_ops += walker.ops;
            if !removed.is_empty() {
                self.forget_removed(id, doc, &removed);
            }
            if !edits.is_empty()
                && let Some(s) = self.seen.get_mut(id)
            {
                s.new_edits = true;
            }
            for (node, r) in edits {
                self.remember(id, node, r);
            }
        }
        self.enforce_capacity(now);
    }

    fn remember(&mut self, id: &SublayerId, node: TreeID, mut r: Recent) {
        r.seq = self.next_seq;
        self.next_seq += 1;
        let list = self
            .recent
            .entry(id.clone())
            .or_default()
            .entry(node)
            .or_default();
        match list
            .iter_mut()
            .find(|e| e.attr == r.attr && e.element == r.element)
        {
            Some(e) => *e = r,
            None => {
                list.push(r);
                self.len += 1;
            }
        }
    }

    /// Forget edits on prims we removed (or below them): whoever else removed
    /// them too, that is no conflict of ours.
    fn forget_removed(&mut self, id: &SublayerId, doc: &LoroDoc, removed: &[TreeID]) {
        let Some(nodes) = self.recent.get_mut(id) else {
            return;
        };
        let removed: FxHashSet<TreeID> = removed.iter().copied().collect();
        nodes.retain(|&n, _| !ops::in_subtrees(doc, n, &removed));
        if nodes.is_empty() {
            self.recent.remove(id);
        }
        self.recount();
    }

    fn retain(&mut self, mut keep: impl FnMut(&Recent) -> bool) {
        for nodes in self.recent.values_mut() {
            for list in nodes.values_mut() {
                list.retain(&mut keep);
            }
            nodes.retain(|_, l| !l.is_empty());
        }
        self.recent.retain(|_, n| !n.is_empty());
        self.recount();
    }

    fn recount(&mut self) {
        self.len = self
            .recent
            .values()
            .flat_map(|n| n.values())
            .map(Vec::len)
            .sum();
    }

    /// Forget edits older than the window and notices older than
    /// `repeat_quiet` (at most once per window unless forced).
    fn prune(&mut self, now: Instant, force: bool) {
        let due = self
            .last_prune
            .is_none_or(|t| now.saturating_duration_since(t) >= self.config.window);
        if !(due || force) {
            return;
        }
        self.last_prune = Some(now);
        let quiet = self.config.repeat_quiet;
        self.last_notice
            .retain(|_, t| now.saturating_duration_since(*t) < quiet);
        if self.len == 0 {
            return;
        }
        let window = self.config.window;
        let mark = self.offline.as_ref().map(|o| o.at);
        self.retain(|r| age(r, mark, now) <= window);
    }

    fn enforce_capacity(&mut self, now: Instant) {
        if self.len <= self.config.max_recent {
            return;
        }
        self.prune(now, true);
        if self.len <= self.config.max_recent {
            return;
        }
        // Drop the oldest quarter at once, so this stays rare.
        let keep = self.config.max_recent * 3 / 4;
        if keep == 0 {
            self.recent.clear();
            self.len = 0;
            return;
        }
        let mut order: Vec<(Instant, u64)> = self
            .recent
            .values()
            .flat_map(|n| n.values())
            .flatten()
            .map(|r| (r.edited_at, r.seq))
            .collect();
        order.sort_unstable();
        let cutoff = order[order.len() - keep];
        self.retain(|r| (r.edited_at, r.seq) >= cutoff);
    }

    // ---- remote imports ----

    /// [`check_at`](Self::check_at) with the current time.
    pub fn check(&mut self, stage: &Stage) -> Vec<ConcurrentEdit> {
        self.check_at(stage, Instant::now())
    }

    /// Look for remote ops that arrived since the last look on what we
    /// changed recently, in every sublayer. Call it after every
    /// `SyncSession::pump` that received something: an import that changes
    /// nothing visible (our value won) emits no event, but the other side's
    /// change of the element still deserves a hint. Where nothing was
    /// committed or imported since the last check, it compares frontiers and
    /// stops.
    ///
    /// While a reconnect is pending, it also applies
    /// [`ConflictConfig::catch_up_timeout`].
    pub fn check_at(&mut self, stage: &Stage, now: Instant) -> Vec<ConcurrentEdit> {
        let ids: Vec<SublayerId> = self.recent.keys().cloned().collect();
        self.check_sublayers(stage, &ids, now)
    }

    fn check_sublayers(
        &mut self,
        stage: &Stage,
        ids: &[SublayerId],
        now: Instant,
    ) -> Vec<ConcurrentEdit> {
        self.catch_up_timeout(stage, now);
        self.check_only(stage, ids, now)
    }

    /// [`check_sublayers`](Self::check_sublayers) without the catch-up
    /// timeout.
    fn check_only(
        &mut self,
        stage: &Stage,
        ids: &[SublayerId],
        now: Instant,
    ) -> Vec<ConcurrentEdit> {
        let crowded = self.last_notice.len() > self.config.max_recent.max(64) * 4;
        self.prune(now, crowded);
        let mut out = std::mem::take(&mut self.carried);
        for id in ids {
            if let Some(layer) = stage.sublayer(id) {
                self.check_layer(id, layer, now, &mut out);
            }
        }
        out
    }

    fn check_layer(
        &mut self,
        id: &SublayerId,
        layer: &Sublayer,
        now: Instant,
        out: &mut Vec<ConcurrentEdit>,
    ) {
        let Some(nodes) = self.recent.get(id) else {
            return;
        };
        let doc = layer.doc();
        let frontiers = doc.oplog_frontiers();
        if self
            .seen
            .get(id)
            .is_some_and(|s| !s.new_edits && s.frontiers == frontiers)
        {
            self.stats.quick_checks += 1;
            return;
        }
        let ours = doc.peer_id();
        let version = doc.oplog_vv();
        let mark = self.offline.as_ref().map(|o| o.at);
        let window = self.config.window;
        // Edits made while a reconnect is pending belong to the merge summary.
        let live = |r: &Recent| mark.is_none_or(|t| r.edited_at < t) && age(r, mark, now) <= window;
        // Remote ops nobody has compared an edit with yet: one comparison per
        // distinct version, as most edits share one.
        let mut arrived_since: FxHashMap<*const VersionVector, bool> = FxHashMap::default();
        let mut arrived = |r: &Recent| {
            *arrived_since
                .entry(Arc::as_ptr(&r.checked))
                .or_insert_with(|| remote_ahead(&version, &r.checked, ours))
        };
        let mut picked: Vec<(TreeID, usize, &Recent)> = Vec::new();
        let mut gone: Vec<TreeID> = Vec::new();
        for (&node, list) in nodes {
            let mut present = None;
            for (i, r) in list.iter().enumerate() {
                if !live(r) || !arrived(r) {
                    continue;
                }
                if *present.get_or_insert_with(|| layer.node_path(node).is_some()) {
                    picked.push((node, i, r));
                } else if gone.last() != Some(&node) {
                    gone.push(node);
                }
            }
        }
        self.stats.versions_compared += arrived_since.len() as u64 + 1;
        if picked.is_empty() && gone.is_empty() {
            self.mark_checked(id, frontiers, version, ours);
            return;
        }
        picked.sort_by_key(|&(n, i, _)| (n, i));
        gone.sort();

        let mut notices = Vec::new();
        let mut drop: FxHashSet<(TreeID, usize)> = FxHashSet::default();
        let mut checked_elements = 0;
        let (remote, decoded) = if picked.is_empty() {
            (FxHashMap::default(), 0)
        } else {
            remote_ops(doc, ours, &version, &picked)
        };
        // Waiting for a reconnect, an edit made before the link was lost only
        // meets ops made without knowing it: the ones it raced with, or that
        // the dead link never delivered. A later change by someone who saw it
        // is no concurrent edit, and when it was made is unknown here.
        let racing_only = mark.is_some();
        for &(node, i, r) in &picked {
            let ops = remote
                .get(&(r.container.clone(), r.key.clone()))
                .map_or(&[][..], Vec::as_slice);
            if ops.is_empty() {
                continue;
            }
            checked_elements += 1;
            if let Some((n, overridden)) = judge(layer, node, r, ops, racing_only, now) {
                notices.push(n);
                if overridden {
                    drop.insert((node, i));
                }
            }
        }

        for node in &gone {
            let deletion = ops::deletion_of(doc, *node);
            if deletion.is_none_or(|d| d.peer == ours) {
                // Removed by ourselves (that edit is ours to know about).
                continue;
            }
            let (author, _) = deletion.map_or((None, 0), |d| ops::author_of(doc, d));
            for r in nodes[node].iter().filter(|r| live(r)) {
                let concurrent = deletion.is_some_and(|d| ops::concurrent_with(doc, d, r.op));
                if racing_only && !concurrent {
                    continue;
                }
                checked_elements += 1;
                notices.push(ConcurrentEdit {
                    sublayer: id.clone(),
                    node: *node,
                    path: r.path.clone(),
                    attr: r.attr.clone(),
                    element: r.element,
                    outcome: ConflictOutcome::PrimRemoved,
                    ours: r.ours.clone(),
                    result: None,
                    theirs: None,
                    winner: author.clone(),
                    others: author.iter().cloned().collect(),
                    concurrent,
                    edited_at: r.edited_at,
                    at: now,
                });
            }
        }

        self.stats.elements_checked += checked_elements;
        self.stats.remote_ops += decoded;
        // Forget what was overridden or removed; remember how far we looked.
        let nodes = self.recent.get_mut(id).expect("looked up above");
        for (node, list) in nodes.iter_mut() {
            let mut i = 0;
            list.retain(|_| {
                let keep = !drop.contains(&(*node, i));
                i += 1;
                keep
            });
        }
        for node in &gone {
            nodes.remove(node);
        }
        nodes.retain(|_, l| !l.is_empty());
        self.recent.retain(|_, n| !n.is_empty());
        self.recount();
        self.mark_checked(id, frontiers, version, ours);

        notices.sort_by(|a, b| {
            (&a.path, &a.attr, a.element, a.node).cmp(&(&b.path, &b.attr, b.element, b.node))
        });
        let quiet = self.config.repeat_quiet;
        for n in notices {
            let key = (id.clone(), n.node, n.attr.clone(), n.element, n.outcome);
            let repeat = self
                .last_notice
                .insert(key, now)
                .is_some_and(|t| now.saturating_duration_since(t) < quiet);
            if repeat {
                self.stats.held_back += 1;
            } else {
                out.push(n);
            }
        }
    }

    /// Every remembered edit of `id` has been compared with the remote ops in
    /// `version`: let them share it, and note where the checks stand.
    fn mark_checked(
        &mut self,
        id: &SublayerId,
        frontiers: Frontiers,
        version: VersionVector,
        ours: PeerID,
    ) {
        // Nobody else's ops arrived since the last check: keep its version
        // (the edits holding it need no update; ours do not matter).
        let shared = match self.seen.get(id) {
            Some(s) if !remote_ahead(&version, &s.version, ours) => s.version.clone(),
            _ => Arc::new(version),
        };
        if let Some(nodes) = self.recent.get_mut(id) {
            for r in nodes.values_mut().flatten() {
                if !Arc::ptr_eq(&r.checked, &shared) {
                    r.checked = shared.clone();
                }
            }
        }
        self.seen.insert(
            id.clone(),
            Seen {
                frontiers,
                version: shared,
                new_edits: false,
            },
        );
    }

    // ---- offline ----

    /// [`connection_lost_at`](Self::connection_lost_at) with the current time.
    pub fn connection_lost(&mut self, stage: &Stage) {
        self.connection_lost_at(stage, Instant::now());
    }

    /// The link to the others is gone (transport error, hub unreachable,
    /// shared folder unreachable, or the app goes offline on purpose): remember
    /// the version we had. A second call before [`caught_up`](Self::caught_up)
    /// keeps the first mark (the summary covers the whole time), and restarts
    /// [`ConflictConfig::catch_up_timeout`]: a catch-up the link dropped again
    /// is not counted on, the next link's catch-up starts the timer anew.
    ///
    /// Until `caught_up` (or [`cancel_offline`](Self::cancel_offline), or
    /// [`ConflictConfig::catch_up_timeout`]), edits made after the mark get
    /// no online notices: they are left to the summary.
    ///
    /// Edits made before the mark may not have reached anyone (made after the
    /// link really dropped, or held back by `min_send_interval`). Until
    /// `caught_up` they stay as old as they were at the mark, so the catch-up
    /// still reports the ops that raced with them.
    pub fn connection_lost_at(&mut self, stage: &Stage, now: Instant) {
        let lost = ProjectVersion::current(stage);
        match &mut self.offline {
            Some(off) => {
                off.lost = lost;
                off.catching_up = None;
            }
            None => {
                self.offline = Some(Offline {
                    since: lost.clone(),
                    at: now,
                    lost,
                    catching_up: None,
                });
            }
        }
    }

    /// Stop waiting for the catch-up of a reconnect: forget the mark of
    /// [`connection_lost`](Self::connection_lost) without a summary. Online
    /// notices resume for every edit from here on; edits made while the mark
    /// was set are not reported for the remote ops that arrived meanwhile
    /// (they were left to the summary). For a catch-up that never completes:
    /// `SyncSession::is_caught_up` stays false while the remote lists a record
    /// that failed to import here, ops Loro holds pending for missing
    /// dependencies, or a document it cannot send in one message (a sublayer
    /// over the hub's message limit; `SyncSession::behind` lists the
    /// documents). [`ConflictConfig::catch_up_timeout`] does this on its
    /// own. Returns whether a reconnect was pending.
    pub fn cancel_offline(&mut self) -> bool {
        self.offline.take().is_some()
    }

    /// Apply [`ConflictConfig::catch_up_timeout`]: note when ops of others
    /// beyond the latest `connection_lost` first show up, and drop the mark
    /// that long after.
    fn catch_up_timeout(&mut self, stage: &Stage, now: Instant) {
        let Some(limit) = self.config.catch_up_timeout else {
            return;
        };
        let Some(off) = &mut self.offline else {
            return;
        };
        let began = match off.catching_up {
            Some(t) => t,
            None => {
                // One version-vector comparison per sublayer, only while a
                // reconnect is pending and until the catch-up began.
                let mut arrived = false;
                for layer in stage.sublayers() {
                    let doc = layer.doc();
                    match off.lost.sublayers.get(layer.id()) {
                        Some(lost) => arrived |= remote_ahead(&doc.oplog_vv(), lost, doc.peer_id()),
                        None => {
                            off.lost
                                .sublayers
                                .insert(layer.id().clone(), doc.oplog_vv());
                        }
                    }
                }
                if !arrived {
                    return;
                }
                *off.catching_up.insert(now)
            }
        };
        if now.saturating_duration_since(began) >= limit {
            self.offline = None;
        }
    }

    /// The version remembered by [`connection_lost`](Self::connection_lost),
    /// while the reconnect is pending.
    pub fn offline_since(&self) -> Option<&ProjectVersion> {
        self.offline.as_ref().map(|o| &o.since)
    }

    /// [`caught_up_at`](Self::caught_up_at) with the current time.
    pub fn caught_up(&mut self, stage: &Stage) -> Result<Option<CaughtUp>, SyncError> {
        self.caught_up_at(stage, Instant::now())
    }

    /// Reconnected and caught up (the first time `SyncSession::is_caught_up`
    /// holds after the reconnect; with `crate::Connection`, while
    /// `is_online`): first [`check_at`](Self::check_at) on what arrived (the
    /// remote ops that raced with edits made before the mark), then what the
    /// merge did, from the version remembered by
    /// [`connection_lost`](Self::connection_lost) to now ([`merge_summary`]).
    /// Both come back in [`CaughtUp`], so the check cannot come too late: a
    /// summary first would count the catch-up's remote ops as compared, and
    /// the edits the dead link never delivered would miss their notices.
    /// `None` if no reconnect was pending (also after
    /// [`cancel_offline`](Self::cancel_offline) or
    /// [`ConflictConfig::catch_up_timeout`]).
    ///
    /// On an error the mark is kept; the notices of the check are not lost,
    /// the next check (or `caught_up`) returns them.
    ///
    /// Costs a [`merge_summary`]: several snapshot exports and imports per
    /// sublayer that changed. Do not call it on the UI thread of a large
    /// project; [`caught_up_detached`](Self::caught_up_detached) leaves that
    /// part to a worker thread.
    pub fn caught_up_at(
        &mut self,
        stage: &Stage,
        now: Instant,
    ) -> Result<Option<CaughtUp>, SyncError> {
        let Some(since) = self.offline.as_ref().map(|o| o.since.clone()) else {
            return Ok(None);
        };
        let notices = self.catch_up_check(stage, now);
        match merge_summary(stage, &since) {
            Ok(summary) => {
                self.caught_up_done(stage);
                Ok(Some(CaughtUp { notices, summary }))
            }
            Err(e) => {
                self.carried = notices;
                Err(e)
            }
        }
    }

    /// [`caught_up_at`](Self::caught_up_at) with the merge summary left to
    /// another thread: the check runs here, the mark is cleared, and
    /// [`DetachedMerge::summarize`] (on any thread) computes the summary from
    /// a pinned copy of the stage ([`Stage::snapshot_reader`]; taking it only
    /// clones handles). An error there does not bring the mark back.
    pub fn caught_up_detached(&mut self, stage: &Stage) -> Option<CaughtUpDetached> {
        self.caught_up_detached_at(stage, Instant::now())
    }

    /// [`caught_up_detached`](Self::caught_up_detached) with an explicit
    /// clock.
    pub fn caught_up_detached_at(
        &mut self,
        stage: &Stage,
        now: Instant,
    ) -> Option<CaughtUpDetached> {
        let since = self.offline.as_ref().map(|o| o.since.clone())?;
        let notices = self.catch_up_check(stage, now);
        let merge = DetachedMerge::new(stage, since);
        self.caught_up_done(stage);
        Some(CaughtUpDetached { notices, merge })
    }

    /// The check of a catch-up: like [`check_at`](Self::check_at) (with the
    /// mark still set, so edits made before it only meet the ops that raced
    /// with them), without the catch-up timeout.
    fn catch_up_check(&mut self, stage: &Stage, now: Instant) -> Vec<ConcurrentEdit> {
        let ids: Vec<SublayerId> = self.recent.keys().cloned().collect();
        self.check_only(stage, &ids, now)
    }

    /// The reconnect is summarized: clear the mark; the remote ops merged so
    /// far are in the summary (or were checked against the edits made before
    /// the mark).
    fn caught_up_done(&mut self, stage: &Stage) {
        self.offline = None;
        let ids: Vec<SublayerId> = self.recent.keys().cloned().collect();
        for id in ids {
            if let Some(layer) = stage.sublayer(&id) {
                let doc = layer.doc();
                self.mark_checked(&id, doc.oplog_frontiers(), doc.oplog_vv(), doc.peer_id());
            }
        }
    }
}

/// What [`ConflictTracker::caught_up`] found once a reconnect caught up.
#[derive(Clone, Debug, PartialEq)]
pub struct CaughtUp {
    /// Online notices of the check that ran first: edits made before the
    /// connection was lost, met by remote ops that raced with them (or that
    /// the dead link never delivered).
    pub notices: Vec<ConcurrentEdit>,
    /// What the merge did to the edits made since.
    pub summary: MergeSummary,
}

/// [`CaughtUp`] with the summary still to compute
/// ([`ConflictTracker::caught_up_detached`]).
#[derive(Debug)]
pub struct CaughtUpDetached {
    pub notices: Vec<ConcurrentEdit>,
    /// Send it to a worker thread and call [`DetachedMerge::summarize`].
    pub merge: DetachedMerge,
}

/// [`merge_summary`] of a pinned copy of a stage, to compute on another
/// thread: a large project's summary (several snapshot exports and imports
/// per changed sublayer) does not stall the thread that owns the stage.
///
/// Paths come from the copy's own mirror, built from scratch: where
/// concurrently created prims share a name, a client's live mirror may show
/// them under other paths than a rebuilt one (`docs/spec/sync.md` 5.3); the
/// prims, nodes and changes are the same.
pub struct DetachedMerge {
    since: ProjectVersion,
    /// The stage's Loro peer per sublayer: the copy's documents get new ones,
    /// and "ours" are the ops of these.
    peers: Vec<(SublayerId, PeerID)>,
    read: Box<dyn FnOnce() -> Result<Stage, bae_model::ModelError> + Send>,
}

impl std::fmt::Debug for DetachedMerge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DetachedMerge")
            .field("since", &self.since)
            .field("peers", &self.peers)
            .finish_non_exhaustive()
    }
}

impl DetachedMerge {
    /// Pin `stage` now (cheap: no export happens here) for the summary of
    /// what merged since `since` (a version of this stage, like
    /// [`merge_summary`]'s).
    pub fn new(stage: &Stage, since: ProjectVersion) -> Self {
        DetachedMerge {
            since,
            peers: stage
                .sublayers()
                .map(|l| (l.id().clone(), l.doc().peer_id()))
                .collect(),
            read: stage.snapshot_reader(),
        }
    }

    /// The version the summary starts from.
    pub fn since(&self) -> &ProjectVersion {
        &self.since
    }

    /// Materialize the pinned stage and compute [`merge_summary`] on it. Runs
    /// on any thread; costs a copy of the project plus the summary.
    pub fn summarize(self) -> Result<MergeSummary, SyncError> {
        let stage = (self.read)()?;
        for (id, peer) in &self.peers {
            if let Some(layer) = stage.sublayer(id) {
                layer.doc().set_peer_id(*peer)?;
            }
        }
        merge_summary(&stage, &self.since)
    }
}

/// How old `r` counts for the window: while a reconnect is pending (`mark`),
/// an edit made before the link was lost is as old as it was then.
fn age(r: &Recent, mark: Option<Instant>, now: Instant) -> Duration {
    match mark {
        Some(t) if r.edited_at < t => t.saturating_duration_since(r.edited_at),
        _ => now.saturating_duration_since(r.edited_at),
    }
}

/// Whether `version` holds ops of a peer other than `ours` that `known` does
/// not.
fn remote_ahead(version: &VersionVector, known: &VersionVector, ours: PeerID) -> bool {
    version
        .iter()
        .any(|(p, &end)| *p != ours && known.get(p).copied().unwrap_or(0) < end)
}

/// Remote ops (other peers, not yet looked at by some candidate) on the maps of
/// `picked`, by element; and how many ops were decoded. Only spans around ops
/// that wrote one of those maps are decoded.
fn remote_ops(
    doc: &LoroDoc,
    ours: u64,
    version: &VersionVector,
    picked: &[(TreeID, usize, &Recent)],
) -> (FxHashMap<ElementKey, Vec<RemoteOp>>, u64) {
    let interest: FxHashSet<&ContainerID> = picked.iter().map(|(_, _, r)| &r.container).collect();
    // The versions the candidates were last compared with (most share one).
    let mut distinct = FxHashSet::default();
    let compared: Vec<&VersionVector> = picked
        .iter()
        .filter(|(_, _, r)| distinct.insert(Arc::as_ptr(&r.checked)))
        .map(|(_, _, r)| &*r.checked)
        .collect();
    let mut found: FxHashMap<ElementKey, Vec<RemoteOp>> = FxHashMap::default();
    let mut walker = Walker::new(doc);
    for (&peer, &end) in version.iter() {
        if peer == ours {
            continue;
        }
        let start = compared
            .iter()
            .map(|v| v.get(&peer).copied().unwrap_or(0))
            .min()
            .unwrap_or(end);
        each_span_writing(doc, peer, start, end, &interest, &mut |span| {
            walker.walk(span, |op| {
                let (Target::Element { value, .. }, Some((c, k))) = (op.target, op.key) else {
                    return;
                };
                if !interest.contains(c) {
                    return;
                }
                found
                    .entry((c.clone(), k.to_owned()))
                    .or_default()
                    .push(RemoteOp {
                        id: op.id,
                        lamport: op.lamport,
                        author: op.author.cloned(),
                        value: value.cloned(),
                        rollback: op.rollback,
                    });
            });
        });
    }
    (found, walker.ops)
}

/// Spans at most this long are decoded as they are; longer ones are halved
/// while only one half writes a map of interest.
const LEAF_OPS: i32 = 32;

/// Call `f` with parts of `peer`'s ops in `start..end` that cover every op
/// writing one of `interest`, and little else. Finding which containers a span
/// wrote does not decode it, so a large import is decoded only around the ops
/// that matter. (Consecutive commits of one author with the same label are
/// merged into one Loro change, so whole changes would not be selective.)
fn each_span_writing(
    doc: &LoroDoc,
    peer: u64,
    start: i32,
    end: i32,
    interest: &FxHashSet<&ContainerID>,
    f: &mut impl FnMut(IdSpan),
) {
    if start >= end {
        return;
    }
    let len = usize::try_from(end - start).unwrap_or(0);
    let writes = doc
        .get_changed_containers_in(ID::new(peer, start), len)
        .iter()
        .any(|c| interest.contains(c));
    if !writes {
        return;
    }
    if end - start <= LEAF_OPS {
        f(IdSpan::new(peer, start, end));
        return;
    }
    let mid = start + (end - start) / 2;
    each_span_writing(doc, peer, start, mid, interest, f);
    each_span_writing(doc, peer, mid, end, interest, f);
}

/// The notice for one remembered element an import touched, and whether our
/// value was overridden (then it is forgotten). `racing_only`: only ops made
/// without knowing ours count as new.
fn judge(
    layer: &Sublayer,
    node: TreeID,
    r: &Recent,
    ops: &[RemoteOp],
    racing_only: bool,
    now: Instant,
) -> Option<(ConcurrentEdit, bool)> {
    let doc = layer.doc();
    let ours = doc.peer_id();
    // Ops this edit has not been compared with yet (the scan started at the
    // oldest version of all candidates).
    let fresh: Vec<&RemoteOp> = ops
        .iter()
        .filter(|o| o.id.counter >= r.checked.get(&o.id.peer).copied().unwrap_or(0))
        .filter(|o| !racing_only || ops::concurrent_with(doc, o.id, r.op))
        .collect();
    if fresh.is_empty() {
        return None;
    }
    let path = layer.node_path(node)?;
    let result = layer.attr(path, &r.attr).and_then(|a| r.element.get(a));
    let winner_peer = ops::last_editor(doc, &r.container, &r.key);
    let notice = |outcome, theirs, winner, others, concurrent| ConcurrentEdit {
        sublayer: layer.id().clone(),
        node,
        path: path.clone(),
        attr: r.attr.clone(),
        element: r.element,
        outcome,
        ours: r.ours.clone(),
        result: result.clone(),
        theirs,
        winner,
        others,
        concurrent,
        edited_at: r.edited_at,
        at: now,
    };
    if winner_peer == Some(ours) {
        // Our value stands. Report the other changes of it that lost, unless
        // they set the same or were a failed transaction's rollback.
        let lost: Vec<&RemoteOp> = fresh
            .into_iter()
            .filter(|o| !o.rollback && r.element.decode(o.value.as_ref()) != r.ours)
            .collect();
        let last = lost.iter().max_by_key(|o| o.order())?;
        let theirs = r.element.decode(last.value.as_ref());
        // An op that knew ours wins over it, unless a later op of ours (a
        // rollback's write) holds the element now.
        let concurrent = lost.iter().any(|o| ops::concurrent_with(doc, o.id, r.op));
        return Some((
            notice(
                ConflictOutcome::Kept,
                theirs,
                r.author.clone(),
                authors(&lost),
                concurrent,
            ),
            false,
        ));
    }
    if result == r.ours {
        return None;
    }
    // The winner may be a rollback's write: it replaced our value all the same.
    let win = ops
        .iter()
        .filter(|o| winner_peer.is_none_or(|p| o.id.peer == p))
        .max_by_key(|o| o.order());
    let winner = win.and_then(|w| w.author.clone());
    let mut others = authors(&fresh);
    if let Some(w) = &winner
        && !others.contains(w)
    {
        others.push(w.clone());
        others.sort();
    }
    let concurrent = win.is_some_and(|w| ops::concurrent_with(doc, w.id, r.op));
    Some((
        notice(
            ConflictOutcome::Overridden,
            result.clone(),
            winner,
            others,
            concurrent,
        ),
        true,
    ))
}

fn authors(ops: &[&RemoteOp]) -> Vec<AuthorId> {
    let set: BTreeSet<AuthorId> = ops.iter().filter_map(|o| o.author.clone()).collect();
    set.into_iter().collect()
}

// ---- offline merge summary ----

/// What a reconnect merged: our edits since we went offline against what the
/// others did meanwhile. See [`merge_summary`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeSummary {
    /// The version we had when the connection was lost.
    pub since: ProjectVersion,
    /// Our attribute edits whose value stands (on prims that existed before).
    pub kept: Vec<OfflineEdit>,
    /// Our attribute edits replaced by another author's value, or lost with a
    /// prim another author removed.
    pub overridden: Vec<OfflineEdit>,
    /// Prims we added or removed, and where they stand.
    pub prims: Vec<OfflinePrim>,
    /// What the others changed meanwhile, by prim: `history::diff` from what
    /// we had right before merging (`since` plus our ops made before any of
    /// theirs arrived) to that plus all of their ops, so only their changes
    /// (including where they won over ours), also when we kept editing while
    /// the catch-up came in.
    pub merged: Vec<PrimDiff>,
}

impl MergeSummary {
    /// Nothing happened on either side.
    pub fn is_empty(&self) -> bool {
        self.kept.is_empty()
            && self.overridden.is_empty()
            && self.prims.is_empty()
            && self.merged.is_empty()
    }

    /// Our edits that did not survive as we made them: overridden elements and
    /// conflicting prim edits.
    pub fn conflicts(&self) -> usize {
        self.overridden.len() + self.prims.iter().filter(|p| p.change.is_conflict()).count()
    }
}

/// One of our offline attribute edits.
#[derive(Clone, Debug, PartialEq)]
pub struct OfflineEdit {
    pub sublayer: SublayerId,
    pub node: TreeID,
    /// The prim's current path; for a removed prim, its path in our version.
    pub path: PrimPath,
    pub attr: String,
    pub element: Element,
    /// Who made it on this client (from the commit).
    pub author: Option<AuthorId>,
    /// What we set (`None`: we removed it).
    pub ours: Option<ElementValue>,
    /// The converged content.
    pub result: Option<ElementValue>,
    /// For an overridden edit, the author of the change that holds `result`
    /// (or removed the prim); `None` if kept or unknown.
    pub winner: Option<AuthorId>,
    /// Others who changed the same element meanwhile.
    pub others: Vec<AuthorId>,
    /// The prim is gone: another author removed it.
    pub prim_removed: bool,
}

/// A prim we added or removed while offline.
#[derive(Clone, Debug, PartialEq)]
pub struct OfflinePrim {
    pub sublayer: SublayerId,
    pub node: TreeID,
    /// Its current path; if it is gone, its path in our version (added) or
    /// before we went offline (removed).
    pub path: PrimPath,
    pub change: OfflinePrimChange,
    /// The other authors involved.
    pub by: Vec<AuthorId>,
}

/// Where a prim we added or removed stands after the merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfflinePrimChange {
    /// We added it; it is there.
    Added,
    /// We added it; another author removed its parent, so it is gone.
    AddedButRemoved,
    /// We removed it; nobody else touched it.
    Removed,
    /// We removed it; others changed it (or something below it) meanwhile,
    /// and their changes are gone with it.
    RemovedButChanged,
    /// We removed it; another author's concurrent move won, so it is back.
    RemovalUndone,
}

impl OfflinePrimChange {
    /// Not what we did, or it undid someone else's work.
    pub fn is_conflict(&self) -> bool {
        !matches!(self, OfflinePrimChange::Added | OfflinePrimChange::Removed)
    }
}

/// Our latest op on an element.
struct MyEdit {
    node: TreeID,
    attr: String,
    element: Element,
    ours: Option<ElementValue>,
    author: Option<AuthorId>,
}

/// What merging the others' changes did to ours since `since` (the version
/// [`ConflictTracker::connection_lost`] remembered): "ours" are the ops this
/// stage's own Loro peers made after `since` (local edits, undo and redo, by
/// any author on this client); everything else after `since` is the others'.
/// Works the same for hub and shared-folder sessions: it only reads version
/// vectors and history.
///
/// Expensive: for every sublayer that changed since `since`, up to three
/// read-only snapshots of past versions (paths of prims that are gone, built
/// on demand: an `ExportMode::SnapshotAt` export plus an import and a mirror
/// build each) and the two of the `history::diff` for `merged`. Call it once
/// per reconnect, never per frame, and not on the UI thread of a large
/// project: [`DetachedMerge`] runs it on a worker thread.
///
/// The stage's peers change when the project is reopened, so `since` must
/// come from this stage, not from before a restart.
pub fn merge_summary(stage: &Stage, since: &ProjectVersion) -> Result<MergeSummary, SyncError> {
    let now = ProjectVersion::current(stage);
    // What we had right before merging, and that plus all of the others' ops.
    let mut before = ProjectVersion::default();
    let mut after = ProjectVersion::default();
    let mut out = MergeSummary {
        since: since.clone(),
        ..MergeSummary::default()
    };
    for id in stage.layer_stack() {
        let layer = stage.sublayer(id).expect("in stack");
        let (from, to) = (since.get(id), now.get(id));
        if !to.includes_vv(&from) {
            // Not the document we had then (replaced or reloaded).
            before.sublayers.insert(id.clone(), to.clone());
            after.sublayers.insert(id.clone(), to);
            continue;
        }
        let doc = layer.doc();
        let peer = doc.peer_id();
        let start = from.get(&peer).copied().unwrap_or(0);
        let end = to.get(&peer).copied().unwrap_or(0).max(start);
        let with_own = |upto: i32| {
            let mut v = from.clone();
            if upto > start {
                v.insert(peer, upto);
            }
            v
        };
        // All our ops; and those made before any of theirs arrived (later
        // ones depend on theirs, so a version with them would bring theirs).
        let mine = with_own(end);
        let base = with_own(first_op_knowing_theirs(doc, peer, &from, start, end));
        let mut merged = base.clone();
        for (&p, &c) in to.iter() {
            if p != peer {
                merged.insert(p, c);
            }
        }
        if from != to {
            summarize_layer(id, layer, [&from, &base, &mine], &to, &mut out)?;
        }
        before.sublayers.insert(id.clone(), base);
        after.sublayers.insert(id.clone(), merged);
    }
    out.merged = history::diff(stage, &before, &after)?;
    Ok(out)
}

/// The first of `peer`'s ops in `start..end` that was made after an op of
/// another peer beyond `since` had arrived (`end` if none). Once one change
/// knows such an op every later one does, so this halves the range over
/// whole changes: a few lookups per reconnect.
fn first_op_knowing_theirs(
    doc: &LoroDoc,
    peer: PeerID,
    since: &VersionVector,
    start: i32,
    end: i32,
) -> i32 {
    let (mut lo, mut hi) = (start, end);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let Some(change) = doc.get_change(ID::new(peer, mid)) else {
            return lo;
        };
        let knows = doc
            .frontiers_to_vv(&change.deps)
            .is_some_and(|past| remote_ahead(&past, since, peer));
        if knows {
            hi = change.id.counter.max(lo);
        } else {
            let len = i32::try_from(change.len).unwrap_or(i32::MAX);
            lo = change.id.counter.saturating_add(len).clamp(mid + 1, hi);
        }
    }
    lo
}

/// Paths of prims that are gone, from read-only snapshots built on demand
/// (at most one per version).
struct PastPaths<'a> {
    live: &'a Sublayer,
    /// `since`, what we had right before merging, all our ops.
    versions: [&'a VersionVector; 3],
    layers: [Option<history::LayerAt<'a>>; 3],
}

impl PastPaths<'_> {
    fn at(&mut self, which: usize, node: TreeID) -> Result<Option<PrimPath>, SyncError> {
        let layer = match &mut self.layers[which] {
            Some(l) => l,
            slot => slot.insert(history::layer_at(self.live, self.versions[which])?),
        };
        Ok(layer.get().node_path(node).cloned())
    }

    /// Path of `node` at `since`.
    fn before(&mut self, node: TreeID) -> Result<PrimPath, SyncError> {
        Ok(self.at(0, node)?.unwrap_or_else(PrimPath::root))
    }

    /// Path of `node` in our version: right before merging or, for a prim we
    /// added while the catch-up came in, with all our ops.
    fn ours(&mut self, node: TreeID) -> Result<PrimPath, SyncError> {
        if let Some(path) = self.at(1, node)? {
            return Ok(path);
        }
        if self.versions[2] == self.versions[1] {
            return Ok(PrimPath::root());
        }
        Ok(self.at(2, node)?.unwrap_or_else(PrimPath::root))
    }
}

/// `versions`: `since`, what we had right before merging, all our ops.
fn summarize_layer(
    id: &SublayerId,
    layer: &Sublayer,
    versions: [&VersionVector; 3],
    to: &VersionVector,
    out: &mut MergeSummary,
) -> Result<(), SyncError> {
    let [from, _, mine] = versions;
    let doc = layer.doc();
    let peer = doc.peer_id();
    let mut walker = Walker::new(doc);

    // Our ops: the latest per element, and the prims we created and deleted.
    let mut edits: FxHashMap<ElementKey, MyEdit> = FxHashMap::default();
    let mut created: Vec<(TreeID, Option<TreeID>)> = Vec::new();
    let mut deleted: Vec<TreeID> = Vec::new();
    let start = from.get(&peer).copied().unwrap_or(0);
    let end = mine.get(&peer).copied().unwrap_or(0);
    if start < end {
        walker.walk(IdSpan::new(peer, start, end), |op| {
            if op.rollback {
                return;
            }
            match op.target {
                Target::Create(n, parent) => created.push((n, parent)),
                Target::Delete(n) => deleted.push(n),
                Target::Element {
                    node,
                    attr,
                    element,
                    value,
                } => {
                    if let Some((c, k)) = op.key {
                        edits.insert(
                            (c.clone(), k.to_owned()),
                            MyEdit {
                                node,
                                attr: attr.to_owned(),
                                element,
                                ours: element.decode(value),
                                author: op.author.cloned(),
                            },
                        );
                    }
                }
                Target::Move(_) | Target::Other(_) => {}
            }
        });
    }
    let created_set: FxHashSet<TreeID> = created.iter().map(|(n, _)| *n).collect();
    let deleted_set: FxHashSet<TreeID> = deleted.iter().copied().collect();
    // We removed it, or something above it, ourselves: whoever else did too.
    let removed_by_us = |n: TreeID| ops::in_subtrees(doc, n, &deleted_set);

    // Their ops: changes of the elements we edited, and (if we deleted
    // prims) who touched which prim. Without deletions only the ops around
    // writes of our elements' maps are decoded.
    let containers: FxHashSet<&ContainerID> = edits.keys().map(|(c, _)| c).collect();
    let all = !deleted.is_empty();
    let mut theirs: FxHashMap<ElementKey, Vec<RemoteOp>> = FxHashMap::default();
    let mut touched: FxHashMap<TreeID, BTreeSet<AuthorId>> = FxHashMap::default();
    for (&p, &e) in to.iter() {
        let s = from.get(&p).copied().unwrap_or(0);
        if p == peer || s >= e || (!all && containers.is_empty()) {
            continue;
        }
        let mut read = |span: IdSpan| {
            walker.walk(span, |op| {
                // Removing a prim we removed too is no conflict, and a failed
                // transaction's rollback changed nothing anyone asked for.
                if all && !op.rollback && !matches!(op.target, Target::Delete(_)) {
                    let t = touched.entry(op.target.node()).or_default();
                    t.extend(op.author.cloned());
                }
                // A rollback's writes still compete for the element.
                if let (Target::Element { value, .. }, Some((c, k))) = (op.target, op.key)
                    && containers.contains(c)
                {
                    theirs
                        .entry((c.clone(), k.to_owned()))
                        .or_default()
                        .push(RemoteOp {
                            id: op.id,
                            lamport: op.lamport,
                            author: op.author.cloned(),
                            value: value.cloned(),
                            rollback: op.rollback,
                        });
                }
            });
        };
        if all {
            read(IdSpan::new(p, s, e));
        } else {
            each_span_writing(doc, p, s, e, &containers, &mut read);
        }
    }

    let mut past = PastPaths {
        live: layer,
        versions,
        layers: [None, None, None],
    };
    // The op that removed `n`, if someone else did.
    let removed_by_others = |n: TreeID| ops::deletion_of(doc, n).filter(|d| d.peer != peer);

    let (mut kept, mut overridden) = (Vec::new(), Vec::new());
    for (key, e) in edits {
        if created_set.contains(&e.node) {
            // On a prim we added: reported with the prim.
            continue;
        }
        let ops = theirs.get(&key).map_or(&[][..], Vec::as_slice);
        let differing: Vec<&RemoteOp> = ops
            .iter()
            .filter(|o| !o.rollback && e.element.decode(o.value.as_ref()) != e.ours)
            .collect();
        let mut edit = OfflineEdit {
            sublayer: id.clone(),
            node: e.node,
            path: PrimPath::root(),
            attr: e.attr,
            element: e.element,
            author: e.author,
            ours: e.ours,
            result: None,
            winner: None,
            others: authors(&differing),
            prim_removed: false,
        };
        let Some(path) = layer.node_path(e.node) else {
            // The prim is gone: by us (reported with the prim), or by someone
            // else.
            if removed_by_us(e.node) {
                continue;
            }
            let Some(d) = removed_by_others(e.node) else {
                continue;
            };
            edit.winner = ops::author_of(doc, d).0;
            if let Some(w) = &edit.winner
                && !edit.others.contains(w)
            {
                edit.others.push(w.clone());
                edit.others.sort();
            }
            edit.path = past.ours(e.node)?;
            edit.prim_removed = true;
            overridden.push(edit);
            continue;
        };
        edit.path = path.clone();
        edit.result = layer
            .attr(path, &edit.attr)
            .and_then(|a| edit.element.get(a));
        let winner_peer = ops::last_editor(doc, &key.0, &key.1);
        if winner_peer == Some(peer) || edit.result == edit.ours {
            kept.push(edit);
        } else {
            // Possibly a rollback's write: it replaced ours all the same.
            edit.winner = ops
                .iter()
                .filter(|o| winner_peer.is_none_or(|p| o.id.peer == p))
                .max_by_key(|o| o.order())
                .and_then(|o| o.author.clone());
            overridden.push(edit);
        }
    }

    // Who else touched each prim we deleted, or anything below it.
    let mut changed_below: FxHashMap<TreeID, BTreeSet<AuthorId>> = FxHashMap::default();
    if !deleted_set.is_empty() {
        for (n, who) in &touched {
            ops::for_each_ancestor(doc, *n, |a| {
                if deleted_set.contains(&a) {
                    changed_below
                        .entry(a)
                        .or_default()
                        .extend(who.iter().cloned());
                }
            });
        }
    }

    let mut prims = Vec::new();
    // Prims we added (top-most only: children come with them).
    for &(n, parent) in &created {
        if parent.is_some_and(|p| created_set.contains(&p)) {
            continue;
        }
        let entry = |path, change, by: Vec<AuthorId>| OfflinePrim {
            sublayer: id.clone(),
            node: n,
            path,
            change,
            by,
        };
        match layer.node_path(n) {
            Some(path) => prims.push(entry(path.clone(), OfflinePrimChange::Added, Vec::new())),
            None => {
                if removed_by_us(n) {
                    continue;
                }
                let Some(d) = removed_by_others(n) else {
                    continue;
                };
                let by = ops::author_of(doc, d).0.into_iter().collect();
                let path = past.ours(n)?;
                prims.push(entry(path, OfflinePrimChange::AddedButRemoved, by));
            }
        }
    }
    // Prims we removed.
    let mut seen = FxHashSet::default();
    for &n in &deleted {
        if created_set.contains(&n) || !seen.insert(n) {
            continue;
        }
        let (path, change, by) = match layer.node_path(n) {
            Some(path) => {
                // Back: another author's move won over our delete.
                let by: Vec<AuthorId> = ops::last_move_of(doc, n)
                    .filter(|m| m.peer != peer)
                    .and_then(|m| ops::author_of(doc, m).0)
                    .into_iter()
                    .collect();
                (path.clone(), OfflinePrimChange::RemovalUndone, by)
            }
            None => {
                let by: Vec<AuthorId> = changed_below
                    .get(&n)
                    .map(|s| s.iter().cloned().collect())
                    .unwrap_or_default();
                let change = if by.is_empty() {
                    OfflinePrimChange::Removed
                } else {
                    OfflinePrimChange::RemovedButChanged
                };
                let path = past.before(n)?;
                (path, change, by)
            }
        };
        prims.push(OfflinePrim {
            sublayer: id.clone(),
            node: n,
            path,
            change,
            by,
        });
    }

    let by_place = |a: &OfflineEdit, b: &OfflineEdit| {
        (&a.path, &a.attr, a.element, a.node).cmp(&(&b.path, &b.attr, b.element, b.node))
    };
    kept.sort_by(by_place);
    overridden.sort_by(by_place);
    prims.sort_by(|a, b| (&a.path, a.node).cmp(&(&b.path, b.node)));
    out.kept.extend(kept);
    out.overridden.extend(overridden);
    out.prims.extend(prims);
    Ok(())
}
