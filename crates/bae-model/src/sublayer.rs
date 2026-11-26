//! A sublayer ("工程层"): one set of opinions, stored in one Loro document.
//!
//! # Storage layout (Loro)
//!
//! - Movable tree `prims`: one node per prim spec. Children are ordered (fractional
//!   index). Node metadata map:
//!   - `name`: prim name (string)
//!   - `spec`: `"def"` | `"over"`
//!   - `type`: prim type (string, optional)
//!   - `refs`: JSON array of [`Reference`]s (optional)
//!   - `attrs`: mergeable map, attribute name → mergeable map:
//!     - `value`: JSON of [`Value`] (optional)
//!     - `expr`: JSON of [`Expression`] (optional)
//!     - `keys`: mergeable map, time in flicks (decimal string) → JSON of [`Keyframe`]
//!
//! Every field is last-writer-wins on its own, so two people editing different
//! attributes or different keyframes of the same prim merge cleanly. Mergeable child
//! maps make concurrent creation of the same attribute converge instead of one
//! side's container replacing the other's.
//!
//! # Mirror
//!
//! Reads never touch Loro: the sublayer keeps a path-indexed mirror of all specs that
//! local ops update incrementally and remote imports rebuild.

use std::collections::BTreeMap;

use loro::{
    Container, ExportMode, LoroDoc, LoroMap, LoroTree, LoroValue, TreeID, TreeParentId,
    ValueOrContainer, VersionVector,
};
use rustc_hash::FxHashMap;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::attr::{AttrSpec, Expression, Keyframe};
use crate::error::{ModelError, Result, validate_attr_name};
use crate::event::Change;
use crate::ids::SublayerId;
use crate::op::Op;
use crate::path::{PrimPath, validate_name};
use crate::prim::{PrimSpec, PrimTree, PrimType, Reference, Specifier};
use crate::time::{Time, TimeRange};
use crate::value::Value;

const TREE: &str = "prims";
const K_NAME: &str = "name";
const K_SPEC: &str = "spec";
const K_TYPE: &str = "type";
const K_REFS: &str = "refs";
const K_ATTRS: &str = "attrs";
const K_VALUE: &str = "value";
const K_EXPR: &str = "expr";
const K_KEYS: &str = "keys";

#[derive(Clone, Debug)]
struct Node {
    path: PrimPath,
    spec: PrimSpec,
}

/// Result of applying one op.
pub(crate) struct Applied {
    pub inverse: Op,
    pub changes: Vec<Change>,
}

/// One sublayer: prim specs keyed by path, backed by a Loro document.
pub struct Sublayer {
    id: SublayerId,
    doc: LoroDoc,
    tree: LoroTree,
    by_path: FxHashMap<PrimPath, TreeID>,
    nodes: FxHashMap<TreeID, Node>,
}

impl std::fmt::Debug for Sublayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sublayer")
            .field("id", &self.id)
            .field("specs", &self.nodes.len())
            .finish_non_exhaustive()
    }
}

impl Sublayer {
    /// An empty sublayer.
    pub fn new(id: SublayerId) -> Self {
        let doc = LoroDoc::new();
        // Commit timestamps make "the project at any time" and "when was this changed"
        // answerable from history (bae-sync). Runtime setting, so set on every load.
        doc.set_record_timestamp(true);
        let tree = doc.get_tree(TREE);
        Sublayer {
            id,
            doc,
            tree,
            by_path: FxHashMap::default(),
            nodes: FxHashMap::default(),
        }
    }

    /// Pin a revision now and copy its tree on the caller's background thread.
    pub(crate) fn snapshot_reader(&self) -> impl FnOnce() -> Result<Self> + Send + use<> {
        let doc = self.doc.clone();
        let version = doc.state_frontiers();
        let id = self.id.clone();
        move || {
            let doc = doc.fork_at(&version)?;
            let mut layer = Self {
                tree: doc.get_tree(TREE),
                doc,
                id,
                by_path: FxHashMap::default(),
                nodes: FxHashMap::default(),
            };
            layer.rebuild();
            Ok(layer)
        }
    }

    /// Load from a Loro snapshot (or any update blob).
    pub fn from_snapshot(id: SublayerId, bytes: &[u8]) -> Result<Self> {
        let mut layer = Sublayer::new(id);
        layer.doc.import(bytes)?;
        layer.rebuild();
        Ok(layer)
    }

    pub fn id(&self) -> &SublayerId {
        &self.id
    }

    /// Number of prim specs.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn has_spec(&self, path: &PrimPath) -> bool {
        self.by_path.contains_key(path)
    }

    /// This sublayer's spec at `path`, if any.
    pub fn spec(&self, path: &PrimPath) -> Option<&PrimSpec> {
        self.by_path.get(path).map(|id| &self.nodes[id].spec)
    }

    /// Opinion about one attribute.
    pub fn attr(&self, path: &PrimPath, attr: &str) -> Option<&AttrSpec> {
        self.spec(path)?.attrs.get(attr)
    }

    /// Paths of all specs (unordered).
    pub fn paths(&self) -> impl Iterator<Item = &PrimPath> {
        self.by_path.keys()
    }

    /// Ordered child spec paths of `path` (`/` for top-level prims).
    pub fn children(&self, path: &PrimPath) -> Vec<PrimPath> {
        let Some(parent) = self.parent_id(path) else {
            return Vec::new();
        };
        self.child_ids(parent)
            .into_iter()
            .filter_map(|id| self.nodes.get(&id).map(|n| n.path.clone()))
            .collect()
    }

    /// Position of `path` among its siblings.
    pub fn child_index(&self, path: &PrimPath) -> Option<usize> {
        let id = *self.by_path.get(path)?;
        let parent = self.parent_id(&path.parent()?)?;
        self.child_ids(parent).iter().position(|c| *c == id)
    }

    /// The spec at `path` with all descendants.
    pub fn subtree(&self, path: &PrimPath) -> Option<PrimTree> {
        let id = *self.by_path.get(path)?;
        Some(self.tree_of(id))
    }

    /// All top-level spec trees, in order.
    pub fn to_trees(&self) -> Vec<PrimTree> {
        self.child_ids(TreeParentId::Root)
            .into_iter()
            .map(|id| self.tree_of(id))
            .collect()
    }

    // ---- sync interface (workstream 09) ----

    /// The underlying Loro document. Read-only use only: writing through it bypasses
    /// the mirror; apply remote data with [`crate::Stage::import_updates`].
    pub fn doc(&self) -> &LoroDoc {
        &self.doc
    }

    /// Version vector of all ops known to this sublayer.
    pub fn version(&self) -> VersionVector {
        self.doc.oplog_vv()
    }

    pub fn export_snapshot(&self) -> Result<Vec<u8>> {
        self.doc
            .export(ExportMode::Snapshot)
            .map_err(|e| ModelError::Storage(e.to_string()))
    }

    /// Ops not yet seen by a peer at version `since`.
    pub fn export_updates(&self, since: &VersionVector) -> Result<Vec<u8>> {
        self.doc
            .export(ExportMode::updates(since))
            .map_err(|e| ModelError::Storage(e.to_string()))
    }

    /// Import updates or a snapshot from a collaborator; returns what changed.
    /// Path of the prim spec stored in Loro tree node `id` (maps CRDT history to prims).
    pub fn node_path(&self, id: TreeID) -> Option<&PrimPath> {
        self.nodes.get(&id).map(|n| &n.path)
    }

    /// Loro tree node of the spec at `path`.
    pub fn node_id(&self, path: &PrimPath) -> Option<TreeID> {
        self.by_path.get(path).copied()
    }

    /// Import several update blobs, rebuilding the mirror once.
    pub(crate) fn import_batch(&mut self, blobs: &[Vec<u8>]) -> Result<Vec<Change>> {
        self.doc.import_batch(blobs)?;
        let old = std::mem::take(&mut self.nodes);
        self.rebuild();
        Ok(diff(&old, &self.nodes))
    }

    pub(crate) fn import(&mut self, bytes: &[u8]) -> Result<Vec<Change>> {
        self.doc.import(bytes)?;
        let old = std::mem::take(&mut self.nodes);
        self.rebuild();
        Ok(diff(&old, &self.nodes))
    }

    pub(crate) fn commit(&self, message: &str) {
        self.doc.set_next_commit_message(message);
        self.doc.commit();
    }

    // ---- ops ----

    /// Apply one op. All validation happens before anything is written, so a failed
    /// op leaves the sublayer untouched.
    pub(crate) fn apply(&mut self, op: &Op) -> Result<Applied> {
        match op {
            Op::CreatePrim {
                path,
                specifier,
                prim_type,
                index,
            } => {
                let spec = PrimSpec {
                    specifier: *specifier,
                    prim_type: prim_type.clone(),
                    ..Default::default()
                };
                self.insert_tree(path, &spec, &[], *index)?;
                Ok(Applied {
                    inverse: Op::DeletePrim { path: path.clone() },
                    changes: vec![Change::PrimCreated { path: path.clone() }],
                })
            }
            Op::RestorePrim { path, tree, index } => {
                self.insert_tree(path, &tree.spec, &tree.children, *index)?;
                Ok(Applied {
                    inverse: Op::DeletePrim { path: path.clone() },
                    changes: vec![Change::PrimCreated { path: path.clone() }],
                })
            }
            Op::DeletePrim { path } => {
                let id = self.editable_id(path)?;
                let snapshot = self.tree_of(id);
                let index = self.child_index(path);
                let ids = self.subtree_ids(id);
                self.tree.delete(id)?;
                for id in ids {
                    if let Some(node) = self.nodes.remove(&id) {
                        self.by_path.remove(&node.path);
                    }
                }
                Ok(Applied {
                    inverse: Op::RestorePrim {
                        path: path.clone(),
                        tree: snapshot,
                        index,
                    },
                    changes: vec![Change::PrimRemoved { path: path.clone() }],
                })
            }
            Op::MovePrim { from, to, index } => self.move_prim(from, to, *index),
            Op::SetPrimType { path, prim_type } => {
                let id = self.editable_id(path)?;
                let meta = self.tree.get_meta(id)?;
                match prim_type {
                    Some(t) => meta.insert(K_TYPE, t.as_str())?,
                    None => meta.delete(K_TYPE)?,
                }
                let node = self.nodes.get_mut(&id).expect("mirror");
                let old = std::mem::replace(&mut node.spec.prim_type, prim_type.clone());
                Ok(Applied {
                    inverse: Op::SetPrimType {
                        path: path.clone(),
                        prim_type: old,
                    },
                    changes: vec![Change::PrimChanged { path: path.clone() }],
                })
            }
            Op::SetSpecifier { path, specifier } => {
                let id = self.editable_id(path)?;
                self.tree.get_meta(id)?.insert(K_SPEC, specifier.as_str())?;
                let node = self.nodes.get_mut(&id).expect("mirror");
                let old = std::mem::replace(&mut node.spec.specifier, *specifier);
                Ok(Applied {
                    inverse: Op::SetSpecifier {
                        path: path.clone(),
                        specifier: old,
                    },
                    changes: vec![Change::Resynced { path: path.clone() }],
                })
            }
            Op::SetReferences { path, references } => {
                let id = self.editable_id(path)?;
                let meta = self.tree.get_meta(id)?;
                if references.is_empty() {
                    meta.delete(K_REFS)?;
                } else {
                    meta.insert(K_REFS, to_json(references))?;
                }
                let node = self.nodes.get_mut(&id).expect("mirror");
                let old = std::mem::replace(&mut node.spec.references, references.clone());
                Ok(Applied {
                    inverse: Op::SetReferences {
                        path: path.clone(),
                        references: old,
                    },
                    changes: vec![Change::Resynced { path: path.clone() }],
                })
            }
            Op::SetValue { path, attr, value } => {
                let (id, am) = self.attr_map(path, attr)?;
                set_json(&am, K_VALUE, value.as_ref())?;
                let old =
                    self.mirror_attr(id, attr, |a| std::mem::replace(&mut a.value, value.clone()));
                Ok(Applied {
                    inverse: Op::SetValue {
                        path: path.clone(),
                        attr: attr.clone(),
                        value: old,
                    },
                    changes: vec![attr_changed(path, attr, TimeRange::ALL)],
                })
            }
            Op::SetExpression {
                path,
                attr,
                expression,
            } => {
                let (id, am) = self.attr_map(path, attr)?;
                set_json(&am, K_EXPR, expression.as_ref())?;
                let old = self.mirror_attr(id, attr, |a| {
                    std::mem::replace(&mut a.expression, expression.clone())
                });
                Ok(Applied {
                    inverse: Op::SetExpression {
                        path: path.clone(),
                        attr: attr.clone(),
                        expression: old,
                    },
                    changes: vec![attr_changed(path, attr, TimeRange::ALL)],
                })
            }
            Op::SetKeyframe {
                path,
                attr,
                time,
                keyframe,
            } => {
                let (id, am) = self.attr_map(path, attr)?;
                let keys = am.ensure_mergeable_map(K_KEYS)?;
                keys.insert(&time_key(*time), to_json(keyframe))?;
                let (old, range) = self.mirror_attr(id, attr, |a| {
                    let old = a.keyframes.insert(*time, keyframe.clone());
                    (old, a.keyframe_influence(*time))
                });
                let inverse = match old {
                    Some(k) => Op::SetKeyframe {
                        path: path.clone(),
                        attr: attr.clone(),
                        time: *time,
                        keyframe: k,
                    },
                    None => Op::RemoveKeyframe {
                        path: path.clone(),
                        attr: attr.clone(),
                        time: *time,
                    },
                };
                Ok(Applied {
                    inverse,
                    changes: vec![attr_changed(path, attr, range)],
                })
            }
            Op::RemoveKeyframe { path, attr, time } => {
                let missing = || ModelError::KeyframeNotFound {
                    path: path.clone(),
                    attr: attr.clone(),
                    time: *time,
                };
                let current = self.attr(path, attr).ok_or_else(missing)?;
                if !current.keyframes.contains_key(time) {
                    return Err(missing());
                }
                let range = current.keyframe_influence(*time);
                let (id, am) = self.attr_map(path, attr)?;
                am.ensure_mergeable_map(K_KEYS)?.delete(&time_key(*time))?;
                let old = self
                    .mirror_attr(id, attr, |a| a.keyframes.remove(time))
                    .expect("checked above");
                Ok(Applied {
                    inverse: Op::SetKeyframe {
                        path: path.clone(),
                        attr: attr.clone(),
                        time: *time,
                        keyframe: old,
                    },
                    changes: vec![attr_changed(path, attr, range)],
                })
            }
            Op::SetAttr { path, attr, spec } => {
                let (id, am) = self.attr_map(path, attr)?;
                let empty = AttrSpec::default();
                let new = spec.as_ref().unwrap_or(&empty);
                write_attr(&am, new)?;
                let old = self.mirror_attr(id, attr, |a| std::mem::replace(a, new.clone()));
                Ok(Applied {
                    inverse: Op::SetAttr {
                        path: path.clone(),
                        attr: attr.clone(),
                        spec: (!old.is_empty()).then_some(old),
                    },
                    changes: vec![attr_changed(path, attr, TimeRange::ALL)],
                })
            }
        }
    }

    fn move_prim(
        &mut self,
        from: &PrimPath,
        to: &PrimPath,
        index: Option<usize>,
    ) -> Result<Applied> {
        let id = self.editable_id(from)?;
        if to.is_root() {
            return Err(ModelError::RootNotEditable);
        }
        if from.is_ancestor_of(to) {
            return Err(ModelError::CyclicMove {
                from: from.clone(),
                to: to.clone(),
            });
        }
        if from != to && self.by_path.contains_key(to) {
            return Err(ModelError::PrimExists {
                sublayer: self.id.clone(),
                path: to.clone(),
            });
        }
        let old_parent = from.parent().expect("not root");
        let new_parent_path = to.parent().expect("not root");
        let new_parent =
            self.parent_id(&new_parent_path)
                .ok_or_else(|| ModelError::PrimNotFound {
                    sublayer: self.id.clone(),
                    path: new_parent_path.clone(),
                })?;
        let old_index = self.child_index(from).expect("has parent");

        if old_parent != new_parent_path || index.is_some() {
            match index {
                Some(i) => {
                    let mut len = self.child_ids(new_parent).len();
                    if old_parent == new_parent_path {
                        len -= 1;
                    }
                    self.tree.mov_to(id, new_parent, i.min(len))?;
                }
                None => self.tree.mov(id, new_parent)?,
            }
        }
        if from.name() != to.name() {
            self.tree.get_meta(id)?.insert(K_NAME, to.name())?;
        }
        if from != to {
            for sub in self.subtree_ids(id) {
                let node = self.nodes.get_mut(&sub).expect("mirror");
                let new_path = node.path.reparent(from, to).expect("in subtree");
                self.by_path.remove(&node.path);
                self.by_path.insert(new_path.clone(), sub);
                node.path = new_path;
            }
        }
        Ok(Applied {
            inverse: Op::MovePrim {
                from: to.clone(),
                to: from.clone(),
                index: Some(old_index),
            },
            changes: vec![Change::PrimMoved {
                from: from.clone(),
                to: to.clone(),
            }],
        })
    }

    /// Create `path` with `spec` and recursively `children`.
    fn insert_tree(
        &mut self,
        path: &PrimPath,
        spec: &PrimSpec,
        children: &[PrimTree],
        index: Option<usize>,
    ) -> Result<()> {
        if path.is_root() {
            return Err(ModelError::RootNotEditable);
        }
        if self.by_path.contains_key(path) {
            return Err(ModelError::PrimExists {
                sublayer: self.id.clone(),
                path: path.clone(),
            });
        }
        let parent_path = path.parent().expect("not root");
        let parent = self
            .parent_id(&parent_path)
            .ok_or_else(|| ModelError::PrimNotFound {
                sublayer: self.id.clone(),
                path: parent_path,
            })?;
        for (name, a) in &spec.attrs {
            validate_attr_name(name)?;
            let _ = a;
        }
        validate_tree(children)?;

        let id = match index {
            Some(i) => {
                let len = self.child_ids(parent).len();
                self.tree.create_at(parent, i.min(len))?
            }
            None => self.tree.create(parent)?,
        };
        let meta = self.tree.get_meta(id)?;
        write_spec(&meta, path.name(), spec)?;
        self.by_path.insert(path.clone(), id);
        self.nodes.insert(
            id,
            Node {
                path: path.clone(),
                spec: prune(spec.clone()),
            },
        );
        for child in children {
            let child_path = path.child_unchecked(&child.name);
            self.insert_tree(&child_path, &child.spec, &child.children, None)?;
        }
        Ok(())
    }

    // ---- helpers ----

    fn editable_id(&self, path: &PrimPath) -> Result<TreeID> {
        if path.is_root() {
            return Err(ModelError::RootNotEditable);
        }
        self.by_path
            .get(path)
            .copied()
            .ok_or_else(|| ModelError::PrimNotFound {
                sublayer: self.id.clone(),
                path: path.clone(),
            })
    }

    fn parent_id(&self, path: &PrimPath) -> Option<TreeParentId> {
        if path.is_root() {
            Some(TreeParentId::Root)
        } else {
            self.by_path.get(path).map(|id| TreeParentId::Node(*id))
        }
    }

    fn child_ids(&self, parent: TreeParentId) -> Vec<TreeID> {
        self.tree.children(parent).unwrap_or_default()
    }

    fn subtree_ids(&self, id: TreeID) -> Vec<TreeID> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            out.extend(self.child_ids(TreeParentId::Node(out[i])));
            i += 1;
        }
        out
    }

    fn tree_of(&self, id: TreeID) -> PrimTree {
        let node = &self.nodes[&id];
        PrimTree {
            name: node.path.name().to_owned(),
            spec: node.spec.clone(),
            children: self
                .child_ids(TreeParentId::Node(id))
                .into_iter()
                .filter(|c| self.nodes.contains_key(c))
                .map(|c| self.tree_of(c))
                .collect(),
        }
    }

    /// Loro map of an attribute (created on demand) after validating path and name.
    fn attr_map(&self, path: &PrimPath, attr: &str) -> Result<(TreeID, LoroMap)> {
        validate_attr_name(attr)?;
        let id = self.editable_id(path)?;
        let attrs = self.tree.get_meta(id)?.ensure_mergeable_map(K_ATTRS)?;
        Ok((id, attrs.ensure_mergeable_map(attr)?))
    }

    /// Edit the mirrored attribute, dropping it again if it ends up empty.
    fn mirror_attr<R>(&mut self, id: TreeID, attr: &str, f: impl FnOnce(&mut AttrSpec) -> R) -> R {
        let attrs = &mut self.nodes.get_mut(&id).expect("mirror").spec.attrs;
        let a = attrs.entry(attr.to_owned()).or_default();
        let r = f(a);
        if a.is_empty() {
            attrs.remove(attr);
        }
        r
    }

    /// Rebuild the mirror from the Loro document.
    fn rebuild(&mut self) {
        self.by_path.clear();
        self.nodes.clear();
        let mut stack: Vec<(PrimPath, TreeParentId)> = vec![(PrimPath::root(), TreeParentId::Root)];
        while let Some((parent_path, parent)) = stack.pop() {
            for id in self.child_ids(parent) {
                let Ok(meta) = self.tree.get_meta(id) else {
                    continue;
                };
                let (name, spec) = read_spec(&meta);
                let path = match parent_path.child(&name) {
                    Ok(p) if !self.by_path.contains_key(&p) => p,
                    // Concurrent creation of the same name (or a corrupt name): keep a
                    // deterministic unique path so no data is hidden.
                    _ => parent_path.child_unchecked(&format!(
                        "{}_{}_{}",
                        sanitize(&name),
                        id.peer,
                        id.counter
                    )),
                };
                self.by_path.insert(path.clone(), id);
                self.nodes.insert(
                    id,
                    Node {
                        path: path.clone(),
                        spec,
                    },
                );
                stack.push((path, TreeParentId::Node(id)));
            }
        }
    }
}

fn validate_tree(children: &[PrimTree]) -> Result<()> {
    for c in children {
        validate_name(&c.name)?;
        for name in c.spec.attrs.keys() {
            validate_attr_name(name)?;
        }
        validate_tree(&c.children)?;
    }
    Ok(())
}

fn attr_changed(path: &PrimPath, attr: &str, range: TimeRange) -> Change {
    Change::AttrChanged {
        path: path.clone(),
        attr: attr.to_owned(),
        range,
    }
}

fn prune(mut spec: PrimSpec) -> PrimSpec {
    spec.attrs.retain(|_, a| !a.is_empty());
    spec
}

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() || s.starts_with('-') {
        format!("prim{s}")
    } else {
        s
    }
}

fn time_key(t: Time) -> String {
    t.flicks().to_string()
}

fn to_json<T: Serialize + ?Sized>(v: &T) -> String {
    serde_json::to_string(v).expect("model types always serialize")
}

fn from_json<T: DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_str(s).ok()
}

fn set_json<T: Serialize>(map: &LoroMap, key: &str, v: Option<&T>) -> Result<()> {
    match v {
        Some(v) => map.insert(key, to_json(v))?,
        None => map.delete(key)?,
    }
    Ok(())
}

fn write_spec(meta: &LoroMap, name: &str, spec: &PrimSpec) -> Result<()> {
    meta.insert(K_NAME, name)?;
    meta.insert(K_SPEC, spec.specifier.as_str())?;
    if let Some(t) = &spec.prim_type {
        meta.insert(K_TYPE, t.as_str())?;
    }
    if !spec.references.is_empty() {
        meta.insert(K_REFS, to_json(&spec.references))?;
    }
    if spec.attrs.values().any(|a| !a.is_empty()) {
        let attrs = meta.ensure_mergeable_map(K_ATTRS)?;
        for (name, a) in spec.attrs.iter().filter(|(_, a)| !a.is_empty()) {
            write_attr(&attrs.ensure_mergeable_map(name)?, a)?;
        }
    }
    Ok(())
}

fn write_attr(am: &LoroMap, a: &AttrSpec) -> Result<()> {
    set_json(am, K_VALUE, a.value.as_ref())?;
    set_json(am, K_EXPR, a.expression.as_ref())?;
    let keys = am.ensure_mergeable_map(K_KEYS)?;
    keys.clear()?;
    for (t, k) in &a.keyframes {
        keys.insert(&time_key(*t), to_json(k))?;
    }
    Ok(())
}

fn get_str(map: &LoroMap, key: &str) -> Option<String> {
    match map.get(key)? {
        ValueOrContainer::Value(LoroValue::String(s)) => Some(s.to_string()),
        _ => None,
    }
}

fn get_map(map: &LoroMap, key: &str) -> Option<LoroMap> {
    match map.get(key)? {
        ValueOrContainer::Container(Container::Map(m)) => Some(m),
        _ => None,
    }
}

fn read_spec(meta: &LoroMap) -> (String, PrimSpec) {
    let name = get_str(meta, K_NAME).unwrap_or_default();
    let mut spec = PrimSpec {
        specifier: get_str(meta, K_SPEC)
            .and_then(|s| Specifier::parse(&s))
            .unwrap_or_default(),
        prim_type: get_str(meta, K_TYPE).map(|s| PrimType::parse(&s)),
        references: get_str(meta, K_REFS)
            .and_then(|s| from_json::<Vec<Reference>>(&s))
            .unwrap_or_default(),
        ..Default::default()
    };
    if let Some(attrs) = get_map(meta, K_ATTRS) {
        let mut list: Vec<(String, AttrSpec)> = Vec::new();
        attrs.for_each(|name, v| {
            if let ValueOrContainer::Container(Container::Map(am)) = v {
                let a = read_attr(&am);
                if !a.is_empty() {
                    list.push((name.to_owned(), a));
                }
            }
        });
        spec.attrs = list.into_iter().collect();
    }
    (name, spec)
}

fn read_attr(am: &LoroMap) -> AttrSpec {
    let mut a = AttrSpec {
        value: get_str(am, K_VALUE).and_then(|s| from_json::<Value>(&s)),
        expression: get_str(am, K_EXPR).and_then(|s| from_json::<Expression>(&s)),
        keyframes: BTreeMap::new(),
    };
    if let Some(keys) = get_map(am, K_KEYS) {
        keys.for_each(|t, v| {
            if let (Ok(t), ValueOrContainer::Value(LoroValue::String(s))) = (t.parse::<i64>(), v)
                && let Some(k) = from_json::<Keyframe>(&s)
            {
                a.keyframes.insert(Time(t), k);
            }
        });
    }
    a
}

/// Changes between two mirror states (remote import).
fn diff(old: &FxHashMap<TreeID, Node>, new: &FxHashMap<TreeID, Node>) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut moves: Vec<(PrimPath, PrimPath)> = Vec::new();
    for (id, o) in old {
        let Some(n) = new.get(id) else {
            changes.push(Change::PrimRemoved {
                path: o.path.clone(),
            });
            continue;
        };
        if o.path != n.path {
            moves.push((o.path.clone(), n.path.clone()));
        }
        if o.spec.references != n.spec.references || o.spec.specifier != n.spec.specifier {
            changes.push(Change::Resynced {
                path: n.path.clone(),
            });
        } else if o.spec.prim_type != n.spec.prim_type {
            changes.push(Change::PrimChanged {
                path: n.path.clone(),
            });
        }
        for (name, a) in &o.spec.attrs {
            if n.spec.attrs.get(name) != Some(a) {
                changes.push(attr_changed(&n.path, name, TimeRange::ALL));
            }
        }
        for name in n.spec.attrs.keys() {
            if !o.spec.attrs.contains_key(name) {
                changes.push(attr_changed(&n.path, name, TimeRange::ALL));
            }
        }
    }
    for (id, n) in new {
        if !old.contains_key(id) {
            changes.push(Change::PrimCreated {
                path: n.path.clone(),
            });
        }
    }
    // Report only the topmost of nested moves (descendants move implicitly).
    moves.sort_by_key(|(from, _)| from.depth());
    let mut reported: Vec<(PrimPath, PrimPath)> = Vec::new();
    for (from, to) in moves {
        let implied = reported
            .iter()
            .any(|(f, t)| from.reparent(f, t).as_ref() == Some(&to));
        if !implied {
            changes.push(Change::PrimMoved {
                from: from.clone(),
                to: to.clone(),
            });
            reported.push((from, to));
        }
    }
    changes
}
