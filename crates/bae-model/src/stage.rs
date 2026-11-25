//! The stage: sublayers composed by strength, transactions, undo and events.

use std::sync::atomic::{AtomicU64, Ordering};

use rustc_hash::FxHashMap;

use crate::error::{ModelError, Result};
use crate::event::{Change, EventOrigin, StageEvent};
use crate::ids::{AuthorId, SublayerId};
use crate::op::{Edit, Op};
use crate::path::PrimPath;
use crate::prim::{PrimType, Specifier};
use crate::sublayer::Sublayer;

/// Default maximum of undo steps kept per author ([`Stage::set_undo_limit`]).
pub const DEFAULT_UNDO_LIMIT: usize = 1000;

/// Sublayer IDs that differ only in case map to the same file on macOS/Windows.
fn same_file_name(a: &SublayerId, b: &SublayerId) -> bool {
    a.as_str().eq_ignore_ascii_case(b.as_str())
}

/// Handle returned by [`Stage::subscribe`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

// `Sync` so that `&Stage` can be shared across threads for parallel evaluation
// (bae-anim / bae-render); callbacks are only ever called through `&mut Stage`.
type Subscriber = Box<dyn FnMut(&StageEvent) + Send + Sync>;

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
    revision: u64,
    pub(crate) sublayers: FxHashMap<SublayerId, Sublayer>,
    /// Strongest first.
    pub(crate) stack: Vec<SublayerId>,
    /// Other projects that cross-project references resolve against, by the
    /// `Reference::project` key.
    pub(crate) externals: FxHashMap<String, Stage>,
    /// Same-project references `(referencing prim, target prim)`, rebuilt lazily
    /// after structural changes; used to report changes at referencing paths.
    pub(crate) ref_index: Option<Vec<(PrimPath, PrimPath)>>,
    edit_target: SublayerId,
    undo: FxHashMap<AuthorId, Vec<UndoEntry>>,
    redo: FxHashMap<AuthorId, Vec<UndoEntry>>,
    /// Undo steps kept per author (AE: Preferences > General > Levels of Undo).
    undo_limit: usize,
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

static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

impl Stage {
    /// Identifies this exact composed document state, including after project replacement.
    pub fn revision(&self) -> u64 {
        self.revision
    }

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
        let mut map: FxHashMap<SublayerId, Sublayer> = FxHashMap::default();
        for s in sublayers {
            let id = s.id().clone();
            if map.keys().any(|k| same_file_name(k, &id)) {
                return Err(ModelError::SublayerExists(id));
            }
            map.insert(id, s);
        }
        Ok(Stage {
            revision: NEXT_REVISION.fetch_add(1, Ordering::Relaxed),
            sublayers: map,
            stack,
            edit_target,
            undo: FxHashMap::default(),
            redo: FxHashMap::default(),
            undo_limit: DEFAULT_UNDO_LIMIT,
            subscribers: Vec::new(),
            next_subscription: 0,
            externals: FxHashMap::default(),
            ref_index: None,
        })
    }

    /// Undo steps kept per author (at least 1); older steps are dropped now and
    /// from then on.
    pub fn set_undo_limit(&mut self, limit: usize) {
        self.undo_limit = limit.max(1);
        for stack in self.undo.values_mut().chain(self.redo.values_mut()) {
            let extra = stack.len().saturating_sub(self.undo_limit);
            stack.drain(..extra);
        }
    }

    /// Undo steps kept per author.
    pub fn undo_limit(&self) -> usize {
        self.undo_limit
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
        if self.sublayers.keys().any(|k| same_file_name(k, &id)) {
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
        self.publish(event)
    }

    // ---- events ----

    /// Call `f` after every committed change. Callbacks run synchronously on the
    /// thread that changed the stage; keep them cheap (queue work elsewhere).
    pub fn subscribe(
        &mut self,
        f: impl FnMut(&StageEvent) + Send + Sync + 'static,
    ) -> SubscriptionId {
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

    /// Add changes seen through references, notify subscribers, return the event.
    pub(crate) fn publish(&mut self, mut event: StageEvent) -> StageEvent {
        if event.is_empty() {
            return event;
        }
        if event
            .changes
            .iter()
            .any(|c| !matches!(c, Change::AttrChanged { .. }))
        {
            self.ref_index = None;
        }
        self.revision = NEXT_REVISION.fetch_add(1, Ordering::Relaxed);
        self.expand_through_references(&mut event);
        for (_, f) in &mut self.subscribers {
            f(&event);
        }
        event
    }

    // ---- layer lock ----

    /// The layer `path` belongs to (itself for a layer prim).
    pub fn layer_of(&self, path: &PrimPath) -> Option<PrimPath> {
        std::iter::once(path.clone())
            .chain(path.ancestors())
            .find(|p| !p.is_root() && self.prim(p).is_some_and(|v| v.is_a(&PrimType::LAYER)))
    }

    /// The layer's Lock switch is on.
    pub fn is_layer_locked(&self, layer: &PrimPath) -> bool {
        self.prim(layer)
            .and_then(|v| v.value("locked"))
            .and_then(|v| v.as_bool())
            == Some(true)
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
        self.transact_inner(author, label, None, f)
    }

    /// [`Stage::transact`] with a write guard: every op on, new sublayer named, or
    /// edit-target switch to a sublayer for which `allow` returns false fails with
    /// [`ModelError::WriteDenied`] *before* anything is written, so a refused
    /// transaction leaves no trace in that sublayer's history. Used for agent
    /// permissions (`bae-agent`) and locked sublayers.
    pub fn transact_guarded<R, E>(
        &mut self,
        author: &AuthorId,
        label: &str,
        allow: &dyn Fn(&SublayerId) -> bool,
        f: impl FnOnce(&mut Transaction<'_>) -> std::result::Result<R, E>,
    ) -> std::result::Result<(R, StageEvent), E>
    where
        E: From<ModelError>,
    {
        self.transact_inner(author, label, Some(allow), f)
    }

    fn transact_inner<R, E>(
        &mut self,
        author: &AuthorId,
        label: &str,
        guard: Option<&dyn Fn(&SublayerId) -> bool>,
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
            guard,
            locked_at_start: FxHashMap::default(),
            allow_locked: false,
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
            if stack.len() > self.undo_limit {
                stack.remove(0);
            }
            if let Some(redo) = clear {
                redo.remove(author);
            }
        }
        self.publish(StageEvent {
            origin,
            author: Some(author.clone()),
            sublayers: touched,
            changes,
        })
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
    /// Import several update blobs for one sublayer in one go: the mirror is rebuilt
    /// once and one event is emitted (use this for bursts of small hub updates).
    pub fn import_updates_batch(
        &mut self,
        sublayer: &SublayerId,
        blobs: &[Vec<u8>],
    ) -> Result<StageEvent> {
        let layer = self
            .sublayers
            .get_mut(sublayer)
            .ok_or_else(|| ModelError::UnknownSublayer(sublayer.clone()))?;
        let changes = layer.import_batch(blobs)?;
        Ok(self.publish(StageEvent {
            origin: EventOrigin::Remote,
            author: None,
            sublayers: vec![sublayer.clone()],
            changes,
        }))
    }

    pub fn import_updates(&mut self, sublayer: &SublayerId, bytes: &[u8]) -> Result<StageEvent> {
        let layer = self
            .sublayers
            .get_mut(sublayer)
            .ok_or_else(|| ModelError::UnknownSublayer(sublayer.clone()))?;
        let changes = layer.import(bytes)?;
        Ok(self.publish(StageEvent {
            origin: EventOrigin::Remote,
            author: None,
            sublayers: vec![sublayer.clone()],
            changes,
        }))
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
    guard: Option<&'s dyn Fn(&SublayerId) -> bool>,
    /// Lock state of each layer when the transaction first touched it.
    locked_at_start: FxHashMap<PrimPath, bool>,
    /// Inside [`Transaction::allow_locked`].
    allow_locked: bool,
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

    /// Whether the transaction's write guard (if any) allows writing `sublayer`.
    pub fn may_write(&self, sublayer: &SublayerId) -> bool {
        self.guard.is_none_or(|allow| allow(sublayer))
    }

    fn check_write(&self, sublayer: &SublayerId) -> Result<()> {
        if self.may_write(sublayer) {
            Ok(())
        } else {
            Err(ModelError::WriteDenied(sublayer.clone()))
        }
    }

    /// Run `f` with the layer lock check off: for consequences of an edit
    /// elsewhere that AE also applies to locked layers (deleting a parent
    /// unparents its locked children and keeps them in place). Never for
    /// edits the caller asked for on the locked layer itself.
    pub fn allow_locked<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let before = std::mem::replace(&mut self.allow_locked, true);
        let r = f(self);
        self.allow_locked = before;
        r
    }

    /// Inside [`Transaction::allow_locked`].
    pub fn locks_allowed(&self) -> bool {
        self.allow_locked
    }

    /// Every transaction (commands, agents, scripts) refuses ops on a layer
    /// that was locked when the transaction first touched it and still is:
    /// the layer itself, its properties, keyframes and children. Allowed: the
    /// layer's own `locked` attribute, and pure reorders (another layer moved
    /// past it). Undo / redo replay history directly and are not checked.
    fn check_locks(&mut self, op: &Op) -> Result<()> {
        if self.allow_locked {
            return Ok(());
        }
        let paths: Vec<&PrimPath> = match op {
            Op::MovePrim { from, to, .. } if from == to => return Ok(()),
            Op::MovePrim { from, to, .. } => vec![from, to],
            // An `over` with no type changes nothing by itself; it only makes
            // room for the edits that follow (which are checked).
            Op::CreatePrim {
                specifier: Specifier::Over,
                prim_type: None,
                ..
            } => return Ok(()),
            other => vec![other.path()],
        };
        for path in paths {
            let Some(layer) = self.stage.layer_of(path) else {
                continue;
            };
            let locked_now = self.stage.is_layer_locked(&layer);
            let locked_at_start = *self
                .locked_at_start
                .entry(layer.clone())
                .or_insert(locked_now);
            if !(locked_at_start && locked_now) {
                continue;
            }
            let lock_switch = match op {
                Op::SetValue { path, attr, .. }
                | Op::SetAttr { path, attr, .. }
                | Op::SetKeyframe { path, attr, .. }
                | Op::RemoveKeyframe { path, attr, .. }
                | Op::SetExpression { path, attr, .. } => *path == layer && attr == "locked",
                _ => false,
            };
            if !lock_switch {
                let name = self
                    .stage
                    .prim(&layer)
                    .and_then(|v| v.value("name").and_then(|n| n.as_str().map(str::to_owned)))
                    .unwrap_or_else(|| layer.name().to_owned());
                return Err(ModelError::LayerLocked { layer, name });
            }
        }
        // A layer made in this transaction (new, pasted, duplicated, undone
        // delete) was not locked when it started, even if it arrives locked.
        if let Op::CreatePrim { path, .. } | Op::RestorePrim { path, .. } = op {
            self.locked_at_start.insert(path.clone(), false);
        }
        Ok(())
    }

    /// Apply an op to a specific sublayer.
    pub fn apply_to(&mut self, sublayer: &SublayerId, op: Op) -> Result<()> {
        self.check_write(sublayer)?;
        self.check_locks(&op)?;
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
        self.check_write(&id)?;
        self.stage.create_sublayer(id, index).map(|_| ())
    }

    /// Change the stage's edit target (session state, not undoable).
    pub fn set_edit_target(&mut self, id: &SublayerId) -> Result<()> {
        self.check_write(id)?;
        self.stage.set_edit_target(id)?;
        self.target = id.clone();
        Ok(())
    }
}
