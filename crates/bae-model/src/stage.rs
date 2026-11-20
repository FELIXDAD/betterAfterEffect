//! The stage: sublayers composed by strength, transactions, undo and events.

use rustc_hash::FxHashMap;

use crate::attr::AttrSpec;
use crate::error::{ModelError, Result};
use crate::event::{Change, EventOrigin, StageEvent};
use crate::ids::{AuthorId, SublayerId};
use crate::op::{Edit, Op};
use crate::path::PrimPath;
use crate::prim::{PrimSpec, PrimType, Reference, Specifier};
use crate::sublayer::Sublayer;
use crate::value::Value;

/// Maximum undo steps kept per author.
const UNDO_LIMIT: usize = 1000;

/// Handle returned by [`Stage::subscribe`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

type Subscriber = Box<dyn FnMut(&StageEvent) + Send>;

#[derive(Debug)]
struct UndoEntry {
    label: String,
    /// Edits that revert the step, in the order they must be applied.
    edits: Vec<Edit>,
}

/// An open project: a stack of sublayers composed strongest-first.
///
/// For every prim path, each sublayer may hold a spec; for every attribute the
/// strongest sublayer with an opinion wins. A prim exists on the stage when at least
/// one sublayer has a `def` spec for it and its parent exists.
pub struct Stage {
    sublayers: FxHashMap<SublayerId, Sublayer>,
    /// Strongest first.
    stack: Vec<SublayerId>,
    edit_target: SublayerId,
    undo: FxHashMap<AuthorId, Vec<UndoEntry>>,
    redo: FxHashMap<AuthorId, Vec<UndoEntry>>,
    subscribers: Vec<(SubscriptionId, Subscriber)>,
    next_subscription: u64,
}

impl std::fmt::Debug for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stage")
            .field("stack", &self.stack)
            .field("edit_target", &self.edit_target)
            .finish_non_exhaustive()
    }
}

impl Default for Stage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage {
    /// A stage with one empty `root` sublayer as the edit target.
    pub fn new() -> Self {
        Self::from_sublayers(vec![Sublayer::new(SublayerId::root())]).expect("one sublayer")
    }

    /// A stage over loaded sublayers, strongest first. The weakest one becomes the
    /// edit target.
    pub fn from_sublayers(sublayers: Vec<Sublayer>) -> Result<Self> {
        let stack: Vec<SublayerId> = sublayers.iter().map(|s| s.id().clone()).collect();
        let edit_target = stack
            .last()
            .cloned()
            .ok_or(ModelError::NotInStack(SublayerId::root()))?;
        let mut map = FxHashMap::default();
        for s in sublayers {
            let id = s.id().clone();
            if map.insert(id.clone(), s).is_some() {
                return Err(ModelError::SublayerExists(id));
            }
        }
        Ok(Stage {
            sublayers: map,
            stack,
            edit_target,
            undo: FxHashMap::default(),
            redo: FxHashMap::default(),
            subscribers: Vec::new(),
            next_subscription: 0,
        })
    }

    // ---- layer stack ----

    /// Sublayer IDs, strongest first.
    pub fn layer_stack(&self) -> &[SublayerId] {
        &self.stack
    }

    pub fn sublayer(&self, id: &SublayerId) -> Option<&Sublayer> {
        self.sublayers.get(id)
    }

    /// Sublayers, strongest first.
    pub fn sublayers(&self) -> impl Iterator<Item = &Sublayer> {
        self.stack.iter().map(|id| &self.sublayers[id])
    }

    pub fn edit_target(&self) -> &SublayerId {
        &self.edit_target
    }

    pub fn set_edit_target(&mut self, id: &SublayerId) -> Result<()> {
        self.check_in_stack(id)?;
        self.edit_target = id.clone();
        Ok(())
    }

    /// Add an empty sublayer at `index` in the stack (0 = strongest).
    pub fn create_sublayer(&mut self, id: SublayerId, index: usize) -> Result<StageEvent> {
        self.insert_sublayer(Sublayer::new(id), index)
    }

    /// Add a loaded sublayer at `index` in the stack (0 = strongest).
    pub fn insert_sublayer(&mut self, sublayer: Sublayer, index: usize) -> Result<StageEvent> {
        let id = sublayer.id().clone();
        if self.sublayers.contains_key(&id) {
            return Err(ModelError::SublayerExists(id));
        }
        self.sublayers.insert(id.clone(), sublayer);
        self.stack.insert(index.min(self.stack.len()), id.clone());
        Ok(self.stack_event(id))
    }

    /// Remove a sublayer from the stack. The last remaining sublayer cannot be removed.
    pub fn remove_sublayer(&mut self, id: &SublayerId) -> Result<(Sublayer, StageEvent)> {
        self.check_in_stack(id)?;
        if self.stack.len() == 1 {
            return Err(ModelError::NotInStack(id.clone()));
        }
        self.stack.retain(|s| s != id);
        if &self.edit_target == id {
            self.edit_target = self.stack.last().expect("non-empty").clone();
        }
        let layer = self.sublayers.remove(id).expect("in stack");
        Ok((layer, self.stack_event(id.clone())))
    }

    /// Move a sublayer to `index` in the stack (0 = strongest).
    pub fn move_sublayer(&mut self, id: &SublayerId, index: usize) -> Result<StageEvent> {
        self.check_in_stack(id)?;
        self.stack.retain(|s| s != id);
        self.stack.insert(index.min(self.stack.len()), id.clone());
        Ok(self.stack_event(id.clone()))
    }

    fn check_in_stack(&self, id: &SublayerId) -> Result<()> {
        if self.sublayers.contains_key(id) {
            Ok(())
        } else {
            Err(ModelError::UnknownSublayer(id.clone()))
        }
    }

    fn stack_event(&mut self, id: SublayerId) -> StageEvent {
        let event = StageEvent {
            origin: EventOrigin::Stack,
            author: None,
            sublayers: vec![id],
            changes: vec![Change::Resynced {
                path: PrimPath::root(),
            }],
        };
        self.emit(&event);
        event
    }

    // ---- composed queries ----

    /// Specs for `path`, strongest first.
    pub fn specs<'a>(
        &'a self,
        path: &'a PrimPath,
    ) -> impl Iterator<Item = (&'a SublayerId, &'a PrimSpec)> + 'a {
        self.stack
            .iter()
            .filter_map(move |id| self.sublayers[id].spec(path).map(|s| (id, s)))
    }

    /// Whether `path` is a prim on the composed stage (the pseudo-root always is).
    pub fn has_prim(&self, path: &PrimPath) -> bool {
        if path.is_root() {
            return true;
        }
        self.specs(path).any(|(_, s)| s.specifier == Specifier::Def)
            && path.parent().is_some_and(|p| self.has_prim(&p))
    }

    pub fn prim<'a>(&'a self, path: &PrimPath) -> Option<PrimView<'a>> {
        self.has_prim(path).then(|| PrimView {
            stage: self,
            path: path.clone(),
        })
    }

    /// The prim or [`ModelError::NoSuchPrim`].
    pub fn require_prim<'a>(&'a self, path: &PrimPath) -> Result<PrimView<'a>> {
        self.prim(path)
            .ok_or_else(|| ModelError::NoSuchPrim(path.clone()))
    }

    /// Composed child prims of `path`, in order. Children keep the order of the
    /// weakest sublayer that has them; children added by stronger sublayers follow.
    pub fn children(&self, path: &PrimPath) -> Vec<PrimPath> {
        let mut out: Vec<PrimPath> = Vec::new();
        for id in self.stack.iter().rev() {
            for c in self.sublayers[id].children(path) {
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        out.retain(|c| self.specs(c).any(|(_, s)| s.specifier == Specifier::Def));
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

    /// Strongest opinion about an attribute and the sublayer it comes from.
    pub fn attr_opinion(&self, path: &PrimPath, attr: &str) -> Option<(&SublayerId, &AttrSpec)> {
        self.specs_static(path)
            .find_map(|(id, s)| s.attrs.get(attr).map(|a| (id, a)))
    }

    // ---- events ----

    /// Call `f` after every committed change. Callbacks run synchronously on the
    /// thread that changed the stage; keep them cheap (queue work elsewhere).
    pub fn subscribe(&mut self, f: impl FnMut(&StageEvent) + Send + 'static) -> SubscriptionId {
        let id = SubscriptionId(self.next_subscription);
        self.next_subscription += 1;
        self.subscribers.push((id, Box::new(f)));
        id
    }

    pub fn unsubscribe(&mut self, id: SubscriptionId) -> bool {
        let before = self.subscribers.len();
        self.subscribers.retain(|(s, _)| *s != id);
        before != self.subscribers.len()
    }

    fn emit(&mut self, event: &StageEvent) {
        for (_, f) in &mut self.subscribers {
            f(event);
        }
    }

    // ---- editing ----

    /// Run `f` as one undoable step by `author`. Edits go to the edit target unless
    /// the transaction targets another sublayer. If `f` fails, everything it applied
    /// is reverted and no event is emitted.
    pub fn transact<R, E>(
        &mut self,
        author: &AuthorId,
        label: &str,
        f: impl FnOnce(&mut Transaction<'_>) -> std::result::Result<R, E>,
    ) -> std::result::Result<(R, StageEvent), E>
    where
        E: From<ModelError>,
    {
        let target = self.edit_target.clone();
        let mut tx = Transaction {
            stage: self,
            author: author.clone(),
            target,
            inverse: Vec::new(),
            changes: Vec::new(),
            touched: Vec::new(),
        };
        match f(&mut tx) {
            Ok(r) => {
                let Transaction {
                    inverse,
                    changes,
                    touched,
                    ..
                } = tx;
                let event =
                    self.finish(author, label, inverse, changes, touched, EventOrigin::Local);
                Ok((r, event))
            }
            Err(e) => {
                let Transaction {
                    inverse, touched, ..
                } = tx;
                for edit in inverse.into_iter().rev() {
                    if let Some(layer) = self.sublayers.get_mut(&edit.sublayer) {
                        let _ = layer.apply(&edit.op);
                    }
                }
                self.commit_layers(&touched, author, &format!("rollback: {label}"));
                Err(e)
            }
        }
    }

    /// Apply raw edits as one undoable step (agents, scripts, importers).
    pub fn apply_edits(
        &mut self,
        author: &AuthorId,
        label: &str,
        edits: Vec<Edit>,
    ) -> Result<StageEvent> {
        self.transact(author, label, |tx| {
            for e in edits {
                tx.apply_to(&e.sublayer, e.op)?;
            }
            Ok::<_, ModelError>(())
        })
        .map(|(_, ev)| ev)
    }

    fn finish(
        &mut self,
        author: &AuthorId,
        label: &str,
        inverse: Vec<Edit>,
        changes: Vec<Change>,
        touched: Vec<SublayerId>,
        origin: EventOrigin,
    ) -> StageEvent {
        self.commit_layers(&touched, author, label);
        if !inverse.is_empty() {
            let entry = UndoEntry {
                label: label.to_owned(),
                edits: inverse.into_iter().rev().collect(),
            };
            let (push, clear) = match origin {
                EventOrigin::Undo => (&mut self.redo, None),
                EventOrigin::Redo => (&mut self.undo, None),
                _ => (&mut self.undo, Some(&mut self.redo)),
            };
            let stack = push.entry(author.clone()).or_default();
            stack.push(entry);
            if stack.len() > UNDO_LIMIT {
                stack.remove(0);
            }
            if let Some(redo) = clear {
                redo.remove(author);
            }
        }
        let event = StageEvent {
            origin,
            author: Some(author.clone()),
            sublayers: touched,
            changes,
        };
        if !event.is_empty() {
            self.emit(&event);
        }
        event
    }

    fn commit_layers(&self, touched: &[SublayerId], author: &AuthorId, label: &str) {
        if touched.is_empty() {
            return;
        }
        let message = serde_json::json!({ "author": author, "label": label }).to_string();
        for id in touched {
            if let Some(layer) = self.sublayers.get(id) {
                layer.commit(&message);
            }
        }
    }

    /// Undo `author`'s most recent step (other authors' changes are untouched).
    /// Edits that no longer apply (e.g. a collaborator deleted the prim) are skipped.
    pub fn undo(&mut self, author: &AuthorId) -> Option<StageEvent> {
        self.replay(author, true)
    }

    pub fn redo(&mut self, author: &AuthorId) -> Option<StageEvent> {
        self.replay(author, false)
    }

    pub fn can_undo(&self, author: &AuthorId) -> bool {
        self.undo.get(author).is_some_and(|s| !s.is_empty())
    }

    pub fn can_redo(&self, author: &AuthorId) -> bool {
        self.redo.get(author).is_some_and(|s| !s.is_empty())
    }

    /// Label of the step [`Stage::undo`] would revert.
    pub fn undo_label(&self, author: &AuthorId) -> Option<&str> {
        self.undo.get(author)?.last().map(|e| e.label.as_str())
    }

    pub fn redo_label(&self, author: &AuthorId) -> Option<&str> {
        self.redo.get(author)?.last().map(|e| e.label.as_str())
    }

    fn replay(&mut self, author: &AuthorId, undo: bool) -> Option<StageEvent> {
        let source = if undo { &mut self.undo } else { &mut self.redo };
        let entry = source.get_mut(author)?.pop()?;
        let mut inverse = Vec::new();
        let mut changes = Vec::new();
        let mut touched: Vec<SublayerId> = Vec::new();
        for edit in entry.edits {
            let Some(layer) = self.sublayers.get_mut(&edit.sublayer) else {
                continue;
            };
            if let Ok(applied) = layer.apply(&edit.op) {
                inverse.push(Edit::new(edit.sublayer.clone(), applied.inverse));
                changes.extend(applied.changes);
                if !touched.contains(&edit.sublayer) {
                    touched.push(edit.sublayer);
                }
            }
        }
        let origin = if undo {
            EventOrigin::Undo
        } else {
            EventOrigin::Redo
        };
        Some(self.finish(author, &entry.label, inverse, changes, touched, origin))
    }

    // ---- sync ----

    /// Import updates (or a snapshot) for one sublayer from a collaborator.
    pub fn import_updates(&mut self, sublayer: &SublayerId, bytes: &[u8]) -> Result<StageEvent> {
        let layer = self
            .sublayers
            .get_mut(sublayer)
            .ok_or_else(|| ModelError::UnknownSublayer(sublayer.clone()))?;
        let changes = layer.import(bytes)?;
        let event = StageEvent {
            origin: EventOrigin::Remote,
            author: None,
            sublayers: vec![sublayer.clone()],
            changes,
        };
        if !event.is_empty() {
            self.emit(&event);
        }
        Ok(event)
    }
}

/// An open, undoable step. Created by [`Stage::transact`].
pub struct Transaction<'s> {
    stage: &'s mut Stage,
    author: AuthorId,
    target: SublayerId,
    inverse: Vec<Edit>,
    changes: Vec<Change>,
    touched: Vec<SublayerId>,
}

impl Transaction<'_> {
    /// Read access to the stage, including this transaction's edits so far.
    pub fn stage(&self) -> &Stage {
        self.stage
    }

    pub fn author(&self) -> &AuthorId {
        &self.author
    }

    /// Sublayer that [`Transaction::apply`] writes to.
    pub fn target(&self) -> &SublayerId {
        &self.target
    }

    /// Write subsequent [`Transaction::apply`] calls to another sublayer.
    pub fn set_target(&mut self, id: &SublayerId) -> Result<()> {
        self.stage.check_in_stack(id)?;
        self.target = id.clone();
        Ok(())
    }

    /// Apply an op to the target sublayer.
    pub fn apply(&mut self, op: Op) -> Result<()> {
        let target = self.target.clone();
        self.apply_to(&target, op)
    }

    /// Apply an op to a specific sublayer.
    pub fn apply_to(&mut self, sublayer: &SublayerId, op: Op) -> Result<()> {
        let layer = self
            .stage
            .sublayers
            .get_mut(sublayer)
            .ok_or_else(|| ModelError::UnknownSublayer(sublayer.clone()))?;
        let applied = layer.apply(&op)?;
        self.inverse
            .push(Edit::new(sublayer.clone(), applied.inverse));
        self.changes.extend(applied.changes);
        if !self.touched.contains(sublayer) {
            self.touched.push(sublayer.clone());
        }
        Ok(())
    }

    /// Make sure `sublayer` has a spec at `path`, creating `over` specs for it and any
    /// missing ancestors (USD `CreatePrimInLayer`).
    pub fn ensure_spec_in(&mut self, sublayer: &SublayerId, path: &PrimPath) -> Result<()> {
        if path.is_root() {
            return Ok(());
        }
        let layer = self
            .stage
            .sublayer(sublayer)
            .ok_or_else(|| ModelError::UnknownSublayer(sublayer.clone()))?;
        if layer.has_spec(path) {
            return Ok(());
        }
        let mut missing: Vec<PrimPath> = std::iter::once(path.clone())
            .chain(path.ancestors().filter(|p| !p.is_root()))
            .take_while(|p| !layer.has_spec(p))
            .collect();
        missing.reverse();
        for p in missing {
            self.apply_to(
                sublayer,
                Op::CreatePrim {
                    path: p,
                    specifier: Specifier::Over,
                    prim_type: None,
                    index: None,
                },
            )?;
        }
        Ok(())
    }

    /// [`Transaction::ensure_spec_in`] for the target sublayer.
    pub fn ensure_spec(&mut self, path: &PrimPath) -> Result<()> {
        let target = self.target.clone();
        self.ensure_spec_in(&target, path)
    }

    /// Add a sublayer to the stack. Not undoable (layer stack edits are not part of
    /// any sublayer); the stack change is emitted as a separate event.
    pub fn create_sublayer(&mut self, id: SublayerId, index: usize) -> Result<()> {
        self.stage.create_sublayer(id, index).map(|_| ())
    }

    /// Change the stage's edit target (session state, not undoable).
    pub fn set_edit_target(&mut self, id: &SublayerId) -> Result<()> {
        self.stage.set_edit_target(id)?;
        self.target = id.clone();
        Ok(())
    }
}

/// A composed prim.
#[derive(Clone)]
pub struct PrimView<'a> {
    stage: &'a Stage,
    path: PrimPath,
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

    /// Strongest type opinion.
    pub fn prim_type(&self) -> Option<&'a PrimType> {
        self.stage
            .specs_static(&self.path)
            .find_map(|(_, s)| s.prim_type.as_ref())
    }

    pub fn is_a(&self, t: &PrimType) -> bool {
        self.prim_type() == Some(t)
    }

    /// Strongest opinion about `attr`.
    pub fn attr(&self, attr: &str) -> Option<&'a AttrSpec> {
        self.stage
            .specs_static(&self.path)
            .find_map(|(_, s)| s.attrs.get(attr))
    }

    /// The sublayer whose opinion about `attr` wins ("这个值来自哪一层").
    pub fn attr_source(&self, attr: &str) -> Option<&'a SublayerId> {
        self.stage
            .specs_static(&self.path)
            .find_map(|(id, s)| s.attrs.contains_key(attr).then_some(id))
    }

    /// Static value of `attr` (ignores animation; `bae-anim` evaluates keyframes).
    pub fn value(&self, attr: &str) -> Option<&'a Value> {
        self.attr(attr)?.value.as_ref()
    }

    /// Names of all attributes with an opinion in any sublayer (strongest first).
    pub fn attr_names(&self) -> Vec<&'a str> {
        let mut out: Vec<&'a str> = Vec::new();
        for (_, s) in self.stage.specs_static(&self.path) {
            for name in s.attrs.keys() {
                if !out.contains(&name.as_str()) {
                    out.push(name);
                }
            }
        }
        out
    }

    /// Strongest non-empty references opinion.
    pub fn references(&self) -> &'a [Reference] {
        self.stage
            .specs_static(&self.path)
            .find_map(|(_, s)| (!s.references.is_empty()).then_some(s.references.as_slice()))
            .unwrap_or(&[])
    }

    pub fn children(&self) -> Vec<PrimPath> {
        self.stage.children(&self.path)
    }

    pub fn parent(&self) -> Option<PrimView<'a>> {
        self.stage.prim(&self.path.parent()?)
    }
}

impl Stage {
    /// Like [`Stage::specs`] but borrowing only `self` (the path is cloned in).
    fn specs_static(&self, path: &PrimPath) -> impl Iterator<Item = (&SublayerId, &PrimSpec)> + '_ {
        let path = path.clone();
        self.stack
            .iter()
            .filter_map(move |id| self.sublayers[id].spec(&path).map(|s| (id, s)))
    }
}
