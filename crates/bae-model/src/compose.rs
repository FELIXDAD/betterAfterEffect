//! Composition: layer-stack strength plus references (ARCHITECTURE §4.1).
//!
//! Opinions about a composed prim are ordered strongest first:
//!
//! 1. specs at the same path in the local layer stack, strongest sublayer first;
//! 2. the subtree of every prim referenced by the prim itself;
//! 3. the subtrees referenced by its ancestors, nearest ancestor first.
//!
//! Referenced prims are composed recursively (their own layer stack and references),
//! so references nest. A reference with `project: Some(key)` resolves against a stage
//! attached with [`Stage::attach_project`]. When a prim carries reference opinions in
//! several sublayers, the strongest non-empty list applies (no list merging yet).

use std::collections::{BTreeMap, BTreeSet};

use crate::attr::AttrSpec;
use crate::error::{ModelError, Result};
use crate::event::{Change, EventOrigin, StageEvent};
use crate::ids::SublayerId;
use crate::path::PrimPath;
use crate::prim::{PrimSpec, PrimTree, PrimType, Reference, Specifier};
use crate::stage::Stage;
use crate::sublayer::Sublayer;
use crate::value::Value;

/// Reference chains deeper than this are ignored (guards against runaway nesting).
pub const MAX_REFERENCE_DEPTH: usize = 16;

/// One spec contributing to a composed prim.
#[derive(Clone, Debug)]
pub struct Opinion<'a> {
    /// Key of the attached project the spec lives in; `None` for this project.
    pub project: Option<&'a str>,
    pub layer: &'a Sublayer,
    /// Path of the spec inside its sublayer. Differs from the composed path when the
    /// opinion arrives through a reference.
    pub path: PrimPath,
    pub spec: &'a PrimSpec,
}

impl Opinion<'_> {
    pub fn sublayer(&self) -> &SublayerId {
        self.layer.id()
    }

    /// Whether the opinion comes through a reference rather than the local stack.
    pub fn is_referenced(&self, composed: &PrimPath) -> bool {
        self.project.is_some() || &self.path != composed
    }
}

struct Walk<'a> {
    out: Vec<Opinion<'a>>,
    visited: Vec<(*const Stage, PrimPath)>,
}

impl Stage {
    // ---- attached projects (cross-project references) ----

    /// Make another project available to references with `project: Some(key)`.
    /// The key is the referenced project's path relative to this project.
    pub fn attach_project(&mut self, key: impl Into<String>, stage: Stage) -> StageEvent {
        self.externals.insert(key.into(), stage);
        self.resynced_root()
    }

    pub fn detach_project(&mut self, key: &str) -> Option<(Stage, StageEvent)> {
        let stage = self.externals.remove(key)?;
        Some((stage, self.resynced_root()))
    }

    pub fn attached_project(&self, key: &str) -> Option<&Stage> {
        self.externals.get(key)
    }

    pub fn attached_projects(&self) -> impl Iterator<Item = &str> {
        self.externals.keys().map(String::as_str)
    }

    fn resynced_root(&mut self) -> StageEvent {
        self.publish(StageEvent {
            origin: EventOrigin::Stack,
            author: None,
            sublayers: Vec::new(),
            changes: vec![Change::Resynced {
                path: PrimPath::root(),
            }],
        })
    }

    // ---- composed queries ----

    /// All opinions about `path`, strongest first.
    pub fn opinions(&self, path: &PrimPath) -> Vec<Opinion<'_>> {
        let mut walk = Walk {
            out: Vec::new(),
            visited: Vec::new(),
        };
        self.collect(path, None, None, 0, &mut walk);
        walk.out
    }

    fn collect<'a>(
        &'a self,
        path: &PrimPath,
        project: Option<&'a str>,
        stop_at: Option<&PrimPath>,
        depth: usize,
        walk: &mut Walk<'a>,
    ) {
        if depth > MAX_REFERENCE_DEPTH {
            return;
        }
        let key = (self as *const Stage, path.clone());
        if walk.visited.contains(&key) {
            return;
        }
        walk.visited.push(key);

        for id in &self.stack {
            let layer = &self.sublayers[id];
            if let Some(spec) = layer.spec(path) {
                walk.out.push(Opinion {
                    project,
                    layer,
                    path: path.clone(),
                    spec,
                });
            }
        }
        // References on the prim itself, then on its ancestors (nearest first), up to
        // the root of the referenced subtree we are composing.
        let mut anc = Some(path.clone());
        while let Some(a) = anc {
            if a.is_root() {
                break;
            }
            if let Some(refs) = self.local_references(&a) {
                for r in refs {
                    let (stage, proj) = match &r.project {
                        None => (self, project),
                        Some(key) => match self.externals.get(key) {
                            Some(s) => (s, Some(key.as_str())),
                            None => continue,
                        },
                    };
                    let mapped = path.reparent(&a, &r.prim).expect("a is a prefix of path");
                    stage.collect(&mapped, proj, Some(&r.prim), depth + 1, walk);
                }
            }
            if stop_at == Some(&a) {
                break;
            }
            anc = a.parent();
        }
    }

    /// Strongest non-empty reference list authored at `path` in the local stack.
    fn local_references(&self, path: &PrimPath) -> Option<&[Reference]> {
        self.stack.iter().find_map(|id| {
            self.sublayers[id]
                .spec(path)
                .filter(|s| !s.references.is_empty())
                .map(|s| s.references.as_slice())
        })
    }

    /// Whether `path` is a prim on the composed stage (the pseudo-root always is):
    /// its parent exists and at least one opinion is a `def`.
    pub fn has_prim(&self, path: &PrimPath) -> bool {
        if path.is_root() {
            return true;
        }
        path.parent().is_some_and(|p| self.has_prim(&p)) && self.is_defined_here(path)
    }

    fn is_defined_here(&self, path: &PrimPath) -> bool {
        self.opinions(path)
            .iter()
            .any(|o| o.spec.specifier == Specifier::Def)
    }

    /// Whether the local layer stack (not a reference) defines `path`. Only such
    /// prims can be deleted, moved or renamed.
    pub fn has_local_def(&self, path: &PrimPath) -> bool {
        self.stack.iter().any(|id| {
            self.sublayers[id]
                .spec(path)
                .is_some_and(|s| s.specifier == Specifier::Def)
        })
    }

    pub fn prim<'a>(&'a self, path: &PrimPath) -> Option<PrimView<'a>> {
        if !self.has_prim(path) {
            return None;
        }
        Some(PrimView {
            stage: self,
            path: path.clone(),
            opinions: self.opinions(path),
        })
    }

    /// The prim or [`ModelError::NoSuchPrim`].
    pub fn require_prim<'a>(&'a self, path: &PrimPath) -> Result<PrimView<'a>> {
        self.prim(path)
            .ok_or_else(|| ModelError::NoSuchPrim(path.clone()))
    }

    /// Composed child prims of `path`, in order: children from the weakest source
    /// first (referenced content, then weak sublayers), children added by stronger
    /// sources after them.
    pub fn children(&self, path: &PrimPath) -> Vec<PrimPath> {
        let mut out: Vec<PrimPath> = Vec::new();
        let opinions = self.opinions(path);
        let mut sources = opinions.len();
        for o in opinions.iter().rev() {
            for c in o.layer.children(&o.path) {
                let composed = path.child_unchecked(c.name());
                out.push(composed);
            }
        }
        // Root-level children have no opinions to walk; take them from the stack.
        if path.is_root() {
            sources += self.stack.len();
            for id in self.stack.iter().rev() {
                out.extend(self.sublayers[id].children(path));
            }
        }
        if sources > 1 {
            let mut seen = BTreeSet::new();
            out.retain(|c| seen.insert(c.clone()));
        }
        out.retain(|c| self.is_defined_here(c));
        out
    }

    /// All prims, depth-first pre-order (excluding the pseudo-root).
    pub fn traverse(&self) -> Vec<PrimPath> {
        let mut out = Vec::new();
        let mut stack = self.children(&PrimPath::root());
        stack.reverse();
        while let Some(p) = stack.pop() {
            let mut kids = self.children(&p);
            kids.reverse();
            stack.extend(kids);
            out.push(p);
        }
        out
    }

    /// Winning opinion about an attribute.
    pub fn attr_opinion(&self, path: &PrimPath, attr: &str) -> Option<(Opinion<'_>, &AttrSpec)> {
        self.opinions(path).into_iter().find_map(|o| {
            let a = o.spec.attrs.get(attr)?;
            Some((o, a))
        })
    }

    /// Local references whose project is not attached or whose target prim does not
    /// exist (shown as "missing" in the UI).
    pub fn unresolved_references(&self) -> Vec<(PrimPath, Reference)> {
        let mut out = Vec::new();
        for id in &self.stack {
            let layer = &self.sublayers[id];
            for path in layer.paths() {
                for r in &layer.spec(path).expect("listed").references {
                    let ok = match &r.project {
                        None => self.has_prim(&r.prim),
                        Some(key) => self.externals.get(key).is_some_and(|s| s.has_prim(&r.prim)),
                    };
                    if !ok && !out.iter().any(|(p, x)| p == path && x == r) {
                        out.push((path.clone(), r.clone()));
                    }
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// The composed stage as plain `def` trees: one spec per prim with the winning
    /// opinion for every attribute, references resolved (and dropped). Used for
    /// export to formats without layers (AE, Lottie) and for agent summaries.
    pub fn flatten(&self) -> Vec<PrimTree> {
        self.children(&PrimPath::root())
            .iter()
            .map(|c| self.flatten_prim(c))
            .collect()
    }

    fn flatten_prim(&self, path: &PrimPath) -> PrimTree {
        let opinions = self.opinions(path);
        let mut attrs: BTreeMap<String, AttrSpec> = BTreeMap::new();
        for o in &opinions {
            for (name, a) in &o.spec.attrs {
                attrs.entry(name.clone()).or_insert_with(|| a.clone());
            }
        }
        PrimTree {
            name: path.name().to_owned(),
            spec: PrimSpec {
                specifier: Specifier::Def,
                prim_type: opinions.iter().find_map(|o| o.spec.prim_type.clone()),
                attrs,
                references: Vec::new(),
            },
            children: self
                .children(path)
                .iter()
                .map(|c| self.flatten_prim(c))
                .collect(),
        }
    }

    // ---- change propagation ----

    /// Add the changes that referencing prims see (same-project references, nested).
    pub(crate) fn expand_through_references(&mut self, event: &mut StageEvent) {
        let index = self.ref_index.get_or_insert_with(|| {
            let mut index = Vec::new();
            for layer in self.sublayers.values() {
                for path in layer.paths() {
                    for r in &layer.spec(path).expect("listed").references {
                        if r.project.is_none() && !index.contains(&(path.clone(), r.prim.clone())) {
                            index.push((path.clone(), r.prim.clone()));
                        }
                    }
                }
            }
            index
        });
        if index.is_empty() {
            return;
        }
        let mut frontier = event.changes.clone();
        for _ in 0..MAX_REFERENCE_DEPTH {
            let mut next = Vec::new();
            for c in &frontier {
                for (referencing, target) in index.iter() {
                    if let Some(m) = map_change(c, target, referencing)
                        && !event.changes.contains(&m)
                        && !next.contains(&m)
                    {
                        next.push(m);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            event.changes.extend(next.iter().cloned());
            frontier = next;
        }
    }
}

/// How a change under `target` looks from the prim that references it.
fn map_change(c: &Change, target: &PrimPath, referencing: &PrimPath) -> Option<Change> {
    let map = |p: &PrimPath| p.reparent(target, referencing);
    let whole = || Change::Resynced {
        path: referencing.clone(),
    };
    match c {
        Change::PrimMoved { from, to } => match (map(from), map(to)) {
            (Some(from), Some(to)) => Some(Change::PrimMoved { from, to }),
            (None, None) if !from.is_prefix_of(target) && !to.is_prefix_of(target) => None,
            _ => Some(whole()),
        },
        Change::AttrChanged { path, attr, range } => map(path).map(|path| Change::AttrChanged {
            path,
            attr: attr.clone(),
            range: *range,
        }),
        Change::PrimCreated { path }
        | Change::PrimRemoved { path }
        | Change::PrimChanged { path }
        | Change::Resynced { path } => {
            // The target itself (or an ancestor) appeared, vanished or changed shape:
            // everything the referencing prim gets from it may differ.
            if path.is_prefix_of(target) {
                return Some(whole());
            }
            let mapped = map(path)?;
            Some(match c {
                Change::PrimCreated { .. } => Change::PrimCreated { path: mapped },
                Change::PrimRemoved { .. } => Change::PrimRemoved { path: mapped },
                Change::PrimChanged { .. } => Change::PrimChanged { path: mapped },
                _ => Change::Resynced { path: mapped },
            })
        }
    }
}

/// A composed prim.
#[derive(Clone)]
pub struct PrimView<'a> {
    stage: &'a Stage,
    path: PrimPath,
    opinions: Vec<Opinion<'a>>,
}

impl std::fmt::Debug for PrimView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrimView")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl<'a> PrimView<'a> {
    pub fn path(&self) -> &PrimPath {
        &self.path
    }

    pub fn name(&self) -> &str {
        self.path.name()
    }

    /// All opinions, strongest first.
    pub fn opinions(&self) -> &[Opinion<'a>] {
        &self.opinions
    }

    /// Strongest type opinion.
    pub fn prim_type(&self) -> Option<&'a PrimType> {
        self.opinions.iter().find_map(|o| o.spec.prim_type.as_ref())
    }

    pub fn is_a(&self, t: &PrimType) -> bool {
        self.prim_type() == Some(t)
    }

    /// Winning opinion about `attr`.
    pub fn attr(&self, attr: &str) -> Option<&'a AttrSpec> {
        self.opinions.iter().find_map(|o| o.spec.attrs.get(attr))
    }

    /// The opinion that wins for `attr` ("这个值来自哪一层"), including whether it
    /// arrives through a reference.
    pub fn attr_origin(&self, attr: &str) -> Option<&Opinion<'a>> {
        self.opinions
            .iter()
            .find(|o| o.spec.attrs.contains_key(attr))
    }

    /// Sublayer of the winning opinion about `attr`.
    pub fn attr_source(&self, attr: &str) -> Option<&'a SublayerId> {
        self.opinions
            .iter()
            .find(|o| o.spec.attrs.contains_key(attr))
            .map(|o| o.layer.id())
    }

    /// Static value of `attr` (ignores animation; `bae-anim` evaluates keyframes).
    pub fn value(&self, attr: &str) -> Option<&'a Value> {
        self.attr(attr)?.value.as_ref()
    }

    /// Names of all attributes with an opinion anywhere (strongest source first).
    pub fn attr_names(&self) -> Vec<&'a str> {
        let mut out: Vec<&'a str> = Vec::new();
        for o in &self.opinions {
            for name in o.spec.attrs.keys() {
                if !out.contains(&name.as_str()) {
                    out.push(name);
                }
            }
        }
        out
    }

    /// Strongest non-empty references opinion authored on this prim.
    pub fn references(&self) -> &'a [Reference] {
        self.opinions
            .iter()
            .find_map(|o| (!o.spec.references.is_empty()).then_some(o.spec.references.as_slice()))
            .unwrap_or(&[])
    }

    pub fn children(&self) -> Vec<PrimPath> {
        self.stage.children(&self.path)
    }

    pub fn parent(&self) -> Option<PrimView<'a>> {
        self.stage.prim(&self.path.parent()?)
    }
}
