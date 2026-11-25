//! Effect commands (AE: the Effect menu, the Effects & Presets panel, the
//! Effect Controls panel and the layer's Effects group in the timeline).
//!
//! An effect instance is an `Effect` prim under the layer's `effects` folder,
//! in stack order (top = applied first, AE). Its identity is `match_name` (the
//! AE matchName for AE effects); `name` is the instance's display name (AE:
//! "Gaussian Blur", "Gaussian Blur 2" …); parameter values are `param:NNNN`
//! attributes (see `bae_model::effect`), written with their defaults when the
//! effect is applied, so they animate and show like any other property.
//! Parameter edits use the generic value / keyframe commands (`attr.set_at`,
//! `attr.set_animated` …).
//!
//! [`ApplyEffect`] and [`ResetEffects`] read parameter definitions from the
//! app's `Registry<EffectDef>` (built-in and plugin effects), so they are
//! constructed with it ([`effect_commands`]).

use std::sync::Arc;

use bae_app::Registry;
use bae_model::effect::param_attr;
use bae_model::{
    AttrSpec, CommandError, CommandRegistry, DynCommand, EffectDef, Keyframe, Op, PrimPath,
    PrimType, Stage, Time, Transaction, Value,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query;
use crate::util::{child, composed_tree, create, ensure_unlocked, fail, set, write_attr};

/// Name of a layer's effect folder.
pub const EFFECTS_FOLDER: &str = "effects";

/// The effect instances of `layer`, in stack order (top first).
pub fn effects_of(stage: &Stage, layer: &PrimPath) -> Vec<PrimPath> {
    let Ok(folder) = layer.child(EFFECTS_FOLDER) else {
        return Vec::new();
    };
    stage
        .children(&folder)
        .into_iter()
        .filter(|p| stage.prim(p).is_some_and(|v| v.is_a(&PrimType::EFFECT)))
        .collect()
}

/// The layer an effect instance belongs to.
pub fn layer_of_effect(effect: &PrimPath) -> Option<PrimPath> {
    let folder = effect.parent()?;
    (folder.name() == EFFECTS_FOLDER).then(|| folder.parent())?
}

/// The definition of `match_name` in `defs`.
fn find_def(defs: &Registry<EffectDef>, match_name: &str) -> Option<Arc<EffectDef>> {
    defs.snapshot()
        .iter()
        .find(|e| e.item().match_name == match_name)
        .map(|e| e.item().clone())
}

fn require_effect(stage: &Stage, effect: &PrimPath) -> Result<PrimPath, CommandError> {
    stage.require_prim(effect)?;
    if !stage
        .prim(effect)
        .is_some_and(|v| v.is_a(&PrimType::EFFECT))
    {
        return Err(fail(format!("{effect} is not an effect")));
    }
    layer_of_effect(effect).ok_or_else(|| fail(format!("{effect} is not in a layer's effects")))
}

/// The display name for a new instance of `base` on `layer`: AE numbers
/// repeated effects ("Gaussian Blur", "Gaussian Blur 2" …).
fn instance_name(stage: &Stage, layer: &PrimPath, base: &str) -> String {
    let taken: Vec<String> = effects_of(stage, layer)
        .iter()
        .map(|e| query::display_name_of(stage, e))
        .collect();
    if !taken.iter().any(|n| n == base) {
        return base.to_owned();
    }
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| !taken.contains(n))
        .expect("unbounded")
}

/// Paths of new effect instances.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct EffectsOutput {
    pub effects: Vec<PrimPath>,
}

// ---------------------------------------------------------------------------

pub struct ApplyEffect {
    defs: Arc<Registry<EffectDef>>,
}

impl ApplyEffect {
    /// The command, reading definitions from `defs`.
    pub fn new(defs: Arc<Registry<EffectDef>>) -> Self {
        Self { defs }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct ApplyEffectParams {
    /// Layers to add the effect to (AE applies it to every selected layer).
    pub layers: Vec<PrimPath>,
    /// Effect matchName, e.g. `ADBE Gaussian Blur 2` (built-in effects use AE's
    /// matchNames; plugin effects `<plugin-id>/<effect>`).
    pub match_name: String,
    /// Position in each layer's effect stack, 0 = top (applied first); none =
    /// at the bottom (AE: Effect menu and double-click in Effects & Presets).
    #[serde(default)]
    pub index: Option<usize>,
}

impl bae_model::Command for ApplyEffect {
    type Params = ApplyEffectParams;
    type Output = EffectsOutput;
    const ID: &'static str = "effect.apply";
    const TITLE: &'static str = "Apply Effect";
    const DESCRIPTION: &'static str = "Add an effect to layers (AE: Effect menu, Effects & Presets). Every parameter starts at its default; the effect goes to the bottom of each layer's stack unless `index` is given.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<EffectsOutput, CommandError> {
        if p.layers.is_empty() {
            return Err(fail("no layers given"));
        }
        let def = find_def(&self.defs, &p.match_name)
            .ok_or_else(|| fail(format!("unknown effect `{}`", p.match_name)))?;
        ensure_unlocked(tx.stage(), &p.layers)?;
        let mut out = Vec::new();
        for layer in &p.layers {
            if !query::is_layer(tx.stage(), layer) {
                return Err(fail(format!("{layer} is not a layer")));
            }
            out.push(add_instance(tx, layer, &def, p.index)?);
        }
        Ok(EffectsOutput { effects: out })
    }
}

// Built-in AE point defaults were measured on a 640 × 360 reference layer.
fn instance_default(
    stage: &Stage,
    layer: &PrimPath,
    def: &EffectDef,
    param: &bae_model::EffectParamDef,
) -> Option<Value> {
    let relative = matches!(
        param.match_name.as_deref(),
        Some("ADBE Radial Blur-0002" | "ADBE Tile-0001" | "ADBE Radial Shadow-0003")
    ) || (def.plugin.is_none()
        && (def.category == "Distort" || def.match_name == "ADBE Geometry"));
    if let (true, Some(Value::Vec2(reference))) = (relative, &param.default) {
        let source_size = stage
            .prim(layer)
            .and_then(|v| {
                crate::layer::source_size_and_duration(stage, v.value("source")?.as_prim_ref()?)
            })
            .map(|(size, _)| size)
            .filter(|[w, h]| *w > 0. && *h > 0.);
        let size = source_size.or_else(|| {
            let comp = query::comp_info(stage, &query::owning_comp(stage, layer)?)?;
            Some([f64::from(comp.width), f64::from(comp.height)])
        });
        if let Some(size) = size {
            return Some(Value::Vec2([
                size[0] * reference[0] / 640.,
                size[1] * reference[1] / 360.,
            ]));
        }
    }
    param.default.clone()
}

fn add_instance(
    tx: &mut Transaction<'_>,
    layer: &PrimPath,
    def: &EffectDef,
    index: Option<usize>,
) -> Result<PrimPath, CommandError> {
    let folder = child(layer, EFFECTS_FOLDER)?;
    if !tx.stage().has_prim(&folder) {
        create(tx, &folder, PrimType::FOLDER, None)?;
    }
    let display = instance_name(tx.stage(), layer, &def.name);
    let name = query::unique_child_name(tx.stage(), &folder, &def.name);
    let path = child(&folder, &name)?;
    let count = effects_of(tx.stage(), layer).len();
    create(tx, &path, PrimType::EFFECT, index.map(|i| i.min(count)))?;
    set(
        tx,
        &path,
        "match_name",
        Value::String(def.match_name.clone()),
    )?;
    set(tx, &path, "name", Value::String(display))?;
    if let Some(plugin) = &def.plugin {
        set(tx, &path, "plugin:id", Value::String(plugin.id.clone()))?;
        set(
            tx,
            &path,
            "plugin:version",
            Value::String(plugin.version.clone()),
        )?;
    }
    for param in &def.params {
        if param.kind.value_type().is_none() {
            continue;
        }
        if let Some(value) = instance_default(tx.stage(), layer, def, param) {
            set(tx, &path, &param_attr(param.id), value)?;
        }
    }
    Ok(path)
}

// ---------------------------------------------------------------------------

pub struct RemoveEffects;

#[derive(Deserialize, JsonSchema)]
pub struct RemoveEffectsParams {
    /// Effect instances to delete (AE: select effects, Delete).
    pub effects: Vec<PrimPath>,
}

impl bae_model::Command for RemoveEffects {
    type Params = RemoveEffectsParams;
    type Output = ();
    const ID: &'static str = "effect.remove";
    const TITLE: &'static str = "Remove Effect";
    const DESCRIPTION: &'static str =
        "Delete effect instances (with their keyframes and expressions).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for e in &p.effects {
            let layer = require_effect(tx.stage(), e)?;
            ensure_unlocked(tx.stage(), [&layer])?;
        }
        delete_all(tx, &p.effects)
    }
}

/// Deletes `paths` in every sublayer that has a spec for them.
fn delete_all(tx: &mut Transaction<'_>, paths: &[PrimPath]) -> Result<(), CommandError> {
    for path in paths {
        let stack: Vec<_> = tx.stage().layer_stack().to_vec();
        for id in stack {
            if tx.stage().sublayer(&id).is_some_and(|s| s.has_spec(path)) {
                tx.apply_to(&id, Op::DeletePrim { path: path.clone() })?;
            }
        }
    }
    Ok(())
}

pub struct RemoveAllEffects;

#[derive(Deserialize, JsonSchema)]
pub struct RemoveAllEffectsParams {
    /// Layers whose effects are all deleted.
    pub layers: Vec<PrimPath>,
}

impl bae_model::Command for RemoveAllEffects {
    type Params = RemoveAllEffectsParams;
    type Output = ();
    const ID: &'static str = "effect.remove_all";
    const TITLE: &'static str = "Remove All";
    const DESCRIPTION: &'static str =
        "Delete every effect of the layers (AE: Effect > Remove All).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        let mut all = Vec::new();
        for layer in &p.layers {
            all.extend(effects_of(tx.stage(), layer));
        }
        delete_all(tx, &all)
    }
}

// ---------------------------------------------------------------------------

pub struct ReorderEffects;

#[derive(Deserialize, JsonSchema)]
pub struct ReorderEffectsParams {
    /// Effects of one layer to move; they keep their relative order.
    pub effects: Vec<PrimPath>,
    /// Where the first moved effect ends up, 0 = top of the stack.
    pub index: usize,
}

impl bae_model::Command for ReorderEffects {
    type Params = ReorderEffectsParams;
    type Output = ();
    const ID: &'static str = "effect.reorder";
    const TITLE: &'static str = "Reorder Effects";
    const DESCRIPTION: &'static str = "Move effects within their layer's stack (AE: drag effects in Effect Controls or the timeline). Effects render top to bottom.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let mut layer: Option<PrimPath> = None;
        for e in &p.effects {
            let l = require_effect(tx.stage(), e)?;
            match &layer {
                Some(prev) if prev != &l => {
                    return Err(fail("effects must belong to the same layer"));
                }
                _ => layer = Some(l),
            }
        }
        let layer = layer.ok_or_else(|| fail("no effects given"))?;
        ensure_unlocked(tx.stage(), [&layer])?;
        let mut order: Vec<PrimPath> = effects_of(tx.stage(), &layer)
            .into_iter()
            .filter(|e| !p.effects.contains(e))
            .collect();
        let moved: Vec<PrimPath> = effects_of(tx.stage(), &layer)
            .into_iter()
            .filter(|e| p.effects.contains(e))
            .collect();
        let at = p.index.min(order.len());
        order.splice(at..at, moved);
        crate::layer::apply_order(tx, &child(&layer, EFFECTS_FOLDER)?, &order)
    }
}

// ---------------------------------------------------------------------------

pub struct SetEffectsEnabled;

#[derive(Deserialize, JsonSchema)]
pub struct SetEffectsEnabledParams {
    /// Effect instances.
    pub effects: Vec<PrimPath>,
    /// On (the effect renders) or off (AE: the fx switch next to the effect's
    /// name in Effect Controls or the timeline).
    pub enabled: bool,
}

impl bae_model::Command for SetEffectsEnabled {
    type Params = SetEffectsEnabledParams;
    type Output = ();
    const ID: &'static str = "effect.set_enabled";
    const TITLE: &'static str = "Effect On/Off";
    const DESCRIPTION: &'static str =
        "Turn effect instances on or off (AE: the fx switch of an effect).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for e in &p.effects {
            let layer = require_effect(tx.stage(), e)?;
            ensure_unlocked(tx.stage(), [&layer])?;
            set(tx, e, "enabled", Value::Bool(p.enabled))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct ResetEffects {
    defs: Arc<Registry<EffectDef>>,
}

impl ResetEffects {
    /// The command, reading defaults from `defs`.
    pub fn new(defs: Arc<Registry<EffectDef>>) -> Self {
        Self { defs }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct ResetEffectsParams {
    /// Effect instances.
    pub effects: Vec<PrimPath>,
    /// Layer time (flicks) for parameters that have keyframes: they get a
    /// keyframe with the default value there (AE); others become static.
    #[serde(default)]
    pub time: Time,
}

impl bae_model::Command for ResetEffects {
    type Params = ResetEffectsParams;
    type Output = ();
    const ID: &'static str = "effect.reset";
    const TITLE: &'static str = "Reset";
    const DESCRIPTION: &'static str = "Set every parameter of effect instances back to its default (AE: Reset in Effect Controls). Keyframed parameters get a default keyframe at `time`.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        for e in &p.effects {
            let layer = require_effect(tx.stage(), e)?;
            ensure_unlocked(tx.stage(), [&layer])?;
            let match_name = tx
                .stage()
                .prim(e)
                .and_then(|v| v.attr("match_name").and_then(|a| a.value.clone()))
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            let def = find_def(&self.defs, &match_name)
                .ok_or_else(|| fail(format!("unknown effect `{match_name}`")))?;
            for param in &def.params {
                let Some(default) = instance_default(tx.stage(), &layer, &def, param) else {
                    continue;
                };
                let attr = param_attr(param.id);
                let animated = tx
                    .stage()
                    .prim(e)
                    .and_then(|v| v.attr(&attr).cloned())
                    .is_some_and(|a| a.is_animated());
                if animated {
                    tx.ensure_spec(e)?;
                    tx.apply(Op::SetKeyframe {
                        path: e.clone(),
                        attr,
                        time: p.time,
                        keyframe: Keyframe::linear(default),
                    })?;
                } else {
                    write_attr(tx, e, &attr, AttrSpec::with_value(default))?;
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct DuplicateEffects;

#[derive(Deserialize, JsonSchema)]
pub struct DuplicateEffectsParams {
    /// Effect instances; each copy goes directly below its original.
    pub effects: Vec<PrimPath>,
}

impl bae_model::Command for DuplicateEffects {
    type Params = DuplicateEffectsParams;
    type Output = EffectsOutput;
    const ID: &'static str = "effect.duplicate";
    const TITLE: &'static str = "Duplicate";
    const DESCRIPTION: &'static str = "Duplicate effect instances with their values, keyframes and expressions (AE: Edit > Duplicate with effects selected).";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<EffectsOutput, CommandError> {
        let mut out = Vec::new();
        for e in &p.effects {
            let layer = require_effect(tx.stage(), e)?;
            ensure_unlocked(tx.stage(), [&layer])?;
            let folder = child(&layer, EFFECTS_FOLDER)?;
            let base = query::display_name_of(tx.stage(), e);
            let base = base_name(&base);
            let display = instance_name(tx.stage(), &layer, base);
            let name = query::unique_child_name(tx.stage(), &folder, base);
            let path = child(&folder, &name)?;
            let index = effects_of(tx.stage(), &layer)
                .iter()
                .position(|x| x == e)
                .map(|i| i + 1);
            let tree = composed_tree(tx.stage(), e);
            tx.ensure_spec(&folder)?;
            tx.apply(Op::RestorePrim {
                path: path.clone(),
                tree,
                index,
            })?;
            set(tx, &path, "name", Value::String(display))?;
            out.push(path);
        }
        Ok(EffectsOutput { effects: out })
    }
}

/// "Gaussian Blur 2" → "Gaussian Blur" (AE numbers copies from the base name).
fn base_name(name: &str) -> &str {
    match name.rsplit_once(' ') {
        Some((base, n)) if !base.is_empty() && n.parse::<u32>().is_ok() => base,
        _ => name,
    }
}

// ---------------------------------------------------------------------------

/// The effect commands, reading definitions from `defs` (the app's
/// `Registry<EffectDef>`).
pub fn effect_commands(defs: Arc<Registry<EffectDef>>) -> Vec<Arc<dyn DynCommand>> {
    let mut r = CommandRegistry::new();
    register_effect_commands(&mut r, defs);
    let ids: Vec<String> = r.infos().map(|i| i.id.clone()).collect();
    ids.iter().filter_map(|id| r.get(id).cloned()).collect()
}

/// Registers the effect commands into `r`.
pub fn register_effect_commands(r: &mut CommandRegistry, defs: Arc<Registry<EffectDef>>) {
    let results = [
        r.register(ApplyEffect::new(defs.clone())),
        r.register(RemoveEffects),
        r.register(RemoveAllEffects),
        r.register(ReorderEffects),
        r.register(SetEffectsEnabled),
        r.register(ResetEffects::new(defs)),
        r.register(DuplicateEffects),
    ];
    for res in results {
        res.expect("effect command IDs are unique");
    }
}
