//! Prim identity across versions and clients (`docs/spec/sync.md` §5.3, §5.4).
//!
//! Mirror paths are not identifiers. When several people create the same name
//! concurrently, `Sublayer` keeps the plain name for one prim and gives the others
//! a `_<peer>_<counter>` suffix, but only when it rebuilds its mirror; local
//! structural edits patch the mirror in place. So two clients holding the same
//! version can show one prim at different paths, and one path can name different
//! prims in different versions. Differences and rollback therefore match prims by
//! their Loro tree node ([`TreeID`]), which never changes.

use bae_model::loro::TreeID;
use bae_model::{PrimPath, PrimSpec, Sublayer};
use rustc_hash::FxHashMap;

/// One prim spec of a [`NodeIndex`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct NodeInfo<'a> {
    /// Parent node; `None` for top-level prims.
    pub parent: Option<TreeID>,
    /// Name without the collision suffix ([`raw_name`]).
    pub name: &'a str,
    /// Position among its siblings.
    pub index: usize,
    /// Path in this mirror: for display and for building ops, never for matching.
    pub path: &'a PrimPath,
    pub spec: &'a PrimSpec,
}

/// The prim specs of one sublayer at one version, by tree node. Built only from
/// the public mirror API (`children`, `node_id`, `node_path`, `spec`).
pub(crate) struct NodeIndex<'a> {
    nodes: FxHashMap<TreeID, NodeInfo<'a>>,
    children: FxHashMap<Option<TreeID>, Vec<TreeID>>,
}

impl<'a> NodeIndex<'a> {
    pub fn new(layer: &'a Sublayer) -> Self {
        let mut nodes = FxHashMap::default();
        let mut children: FxHashMap<Option<TreeID>, Vec<TreeID>> = FxHashMap::default();
        let mut stack: Vec<(PrimPath, Option<TreeID>)> = vec![(PrimPath::root(), None)];
        while let Some((path, parent)) = stack.pop() {
            let mut ids = Vec::new();
            for child in layer.children(&path) {
                let Some(id) = layer.node_id(&child) else {
                    continue;
                };
                let (Some(path), Some(spec)) = (layer.node_path(id), layer.spec(&child)) else {
                    continue;
                };
                nodes.insert(
                    id,
                    NodeInfo {
                        parent,
                        name: raw_name(path.name(), id),
                        index: ids.len(),
                        path,
                        spec,
                    },
                );
                ids.push(id);
                stack.push((child, Some(id)));
            }
            if !ids.is_empty() {
                children.insert(parent, ids);
            }
        }
        NodeIndex { nodes, children }
    }

    pub fn get(&self, id: TreeID) -> Option<&NodeInfo<'a>> {
        self.nodes.get(&id)
    }

    pub fn contains(&self, id: TreeID) -> bool {
        self.nodes.contains_key(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (TreeID, &NodeInfo<'a>)> {
        self.nodes.iter().map(|(id, n)| (*id, n))
    }

    /// Ordered children of `parent` (`None`: the top level).
    pub fn children(&self, parent: Option<TreeID>) -> &[TreeID] {
        self.children.get(&parent).map_or(&[], Vec::as_slice)
    }

    /// Nodes sorted by mirror path (parents before their children).
    pub fn by_path(&self) -> Vec<(&'a PrimPath, TreeID)> {
        let mut out: Vec<_> = self.iter().map(|(id, n)| (n.path, id)).collect();
        out.sort();
        out
    }
}

/// The name a prim was given, from its mirror name: the `_<peer>_<counter>`
/// suffix that a mirror rebuild adds after a concurrent creation of the same name
/// is removed, but only when it is this node's own ID (the rule of
/// `Sublayer::rebuild`). Every client therefore gets the same name for the same
/// node, whether or not its mirror has been rebuilt since.
pub(crate) fn raw_name(name: &str, id: TreeID) -> &str {
    name.strip_suffix(&format!("_{}_{}", id.peer, id.counter))
        .filter(|s| !s.is_empty())
        .unwrap_or(name)
}

/// How the prims of an older and a newer version of one sublayer correspond.
///
/// 1. The same tree node in both versions is the same prim.
/// 2. Rebuild alias: a node only in the newer version, created under (the
///    counterpart of) parent `p`, stands for a node only in the older version if
///    that is the only such node under `p` with the same raw name and type and
///    the newer one is the only candidate for it. Undoing a delete
///    (`RestorePrim`) and rolling back re-create prims with new tree nodes; this
///    keeps showing them as the prim they put back. Applied parents first, so a
///    re-created subtree matches as a whole.
pub(crate) struct Matching {
    to_old: FxHashMap<TreeID, TreeID>,
    to_new: FxHashMap<TreeID, TreeID>,
}

/// Parent (in the older version), raw name and type.
type AliasKey<'a> = (Option<TreeID>, &'a str, Option<&'a str>);

fn alias_key<'a>(n: &NodeInfo<'a>, parent: Option<TreeID>) -> AliasKey<'a> {
    (
        parent,
        n.name,
        n.spec.prim_type.as_ref().map(|t| t.as_str()),
    )
}

impl Matching {
    pub fn new(old: &NodeIndex<'_>, new: &NodeIndex<'_>) -> Self {
        let mut m = Matching {
            to_old: FxHashMap::default(),
            to_new: FxHashMap::default(),
        };
        for (id, _) in new.iter().filter(|(id, _)| old.contains(*id)) {
            m.to_old.insert(id, id);
            m.to_new.insert(id, id);
        }

        let mut gone: FxHashMap<AliasKey<'_>, Vec<TreeID>> = FxHashMap::default();
        for (id, n) in old.iter().filter(|(id, _)| !new.contains(*id)) {
            gone.entry(alias_key(n, n.parent)).or_default().push(id);
        }
        if gone.is_empty() {
            return m;
        }
        let mut fresh: Vec<(usize, TreeID)> = new
            .iter()
            .filter(|(id, _)| !old.contains(*id))
            .map(|(id, n)| (n.path.depth(), id))
            .collect();
        fresh.sort();
        for level in fresh.chunk_by(|a, b| a.0 == b.0) {
            let mut candidates: FxHashMap<AliasKey<'_>, Vec<TreeID>> = FxHashMap::default();
            for &(_, id) in level {
                let n = new.get(id).expect("listed");
                let parent = match n.parent {
                    None => None,
                    Some(p) => match m.to_old.get(&p) {
                        Some(op) => Some(*op),
                        // A new parent: its children are new too.
                        None => continue,
                    },
                };
                candidates.entry(alias_key(n, parent)).or_default().push(id);
            }
            for (k, ids) in candidates {
                if let ([id], Some([was])) = (ids.as_slice(), gone.get(&k).map(Vec::as_slice)) {
                    m.to_old.insert(*id, *was);
                    m.to_new.insert(*was, *id);
                }
            }
        }
        m
    }

    /// The older node that newer node `id` is.
    pub fn old_of(&self, id: TreeID) -> Option<TreeID> {
        self.to_old.get(&id).copied()
    }

    /// The newer node that older node `id` is.
    pub fn new_of(&self, id: TreeID) -> Option<TreeID> {
        self.to_new.get(&id).copied()
    }

    /// Whether newer node `id`, which has a counterpart, has a different parent
    /// or raw name than it had: a move or rename. A path that changed only because
    /// an ancestor moved, or only by the collision suffix, is not one.
    pub fn moved(&self, old: &NodeIndex<'_>, new: &NodeIndex<'_>, id: TreeID) -> bool {
        let (Some(was), Some(now)) = (self.old_of(id).and_then(|o| old.get(o)), new.get(id)) else {
            return false;
        };
        let parent = match now.parent {
            None => Some(None),
            Some(p) => self.old_of(p).map(Some),
        };
        was.name != now.name || parent != Some(was.parent)
    }
}
