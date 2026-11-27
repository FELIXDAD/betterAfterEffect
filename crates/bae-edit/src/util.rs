//! Write helpers used by the edit commands. Every write is a `bae-model` op in the
//! running transaction, so a command is exactly one undo step.

use std::collections::{BTreeMap, BTreeSet};

use bae_model::schema;
use bae_model::{
    AttrSpec, CommandError, ModelError, Op, PrimPath, PrimSpec, PrimTree, PrimType, Specifier,
    Stage, SublayerId, Time, Transaction, Value,
};

use crate::query::{self, ClipInfo, SequenceInfo, TrackInfo, attrs};

pub(crate) fn fail(msg: impl Into<String>) -> CommandError {
    CommandError::Failed(msg.into())
}

pub(crate) fn child(parent: &PrimPath, name: &str) -> Result<PrimPath, CommandError> {
    parent
        .child(name)
        .map_err(|e| CommandError::from(ModelError::from(e)))
}

/// Create a `def` prim of `ty` at `path` (its parent must exist on the stage).
pub(crate) fn create(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    ty: PrimType,
    index: Option<usize>,
) -> Result<(), CommandError> {
    let parent = path.parent().ok_or(ModelError::RootNotEditable)?;
    tx.stage().require_prim(&parent)?;
    if tx.stage().has_prim(path) {
        return Err(ModelError::PrimExists {
            sublayer: tx.target().clone(),
            path: path.clone(),
        }
        .into());
    }
    tx.ensure_spec(&parent)?;
    tx.apply(Op::CreatePrim {
        path: path.clone(),
        specifier: Specifier::Def,
        prim_type: Some(ty),
        index,
    })?;
    Ok(())
}

/// Make sure a folder exists at `path` (creating missing ancestors as folders).
pub(crate) fn ensure_folder(tx: &mut Transaction<'_>, path: &PrimPath) -> Result<(), CommandError> {
    if path.is_root() || tx.stage().has_prim(path) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        ensure_folder(tx, &parent)?;
    }
    create(tx, path, PrimType::FOLDER, None)
}

/// Set a static value after checking it against the prim schema.
pub(crate) fn set(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    attr: &str,
    value: impl Into<Value>,
) -> Result<(), CommandError> {
    let value = value.into();
    let ty = tx.stage().prim(path).and_then(|p| p.prim_type().cloned());
    schema::check_value(path, ty.as_ref(), attr, &value)?;
    tx.ensure_spec(path)?;
    tx.apply(Op::SetValue {
        path: path.clone(),
        attr: attr.to_owned(),
        value: Some(value),
    })?;
    Ok(())
}

/// Remove the edit target's static value of `attr` (no-op when it has none).
pub(crate) fn unset(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    attr: &str,
) -> Result<(), CommandError> {
    let has = tx
        .stage()
        .sublayer(tx.target())
        .and_then(|s| s.attr(path, attr))
        .is_some();
    if has {
        tx.apply(Op::SetAttr {
            path: path.clone(),
            attr: attr.to_owned(),
            spec: None,
        })?;
    }
    // A weaker sublayer may still hold a value; override it with an empty one.
    if tx.stage().prim(path).is_some_and(|v| v.value(attr).is_some()) {
        set(tx, path, attr, Value::String(String::new()))?;
    }
    Ok(())
}

pub(crate) fn set_time(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    attr: &str,
    t: Time,
) -> Result<(), CommandError> {
    set(tx, path, attr, Value::Time(t))
}

fn sublayers_with_spec(stage: &Stage, path: &PrimPath) -> Vec<SublayerId> {
    stage
        .layer_stack()
        .iter()
        .filter(|id| stage.sublayer(id).is_some_and(|s| s.has_spec(path)))
        .cloned()
        .collect()
}

/// Delete a prim (and its subtree) from every sublayer that has it.
pub(crate) fn delete(tx: &mut Transaction<'_>, path: &PrimPath) -> Result<(), CommandError> {
    if !tx.stage().has_local_def(path) {
        return Err(ModelError::ProvidedByReference(path.clone()).into());
    }
    for id in sublayers_with_spec(tx.stage(), path) {
        tx.apply_to(&id, Op::DeletePrim { path: path.clone() })?;
    }
    Ok(())
}

/// Move a prim (and its subtree) in every sublayer that has it.
pub(crate) fn move_prim(
    tx: &mut Transaction<'_>,
    from: &PrimPath,
    to: &PrimPath,
) -> Result<(), CommandError> {
    if from == to {
        return Ok(());
    }
    if !tx.stage().has_local_def(from) {
        return Err(ModelError::ProvidedByReference(from.clone()).into());
    }
    let new_parent = to.parent().ok_or(ModelError::RootNotEditable)?;
    for id in sublayers_with_spec(tx.stage(), from) {
        tx.ensure_spec_in(&id, &new_parent)?;
        tx.apply_to(
            &id,
            Op::MovePrim {
                from: from.clone(),
                to: to.clone(),
                index: None,
            },
        )?;
    }
    Ok(())
}

/// The composed subtree at `path` as plain `def` specs (winning opinion of every
/// attribute), used to split and nest clips.
pub(crate) fn composed_tree(stage: &Stage, path: &PrimPath) -> PrimTree {
    let view = stage.prim(path);
    let mut attrs: BTreeMap<String, AttrSpec> = BTreeMap::new();
    let mut prim_type = None;
    if let Some(view) = &view {
        prim_type = view.prim_type().cloned();
        for name in view.attr_names() {
            if let Some(a) = view.attr(name) {
                attrs.insert(name.to_owned(), a.clone());
            }
        }
    }
    PrimTree {
        name: path.name().to_owned(),
        spec: PrimSpec {
            specifier: Specifier::Def,
            prim_type,
            attrs,
            references: view.map(|v| v.references().to_vec()).unwrap_or_default(),
        },
        children: stage
            .children(path)
            .iter()
            .map(|c| composed_tree(stage, c))
            .collect(),
    }
}

/// Recreate `tree` at `path` in the edit target.
pub(crate) fn restore(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    tree: PrimTree,
) -> Result<(), CommandError> {
    let parent = path.parent().ok_or(ModelError::RootNotEditable)?;
    tx.ensure_spec(&parent)?;
    tx.apply(Op::RestorePrim {
        path: path.clone(),
        tree,
        index: None,
    })?;
    Ok(())
}

// ---- sequences, tracks, clips ------------------------------------------------

pub(crate) fn require_sequence(
    stage: &Stage,
    seq: &PrimPath,
) -> Result<SequenceInfo, CommandError> {
    query::sequence_info(stage, seq).ok_or_else(|| fail(format!("{seq} is not a sequence")))
}

pub(crate) fn require_track(stage: &Stage, track: &PrimPath) -> Result<TrackInfo, CommandError> {
    let info =
        query::track_info(stage, track).ok_or_else(|| fail(format!("{track} is not a track")))?;
    Ok(info)
}

/// A track that may be edited (exists and is not locked).
pub(crate) fn require_unlocked(stage: &Stage, track: &PrimPath) -> Result<TrackInfo, CommandError> {
    let info = require_track(stage, track)?;
    if info.locked {
        return Err(fail(format!("track {} is locked", info.label)));
    }
    Ok(info)
}

pub(crate) fn require_clip(stage: &Stage, clip: &PrimPath) -> Result<ClipInfo, CommandError> {
    query::clip_info(stage, clip).ok_or_else(|| fail(format!("{clip} is not a clip")))
}

/// The clips plus, when `linked`, every clip linked to them; deduplicated, in a
/// stable order. Fails on non-clips and clips on locked tracks.
pub(crate) fn expand_linked(
    stage: &Stage,
    clips: &[PrimPath],
    linked: bool,
) -> Result<Vec<ClipInfo>, CommandError> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for c in clips {
        require_clip(stage, c)?;
        let group = if linked {
            query::linked_clips(stage, c)
        } else {
            vec![c.clone()]
        };
        for p in group {
            if seen.insert(p.clone()) {
                let info = require_clip(stage, &p)?;
                require_unlocked(stage, &info.track)?;
                out.push(info);
            }
        }
    }
    Ok(out)
}

/// A clip name unique within the whole sequence (`clip_N`), so clips can move
/// between tracks without renaming.
pub(crate) fn new_clip_name(stage: &Stage, seq: &PrimPath, taken: &BTreeSet<String>) -> String {
    let used: BTreeSet<String> = stage
        .children(seq)
        .iter()
        .flat_map(|t| stage.children(t))
        .map(|c| c.name().to_owned())
        .chain(taken.iter().cloned())
        .collect();
    (1..)
        .map(|i| format!("clip_{i}"))
        .find(|n| !used.contains(n))
        .expect("unbounded")
}

/// A link group id unused in the sequence (`link_N`).
pub(crate) fn new_link_id(stage: &Stage, seq: &PrimPath, taken: &BTreeSet<String>) -> String {
    let used: BTreeSet<String> = query::sequence_clips(stage, seq)
        .into_iter()
        .filter_map(|c| c.link)
        .chain(taken.iter().cloned())
        .collect();
    (1..)
        .map(|i| format!("link_{i}"))
        .find(|n| !used.contains(n))
        .expect("unbounded")
}

/// The values of a clip the edit commands change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Placement {
    pub start: Time,
    pub duration: Time,
    pub source_in: Time,
}

impl Placement {
    pub fn of(c: &ClipInfo) -> Self {
        Placement {
            start: c.start,
            duration: c.duration,
            source_in: c.source_in,
        }
    }

    pub fn end(&self) -> Time {
        self.start + self.duration
    }
}

/// Check a new placement of `clip` against the sequence frame and its source length.
pub(crate) fn check_placement(
    stage: &Stage,
    seq: &SequenceInfo,
    clip: &ClipInfo,
    p: &Placement,
) -> Result<(), CommandError> {
    if p.duration < seq.frame_duration() {
        return Err(fail(format!("{} would be shorter than a frame", clip.name)));
    }
    if p.start < Time::ZERO {
        return Err(fail(format!(
            "{} would start before the sequence",
            clip.name
        )));
    }
    if p.source_in < Time::ZERO {
        return Err(fail(format!("not enough media before {}", clip.name)));
    }
    if let Some(len) = clip
        .source
        .as_ref()
        .and_then(|s| query::source_duration(stage, s))
        && p.source_in + p.duration > len
    {
        return Err(fail(format!("not enough media after {}", clip.name)));
    }
    Ok(())
}

/// Write the changed parts of a placement.
pub(crate) fn write_placement(
    tx: &mut Transaction<'_>,
    clip: &ClipInfo,
    p: &Placement,
) -> Result<(), CommandError> {
    let old = Placement::of(clip);
    if p.start != old.start {
        set_time(tx, &clip.path, "start", p.start)?;
    }
    if p.duration != old.duration {
        set_time(tx, &clip.path, "duration", p.duration)?;
    }
    if p.source_in != old.source_in {
        set_time(tx, &clip.path, "source_in", p.source_in)?;
    }
    Ok(())
}

/// Move every clip of `track` that starts at or after `from` by `delta` (ripple),
/// except the clips in `skip`.
pub(crate) fn shift_from(
    tx: &mut Transaction<'_>,
    track: &PrimPath,
    from: Time,
    delta: Time,
    skip: &BTreeSet<PrimPath>,
) -> Result<(), CommandError> {
    if delta == Time::ZERO {
        return Ok(());
    }
    for c in query::clips(tx.stage(), track) {
        if c.start >= from && !skip.contains(&c.path) {
            set_time(tx, &c.path, "start", c.start + delta)?;
        }
    }
    Ok(())
}

/// Split `clip` at sequence time `t` (strictly inside it). The left part keeps the
/// path; the right part is a copy named `name`. Returns the right part's path.
pub(crate) fn split_clip(
    tx: &mut Transaction<'_>,
    clip: &ClipInfo,
    t: Time,
    name: &str,
) -> Result<PrimPath, CommandError> {
    debug_assert!(clip.start < t && t < clip.end());
    let right = child(&clip.track, name)?;
    let mut tree = composed_tree(tx.stage(), &clip.path);
    let offset = t - clip.start;
    let put = |tree: &mut PrimTree, attr: &str, v: Value| {
        tree.spec.attrs.insert(
            attr.to_owned(),
            AttrSpec {
                value: Some(v),
                ..AttrSpec::default()
            },
        );
    };
    put(&mut tree, "start", Value::Time(t));
    put(&mut tree, "duration", Value::Time(clip.duration - offset));
    put(&mut tree, "source_in", Value::Time(clip.source_in + offset));
    // The head transition stays with the left part, the tail one moves right.
    tree.spec.attrs.remove(attrs::TRANSITION_IN_KIND);
    tree.spec.attrs.remove(attrs::TRANSITION_IN_DURATION);
    restore(tx, &right, tree)?;
    set_time(tx, &clip.path, "duration", offset)?;
    if clip.transition_out.is_some() {
        unset(tx, &clip.path, attrs::TRANSITION_OUT_KIND)?;
        unset(tx, &clip.path, attrs::TRANSITION_OUT_DURATION)?;
    }
    Ok(right)
}

/// Splits made by one command, so the right halves of linked clips that were split
/// together get a fresh shared link and lone right halves are unlinked.
#[derive(Default)]
pub(crate) struct Splits {
    /// (original link group, right half).
    parts: Vec<(Option<String>, PrimPath)>,
    names: BTreeSet<String>,
}

impl Splits {
    /// Split `clip` at `t`; returns the right part.
    pub fn split(
        &mut self,
        tx: &mut Transaction<'_>,
        seq: &PrimPath,
        clip: &ClipInfo,
        t: Time,
    ) -> Result<PrimPath, CommandError> {
        let name = new_clip_name(tx.stage(), seq, &self.names);
        self.names.insert(name.clone());
        let right = split_clip(tx, clip, t, &name)?;
        self.parts.push((clip.link.clone(), right.clone()));
        Ok(right)
    }

    /// Fix the link groups of the right halves. Returns them.
    pub fn finish(
        self,
        tx: &mut Transaction<'_>,
        seq: &PrimPath,
    ) -> Result<Vec<PrimPath>, CommandError> {
        let mut groups: BTreeMap<String, Vec<PrimPath>> = BTreeMap::new();
        let mut out = Vec::new();
        for (link, right) in self.parts {
            out.push(right.clone());
            if let Some(link) = link {
                groups.entry(link).or_default().push(right);
            }
        }
        let mut taken = BTreeSet::new();
        for (_, members) in groups {
            if members.len() < 2 {
                for m in &members {
                    unset(tx, m, attrs::LINK)?;
                }
                continue;
            }
            let id = new_link_id(tx.stage(), seq, &taken);
            taken.insert(id.clone());
            for m in &members {
                set(tx, m, attrs::LINK, Value::String(id.clone()))?;
            }
        }
        Ok(out)
    }
}

/// Make room for `[range_start, range_end)` on `track` (overwrite): clips inside are
/// deleted, clips crossing an edge are trimmed, a clip covering the whole range is
/// split around it. Clips in `skip` are left alone.
pub(crate) fn clear_range(
    tx: &mut Transaction<'_>,
    splits: &mut Splits,
    seq: &PrimPath,
    track: &PrimPath,
    range_start: Time,
    range_end: Time,
    skip: &BTreeSet<PrimPath>,
) -> Result<(), CommandError> {
    for c in query::clips(tx.stage(), track) {
        if skip.contains(&c.path) || c.end() <= range_start || c.start >= range_end {
            continue;
        }
        let covers_start = c.start < range_start;
        let covers_end = c.end() > range_end;
        match (covers_start, covers_end) {
            (false, false) => delete(tx, &c.path)?,
            (true, false) => set_time(tx, &c.path, "duration", range_start - c.start)?,
            (false, true) => {
                let cut = range_end - c.start;
                let p = Placement {
                    start: range_end,
                    duration: c.duration - cut,
                    source_in: c.source_in + cut,
                };
                write_placement(tx, &c, &p)?;
                if c.transition_in.is_some() {
                    unset(tx, &c.path, attrs::TRANSITION_IN_KIND)?;
                    unset(tx, &c.path, attrs::TRANSITION_IN_DURATION)?;
                }
            }
            (true, true) => {
                let right = splits.split(tx, seq, &c, range_end)?;
                set_time(tx, &c.path, "duration", range_start - c.start)?;
                let _ = right;
            }
        }
    }
    Ok(())
}

/// Snap a sequence time or a duration to the nearest frame.
pub(crate) fn snap(seq: &SequenceInfo, t: Time) -> Time {
    seq.snap(t)
}
