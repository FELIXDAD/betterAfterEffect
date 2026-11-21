//! Keyframe commands: the stopwatch (`attr.set_animated`), value edits at a time
//! (`attr.set_at`) and AE's keyframe assistants (`key.ease`, `key.set_interpolation`,
//! `key.move`, `key.delete`, `key.reverse`, `key.paste`).
//!
//! **Time base.** Keyframe times are the times stored in the model: layer time for
//! layer attributes and the prims below a layer, comp time for comp attributes
//! (`docs/spec/data-model.md` §10). Use [`crate::query::to_key_time`] to convert a
//! comp time.

use std::collections::BTreeMap;

use bae_model::{
    AttrSpec, Command, CommandError, Ease, Interpolation, Keyframe, ModelError, PrimPath, Time,
    Transaction, Value,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query;
use crate::util::{check_animatable, fail, write_attr};

/// One keyframe, identified by its attribute and keyframe time.
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct KeyRef {
    pub path: PrimPath,
    pub attr: String,
    /// Keyframe time in flicks (layer time for layer attributes).
    pub time: Time,
}

impl KeyRef {
    pub fn new(path: PrimPath, attr: impl Into<String>, time: Time) -> Self {
        KeyRef {
            path,
            attr: attr.into(),
            time,
        }
    }
}

/// Output of commands that move or create keyframes: where they are now.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct KeysOutput {
    pub keys: Vec<KeyRef>,
}

type Group = ((PrimPath, String), Vec<Time>);

/// Group keyframe refs by attribute, in first-seen order.
fn group(keys: &[KeyRef]) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    for k in keys {
        let id = (k.path.clone(), k.attr.clone());
        match out.iter_mut().find(|(g, _)| g == &id) {
            Some((_, times)) => {
                if !times.contains(&k.time) {
                    times.push(k.time);
                }
            }
            None => out.push((id, vec![k.time])),
        }
    }
    out
}

/// The composed opinion of an attribute, requiring every time in `times` to be a
/// keyframe.
fn animated_spec(
    tx: &Transaction<'_>,
    path: &PrimPath,
    attr: &str,
    times: &[Time],
) -> Result<AttrSpec, CommandError> {
    let spec = query::attr_spec(tx.stage(), path, attr)
        .cloned()
        .unwrap_or_default();
    for t in times {
        if !spec.keyframes.contains_key(t) {
            return Err(ModelError::KeyframeNotFound {
                path: path.clone(),
                attr: attr.to_owned(),
                time: *t,
            }
            .into());
        }
    }
    Ok(spec)
}

fn check_value(
    tx: &Transaction<'_>,
    path: &PrimPath,
    attr: &str,
    v: &Value,
) -> Result<(), CommandError> {
    let ty = tx.stage().prim(path).and_then(|p| p.prim_type().cloned());
    bae_model::schema::check_value(path, ty.as_ref(), attr, v)?;
    Ok(())
}

// ---------------------------------------------------------------------------

pub struct SetAnimated;

#[derive(Deserialize, JsonSchema)]
pub struct SetAnimatedParams {
    pub path: PrimPath,
    pub attr: String,
    /// true: start animating (adds a keyframe with the current value at `time`);
    /// false: remove every keyframe, keeping the value at `time` as the static value.
    pub animated: bool,
    /// Keyframe time in flicks (layer time for layer attributes).
    pub time: Time,
}

impl Command for SetAnimated {
    type Params = SetAnimatedParams;
    type Output = ();
    const ID: &'static str = "attr.set_animated";
    const TITLE: &'static str = "Time-Vary Stopwatch";
    const DESCRIPTION: &'static str = "Turn an attribute's stopwatch on (first keyframe at the given time with the current value) or off (delete all keyframes, keep the value at that time).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        tx.stage().require_prim(&p.path)?;
        check_animatable(tx.stage(), &p.path, &p.attr)?;
        let mut spec = query::attr_spec(tx.stage(), &p.path, &p.attr)
            .cloned()
            .unwrap_or_default();
        if spec.is_animated() == p.animated {
            return Ok(());
        }
        let value = query::sample_attr(tx.stage(), &p.path, &p.attr, p.time)
            .ok_or_else(|| fail(format!("{}.{} has no value", p.path, p.attr)))?;
        if p.animated {
            spec.keyframes.insert(p.time, Keyframe::linear(value));
        } else {
            spec.keyframes.clear();
            spec.value = Some(value);
        }
        write_attr(tx, &p.path, &p.attr, spec)
    }
}

// ---------------------------------------------------------------------------

pub struct SetAt;

#[derive(Deserialize, JsonSchema)]
pub struct SetAtParams {
    pub path: PrimPath,
    pub attr: String,
    /// Keyframe time in flicks (layer time for layer attributes). Only used when the
    /// attribute is animated.
    pub time: Time,
    pub value: Value,
}

impl Command for SetAt {
    type Params = SetAtParams;
    type Output = ();
    const ID: &'static str = "attr.set_at";
    const TITLE: &'static str = "Change Value";
    const DESCRIPTION: &'static str = "Change an attribute the way editing it in the timeline does: animated attributes get a keyframe at the time (an existing one keeps its interpolation), others get a new static value.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        tx.stage().require_prim(&p.path)?;
        check_value(tx, &p.path, &p.attr, &p.value)?;
        let mut spec = query::attr_spec(tx.stage(), &p.path, &p.attr)
            .cloned()
            .unwrap_or_default();
        if spec.is_animated() {
            match spec.keyframes.get_mut(&p.time) {
                Some(k) => k.value = p.value,
                None => {
                    spec.keyframes.insert(p.time, Keyframe::linear(p.value));
                }
            }
        } else {
            spec.value = Some(p.value);
        }
        write_attr(tx, &p.path, &p.attr, spec)
    }
}

// ---------------------------------------------------------------------------

pub struct SetMany;

/// One value edit of [`SetMany`].
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ValueEdit {
    pub path: PrimPath,
    pub attr: String,
    /// Keyframe time in flicks (layer time for layer attributes).
    pub time: Time,
    pub value: Value,
}

#[derive(Deserialize, JsonSchema)]
pub struct SetManyParams {
    pub edits: Vec<ValueEdit>,
}

impl Command for SetMany {
    type Params = SetManyParams;
    type Output = ();
    const ID: &'static str = "attr.set_many";
    const TITLE: &'static str = "Change Values";
    const DESCRIPTION: &'static str =
        "Several attr.set_at edits as one undo step (e.g. moving several layers in the viewer).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for e in p.edits {
            SetAt.run(
                tx,
                SetAtParams {
                    path: e.path,
                    attr: e.attr,
                    time: e.time,
                    value: e.value,
                },
            )?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

/// Which side(s) of the keyframes an ease applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EaseMode {
    /// F9: both sides.
    EasyEase,
    /// Shift+F9: the incoming side.
    EaseIn,
    /// Cmd/Ctrl+Shift+F9: the outgoing side.
    EaseOut,
}

pub struct EaseKeys;

#[derive(Deserialize, JsonSchema)]
pub struct EaseKeysParams {
    pub keys: Vec<KeyRef>,
    /// easy_ease (F9), ease_in (Shift+F9) or ease_out (Cmd/Ctrl+Shift+F9).
    #[serde(default = "easy")]
    pub mode: EaseMode,
    /// Influence in percent (default 33.33, AE's default).
    #[serde(default)]
    pub influence: Option<f64>,
}

fn easy() -> EaseMode {
    EaseMode::EasyEase
}

impl Command for EaseKeys {
    type Params = EaseKeysParams;
    type Output = ();
    const ID: &'static str = "key.ease";
    const TITLE: &'static str = "Easy Ease";
    const DESCRIPTION: &'static str = "Keyframe assistant: Easy Ease (both sides), Easy Ease In or Easy Ease Out — Bezier interpolation with speed 0 and 33.33% influence on the chosen side.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let influence = p
            .influence
            .unwrap_or(Ease::DEFAULT_INFLUENCE)
            .clamp(0.1, 100.0);
        for ((path, attr), times) in group(&p.keys) {
            let mut spec = animated_spec(tx, &path, &attr, &times)?;
            let probe = spec.clone();
            for t in &times {
                let k = spec.keyframes.get_mut(t).expect("checked");
                let dims = query::ease_dimensions(&attr, &probe, &k.value);
                let ease = vec![
                    Ease {
                        speed: 0.0,
                        influence,
                    };
                    dims
                ];
                if p.mode != EaseMode::EaseOut {
                    k.interp_in = Interpolation::Bezier;
                    k.ease_in = ease.clone();
                }
                if p.mode != EaseMode::EaseIn {
                    k.interp_out = Interpolation::Bezier;
                    k.ease_out = ease;
                }
                k.flags.temporal_auto_bezier = false;
                k.flags.temporal_continuous = p.mode == EaseMode::EasyEase;
            }
            write_attr(tx, &path, &attr, spec)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct SetInterpolation;

#[derive(Deserialize, JsonSchema)]
pub struct SetInterpolationParams {
    pub keys: Vec<KeyRef>,
    /// Incoming temporal interpolation (linear, bezier, hold); omitted keeps it.
    #[serde(default)]
    pub interp_in: Option<Interpolation>,
    /// Outgoing temporal interpolation; omitted keeps it.
    #[serde(default)]
    pub interp_out: Option<Interpolation>,
}

impl Command for SetInterpolation {
    type Params = SetInterpolationParams;
    type Output = ();
    const ID: &'static str = "key.set_interpolation";
    const TITLE: &'static str = "Keyframe Interpolation";
    const DESCRIPTION: &'static str = "Set the temporal interpolation of keyframes (AE Keyframe Interpolation dialog; Toggle Hold Keyframe sets the outgoing side to hold).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for ((path, attr), times) in group(&p.keys) {
            let mut spec = animated_spec(tx, &path, &attr, &times)?;
            let probe = spec.clone();
            for t in &times {
                let k = spec.keyframes.get_mut(t).expect("checked");
                let dims = query::ease_dimensions(&attr, &probe, &k.value);
                if let Some(i) = p.interp_in {
                    k.interp_in = i;
                    if i == Interpolation::Bezier && k.ease_in.is_empty() {
                        k.ease_in = vec![Ease::default(); dims];
                    }
                }
                if let Some(o) = p.interp_out {
                    k.interp_out = o;
                    if o == Interpolation::Bezier && k.ease_out.is_empty() {
                        k.ease_out = vec![Ease::default(); dims];
                    }
                }
                k.flags.temporal_auto_bezier = false;
            }
            write_attr(tx, &path, &attr, spec)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct MoveKeys;

#[derive(Deserialize, JsonSchema)]
pub struct MoveKeysParams {
    pub keys: Vec<KeyRef>,
    /// Offset in flicks added to every keyframe time (may be negative).
    pub delta: Time,
}

impl Command for MoveKeys {
    type Params = MoveKeysParams;
    type Output = KeysOutput;
    const ID: &'static str = "key.move";
    const TITLE: &'static str = "Move Keyframes";
    const DESCRIPTION: &'static str = "Move keyframes in time by an offset (dragging keyframes in the timeline). Keyframes they land on are replaced.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<KeysOutput, CommandError> {
        let mut out = KeysOutput::default();
        if p.delta == Time::ZERO {
            out.keys = p.keys;
            return Ok(out);
        }
        for ((path, attr), times) in group(&p.keys) {
            let mut spec = animated_spec(tx, &path, &attr, &times)?;
            let moved: Vec<(Time, Keyframe)> = times
                .iter()
                .map(|t| (*t, spec.keyframes.remove(t).expect("checked")))
                .collect();
            for (t, k) in moved {
                let nt = t.saturating_add(p.delta);
                spec.keyframes.insert(nt, k);
                out.keys.push(KeyRef::new(path.clone(), attr.clone(), nt));
            }
            write_attr(tx, &path, &attr, spec)?;
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------

pub struct DeleteKeys;

#[derive(Deserialize, JsonSchema)]
pub struct DeleteKeysParams {
    pub keys: Vec<KeyRef>,
}

impl Command for DeleteKeys {
    type Params = DeleteKeysParams;
    type Output = ();
    const ID: &'static str = "key.delete";
    const TITLE: &'static str = "Delete Keyframes";
    const DESCRIPTION: &'static str = "Delete keyframes. Deleting every keyframe of an attribute turns its stopwatch off and keeps the value of the last deleted keyframe.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for ((path, attr), times) in group(&p.keys) {
            let mut spec = animated_spec(tx, &path, &attr, &times)?;
            let mut last = None;
            for t in &times {
                last = spec.keyframes.remove(t).map(|k| k.value);
            }
            if spec.keyframes.is_empty() && spec.value.is_none() {
                spec.value = last;
            }
            write_attr(tx, &path, &attr, spec)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct ReverseKeys;

#[derive(Deserialize, JsonSchema)]
pub struct ReverseKeysParams {
    pub keys: Vec<KeyRef>,
}

fn swap_sides(k: &mut Keyframe) {
    std::mem::swap(&mut k.interp_in, &mut k.interp_out);
    std::mem::swap(&mut k.ease_in, &mut k.ease_out);
    std::mem::swap(&mut k.spatial_in, &mut k.spatial_out);
}

impl Command for ReverseKeys {
    type Params = ReverseKeysParams;
    type Output = KeysOutput;
    const ID: &'static str = "key.reverse";
    const TITLE: &'static str = "Time-Reverse Keyframes";
    const DESCRIPTION: &'static str = "Keyframe assistant: reverse the order of keyframes in time within the span they cover, per attribute.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<KeysOutput, CommandError> {
        let mut out = KeysOutput::default();
        for ((path, attr), times) in group(&p.keys) {
            let mut spec = animated_spec(tx, &path, &attr, &times)?;
            let (lo, hi) = (
                *times.iter().min().expect("non-empty"),
                *times.iter().max().expect("non-empty"),
            );
            let taken: Vec<(Time, Keyframe)> = times
                .iter()
                .map(|t| (*t, spec.keyframes.remove(t).expect("checked")))
                .collect();
            for (t, mut k) in taken {
                let nt = Time(lo.0 + hi.0 - t.0);
                swap_sides(&mut k);
                spec.keyframes.insert(nt, k);
                out.keys.push(KeyRef::new(path.clone(), attr.clone(), nt));
            }
            write_attr(tx, &path, &attr, spec)?;
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------

pub struct PasteKeys;

/// A copied keyframe, relative to the first copied keyframe.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CopiedKey {
    /// Offset from the paste time, in flicks.
    pub offset: Time,
    pub keyframe: Keyframe,
}

#[derive(Deserialize, JsonSchema)]
pub struct PasteKeysParams {
    pub path: PrimPath,
    pub attr: String,
    /// Keyframe time (flicks) of the first pasted keyframe.
    pub time: Time,
    pub keys: Vec<CopiedKey>,
}

impl Command for PasteKeys {
    type Params = PasteKeysParams;
    type Output = KeysOutput;
    const ID: &'static str = "key.paste";
    const TITLE: &'static str = "Paste Keyframes";
    const DESCRIPTION: &'static str =
        "Paste copied keyframes onto an attribute starting at a time (turns the stopwatch on).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<KeysOutput, CommandError> {
        tx.stage().require_prim(&p.path)?;
        check_animatable(tx.stage(), &p.path, &p.attr)?;
        let mut spec = query::attr_spec(tx.stage(), &p.path, &p.attr)
            .cloned()
            .unwrap_or_default();
        let mut out = KeysOutput::default();
        let mut added: BTreeMap<Time, Keyframe> = BTreeMap::new();
        for c in p.keys {
            check_value(tx, &p.path, &p.attr, &c.keyframe.value)?;
            let t = p.time.saturating_add(c.offset);
            added.insert(t, c.keyframe);
            out.keys
                .push(KeyRef::new(p.path.clone(), p.attr.clone(), t));
        }
        spec.keyframes.extend(added);
        write_attr(tx, &p.path, &p.attr, spec)?;
        Ok(out)
    }
}
