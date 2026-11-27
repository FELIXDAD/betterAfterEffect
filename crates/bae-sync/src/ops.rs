//! What the ops in a sublayer's Loro history touched: tree nodes (prims) and
//! attribute elements, with the author from each commit message.
//!
//! Follows the storage layout documented on `bae_model::Sublayer`: tree `prims`
//! → node meta map (`name`, `spec`, `type`, `refs`, `attrs`) → `attrs` map → one
//! map per attribute (`value`, `expr`, `keys`) → `keys` map (time in flicks →
//! keyframe). Every map key is last-writer-wins on its own, so an attribute
//! *element* (static value, expression, one keyframe) is the unit two people
//! can overwrite each other in.

use bae_model::loro::{
    ContainerID, ContainerType, ID, IdSpan, Index, JsonMapOp, JsonOpContent, JsonTreeOp, Lamport,
    LoroDoc, LoroValue, PeerID, TreeID, TreeParentId,
};
use bae_model::{AuthorId, Time};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::conflicts::Element;

/// The prim tree of a sublayer's document.
const TREE: &str = "prims";
const K_ATTRS: &str = "attrs";
const K_VALUE: &str = "value";
const K_EXPR: &str = "expr";
const K_KEYS: &str = "keys";
/// Label prefix of the commit `Stage::transact` writes when it rolls back a
/// failed transaction (no event, no edit anyone asked for).
const ROLLBACK: &str = "rollback: ";
/// Parent chains longer than this are treated as unresolvable (corrupt data).
const MAX_DEPTH: usize = 4096;

/// Where a container sits under a prim.
#[derive(Clone, Debug)]
enum Place {
    /// The node's meta map.
    Meta(TreeID),
    /// One attribute's map (`value`, `expr`, `keys`).
    Attr(TreeID, String),
    /// One attribute's keyframes.
    Keys(TreeID, String),
    /// Elsewhere under the node (the `attrs` map itself, future containers).
    Node(TreeID),
}

/// What one op changed.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Target<'a> {
    /// A tree op on this node; `Create` carries the parent (`None`: top level).
    Create(TreeID, Option<TreeID>),
    Move(TreeID),
    Delete(TreeID),
    /// An attribute element; `value` is `None` for a delete.
    Element {
        node: TreeID,
        attr: &'a str,
        element: Element,
        value: Option<&'a LoroValue>,
    },
    /// Anything else under the node: `name`, `spec`, `type`, `refs`, markers.
    Other(TreeID),
}

impl Target<'_> {
    /// The prim (tree node) the op belongs to.
    pub fn node(&self) -> TreeID {
        match *self {
            Target::Create(n, _)
            | Target::Move(n)
            | Target::Delete(n)
            | Target::Element { node: n, .. }
            | Target::Other(n) => n,
        }
    }
}

/// One op, resolved.
#[derive(Debug)]
pub(crate) struct OpInfo<'a> {
    pub id: ID,
    pub lamport: Lamport,
    /// Unix seconds of the commit.
    pub timestamp: i64,
    /// From the commit message (`{"author", "label"}`, written by `Stage::transact`).
    pub author: Option<&'a AuthorId>,
    /// Part of the automatic rollback of a failed transaction.
    pub rollback: bool,
    /// The map the op wrote, and its key (map ops only).
    pub key: Option<(&'a ContainerID, &'a str)>,
    pub target: Target<'a>,
}

/// Walks spans of one document's history. Container positions are cached, so
/// walking many ops on the same prims resolves each container once.
pub(crate) struct Walker<'d> {
    doc: &'d LoroDoc,
    /// Also resolve containers of deleted prims (from the container IDs).
    structural: bool,
    places: FxHashMap<ContainerID, Option<Place>>,
    /// Ops decoded so far (cost accounting).
    pub ops: u64,
}

impl<'d> Walker<'d> {
    /// Resolves the containers of every prim, deleted ones included: the
    /// mergeable maps' IDs name their parent map and key, down to the meta
    /// map of a tree node (`TreeID::associated_meta_container`).
    pub fn new(doc: &'d LoroDoc) -> Self {
        Walker {
            doc,
            structural: true,
            places: FxHashMap::default(),
            ops: 0,
        }
    }

    /// Resolves containers through the current document state only, which
    /// knows no path to the containers of deleted prims (their ops are
    /// skipped). What `history::diff` has always reported authors from.
    pub fn live_only(doc: &'d LoroDoc) -> Self {
        Walker {
            structural: false,
            ..Walker::new(doc)
        }
    }

    /// Call `f` for every op in `span` that belongs to a prim, in order.
    pub fn walk(&mut self, span: IdSpan, mut f: impl FnMut(&OpInfo<'_>)) {
        for change in self.doc.export_json_in_id_span(span) {
            let (author, label) = parse_message(change.msg.as_deref());
            let rollback = label.as_deref().is_some_and(|l| l.starts_with(ROLLBACK));
            for op in &change.ops {
                self.ops += 1;
                let (target, key) = match &op.content {
                    JsonOpContent::Tree(t) => (
                        match t {
                            JsonTreeOp::Create { target, parent, .. } => {
                                Target::Create(*target, *parent)
                            }
                            JsonTreeOp::Move { target, .. } => Target::Move(*target),
                            JsonTreeOp::Delete { target } => Target::Delete(*target),
                        },
                        None,
                    ),
                    content => {
                        let Some(place) = self.place(&op.container) else {
                            continue;
                        };
                        let (k, value) = match content {
                            JsonOpContent::Map(JsonMapOp::Insert { key, value }) => {
                                (Some(key.as_str()), Some(value))
                            }
                            JsonOpContent::Map(JsonMapOp::Delete { key }) => {
                                (Some(key.as_str()), None)
                            }
                            _ => (None, None),
                        };
                        let target = match (place, k) {
                            (Place::Attr(node, attr), Some(K_VALUE)) => Target::Element {
                                node: *node,
                                attr,
                                element: Element::Value,
                                value,
                            },
                            (Place::Attr(node, attr), Some(K_EXPR)) => Target::Element {
                                node: *node,
                                attr,
                                element: Element::Expression,
                                value,
                            },
                            (Place::Keys(node, attr), Some(t)) => match t.parse::<i64>() {
                                Ok(t) => Target::Element {
                                    node: *node,
                                    attr,
                                    element: Element::Keyframe(Time(t)),
                                    value,
                                },
                                Err(_) => Target::Other(*node),
                            },
                            (
                                Place::Meta(node)
                                | Place::Attr(node, _)
                                | Place::Keys(node, _)
                                | Place::Node(node),
                                _,
                            ) => Target::Other(*node),
                        };
                        (target, k.map(|k| (&op.container, k)))
                    }
                };
                let lamport =
                    change.lamport + Lamport::try_from(op.counter - change.id.counter).unwrap_or(0);
                f(&OpInfo {
                    id: ID::new(change.id.peer, op.counter),
                    lamport,
                    timestamp: change.timestamp,
                    author: author.as_ref(),
                    rollback,
                    key,
                    target,
                });
            }
        }
    }

    /// Where `container` sits: the tree node it belongs to, then the map keys
    /// below that node's meta map.
    fn place(&mut self, container: &ContainerID) -> Option<&Place> {
        let (doc, structural) = (self.doc, self.structural);
        self.places
            .entry(container.clone())
            .or_insert_with(|| {
                structural
                    .then(|| place_from_id(doc, container))
                    .flatten()
                    .or_else(|| place_from_path(doc, container))
            })
            .as_ref()
    }
}

fn place_of(node: TreeID, keys: &[&str]) -> Place {
    match keys {
        [] => Place::Meta(node),
        [K_ATTRS, attr] => Place::Attr(node, (*attr).to_owned()),
        [K_ATTRS, attr, K_KEYS] => Place::Keys(node, (*attr).to_owned()),
        _ => Place::Node(node),
    }
}

/// From the container ID alone (no document state, so deleted prims too):
/// mergeable maps down to the meta map of a node of the prim tree.
fn place_from_id(doc: &LoroDoc, container: &ContainerID) -> Option<Place> {
    let mut keys = Vec::new();
    let mut at = container.clone();
    while let Some((parent, key, _)) = at.parse_mergeable() {
        keys.push(key);
        at = parent;
    }
    let ContainerID::Normal {
        peer,
        counter,
        container_type: ContainerType::Map,
    } = at
    else {
        return None;
    };
    let node = TreeID::new(peer, counter);
    if !doc.get_tree(TREE).contains(node) {
        return None;
    }
    keys.reverse();
    let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
    Some(place_of(node, &keys))
}

/// From the container's path in the current state: the first tree node on
/// it, then the map keys below that node.
fn place_from_path(doc: &LoroDoc, container: &ContainerID) -> Option<Place> {
    let path = doc.get_path_to_container(container)?;
    let at = path
        .iter()
        .position(|(_, idx)| matches!(idx, Index::Node(_)))?;
    let Index::Node(node) = path[at].1 else {
        unreachable!("found above");
    };
    let keys: Vec<&str> = path[at + 1..]
        .iter()
        .map(|(_, idx)| match idx {
            Index::Key(k) => k.as_str(),
            _ => "",
        })
        .collect();
    Some(place_of(node, &keys))
}

/// `(author, label)` of a commit message written by `Stage::transact`.
fn parse_message(msg: Option<&str>) -> (Option<AuthorId>, Option<String>) {
    let Some(v) = msg.and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok()) else {
        return (None, None);
    };
    let author = v
        .get("author")
        .and_then(|a| a.as_str())
        .and_then(|s| AuthorId::new(s).ok());
    let label = v.get("label").and_then(|l| l.as_str()).map(str::to_owned);
    (author, label)
}

/// Author and Unix time (seconds) of the commit that holds op `id`.
pub(crate) fn author_of(doc: &LoroDoc, id: ID) -> (Option<AuthorId>, i64) {
    match doc.get_change(id) {
        Some(c) => (parse_message(c.message.as_deref()).0, c.timestamp),
        None => (None, 0),
    }
}

/// Whether op `id` was made without knowing op `known` (`known` is not in its
/// causal past).
pub(crate) fn concurrent_with(doc: &LoroDoc, id: ID, known: ID) -> bool {
    if id.peer == known.peer {
        return false;
    }
    doc.get_change(id)
        .and_then(|c| doc.frontiers_to_vv(&c.deps))
        .is_none_or(|vv| !vv.includes_id(known))
}

/// The op that removed `node` from the tree: the delete of the node itself or
/// of the ancestor it was removed with. `None` if it is not deleted.
pub(crate) fn deletion_of(doc: &LoroDoc, node: TreeID) -> Option<ID> {
    let tree = doc.get_tree(TREE);
    let mut at = node;
    for _ in 0..MAX_DEPTH {
        match tree.parent(at)? {
            TreeParentId::Deleted => return tree.get_last_move_id(&at),
            TreeParentId::Node(p) => at = p,
            TreeParentId::Root | TreeParentId::Unexist => return None,
        }
    }
    None
}

/// The op that last moved `node` (or created or deleted it).
pub(crate) fn last_move_of(doc: &LoroDoc, node: TreeID) -> Option<ID> {
    doc.get_tree(TREE).get_last_move_id(&node)
}

/// Call `f` for `node` and each of its ancestors, nearest first (deleted
/// subtrees included).
pub(crate) fn for_each_ancestor(doc: &LoroDoc, node: TreeID, mut f: impl FnMut(TreeID)) {
    let tree = doc.get_tree(TREE);
    let mut at = node;
    for _ in 0..MAX_DEPTH {
        f(at);
        match tree.parent(at) {
            Some(TreeParentId::Node(p)) => at = p,
            _ => return,
        }
    }
}

/// Peer of the op that holds the current value of `key` in map `container`
/// (a delete counts).
pub(crate) fn last_editor(doc: &LoroDoc, container: &ContainerID, key: &str) -> Option<PeerID> {
    match doc.get_container(container.clone())? {
        bae_model::loro::Container::Map(m) => m.get_last_editor(key),
        _ => None,
    }
}

/// Start of the last change of `peer` before `end`: where a commit that ended
/// at `end` began when nothing else is known.
pub(crate) fn last_change_start(doc: &LoroDoc, peer: PeerID, end: i32) -> i32 {
    if end <= 0 {
        return 0;
    }
    doc.get_change(ID::new(peer, end - 1))
        .map_or(end, |c| c.id.counter)
}

/// Whether `node` or one of its ancestors is in `roots` (deleted subtrees
/// included).
pub(crate) fn in_subtrees(doc: &LoroDoc, node: TreeID, roots: &FxHashSet<TreeID>) -> bool {
    let mut found = false;
    for_each_ancestor(doc, node, |a| found |= roots.contains(&a));
    found
}
