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
//!   ([`MergeSummary`]).
//!
//! An *element* is the unit the CRDT merges on its own, last writer wins: an
//! attribute's static value, its expression, or one keyframe (by time). Two
//! people changing different keyframes, or one the value and the other the
//! expression, never overwrite each other and are not reported.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bae_model::loro::{
    ContainerID, ID, IdSpan, Lamport, LoroDoc, LoroValue, TreeID, VersionVector,
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
    /// Local element edits remembered at most (oldest dropped first). One edit
    /// can exceed it by at most `max_ops_per_edit`.
    pub max_recent: usize,
    /// Local commits with more ops than this (a paste, an import) are not
    /// remembered element by element: they mostly create new prims, which
    /// nobody else can have edited.
    pub max_ops_per_edit: usize,
}

impl Default for ConflictConfig {
    fn default() -> Self {
        ConflictConfig {
            window: Duration::from_secs(10),
            max_recent: 1024,
            max_ops_per_edit: 2048,
        }
    }
}

/// What happened to our edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// Remembered elements an import touched and that were analysed.
    pub elements_checked: u64,
    /// Ops of remote commits decoded (only commits that wrote a candidate's map).
    pub remote_ops: u64,
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
    /// Remote ops in this version have already been looked at.
    checked: Arc<VersionVector>,
}

/// A remote op on an element.
#[derive(Clone, Debug)]
struct RemoteOp {
    id: ID,
    lamport: Lamport,
    author: Option<AuthorId>,
    value: Option<LoroValue>,
}

impl RemoteOp {
    fn order(&self) -> (Lamport, u64) {
        (self.lamport, self.id.peer)
    }
}

type ElementKey = (ContainerID, String);

#[derive(Debug)]
struct Offline {
    since: ProjectVersion,
    at: Instant,
}

/// Turns stage events into [`ConcurrentEdit`] notices and, around a
/// disconnection, a [`MergeSummary`]. One per open stage, on the thread that
/// owns it.
///
/// Feed it every local event, in order (`Stage::transact`, `Stage::undo`,
/// `Stage::redo`, ...), and call [`check`](Self::check) after every
/// `SyncSession::pump` (remote events may also be passed to
/// [`observe`](Self::observe)). A local event costs its own ops. A check
/// costs nothing when no remote op arrived; otherwise it finds which of the
/// newly arrived ops wrote the maps of remembered edits without decoding
/// them, and decodes only around those. The whole project is never scanned.
#[derive(Debug)]
pub struct ConflictTracker {
    config: ConflictConfig,
    /// Remembered local edits by sublayer and prim.
    recent: FxHashMap<SublayerId, FxHashMap<TreeID, Vec<Recent>>>,
    len: usize,
    last_prune: Option<Instant>,
    /// Our op counter per sublayer at the last local event.
    local_seen: FxHashMap<SublayerId, (u64, i32)>,
    offline: Option<Offline>,
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
            last_prune: None,
            local_seen: FxHashMap::default(),
            offline: None,
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

    /// Look at one event right after it happened (`stage` as it is now).
    /// Local events update what is remembered; a remote import is checked
    /// like [`check_at`](Self::check_at) for its sublayers and returns the
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
            EventOrigin::Remote => self.check_sublayers(stage, &event.sublayers, now),
            EventOrigin::Stack => {
                self.recent.retain(|id, _| stage.sublayer(id).is_some());
                self.local_seen.retain(|id, _| stage.sublayer(id).is_some());
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
            let version = doc.oplog_vv();
            let end = version.get(&peer).copied().unwrap_or(0);
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
            let checked = Arc::new(version);
            let mut edits: Vec<(TreeID, Recent)> = Vec::new();
            let mut walker = Walker::new(doc);
            walker.walk(IdSpan::new(peer, start, end), |op| {
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
                    return;
                };
                if op.rollback {
                    return;
                }
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
                        checked: checked.clone(),
                    },
                ));
            });
            self.stats.local_ops += walker.ops;
            for (node, r) in edits {
                self.remember(id, node, r);
            }
        }
        self.enforce_capacity(now);
    }

    fn remember(&mut self, id: &SublayerId, node: TreeID, r: Recent) {
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

    /// Forget edits older than the window (at most once per window).
    fn prune(&mut self, now: Instant, force: bool) {
        let due = self
            .last_prune
            .is_none_or(|t| now.saturating_duration_since(t) >= self.config.window);
        if self.len == 0 || !(due || force) {
            return;
        }
        self.last_prune = Some(now);
        let window = self.config.window;
        self.retain(|r| now.saturating_duration_since(r.edited_at) <= window);
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
        let mut times: Vec<Instant> = self
            .recent
            .values()
            .flat_map(|n| n.values())
            .flatten()
            .map(|r| r.edited_at)
            .collect();
        times.sort_unstable();
        let cutoff = times[times.len() - keep];
        self.retain(|r| r.edited_at >= cutoff);
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
    /// change of the element still deserves a hint. Cheap when nothing arrived
    /// (a version-vector comparison per sublayer with remembered edits).
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
        self.prune(now, false);
        let mut out = Vec::new();
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
        let pending = self.offline.as_ref().map(|o| o.at);
        let window = self.config.window;
        // Edits made while a reconnect is pending belong to the merge summary.
        let live = |r: &Recent| {
            now.saturating_duration_since(r.edited_at) <= window
                && pending.is_none_or(|t| r.edited_at < t)
        };
        let doc = layer.doc();
        let ours = doc.peer_id();
        let version = doc.oplog_vv();
        // Remote ops nobody has looked at yet, per peer.
        let arrived = |r: &Recent| {
            version
                .iter()
                .any(|(p, &end)| *p != ours && r.checked.get(p).copied().unwrap_or(0) < end)
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
        if picked.is_empty() && gone.is_empty() {
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
        for &(node, i, r) in &picked {
            let ops = remote
                .get(&(r.container.clone(), r.key.clone()))
                .map_or(&[][..], Vec::as_slice);
            if ops.is_empty() {
                continue;
            }
            checked_elements += 1;
            if let Some((n, overridden)) = judge(layer, doc, ours, node, r, ops, now) {
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
                    concurrent: deletion.is_some_and(|d| ops::concurrent_with(doc, d, r.op)),
                    edited_at: r.edited_at,
                    at: now,
                });
            }
        }

        self.stats.elements_checked += checked_elements;
        self.stats.remote_ops += decoded;
        // Remember how far we looked; forget what was overridden or removed.
        let checked = Arc::new(version);
        let nodes = self.recent.get_mut(id).expect("looked up above");
        for list in nodes.values_mut() {
            for r in list.iter_mut() {
                r.checked = checked.clone();
            }
        }
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

        notices.sort_by(|a, b| {
            (&a.path, &a.attr, a.element, a.node).cmp(&(&b.path, &b.attr, b.element, b.node))
        });
        out.extend(notices);
    }

    // ---- offline ----

    /// [`connection_lost_at`](Self::connection_lost_at) with the current time.
    pub fn connection_lost(&mut self, stage: &Stage) {
        self.connection_lost_at(stage, Instant::now());
    }

    /// The link to the others is gone (transport error, hub unreachable,
    /// shared folder unreachable, or the app goes offline on purpose): remember
    /// the version we had. A second call before [`caught_up`](Self::caught_up)
    /// keeps the first mark.
    pub fn connection_lost_at(&mut self, stage: &Stage, now: Instant) {
        if self.offline.is_none() {
            self.offline = Some(Offline {
                since: ProjectVersion::current(stage),
                at: now,
            });
        }
    }

    /// The version remembered by [`connection_lost`](Self::connection_lost),
    /// while the reconnect is pending.
    pub fn offline_since(&self) -> Option<&ProjectVersion> {
        self.offline.as_ref().map(|o| &o.since)
    }

    /// Reconnected and caught up (see `SyncSession::is_caught_up`): what the
    /// merge did, from the version remembered by
    /// [`connection_lost`](Self::connection_lost) to now ([`merge_summary`]).
    /// `None` if no reconnect was pending. On an error the mark is kept.
    pub fn caught_up(&mut self, stage: &Stage) -> Result<Option<MergeSummary>, SyncError> {
        let Some(off) = self.offline.take() else {
            return Ok(None);
        };
        match merge_summary(stage, &off.since) {
            Ok(summary) => {
                // Remote ops merged so far are in the summary.
                for (id, nodes) in &mut self.recent {
                    if let Some(layer) = stage.sublayer(id) {
                        let v = Arc::new(layer.version());
                        for r in nodes.values_mut().flatten() {
                            r.checked = v.clone();
                        }
                    }
                }
                Ok(Some(summary))
            }
            Err(e) => {
                self.offline = Some(off);
                Err(e)
            }
        }
    }
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
    let mut found: FxHashMap<ElementKey, Vec<RemoteOp>> = FxHashMap::default();
    let mut walker = Walker::new(doc);
    for (&peer, &end) in version.iter() {
        if peer == ours {
            continue;
        }
        let start = picked
            .iter()
            .map(|(_, _, r)| r.checked.get(&peer).copied().unwrap_or(0))
            .min()
            .unwrap_or(end);
        each_span_writing(doc, peer, start, end, &interest, &mut |span| {
            walker.walk(span, |op| {
                let (Target::Element { value, .. }, Some((c, k))) = (op.target, op.key) else {
                    return;
                };
                if op.rollback || !interest.contains(c) {
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
/// value was overridden (then it is forgotten).
fn judge(
    layer: &Sublayer,
    doc: &LoroDoc,
    ours: u64,
    node: TreeID,
    r: &Recent,
    ops: &[RemoteOp],
    now: Instant,
) -> Option<(ConcurrentEdit, bool)> {
    // Ops this edit has not been compared with yet (the scan started at the
    // oldest of all candidates).
    let fresh: Vec<&RemoteOp> = ops
        .iter()
        .filter(|o| o.id.counter >= r.checked.get(&o.id.peer).copied().unwrap_or(0))
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
        // they set the same.
        let lost: Vec<&RemoteOp> = fresh
            .into_iter()
            .filter(|o| r.element.decode(o.value.as_ref()) != r.ours)
            .collect();
        let last = lost.iter().max_by_key(|o| o.order())?;
        let theirs = r.element.decode(last.value.as_ref());
        // An op that knew ours would have won, so every loser was concurrent.
        return Some((
            notice(
                ConflictOutcome::Kept,
                theirs,
                r.author.clone(),
                authors(&lost),
                true,
            ),
            false,
        ));
    }
    if result == r.ours {
        return None;
    }
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
    /// What the others changed meanwhile, by prim: `history::diff` from our
    /// version (what we had right before merging) to now, so only their
    /// changes (including where they won over ours).
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
/// vectors and history. Costs like `history::diff` (read-only snapshots of the
/// version we had), so call it once per reconnect, not per frame.
///
/// The stage's peers change when the project is reopened, so `since` must
/// come from this stage, not from before a restart.
pub fn merge_summary(stage: &Stage, since: &ProjectVersion) -> Result<MergeSummary, SyncError> {
    let now = ProjectVersion::current(stage);
    let mut mine_version = ProjectVersion::default();
    let mut out = MergeSummary {
        since: since.clone(),
        ..MergeSummary::default()
    };
    for id in stage.layer_stack() {
        let layer = stage.sublayer(id).expect("in stack");
        let (from, to) = (since.get(id), now.get(id));
        if !to.includes_vv(&from) {
            // Not the document we had then (replaced or reloaded).
            mine_version.sublayers.insert(id.clone(), to);
            continue;
        }
        let peer = layer.doc().peer_id();
        let mut mine = from.clone();
        if let Some(&c) = to.get(&peer)
            && c > from.get(&peer).copied().unwrap_or(0)
        {
            mine.insert(peer, c);
        }
        if from != to {
            summarize_layer(id, layer, &from, &mine, &to, &mut out)?;
        }
        mine_version.sublayers.insert(id.clone(), mine);
    }
    out.merged = history::diff(stage, &mine_version, &now)?;
    Ok(out)
}

/// Paths of prims that are gone, from read-only snapshots built on demand
/// (at most one per version).
struct PastPaths<'a> {
    live: &'a Sublayer,
    versions: [&'a VersionVector; 2],
    layers: [Option<history::LayerAt<'a>>; 2],
}

impl PastPaths<'_> {
    /// Path of `node` at version 0 (`since`) or 1 (ours).
    fn path(&mut self, which: usize, node: TreeID) -> Result<PrimPath, SyncError> {
        let layer = match &mut self.layers[which] {
            Some(l) => l,
            slot => slot.insert(history::layer_at(self.live, self.versions[which])?),
        };
        Ok(layer
            .get()
            .node_path(node)
            .cloned()
            .unwrap_or_else(PrimPath::root))
    }
}

fn summarize_layer(
    id: &SublayerId,
    layer: &Sublayer,
    from: &VersionVector,
    mine: &VersionVector,
    to: &VersionVector,
    out: &mut MergeSummary,
) -> Result<(), SyncError> {
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
                if op.rollback {
                    return;
                }
                // Removing a prim we removed too is no conflict.
                if all && !matches!(op.target, Target::Delete(_)) {
                    let t = touched.entry(op.target.node()).or_default();
                    t.extend(op.author.cloned());
                }
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
        versions: [from, mine],
        layers: [None, None],
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
            .filter(|o| e.element.decode(o.value.as_ref()) != e.ours)
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
            // The prim is gone: by someone else, or by us later on.
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
            edit.path = past.path(1, e.node)?;
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
            edit.winner = ops
                .iter()
                .filter(|o| winner_peer.is_none_or(|p| o.id.peer == p))
                .max_by_key(|o| o.order())
                .and_then(|o| o.author.clone());
            overridden.push(edit);
        }
    }

    // Who else touched each prim we deleted, or anything below it.
    let deleted_set: FxHashSet<TreeID> = deleted.iter().copied().collect();
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
                let Some(d) = removed_by_others(n) else {
                    continue;
                };
                let by = ops::author_of(doc, d).0.into_iter().collect();
                let path = past.path(1, n)?;
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
                let path = past.path(0, n)?;
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
