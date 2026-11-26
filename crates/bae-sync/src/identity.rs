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

    /// Every node, parents before their children and siblings in order.
    pub fn pre_order(&self) -> Vec<TreeID> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack: Vec<TreeID> = self.children(None).iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            out.push(id);
            stack.extend(self.children(Some(id)).iter().rev());
        }
        out
    }

    /// Whether `id` is `ancestor` or one of its descendants.
    pub fn is_within(&self, id: TreeID, ancestor: TreeID) -> bool {
        let mut at = Some(id);
        while let Some(n) = at {
            if n == ancestor {
                return true;
            }
            at = self.get(n).and_then(|i| i.parent);
        }
        false
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

/// `name` with the collision suffix of node `id`: the mirror name a rebuild
/// gives `id` when a sibling has `name` too.
pub(crate) fn suffixed(name: &str, id: TreeID) -> String {
    format!("{name}_{}_{}", id.peer, id.counter)
}

/// Whether raw names `a` (of node `a_id`) and `b` (of `b_id`) name the same
/// prim: equal, or one is the other with the other's collision suffix.
fn same_name(a: &str, a_id: TreeID, b: &str, b_id: TreeID) -> bool {
    a == b || b == suffixed(a, a_id) || a == suffixed(b, b_id)
}

/// How the prims of an older and a newer version of one sublayer correspond.
///
/// 1. The same tree node in both versions is the same prim.
/// 2. Rebuild alias. Undoing a delete (`RestorePrim`) and rolling back re-create
///    prims with new tree nodes; these rules keep showing them as the prim they
///    put back. They only pair a node that is only in the newer version, created
///    under (the counterpart of) parent `p`, with a node that is only in the
///    older version, under `p` and of the same type, and only when each is the
///    other's single candidate. Applied parents first, so a re-created subtree
///    matches as a whole.
///    - (a) Re-created under the deleted node's mirror name: the new node's
///      raw name is the old node's raw name with the old node's own collision
///      suffix ([`suffixed`]), or, comparing backwards, the other way round.
///      bae-model's undo restores mirror names, and a rollback names a prim
///      this way when a remaining sibling has its name.
///    - (b) Otherwise, the same raw name.
pub(crate) struct Matching {
    to_old: FxHashMap<TreeID, TreeID>,
    to_new: FxHashMap<TreeID, TreeID>,
}

/// Parent (in the older version), raw name and type.
type AliasKey<'a> = (Option<TreeID>, &'a str, Option<&'a str>);

fn prim_type<'a>(n: &NodeInfo<'a>) -> Option<&'a str> {
    n.spec.prim_type.as_ref().map(|t| t.as_str())
}

fn alias_key<'a>(n: &NodeInfo<'a>, parent: Option<TreeID>) -> AliasKey<'a> {
    (parent, n.name, prim_type(n))
}

impl Matching {
    pub fn new(old: &NodeIndex<'_>, new: &NodeIndex<'_>) -> Self {
        let mut m = Matching {
            to_old: FxHashMap::default(),
            to_new: FxHashMap::default(),
        };
        for (id, _) in new.iter().filter(|(id, _)| old.contains(*id)) {
            m.pair(id, id);
        }

        // Nodes only in the older version, by alias key, and by the key a
        // re-creation under their mirror name has (rule (a)).
        let mut gone: FxHashMap<AliasKey<'_>, Vec<TreeID>> = FxHashMap::default();
        let mut by_mirror_name = FxHashMap::default();
        for (id, n) in old.iter().filter(|(id, _)| !new.contains(*id)) {
            gone.entry(alias_key(n, n.parent)).or_default().push(id);
            by_mirror_name.insert((n.parent, suffixed(n.name, id), prim_type(n)), id);
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
            // This level's new nodes whose parent has a counterpart, with it.
            let level: Vec<(TreeID, &NodeInfo<'_>, Option<TreeID>)> = level
                .iter()
                .filter_map(|&(_, id)| {
                    let n = new.get(id).expect("listed");
                    let parent = match n.parent {
                        None => None,
                        // A new parent: its children are new too.
                        Some(p) => Some(*m.to_old.get(&p)?),
                    };
                    Some((id, n, parent))
                })
                .collect();

            // (a) Named after each other's node (either way round, so that a
            // difference reads the same backwards).
            let mut claims: FxHashMap<TreeID, Vec<TreeID>> = FxHashMap::default();
            for &(id, n, parent) in &level {
                let ty = prim_type(n);
                let after_old = by_mirror_name.get(&(parent, n.name.to_owned(), ty));
                let own = suffixed(n.name, id);
                let after_new = gone.get(&(parent, own.as_str(), ty));
                let mut found = after_old.into_iter().chain(after_new.into_iter().flatten());
                if let (Some(was), None) = (found.next(), found.next()) {
                    claims.entry(*was).or_default().push(id);
                }
            }
            for (was, ids) in claims {
                if let ([id], None) = (ids.as_slice(), m.new_of(was)) {
                    m.pair(*id, was);
                    let w = old.get(was).expect("listed");
                    if let Some(v) = gone.get_mut(&alias_key(w, w.parent)) {
                        v.retain(|g| *g != was);
                    }
                }
            }

            // (b) The same raw name.
            let mut candidates: FxHashMap<AliasKey<'_>, Vec<TreeID>> = FxHashMap::default();
            for &(id, n, parent) in &level {
                if m.old_of(id).is_none() {
                    candidates.entry(alias_key(n, parent)).or_default().push(id);
                }
            }
            for (k, ids) in candidates {
                if let ([id], Some([was])) = (ids.as_slice(), gone.get(&k).map(Vec::as_slice)) {
                    m.pair(*id, *was);
                }
            }
        }
        m
    }

    fn pair(&mut self, new: TreeID, old: TreeID) {
        self.to_old.insert(new, old);
        self.to_new.insert(old, new);
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
    /// an ancestor moved, or only by a collision suffix (including the one rule (a)
    /// matches on), is not one.
    pub fn moved(&self, old: &NodeIndex<'_>, new: &NodeIndex<'_>, id: TreeID) -> bool {
        let Some(o) = self.old_of(id) else {
            return false;
        };
        let (Some(was), Some(now)) = (old.get(o), new.get(id)) else {
            return false;
        };
        let parent = match now.parent {
            None => Some(None),
            Some(p) => self.old_of(p).map(Some),
        };
        !same_name(was.name, o, now.name, id) || parent != Some(was.parent)
    }
}
