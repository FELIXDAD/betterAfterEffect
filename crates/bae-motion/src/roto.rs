//! A seed stroke and its effect are one undoable document edit.
use crate::{
    effect, query,
    util::{ensure_unlocked, fail, set},
};
use bae_app::Registry;
use bae_model::{
    Command, CommandError, EffectDef, PrimPath, Time, Transaction, Value, effect::param_attr,
};
use bae_vision::{FROZEN, ROTO, SPAN, STROKES};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

pub struct Stroke(pub Arc<Registry<EffectDef>>);

#[derive(Deserialize, JsonSchema)]
pub struct Params {
    layer: PrimPath,
    time: Time,
    points: Vec<[f64; 2]>,
    radius: f64,
    background: bool,
}

impl Command for Stroke {
    type Params = Params;
    type Output = PrimPath;
    const ID: &'static str = "roto.stroke";
    const TITLE: &'static str = "Roto Brush Stroke";
    const DESCRIPTION: &'static str = "Mark foreground or background on a source frame.";
    fn run(&self, tx: &mut Transaction<'_>, p: Params) -> Result<PrimPath, CommandError> {
        if !query::is_layer(tx.stage(), &p.layer)
            || p.points.is_empty()
            || !p.radius.is_finite()
            || p.radius <= 0.0
            || p.points.iter().flatten().any(|x| !x.is_finite())
        {
            return Err(fail(
                "A stroke needs a layer, finite points and a positive radius",
            ));
        }
        ensure_unlocked(tx.stage(), std::slice::from_ref(&p.layer))?;
        let existing = effect::effects_of(tx.stage(), &p.layer)
            .into_iter()
            .find(|e| {
                tx.stage().prim(e).is_some_and(|prim| {
                    prim.value("match_name").and_then(Value::as_str) == Some(ROTO)
                })
            });
        let path = match existing {
            Some(path) => path,
            None => {
                if p.background {
                    return Err(fail("Mark foreground first"));
                }
                let def = effect::find_def(&self.0, ROTO)
                    .ok_or_else(|| fail("Roto Brush is unavailable"))?;
                let path = effect::add_instance(tx, &p.layer, &def, None)?;
                let timing = query::layer_timing(tx.stage(), &p.layer);
                let a = timing.layer_time(timing.in_point);
                let b = timing.layer_time(timing.out_point);
                set(
                    tx,
                    &path,
                    &param_attr(SPAN),
                    Value::Vec2([a.min(b).0 as f64, a.max(b).0 as f64]),
                )?;
                path
            }
        };
        let prim = tx.stage().require_prim(&path)?;
        if prim
            .value(&param_attr(FROZEN))
            .and_then(Value::as_prim_ref)
            .is_some()
        {
            return Err(fail("Unfreeze the propagation span before painting"));
        }
        let attr = param_attr(STROKES);
        let mut data = match prim.value(&attr) {
            Some(Value::FloatList(v)) => v.clone(),
            _ => Vec::new(),
        };
        bae_vision::append_stroke(
            &mut data,
            p.time,
            if p.background { -p.radius } else { p.radius },
            &p.points,
        );
        set(tx, &path, &attr, Value::FloatList(data))?;
        Ok(path)
    }
}

pub struct Freeze;
#[derive(Deserialize, JsonSchema)]
pub struct FreezeParams {
    effect: PrimPath,
    /// A completed sequence, or None to resume propagation.
    file: Option<crate::import::ImportFile>,
    #[serde(default)]
    start: Time,
    #[serde(default)]
    duration: Time,
}
impl Command for Freeze {
    type Params = FreezeParams;
    type Output = ();
    const ID: &'static str = "roto.freeze";
    const TITLE: &'static str = "Freeze Roto Brush";
    const FILESYSTEM: bool = true;
    const DESCRIPTION: &'static str = "Use a baked matte sequence for the propagation span.";
    fn run(&self, tx: &mut Transaction<'_>, p: FreezeParams) -> Result<(), CommandError> {
        let prim = tx.stage().require_prim(&p.effect)?;
        if prim.value("match_name").and_then(Value::as_str) != Some(ROTO) {
            return Err(fail("Select a Roto Brush effect"));
        }
        let layer =
            effect::layer_of_effect(&p.effect).ok_or_else(|| fail("Missing source layer"))?;
        ensure_unlocked(tx.stage(), [&layer])?;
        let comp =
            query::owning_comp(tx.stage(), &layer).ok_or_else(|| fail("Missing composition"))?;
        let attr = param_attr(FROZEN);
        let old = prim.value(&attr).and_then(Value::as_prim_ref).cloned();
        if let Some(file) = p.file {
            if old.is_some() || p.duration <= Time::ZERO {
                return Err(fail("Invalid freeze request"));
            }
            let result = crate::import::ImportItems.run(
                tx,
                crate::import::ImportParams {
                    files: vec![file],
                    parent: None,
                    comp: Some(comp),
                },
            )?;
            let footage = result
                .layers
                .first()
                .ok_or_else(|| fail("Missing matte footage"))?;
            for (attr, val) in [
                ("enabled", Value::Bool(false)),
                ("shy", Value::Bool(true)),
                ("start_time", Value::Time(p.start)),
                ("in_point", Value::Time(p.start)),
                ("out_point", Value::Time(p.start + p.duration)),
                ("roto:owner", Value::PrimRef(p.effect.clone())),
            ] {
                set(tx, footage, attr, val)?;
            }
            let timing = query::layer_timing(tx.stage(), &layer);
            set(
                tx,
                &p.effect,
                &param_attr(bae_vision::FROZEN_TIMING),
                Value::Vec3([
                    p.start.0 as f64,
                    timing.layer_time(p.start).0 as f64,
                    timing.stretch_percent(),
                ]),
            )?;
            set(tx, &p.effect, &attr, Value::PrimRef(footage.clone()))?;
        } else if let Some(old) = old {
            if tx.stage().prim(&old).is_some_and(|p2| {
                p2.value("roto:owner").and_then(Value::as_prim_ref) == Some(&p.effect)
            }) {
                effect::delete_all(tx, &[old])?;
            }
            set(tx, &p.effect, &attr, Value::Bool(false))?;
        }
        Ok(())
    }
}
