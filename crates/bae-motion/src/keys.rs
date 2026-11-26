//! Keyframe commands: the stopwatch (`attr.set_animated`), value edits at a time
//! (`attr.set_at`), AE's keyframe assistants (`key.ease`, `key.set_interpolation`,
//! `key.move`, `key.delete`, `key.reverse`, `key.paste`) and free keyframe edits
//! (`key.edit`: the Graph Editor, keyframe dialogs).
//!
//! Commands that change keyframe times or values recompute AE's automatic values
//! afterwards (temporal / spatial auto-Bezier, roving key times; see
//! [`bae_anim::edit`]), so the stored keyframes stay what AE would store.
//!
//! **Time base.** Keyframe times are the times stored in the model: layer time for
//! layer attributes and the prims below a layer, comp time for comp attributes
//! (`docs/spec/data-model.md` §10). Use [`crate::query::to_key_time`] to convert a
//! comp time.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use bae_model::{
    AttrSpec, Command, CommandError, Ease, Interpolation, Keyframe, ModelError, PrimPath, Time,
    Transaction, Value,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query;
use crate::util::{check_animatable, ensure_unlocked, fail, write_attr};

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

/// Recompute AE's automatic keyframe values of `attr` (auto-Bezier eases and
/// tangents, roving times) after its keyframes changed.
/// Preferences > Composition > Default Spatial Interpolation to Linear (on by
/// default). When off, new keyframes of spatial properties (Position, Anchor Point …)
/// get Auto Bezier spatial interpolation, as in AE. The UI sets it from the
/// preferences; the commands read it when they create keyframes.
static DEFAULT_SPATIAL_LINEAR: AtomicBool = AtomicBool::new(true);

/// Set the Default Spatial Interpolation to Linear preference (see above).
pub fn set_default_spatial_linear(on: bool) {
    DEFAULT_SPATIAL_LINEAR.store(on, Ordering::Relaxed);
}

/// A new keyframe of `attr` (keyframes `spec`): linear in time; spatially linear or
/// Auto Bezier as the preference says.
pub(crate) fn new_key(attr: &str, spec: &AttrSpec, value: Value) -> Keyframe {
    let mut k = Keyframe::linear(value);
    if !DEFAULT_SPATIAL_LINEAR.load(Ordering::Relaxed)
        && bae_anim::schema::curve_kind(attr, spec) == bae_anim::CurveKind::Spatial
    {
        k.flags.spatial_auto_bezier = true;
        k.flags.spatial_continuous = true;
    }
    k
}

pub(crate) fn refresh_auto(attr: &str, spec: &mut AttrSpec) {
    if spec.keyframes.len() < 2 {
        return;
    }
    let auto = spec
        .keyframes
        .values()
        .any(|k| k.flags.temporal_auto_bezier || k.flags.spatial_auto_bezier || k.flags.roving);
    if auto {
        let kind = bae_anim::schema::curve_kind(attr, spec);
        spec.keyframes = bae_anim::edit::update_auto_values(&spec.keyframes, kind);
    }
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
        ensure_unlocked(tx.stage(), [&p.path])?;
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
            spec.keyframes
                .insert(p.time, new_key(&p.attr, &spec, value));
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
        ensure_unlocked(tx.stage(), [&p.path])?;
        tx.stage().require_prim(&p.path)?;
        check_value(tx, &p.path, &p.attr, &p.value)?;
        let spec = spec_with_value_at(tx.stage(), &p.path, &p.attr, p.time, p.value);
        write_attr(tx, &p.path, &p.attr, spec)
    }
}

/// `attr` of `path` as [`SetAt`] leaves it: a keyframe at `time` (keyframe
/// time) holding `value` when animated (an existing one keeps its
/// interpolation), else the new static value. Panels preview a value drag
/// with it before committing (`bae_anim::Evaluator::set_overrides`).
pub fn spec_with_value_at(
    stage: &bae_model::Stage,
    path: &PrimPath,
    attr: &str,
    time: Time,
    value: Value,
) -> AttrSpec {
    let mut spec = query::attr_spec(stage, path, attr)
        .cloned()
        .unwrap_or_default();
    if spec.is_animated() {
        match spec.keyframes.get_mut(&time) {
            Some(k) => k.value = value,
            None => {
                let k = new_key(attr, &spec, value);
                spec.keyframes.insert(time, k);
            }
        }
    } else {
        spec.value = Some(value);
    }
    refresh_auto(attr, &mut spec);
    spec
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
        ensure_unlocked(tx.stage(), p.keys.iter().map(|k| &k.path))?;
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
        ensure_unlocked(tx.stage(), p.keys.iter().map(|k| &k.path))?;
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
        ensure_unlocked(tx.stage(), p.keys.iter().map(|k| &k.path))?;
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
            refresh_auto(&attr, &mut spec);
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
        ensure_unlocked(tx.stage(), p.keys.iter().map(|k| &k.path))?;
        for ((path, attr), times) in group(&p.keys) {
            let mut spec = animated_spec(tx, &path, &attr, &times)?;
            let mut last = None;
            for t in &times {
                last = spec.keyframes.remove(t).map(|k| k.value);
            }
            if spec.keyframes.is_empty() && spec.value.is_none() {
                spec.value = last;
            }
            refresh_auto(&attr, &mut spec);
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
        ensure_unlocked(tx.stage(), p.keys.iter().map(|k| &k.path))?;
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
            refresh_auto(&attr, &mut spec);
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
    /// Edit > Paste Reversed Keyframes: the last copied keyframe first, each
    /// keyframe's incoming and outgoing sides swapped.
    #[serde(default)]
    pub reversed: bool,
}

impl Command for PasteKeys {
    type Params = PasteKeysParams;
    type Output = KeysOutput;
    const ID: &'static str = "key.paste";
    const TITLE: &'static str = "Paste Keyframes";
    const DESCRIPTION: &'static str =
        "Paste copied keyframes onto an attribute starting at a time (turns the stopwatch on).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<KeysOutput, CommandError> {
        ensure_unlocked(tx.stage(), [&p.path])?;
        tx.stage().require_prim(&p.path)?;
        check_animatable(tx.stage(), &p.path, &p.attr)?;
        let mut spec = query::attr_spec(tx.stage(), &p.path, &p.attr)
            .cloned()
            .unwrap_or_default();
        let mut out = KeysOutput::default();
        let mut added: BTreeMap<Time, Keyframe> = BTreeMap::new();
        let span = p.keys.iter().map(|c| c.offset).max().unwrap_or(Time::ZERO);
        for mut c in p.keys {
            check_value(tx, &p.path, &p.attr, &c.keyframe.value)?;
            if p.reversed {
                c.offset = span - c.offset;
                swap_sides(&mut c.keyframe);
            }
            let t = p.time.saturating_add(c.offset);
            added.insert(t, c.keyframe);
            out.keys
                .push(KeyRef::new(p.path.clone(), p.attr.clone(), t));
        }
        spec.keyframes.extend(added);
        refresh_auto(&p.attr, &mut spec);
        write_attr(tx, &p.path, &p.attr, spec)?;
        Ok(out)
    }
}

// ---------------------------------------------------------------------------

pub struct EditKeys;

/// One keyframe change of `key.edit`. Omitted fields keep their current value.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct KeyEdit {
    /// The keyframe: its attribute and current time.
    pub key: KeyRef,
    /// New keyframe time in flicks (layer time for layer attributes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<Time>,
    /// New value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// Incoming temporal interpolation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interp_in: Option<Interpolation>,
    /// Outgoing temporal interpolation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interp_out: Option<Interpolation>,
    /// Incoming temporal ease (speed in units per second, influence in percent), one
    /// entry per dimension (one entry for spatial properties such as Position).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ease_in: Option<Vec<Ease>>,
    /// Outgoing temporal ease.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ease_out: Option<Vec<Ease>>,
    /// Temporal continuous: the incoming and outgoing handles stay joined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuous: Option<bool>,
    /// Temporal auto-Bezier: AE computes the eases from the neighbouring keyframes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_bezier: Option<bool>,
    /// Spatial tangents (Position-like properties), relative to the value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spatial_in: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spatial_out: Option<[f64; 3]>,
    /// Spatial continuous: the motion path's incoming and outgoing tangents stay
    /// joined (Continuous Bezier).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spatial_continuous: Option<bool>,
    /// Spatial auto-Bezier: AE computes the tangents from the neighbouring keyframes
    /// (implies continuous).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spatial_auto_bezier: Option<bool>,
    /// Rove across time (spatial properties; not the first or last keyframe).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roving: Option<bool>,
    /// Keyframe label colour (0 = none, 1–16).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<u8>,
}

impl KeyEdit {
    /// An edit of `key` that changes nothing yet.
    pub fn new(key: KeyRef) -> Self {
        KeyEdit {
            key,
            time: None,
            value: None,
            interp_in: None,
            interp_out: None,
            ease_in: None,
            ease_out: None,
            continuous: None,
            auto_bezier: None,
            spatial_in: None,
            spatial_out: None,
            spatial_continuous: None,
            spatial_auto_bezier: None,
            roving: None,
            label: None,
        }
    }

    fn apply(&self, k: &mut Keyframe, dims: usize) {
        if let Some(v) = &self.value {
            k.value = v.clone();
        }
        if let Some(i) = self.interp_in {
            k.interp_in = i;
        }
        if let Some(o) = self.interp_out {
            k.interp_out = o;
        }
        let clamp = |e: &[Ease]| -> Vec<Ease> {
            e.iter()
                .map(|e| Ease {
                    speed: if e.speed.is_finite() { e.speed } else { 0.0 },
                    influence: e.influence.clamp(0.1, 100.0),
                })
                .collect()
        };
        let eased = self.ease_in.is_some() || self.ease_out.is_some();
        if let Some(e) = &self.ease_in {
            k.ease_in = clamp(e);
        }
        if let Some(e) = &self.ease_out {
            k.ease_out = clamp(e);
        }
        // Editing an automatic ease by hand makes it a plain (continuous) Bezier, as
        // dragging an auto-Bezier handle does in AE.
        if eased && self.auto_bezier.is_none() && k.flags.temporal_auto_bezier {
            k.flags.temporal_auto_bezier = false;
            k.flags.temporal_continuous = true;
        }
        if let Some(c) = self.continuous {
            k.flags.temporal_continuous = c;
        }
        match self.auto_bezier {
            Some(true) => {
                k.interp_in = Interpolation::Bezier;
                k.interp_out = Interpolation::Bezier;
                k.flags.temporal_auto_bezier = true;
                k.flags.temporal_continuous = true;
            }
            Some(false) => k.flags.temporal_auto_bezier = false,
            None => {}
        }
        if k.interp_in == Interpolation::Bezier && k.ease_in.is_empty() {
            k.ease_in = vec![Ease::default(); dims];
        }
        if k.interp_out == Interpolation::Bezier && k.ease_out.is_empty() {
            k.ease_out = vec![Ease::default(); dims];
        }
        if let Some(t) = self.spatial_in {
            k.spatial_in = Some(t);
            k.flags.spatial_auto_bezier = false;
        }
        if let Some(t) = self.spatial_out {
            k.spatial_out = Some(t);
            k.flags.spatial_auto_bezier = false;
        }
        if let Some(c) = self.spatial_continuous {
            k.flags.spatial_continuous = c;
        }
        match self.spatial_auto_bezier {
            Some(true) => {
                k.flags.spatial_auto_bezier = true;
                k.flags.spatial_continuous = true;
            }
            Some(false) => k.flags.spatial_auto_bezier = false,
            None => {}
        }
        if let Some(r) = self.roving {
            k.flags.roving = r;
        }
        if let Some(l) = self.label {
            k.label = l.min(16);
        }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct EditKeysParams {
    pub edits: Vec<KeyEdit>,
}

impl Command for EditKeys {
    type Params = EditKeysParams;
    type Output = KeysOutput;
    const ID: &'static str = "key.edit";
    const TITLE: &'static str = "Edit Keyframes";
    const DESCRIPTION: &'static str = "Change keyframes' times, values, temporal interpolation, ease (speed and influence per dimension), continuity, auto-Bezier, spatial tangents, spatial continuity / auto-Bezier, roving or label in one undo step (Graph Editor drags, keyframe dialogs). Keyframes moved onto other keyframes replace them. Returns the keyframes' new references, in input order.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<KeysOutput, CommandError> {
        ensure_unlocked(tx.stage(), p.edits.iter().map(|e| &e.key.path))?;
        let refs: Vec<KeyRef> = p.edits.iter().map(|e| e.key.clone()).collect();
        let mut out = KeysOutput { keys: refs.clone() };
        for ((path, attr), times) in group(&refs) {
            let spec = animated_spec(tx, &path, &attr, &times)?;
            let mine: Vec<(usize, &KeyEdit)> = p
                .edits
                .iter()
                .enumerate()
                .filter(|(_, e)| e.key.path == path && e.key.attr == attr)
                .collect();
            for (_, e) in &mine {
                if let Some(v) = &e.value {
                    check_value(tx, &path, &attr, v)?;
                }
            }
            let edits: Vec<&KeyEdit> = mine.iter().map(|(_, e)| *e).collect();
            let (spec, times) = apply_key_edits(&attr, &spec, &edits);
            for ((i, _), t) in mine.iter().zip(times) {
                out.keys[*i].time = t;
            }
            write_attr(tx, &path, &attr, spec)?;
        }
        Ok(out)
    }
}

/// Apply `edits` to `spec`, the keyframes of `attr`, as `key.edit` does (without
/// checking values), and recompute AE's automatic values. The Graph Editor previews
/// drags with it, so the preview is exactly what the command will store. Edits whose
/// keyframe does not exist are ignored. Returns the new keyframes and each edit's new
/// keyframe time.
pub fn apply_key_edits(attr: &str, spec: &AttrSpec, edits: &[&KeyEdit]) -> (AttrSpec, Vec<Time>) {
    let mut out = spec.clone();
    let mut times: Vec<Time> = edits.iter().map(|e| e.key.time).collect();
    // Take every edited keyframe out first, so keyframes can swap places.
    let mut taken: Vec<(Time, Keyframe, Vec<usize>)> = Vec::new();
    for (i, e) in edits.iter().enumerate() {
        match taken.iter_mut().find(|(t, _, _)| *t == e.key.time) {
            Some(entry) => entry.2.push(i),
            None => {
                if let Some(k) = out.keyframes.remove(&e.key.time) {
                    taken.push((e.key.time, k, vec![i]));
                }
            }
        }
    }
    for (t, mut k, indices) in taken {
        let mut nt = t;
        for i in &indices {
            let e = edits[*i];
            let dims = query::ease_dimensions(attr, spec, &k.value);
            e.apply(&mut k, dims);
            nt = e.time.unwrap_or(nt);
        }
        for i in indices {
            times[i] = nt;
        }
        out.keyframes.insert(nt, k);
    }
    refresh_auto(attr, &mut out);
    (out, times)
}
