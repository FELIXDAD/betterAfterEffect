//! Bake attachment samples in the coordinate system of their destination.
use super::{key, mode, points, require_tracker, setting};
use crate::{
    dimensions,
    effect::{ApplyEffect, ApplyEffectParams},
    query,
    util::{ensure_unlocked, fail},
};
use bae_anim::{
    Evaluator,
    glam::{DMat4, DVec2, DVec3},
};
use bae_app::Registry;
use bae_model::{
    Command, CommandError, EffectDef, PrimPath, Transaction, Value, effect::param_attr,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

pub struct ApplyTrack(pub Arc<Registry<EffectDef>>);
#[derive(Clone, Copy, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum Axes {
    #[default]
    Xy,
    X,
    Y,
}
impl Axes {
    fn merge(self, v: DVec2, old: DVec2) -> DVec2 {
        match self {
            Self::Xy => v,
            Self::X => DVec2::new(v.x, old.y),
            Self::Y => DVec2::new(old.x, v.y),
        }
    }
}
#[derive(Deserialize, JsonSchema)]
pub struct ApplyParams {
    pub tracker: PrimPath,
    #[serde(default)]
    pub axes: Axes,
}
impl Command for ApplyTrack {
    type Params = ApplyParams;
    type Output = ();
    const ID: &'static str = "tracker.apply";
    const TITLE: &'static str = "Apply Motion Tracker";
    const DESCRIPTION: &'static str =
        "Bake tracking data to a transform, an effect point or Corner Pin.";
    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let tracker = &p.tracker;
        require_tracker(tx.stage(), tracker)?;
        let source =
            query::owning_layer(tx.stage(), tracker).ok_or_else(|| fail("missing source"))?;
        let comp = query::owning_comp(tx.stage(), &source).ok_or_else(|| fail("missing comp"))?;
        let kind = mode(tx.stage(), tracker);
        let stabilize = kind == "stabilize";
        let corner = kind == "parallel" || kind == "perspective";
        let dest = if stabilize {
            source.clone()
        } else {
            let Some(Value::PrimRef(target)) = setting(tx.stage(), tracker, "target") else {
                return Err(fail("select a motion target"));
            };
            target
        };
        let target =
            query::owning_layer(tx.stage(), &dest).ok_or_else(|| fail("missing target layer"))?;
        ensure_unlocked(tx.stage(), [&target])?;
        if query::owning_comp(tx.stage(), &target).as_ref() != Some(&comp) {
            return Err(fail("target is in another composition"));
        }
        let effect_point = dest != target;
        let param = setting(tx.stage(), tracker, "target_property")
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        if effect_point
            && (corner
                || !param.starts_with("param:")
                || !matches!(setting(tx.stage(), &dest, &param), Some(Value::Vec2(_))))
        {
            return Err(fail("select a 2D effect point"));
        }
        let pts = points(tx.stage(), tracker);
        let curves: Vec<_> = pts
            .iter()
            .map(|pt| {
                query::attr_spec(tx.stage(), pt, "attach_point")
                    .ok_or_else(|| fail("missing attach point"))
            })
            .collect::<Result<_, _>>()?;
        let first = curves.first().ok_or_else(|| fail("missing track points"))?;
        let enabled = |k| setting(tx.stage(), tracker, k) == Some(Value::Bool(true));
        let (position, rotation, scale) =
            (enabled("position"), enabled("rotation"), enabled("scale"));
        let separated = dimensions::is_separated(tx.stage(), &target);
        let mut corners = Vec::new();
        if corner {
            let def = self
                .0
                .snapshot()
                .iter()
                .find(|e| e.item().match_name == "ADBE Corner Pin")
                .map(|e| e.item().clone())
                .ok_or_else(|| fail("Corner Pin is unavailable"))?;
            for id in 1..=4 {
                let default = def
                    .params
                    .iter()
                    .find(|p| p.id == id)
                    .and_then(|p| crate::effect::instance_default(tx.stage(), &target, p));
                let Some(Value::Vec2(old)) = default else {
                    return Err(fail("missing corner default"));
                };
                corners.push(DVec2::from(old));
            }
        }
        let mut eval = Evaluator::new();
        let mut writes = Vec::new();
        let mut reference = None;
        let mut previous_angle = 0.0;
        let mut turns = 0.0;
        for &time in first.keyframes.keys() {
            let Some(mut coords) = curves
                .iter()
                .map(|c| match &c.keyframes.get(&time)?.value {
                    Value::Vec2(v) => Some(DVec2::from(*v)),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let ct = query::to_comp_time(tx.stage(), tracker, time);
            let dt = query::to_key_time(tx.stage(), &target, ct);
            let frame = eval
                .evaluate_comp(tx.stage(), &comp, ct)
                .map_err(|e| fail(e.to_string()))?;
            let dst = frame.layer(&target).ok_or_else(|| fail("missing target"))?;
            let src = frame.layer(&source).ok_or_else(|| fail("missing source"))?;
            let matrix = if stabilize {
                DMat4::IDENTITY
            } else {
                let parent = if corner || effect_point {
                    dst.world_matrix
                } else {
                    dst.parent
                        .map_or(DMat4::IDENTITY, |i| frame.layers[i].world_matrix)
                };
                parent.inverse() * src.world_matrix
            };
            for xy in &mut coords {
                *xy = matrix.transform_point3(xy.extend(0.0)).truncate();
                if !xy.is_finite() {
                    return Err(fail("motion target has a singular transform"));
                }
            }
            if corner {
                if coords.len() == 3 {
                    coords.push(coords[1] + coords[2] - coords[0]);
                }
                for (i, xy) in coords.iter().enumerate() {
                    let attr = param_attr(i as u32 + 1);
                    writes.push((
                        dt,
                        attr,
                        Value::Vec2(p.axes.merge(*xy, corners[i]).to_array()),
                    ));
                }
                continue;
            }
            let xy = coords[0];
            if effect_point {
                let old = eval.evaluate_attribute(tx.stage(), &dest, &param, ct).ok();
                let Some(Value::Vec2(old)) = old else {
                    return Err(fail("unavailable effect point"));
                };
                writes.push((
                    dt,
                    param.clone(),
                    Value::Vec2(p.axes.merge(xy, old.into()).to_array()),
                ));
                continue;
            }
            let delta = coords.get(1).map_or(DVec2::X, |b| *b - xy);
            let (base_xy, base_axis, base_transform) =
                *reference.get_or_insert((xy, delta, dst.transform));
            if (scale || rotation) && (base_axis.length() < 1e-6 || delta.length() < 1e-6) {
                return Err(fail("track points coincide"));
            }
            if position {
                let (name, value, old) = if stabilize {
                    (
                        "transform:anchor_point",
                        base_transform.anchor_point.truncate() + xy - base_xy,
                        dst.transform.anchor_point,
                    )
                } else {
                    ("transform:position", xy, dst.transform.position)
                };
                let v = p.axes.merge(value, old.truncate());
                if !stabilize && separated {
                    if !matches!(p.axes, Axes::Y) {
                        writes.push((dt, dimensions::COMPONENTS[0].into(), Value::Float(v.x)));
                    }
                    if !matches!(p.axes, Axes::X) {
                        writes.push((dt, dimensions::COMPONENTS[1].into(), Value::Float(v.y)));
                    }
                } else {
                    writes.push((dt, name.into(), Value::Vec3(v.extend(old.z).to_array())));
                }
            }
            if rotation {
                let angle = delta.y.atan2(delta.x).to_degrees()
                    - base_axis.y.atan2(base_axis.x).to_degrees();
                let change = angle - previous_angle;
                turns += (change + 180.0).rem_euclid(360.0) - 180.0;
                previous_angle = angle;
                writes.push((
                    dt,
                    "transform:rotation".into(),
                    Value::Float(
                        base_transform.rotation.z + if stabilize { -turns } else { turns },
                    ),
                ));
            }
            if scale {
                let ratio = delta.length() / base_axis.length();
                let v =
                    base_transform.scale.truncate() * if stabilize { 1.0 / ratio } else { ratio };
                let v = p.axes.merge(v, dst.transform.scale.truncate());
                writes.push((
                    dt,
                    "transform:scale".into(),
                    Value::Vec3(DVec3::new(v.x, v.y, dst.transform.scale.z).to_array()),
                ));
            }
        }
        if writes.is_empty() {
            return Err(fail("no tracking keyframes to apply"));
        }
        let dest = if corner {
            ApplyEffect::new(self.0.clone())
                .run(
                    tx,
                    ApplyEffectParams {
                        layers: vec![target],
                        match_name: "ADBE Corner Pin".into(),
                        index: None,
                    },
                )?
                .effects
                .remove(0)
        } else {
            dest
        };
        for (time, attr, val) in writes {
            key(tx, &dest, &attr, time, val)?;
        }
        Ok(())
    }
}
