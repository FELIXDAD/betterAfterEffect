//! Versions (`docs/spec/sync.md` §5): named checkpoints, read-only views of any
//! version, per-prim differences with authors, and rollback. Differences and
//! rollback match prims by Loro tree node, not by path (`crate::identity`).
//!
//! Every sublayer's Loro document keeps its full history, so a version of the
//! project is just one version vector per sublayer ([`ProjectVersion`]). Named
//! versions live in their own small Loro document ([`History`],
//! [`DocKey::History`]) that syncs like a sublayer.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use bae_model::loro::{
    ContainerID, ExportMode, Index, JsonOpContent, JsonTreeOp, LoroDoc, LoroMap, LoroValue, TreeID,
    ValueOrContainer, VersionVector,
};
use bae_model::{
    AttrSpec, AuthorId, ModelError, Op, PrimPath, PrimSpec, PrimTree, PrimType, Reference,
    Specifier, Stage, StageEvent, Sublayer, SublayerId, Transaction,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::docs::{DocSet, Imported};
use crate::error::SyncError;
use crate::identity::{Matching, NodeIndex};
use crate::protocol::DocKey;

/// File name of the history document inside a `.bae` directory.
pub const HISTORY_FILE: &str = "history.loro";

const CHECKPOINTS: &str = "checkpoints";
const K_NAME: &str = "name";
const K_NOTE: &str = "note";
const K_AUTHOR: &str = "author";
const K_CREATED: &str = "created";
const K_STACK: &str = "stack";
const K_VERSIONS: &str = "versions";

/// A version of the whole project: one version vector per sublayer. Sublayers
/// that are not listed are empty at this version.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectVersion {
    pub sublayers: BTreeMap<SublayerId, VersionVector>,
}

impl ProjectVersion {
    /// The stage as it is now.
    pub fn current(stage: &Stage) -> Self {
        ProjectVersion {
            sublayers: stage
                .sublayers()
                .map(|s| (s.id().clone(), s.version()))
                .collect(),
        }
    }

    /// Version of one sublayer (empty if not listed).
    pub fn get(&self, id: &SublayerId) -> VersionVector {
        self.sublayers.get(id).cloned().unwrap_or_default()
    }

    /// JSON form for tools and commands: sublayer → (Loro peer ID as a decimal
    /// string → number of that peer's ops).
    pub fn to_json(&self) -> VersionJson {
        self.sublayers
            .iter()
            .map(|(id, vv)| {
                let peers = vv
                    .iter()
                    .map(|(peer, counter)| (peer.to_string(), *counter))
                    .collect();
                (id.to_string(), peers)
            })
            .collect()
    }

    pub fn from_json(json: &VersionJson) -> Result<Self, SyncError> {
        let mut sublayers = BTreeMap::new();
        for (id, peers) in json {
            let id = SublayerId::new(id).map_err(|_| SyncError::InvalidDocKey(id.clone()))?;
            let mut vv = VersionVector::new();
            for (peer, counter) in peers {
                let peer: u64 = peer
                    .parse()
                    .map_err(|_| SyncError::Decode(format!("bad peer ID {peer:?}")))?;
                vv.insert(peer, *counter);
            }
            sublayers.insert(id, vv);
        }
        Ok(ProjectVersion { sublayers })
    }
}

/// JSON form of a [`ProjectVersion`]: sublayer → (peer ID → op count).
pub type VersionJson = BTreeMap<String, BTreeMap<String, i32>>;

/// Identifies a named version: a time-ordered UUID (v7) in hex.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CheckpointId(pub String);

impl std::fmt::Display for CheckpointId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A named version ("给客户的第 1 版").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub name: String,
    pub note: String,
    pub author: AuthorId,
    /// Unix milliseconds.
    pub created_ms: i64,
    /// Layer stack (strongest first) when the checkpoint was made.
    pub layer_stack: Vec<SublayerId>,
    pub version: ProjectVersion,
}

/// Project history: named versions, stored in one Loro document that syncs like
/// a sublayer ([`DocKey::History`]) and is saved as `history.loro`.
///
/// Concurrent checkpoints from several people never conflict (each has its own
/// ID); concurrent renames of the same checkpoint are last-writer-wins.
#[derive(Debug)]
pub struct History {
    doc: LoroDoc,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        let doc = LoroDoc::new();
        doc.set_record_timestamp(true);
        History { doc }
    }

    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, SyncError> {
        let h = History::new();
        h.doc.import(bytes)?;
        Ok(h)
    }

    /// Load `history.loro` from a `.bae` directory (empty history if missing).
    pub fn load(project: impl AsRef<Path>) -> Result<Self, SyncError> {
        let path = project.as_ref().join(HISTORY_FILE);
        match fs::read(&path) {
            Ok(bytes) => Self::from_snapshot(&bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(History::new()),
            Err(e) => Err(SyncError::io(&path)(e)),
        }
    }

    /// Save as `history.loro` in a `.bae` directory (temporary file + rename).
    pub fn save(&self, project: impl AsRef<Path>) -> Result<(), SyncError> {
        let path = project.as_ref().join(HISTORY_FILE);
        let tmp = project.as_ref().join(format!(
            "{HISTORY_FILE}.tmp-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let bytes = self.export_snapshot()?;
        let write = || -> std::io::Result<()> {
            use std::io::Write;
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
            drop(f);
            fs::rename(&tmp, &path)
        };
        write().map_err(|e| {
            let _ = fs::remove_file(&tmp);
            SyncError::io(&path)(e)
        })
    }

    /// The underlying document (read-only use).
    pub fn doc(&self) -> &LoroDoc {
        &self.doc
    }

    pub fn version(&self) -> VersionVector {
        self.doc.oplog_vv()
    }

    pub fn export_snapshot(&self) -> Result<Vec<u8>, SyncError> {
        Ok(self.doc.export(ExportMode::Snapshot)?)
    }

    pub fn export_updates(&self, since: &VersionVector) -> Result<Vec<u8>, SyncError> {
        Ok(self.doc.export(ExportMode::updates(since))?)
    }

    pub fn import(&mut self, bytes: &[u8]) -> Result<(), SyncError> {
        self.doc.import(bytes)?;
        Ok(())
    }

    fn checkpoints_map(&self) -> LoroMap {
        self.doc.get_map(CHECKPOINTS)
    }

    fn commit(&self, author: &AuthorId, label: &str) {
        let msg = serde_json::json!({ "author": author, "label": label }).to_string();
        self.doc.set_next_commit_message(&msg);
        self.doc.commit();
    }

    /// Name the stage's current version.
    pub fn create_checkpoint(
        &mut self,
        stage: &Stage,
        author: &AuthorId,
        name: &str,
        note: &str,
    ) -> Result<Checkpoint, SyncError> {
        let cp = Checkpoint {
            id: CheckpointId(uuid::Uuid::now_v7().simple().to_string()),
            name: name.to_owned(),
            note: note.to_owned(),
            author: author.clone(),
            created_ms: now_ms(),
            layer_stack: stage.layer_stack().to_vec(),
            version: ProjectVersion::current(stage),
        };
        let map = self
            .checkpoints_map()
            .insert_container(&cp.id.0, LoroMap::new())?;
        map.insert(K_NAME, cp.name.as_str())?;
        map.insert(K_NOTE, cp.note.as_str())?;
        map.insert(K_AUTHOR, cp.author.as_str())?;
        map.insert(K_CREATED, cp.created_ms)?;
        let stack: Vec<String> = cp
            .layer_stack
            .iter()
            .map(|s| s.as_str().to_owned())
            .collect();
        map.insert(K_STACK, serde_json::to_string(&stack).expect("strings"))?;
        let versions = map.insert_container(K_VERSIONS, LoroMap::new())?;
        for (id, vv) in &cp.version.sublayers {
            versions.insert(id.as_str(), vv.encode())?;
        }
        self.commit(author, "checkpoint.create");
        Ok(cp)
    }

    /// All named versions, oldest first.
    pub fn checkpoints(&self) -> Vec<Checkpoint> {
        let LoroValue::Map(all) = self.checkpoints_map().get_deep_value() else {
            return Vec::new();
        };
        let mut out: Vec<Checkpoint> = all
            .iter()
            .filter_map(|(id, v)| parse_checkpoint(id, v))
            .collect();
        out.sort_by(|a, b| (a.created_ms, &a.id).cmp(&(b.created_ms, &b.id)));
        out
    }

    pub fn checkpoint(&self, id: &CheckpointId) -> Option<Checkpoint> {
        let ValueOrContainer::Container(c) = self.checkpoints_map().get(&id.0)? else {
            return None;
        };
        parse_checkpoint(&id.0, &c.into_map().ok()?.get_deep_value())
    }

    pub fn rename_checkpoint(
        &mut self,
        id: &CheckpointId,
        author: &AuthorId,
        name: &str,
        note: Option<&str>,
    ) -> Result<(), SyncError> {
        let Some(ValueOrContainer::Container(c)) = self.checkpoints_map().get(&id.0) else {
            return Err(SyncError::UnknownCheckpoint(id.0.clone()));
        };
        let map = c
            .into_map()
            .map_err(|_| SyncError::UnknownCheckpoint(id.0.clone()))?;
        map.insert(K_NAME, name)?;
        if let Some(note) = note {
            map.insert(K_NOTE, note)?;
        }
        self.commit(author, "checkpoint.rename");
        Ok(())
    }

    /// Remove a named version (the project history itself is untouched).
    pub fn delete_checkpoint(
        &mut self,
        id: &CheckpointId,
        author: &AuthorId,
    ) -> Result<bool, SyncError> {
        let map = self.checkpoints_map();
        if map.get(&id.0).is_none() {
            return Ok(false);
        }
        map.delete(&id.0)?;
        self.commit(author, "checkpoint.delete");
        Ok(true)
    }

    /// Read-only view of the project at a named version.
    pub fn view(&self, stage: &Stage, id: &CheckpointId) -> Result<VersionView, SyncError> {
        let cp = self
            .checkpoint(id)
            .ok_or_else(|| SyncError::UnknownCheckpoint(id.0.clone()))?;
        view_at(stage, &cp.version, Some(&cp.layer_stack))
    }

    /// Put the project (or part of it) back to a named version, as one new
    /// undoable step (see [`revert`]).
    pub fn revert_to(
        &self,
        stage: &mut Stage,
        author: &AuthorId,
        id: &CheckpointId,
        scope: &RevertScope,
    ) -> Result<StageEvent, SyncError> {
        let cp = self
            .checkpoint(id)
            .ok_or_else(|| SyncError::UnknownCheckpoint(id.0.clone()))?;
        revert(
            stage,
            author,
            &format!("revert to {}", cp.name),
            &cp.version,
            scope,
        )
    }

    /// Per-prim differences from one named version to another (`None`: to the
    /// current stage).
    pub fn diff(
        &self,
        stage: &Stage,
        from: &CheckpointId,
        to: Option<&CheckpointId>,
    ) -> Result<Vec<PrimDiff>, SyncError> {
        let get = |id: &CheckpointId| {
            self.checkpoint(id)
                .map(|c| c.version)
                .ok_or_else(|| SyncError::UnknownCheckpoint(id.0.clone()))
        };
        let from = get(from)?;
        let to = match to {
            Some(id) => get(id)?,
            None => ProjectVersion::current(stage),
        };
        diff(stage, &from, &to)
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn parse_checkpoint(id: &str, v: &LoroValue) -> Option<Checkpoint> {
    let LoroValue::Map(m) = v else {
        return None;
    };
    let string = |k: &str| match m.get(k) {
        Some(LoroValue::String(s)) => Some(s.to_string()),
        _ => None,
    };
    let created_ms = match m.get(K_CREATED) {
        Some(LoroValue::I64(n)) => *n,
        _ => 0,
    };
    let layer_stack: Vec<SublayerId> = string(K_STACK)
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
        .iter()
        .filter_map(|s| SublayerId::new(s).ok())
        .collect();
    let mut sublayers = BTreeMap::new();
    if let Some(LoroValue::Map(vs)) = m.get(K_VERSIONS) {
        for (k, v) in vs.iter() {
            if let (Ok(id), LoroValue::Binary(b)) = (SublayerId::new(k), v)
                && let Ok(vv) = VersionVector::decode(b)
            {
                sublayers.insert(id, vv);
            }
        }
    }
    Some(Checkpoint {
        id: CheckpointId(id.to_owned()),
        name: string(K_NAME).unwrap_or_default(),
        note: string(K_NOTE).unwrap_or_default(),
        author: string(K_AUTHOR)
            .and_then(|a| AuthorId::new(&a).ok())
            .unwrap_or_else(AuthorId::system),
        created_ms,
        layer_stack,
        version: ProjectVersion { sublayers },
    })
}

// ---- read-only views ----

/// The project at some version. Read-only: only `&Stage` is handed out.
#[derive(Debug)]
pub struct VersionView {
    stage: Stage,
    version: ProjectVersion,
    missing: Vec<SublayerId>,
}

impl VersionView {
    pub fn stage(&self) -> &Stage {
        &self.stage
    }

    pub fn version(&self) -> &ProjectVersion {
        &self.version
    }

    /// Sublayers of that version's stack that are no longer in the project, so
    /// they could not be shown.
    pub fn missing(&self) -> &[SublayerId] {
        &self.missing
    }
}

/// Build a read-only stage at `version`. `layer_stack` (strongest first) defaults
/// to the current stack. Cross-project references are not attached.
pub fn view_at(
    stage: &Stage,
    version: &ProjectVersion,
    layer_stack: Option<&[SublayerId]>,
) -> Result<VersionView, SyncError> {
    let stack = layer_stack.unwrap_or(stage.layer_stack());
    let mut layers = Vec::new();
    let mut missing = Vec::new();
    for id in stack {
        match stage.sublayer(id) {
            Some(live) => layers.push(layer_at(live, &version.get(id))?.into_owned()?),
            None => missing.push(id.clone()),
        }
    }
    if layers.is_empty() {
        return Err(SyncError::VersionNotAvailable(
            stack.first().cloned().unwrap_or_else(SublayerId::root),
        ));
    }
    Ok(VersionView {
        stage: Stage::from_sublayers(layers)?,
        version: version.clone(),
        missing,
    })
}

/// A sublayer at some version: the live one if the version is current.
enum LayerAt<'a> {
    Live(&'a Sublayer),
    Past(Sublayer),
}

impl LayerAt<'_> {
    fn get(&self) -> &Sublayer {
        match self {
            LayerAt::Live(s) => s,
            LayerAt::Past(s) => s,
        }
    }

    fn into_owned(self) -> Result<Sublayer, SyncError> {
        match self {
            LayerAt::Past(s) => Ok(s),
            LayerAt::Live(s) => Ok(Sublayer::from_snapshot(
                s.id().clone(),
                &s.export_snapshot()?,
            )?),
        }
    }
}

fn layer_at<'a>(live: &'a Sublayer, vv: &VersionVector) -> Result<LayerAt<'a>, SyncError> {
    let doc = live.doc();
    let now = doc.oplog_vv();
    if *vv == now {
        return Ok(LayerAt::Live(live));
    }
    if vv.is_empty() {
        return Ok(LayerAt::Past(Sublayer::new(live.id().clone())));
    }
    if !now.includes_vv(vv) {
        // A collaborator's version whose ops have not arrived here yet.
        return Err(SyncError::VersionNotAvailable(live.id().clone()));
    }
    let frontiers = doc.vv_to_frontiers(vv);
    let bytes = doc.export(ExportMode::SnapshotAt {
        version: Cow::Owned(frontiers),
    })?;
    Ok(LayerAt::Past(Sublayer::from_snapshot(
        live.id().clone(),
        &bytes,
    )?))
}

// ---- differences ----

/// How a prim spec differs between two versions of one sublayer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimChange {
    Added,
    Removed,
    Modified,
    /// Moved or renamed (and possibly modified: see `attrs` / `fields`).
    Moved {
        from: PrimPath,
    },
}

/// One prim spec that differs between two versions, and who changed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimDiff {
    pub sublayer: SublayerId,
    /// Path at the newer version (the older one for `Removed`).
    pub path: PrimPath,
    pub change: PrimChange,
    /// Attributes whose opinion (value, keyframes or expression) differs.
    pub attrs: Vec<String>,
    /// Other differing fields: `type`, `specifier`, `references`.
    pub fields: Vec<&'static str>,
    /// Authors of the commits between the versions that touched this prim.
    pub authors: Vec<AuthorId>,
    /// Unix seconds of the latest such commit (0 if unknown).
    pub last_changed: i64,
}

/// Per-prim differences between two project versions, for every sublayer of the
/// current stack, strongest sublayer first, paths sorted.
pub fn diff(
    stage: &Stage,
    from: &ProjectVersion,
    to: &ProjectVersion,
) -> Result<Vec<PrimDiff>, SyncError> {
    let mut out = Vec::new();
    for id in stage.layer_stack() {
        let live = stage.sublayer(id).expect("in stack");
        let (vf, vt) = (from.get(id), to.get(id));
        if vf == vt {
            continue;
        }
        let a = layer_at(live, &vf)?;
        let b = layer_at(live, &vt)?;
        let touched = node_authors(live.doc(), &vf, &vt);
        out.extend(diff_layers(id, a.get(), b.get(), &touched));
    }
    Ok(out)
}

#[derive(Default)]
struct Touch {
    authors: BTreeSet<AuthorId>,
    last: i64,
}

/// Differences of one sublayer between versions `a` (older) and `b`, matching
/// prims by tree node ([`Matching`]), never by path.
fn diff_layers(
    id: &SublayerId,
    a: &Sublayer,
    b: &Sublayer,
    touched: &FxHashMap<TreeID, Touch>,
) -> Vec<PrimDiff> {
    let (old, new) = (NodeIndex::new(a), NodeIndex::new(b));
    let m = Matching::new(&old, &new);
    let mut out = Vec::new();
    for (node, n) in new.iter() {
        let (change, before) = match m.old_of(node) {
            Some(o) => {
                let was = old.get(o).expect("matched");
                let change = if m.moved(&old, &new, node) {
                    PrimChange::Moved {
                        from: was.path.clone(),
                    }
                } else {
                    PrimChange::Modified
                };
                (change, Some((o, was.spec)))
            }
            None => (PrimChange::Added, None),
        };
        let (attrs, fields) = match before {
            Some((_, spec)) => spec_diff(spec, n.spec),
            None => (n.spec.attrs.keys().cloned().collect(), Vec::new()),
        };
        if change == PrimChange::Modified && attrs.is_empty() && fields.is_empty() {
            continue;
        }
        let (authors, last_changed) = who(touched, [before.map(|(o, _)| o), Some(node)]);
        out.push(PrimDiff {
            sublayer: id.clone(),
            path: n.path.clone(),
            change,
            attrs,
            fields,
            authors,
            last_changed,
        });
    }
    for (node, was) in old.iter() {
        if m.new_of(node).is_some() {
            continue;
        }
        let (authors, last_changed) = who(touched, [Some(node), None]);
        out.push(PrimDiff {
            sublayer: id.clone(),
            path: was.path.clone(),
            change: PrimChange::Removed,
            attrs: was.spec.attrs.keys().cloned().collect(),
            fields: Vec::new(),
            authors,
            last_changed,
        });
    }
    // By path; where a removed prim's path is now another prim's, that one first.
    let removed = |d: &PrimDiff| d.change == PrimChange::Removed;
    out.sort_by(|x, y| x.path.cmp(&y.path).then(removed(x).cmp(&removed(y))));
    out
}

fn spec_diff(a: &PrimSpec, b: &PrimSpec) -> (Vec<String>, Vec<&'static str>) {
    let names: BTreeSet<&String> = a.attrs.keys().chain(b.attrs.keys()).collect();
    let attrs = names
        .into_iter()
        .filter(|n| a.attrs.get(*n) != b.attrs.get(*n))
        .cloned()
        .collect();
    let mut fields = Vec::new();
    if a.prim_type != b.prim_type {
        fields.push("type");
    }
    if a.specifier != b.specifier {
        fields.push("specifier");
    }
    if a.references != b.references {
        fields.push("references");
    }
    (attrs, fields)
}

fn who(touched: &FxHashMap<TreeID, Touch>, nodes: [Option<TreeID>; 2]) -> (Vec<AuthorId>, i64) {
    let mut authors = BTreeSet::new();
    let mut last = 0;
    for n in nodes.into_iter().flatten() {
        if let Some(t) = touched.get(&n) {
            authors.extend(t.authors.iter().cloned());
            last = last.max(t.last);
        }
    }
    (authors.into_iter().collect(), last)
}

/// For every tree node (prim spec) touched by ops between `a` and `b` (either
/// direction): the commit authors (`{"author", "label"}` messages written by
/// `Stage::transact`) and the latest commit time.
fn node_authors(doc: &LoroDoc, a: &VersionVector, b: &VersionVector) -> FxHashMap<TreeID, Touch> {
    let mut out: FxHashMap<TreeID, Touch> = FxHashMap::default();
    let mut node_of: FxHashMap<ContainerID, Option<TreeID>> = FxHashMap::default();
    let spans: Vec<_> = b.sub_iter(a).chain(a.sub_iter(b)).collect();
    for span in spans {
        for change in doc.export_json_in_id_span(span) {
            let author = change
                .msg
                .as_deref()
                .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                .and_then(|v| v.get("author")?.as_str().map(str::to_owned))
                .and_then(|s| AuthorId::new(&s).ok());
            for op in &change.ops {
                let node = match &op.content {
                    JsonOpContent::Tree(t) => Some(match t {
                        JsonTreeOp::Create { target, .. }
                        | JsonTreeOp::Move { target, .. }
                        | JsonTreeOp::Delete { target } => *target,
                    }),
                    _ => *node_of.entry(op.container.clone()).or_insert_with(|| {
                        doc.get_path_to_container(&op.container)?
                            .iter()
                            .find_map(|(_, idx)| match idx {
                                Index::Node(t) => Some(*t),
                                _ => None,
                            })
                    }),
                };
                if let Some(node) = node {
                    let t = out.entry(node).or_default();
                    if let Some(a) = &author {
                        t.authors.insert(a.clone());
                    }
                    t.last = t.last.max(change.timestamp);
                }
            }
        }
    }
    out
}

// ---- rollback ----

/// What [`revert`] restores.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RevertScope {
    /// Every sublayer of the current stack.
    Project,
    /// One sublayer.
    Sublayer(SublayerId),
    /// One prim and its descendants, in every sublayer.
    Prim(PrimPath),
}

impl RevertScope {
    fn covers(&self, layer: &SublayerId, path: &PrimPath) -> bool {
        match self {
            RevertScope::Project => true,
            RevertScope::Sublayer(id) => id == layer,
            RevertScope::Prim(root) => root.is_prefix_of(path),
        }
    }
}

/// Put (part of) the project back the way it was at `to`, as one new undoable
/// step by `author`: history is never rewritten, collaborators receive the
/// rollback like any other edit, and undo takes it back.
///
/// Prims are matched by tree node, not by path (`crate::identity`). Prims that
/// did not exist then are deleted, prims that existed are restored with their
/// whole subtree, and differing types, specifiers, references and attribute
/// opinions are set back. A prim that was moved or renamed since is deleted and
/// restored where it was. Not restored: sibling order of prims that exist in both
/// versions, and sublayers no longer in the stack. A sublayer that did not exist
/// at `to` is emptied.
pub fn revert(
    stage: &mut Stage,
    author: &AuthorId,
    label: &str,
    to: &ProjectVersion,
    scope: &RevertScope,
) -> Result<StageEvent, SyncError> {
    let plan = revert_edits(stage, to, scope)?;
    // Like undo, a rollback restores locked layers too.
    let (_, event) = stage.transact(author, label, |tx| {
        tx.allow_locked(|tx| plan.apply(tx))?;
        Ok::<_, SyncError>(())
    })?;
    Ok(event)
}

/// The edits [`revert`] applies (also used by the `version.revert` command).
pub(crate) fn revert_edits(
    stage: &Stage,
    to: &ProjectVersion,
    scope: &RevertScope,
) -> Result<RevertPlan, SyncError> {
    let mut steps = Vec::new();
    for id in stage.layer_stack() {
        if matches!(scope, RevertScope::Sublayer(s) if s != id) {
            continue;
        }
        let live = stage.sublayer(id).expect("in stack");
        let past = layer_at(live, &to.get(id))?;
        for step in revert_steps(id, live, past.get(), scope) {
            steps.push((id.clone(), step));
        }
    }
    Ok(RevertPlan(steps))
}

/// The edits of a rollback, by tree node. Each becomes an [`Op`] only right
/// before it is applied, with the paths of the mirror at that moment.
pub(crate) struct RevertPlan(Vec<(SublayerId, Step)>);

impl RevertPlan {
    /// Apply every edit; returns how many ops were applied.
    pub(crate) fn apply(self, tx: &mut Transaction<'_>) -> Result<usize, ModelError> {
        let mut applied = 0;
        for (layer, step) in self.0 {
            let live = tx
                .stage()
                .sublayer(&layer)
                .ok_or_else(|| ModelError::UnknownSublayer(layer.clone()))?;
            if let Some(op) = step.op(&layer, live)? {
                tx.apply_to(&layer, op)?;
                applied += 1;
            }
        }
        Ok(applied)
    }
}

enum Step {
    Delete(TreeID),
    /// Re-create a prim of the old version under `parent` (a live node).
    Restore {
        parent: Parent,
        name: String,
        tree: PrimTree,
        index: Option<usize>,
    },
    Set(TreeID, Field),
}

enum Parent {
    Top,
    Node(TreeID),
    /// The old parent is gone and is not restored (outside the scope).
    Gone(PrimPath),
}

enum Field {
    Specifier(Specifier),
    Type(Option<PrimType>),
    References(Vec<Reference>),
    Attr(String, Option<AttrSpec>),
}

impl Step {
    fn op(self, layer: &SublayerId, live: &Sublayer) -> Result<Option<Op>, ModelError> {
        let path = |n: TreeID| live.node_path(n).cloned();
        Ok(match self {
            Step::Delete(n) => path(n).map(|path| Op::DeletePrim { path }),
            Step::Set(n, field) => path(n).map(|path| field.op(path)),
            Step::Restore {
                parent,
                name,
                tree,
                index,
            } => {
                let missing = |path: PrimPath| ModelError::PrimNotFound {
                    sublayer: layer.clone(),
                    path,
                };
                let at = match parent {
                    Parent::Top => PrimPath::root(),
                    Parent::Node(n) => path(n).ok_or_else(|| missing(PrimPath::root()))?,
                    Parent::Gone(old) => return Err(missing(old)),
                };
                // The name was chosen free of every remaining sibling's raw
                // name; a mirror name can still take it only by an unlikely
                // literal match with a collision suffix.
                let mut path = at.child(&name)?;
                let mut n = 2;
                while live.has_spec(&path) {
                    path = at.child(&format!("{name}_{n}"))?;
                    n += 1;
                }
                Some(Op::RestorePrim { path, tree, index })
            }
        })
    }
}

impl Field {
    fn op(self, path: PrimPath) -> Op {
        match self {
            Field::Specifier(specifier) => Op::SetSpecifier { path, specifier },
            Field::Type(prim_type) => Op::SetPrimType { path, prim_type },
            Field::References(references) => Op::SetReferences { path, references },
            Field::Attr(attr, spec) => Op::SetAttr { path, attr, spec },
        }
    }
}

/// Edits that turn `live` into `past` within `scope`.
fn revert_steps(
    id: &SublayerId,
    live: &Sublayer,
    past: &Sublayer,
    scope: &RevertScope,
) -> Vec<Step> {
    let (now, then) = (NodeIndex::new(live), NodeIndex::new(past));
    let m = Matching::new(&then, &now);
    let in_scope = |p: &PrimPath| scope.covers(id, p);
    let mut steps = Vec::new();

    // Delete what did not exist then, or was elsewhere (top-most only: the
    // subtree goes with it). `gone`: deleted, explicitly or with an ancestor.
    let live_paths = now.by_path();
    let mut gone: FxHashSet<TreeID> = FxHashSet::default();
    for &(path, node) in &live_paths {
        let n = now.get(node).expect("listed");
        if n.parent.is_some_and(|p| gone.contains(&p)) {
            gone.insert(node);
        } else if in_scope(path) && (m.old_of(node).is_none() || m.moved(&then, &now, node)) {
            steps.push(Step::Delete(node));
            gone.insert(node);
        }
    }

    // Restore what existed then and is gone or elsewhere now (top-most only,
    // with its whole subtree), under the parent's node at its current path, at
    // its old position. A name that a remaining sibling has is not reused.
    let mut restored: FxHashSet<TreeID> = FxHashSet::default();
    let mut taken: FxHashMap<Option<TreeID>, FxHashSet<String>> = FxHashMap::default();
    for (path, node) in then.by_path() {
        let n = then.get(node).expect("listed");
        if n.parent.is_some_and(|p| restored.contains(&p)) {
            restored.insert(node);
            continue;
        }
        let stays = m
            .new_of(node)
            .is_some_and(|l| !gone.contains(&l) && !m.moved(&then, &now, l));
        if stays || !in_scope(path) {
            continue;
        }
        // The parent's live node (`Some(None)`: top level), if it remains.
        let live_parent = match n.parent {
            None => Some(None),
            Some(p) => m.new_of(p).filter(|l| !gone.contains(l)).map(Some),
        };
        let (parent, name) = match live_parent {
            Some(key) => {
                let names = taken.entry(key).or_insert_with(|| {
                    now.children(key)
                        .iter()
                        .filter(|c| !gone.contains(*c))
                        .filter_map(|c| now.get(*c))
                        .map(|c| c.name.to_owned())
                        .collect()
                });
                let name = unique_name(n.name, names);
                (key.map_or(Parent::Top, Parent::Node), name)
            }
            None => {
                let old_parent = path.parent().unwrap_or_else(PrimPath::root);
                (Parent::Gone(old_parent), n.name.to_owned())
            }
        };
        steps.push(Step::Restore {
            parent,
            name,
            tree: restore_tree(&then, node),
            index: Some(n.index),
        });
        restored.insert(node);
    }

    // Set back what differs on prims that stay.
    for &(path, node) in &live_paths {
        if gone.contains(&node) || !in_scope(path) {
            continue;
        }
        let Some(was) = m.old_of(node).and_then(|o| then.get(o)) else {
            continue;
        };
        let (now, then) = (now.get(node).expect("listed").spec, was.spec);
        if now.specifier != then.specifier {
            steps.push(Step::Set(node, Field::Specifier(then.specifier)));
        }
        if now.prim_type != then.prim_type {
            steps.push(Step::Set(node, Field::Type(then.prim_type.clone())));
        }
        if now.references != then.references {
            steps.push(Step::Set(node, Field::References(then.references.clone())));
        }
        let names: BTreeSet<&String> = now.attrs.keys().chain(then.attrs.keys()).collect();
        for name in names {
            let old = then.attrs.get(name);
            if now.attrs.get(name) != old {
                steps.push(Step::Set(node, Field::Attr(name.clone(), old.cloned())));
            }
        }
    }
    steps
}

/// `name`, or `name_2`, `name_3`, … if it is in `taken`; the result is added.
fn unique_name(name: &str, taken: &mut FxHashSet<String>) -> String {
    let mut out = name.to_owned();
    let mut n = 2;
    while taken.contains(&out) {
        out = format!("{name}_{n}");
        n += 1;
    }
    taken.insert(out.clone());
    out
}

/// The subtree of `node` with raw names (unique among siblings), as it is
/// re-created.
fn restore_tree(index: &NodeIndex<'_>, node: TreeID) -> PrimTree {
    let n = index.get(node).expect("listed");
    let mut names = FxHashSet::default();
    PrimTree {
        name: n.name.to_owned(),
        spec: n.spec.clone(),
        children: index
            .children(Some(node))
            .iter()
            .map(|c| {
                let mut t = restore_tree(index, *c);
                t.name = unique_name(&t.name, &mut names);
                t
            })
            .collect(),
    }
}

// ---- syncing the history document ----

/// The stage's sublayers plus the history document, as one [`DocSet`] for a
/// [`crate::SyncSession`].
#[derive(Debug)]
pub struct ProjectDocs<'a> {
    pub stage: &'a mut Stage,
    pub history: &'a mut History,
}

impl<'a> ProjectDocs<'a> {
    pub fn new(stage: &'a mut Stage, history: &'a mut History) -> Self {
        ProjectDocs { stage, history }
    }
}

impl DocSet for ProjectDocs<'_> {
    fn doc_keys(&self) -> Vec<DocKey> {
        let mut keys = self.stage.doc_keys();
        keys.push(DocKey::History);
        keys
    }

    fn version(&self, doc: &DocKey) -> Option<VersionVector> {
        match doc {
            DocKey::History => Some(self.history.version()),
            _ => self.stage.version(doc),
        }
    }

    fn export_updates(&self, doc: &DocKey, since: &VersionVector) -> Result<Vec<u8>, SyncError> {
        match doc {
            DocKey::History => self.history.export_updates(since),
            _ => self.stage.export_updates(doc, since),
        }
    }

    fn export_snapshot(&self, doc: &DocKey) -> Result<Vec<u8>, SyncError> {
        match doc {
            DocKey::History => self.history.export_snapshot(),
            _ => self.stage.export_snapshot(doc),
        }
    }

    fn import(&mut self, doc: &DocKey, data: &[u8]) -> Result<Imported, SyncError> {
        match doc {
            DocKey::History => {
                self.history.import(data)?;
                Ok(Imported::Done(None))
            }
            _ => self.stage.import(doc, data),
        }
    }

    fn import_batch(
        &mut self,
        doc: &DocKey,
        batch: &[(VersionVector, Vec<u8>)],
    ) -> Result<Imported, SyncError> {
        match doc {
            DocKey::History => {
                let blobs: Vec<Vec<u8>> = batch.iter().map(|(_, d)| d.clone()).collect();
                self.history.doc.import_batch(&blobs)?;
                Ok(Imported::Done(None::<StageEvent>))
            }
            _ => self.stage.import_batch(doc, batch),
        }
    }
}
