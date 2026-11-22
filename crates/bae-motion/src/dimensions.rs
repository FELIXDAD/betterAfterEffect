//! Separate Dimensions of Position (`transform.separate_dimensions`, AE-ANI-052):
//! Position becomes X / Y (/ Z) Position, each with its own keyframes, or the
//! components are recombined into one spatial Position.
//!
//! The model keeps both forms (`transform:position` and `transform:position_x/y/z`)
//! and `transform:separate_dimensions` says which one is evaluated, as in AE where the
//! "leader" Position stays in the property tree.

use bae_model::{
    AttrSpec, Command, CommandError, Ease, Interpolation, Keyframe, PrimPath, Time, Transaction,
    Value,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::keys::refresh_auto;
use crate::query;
use crate::util::{ensure_unlocked, set, write_attr};

const POSITION: &str = "transform:position";
const SEPARATE: &str = "transform:separate_dimensions";
/// X, Y, Z Position.
pub const COMPONENTS: [&str; 3] = [
    "transform:position_x",
    "transform:position_y",
    "transform:position_z",
];

pub struct SeparateDimensions;

#[derive(Deserialize, JsonSchema)]
pub struct SeparateDimensionsParams {
    pub layers: Vec<PrimPath>,
    /// true: separate Position into X / Y (/ Z) Position; false: recombine. Omitted
    /// toggles each layer.
    #[serde(default)]
    pub separated: Option<bool>,
}

impl Command for SeparateDimensions {
    type Params = SeparateDimensionsParams;
    type Output = ();
    const ID: &'static str = "transform.separate_dimensions";
    const TITLE: &'static str = "Separate Dimensions";
    const DESCRIPTION: &'static str = "Animation > Separate Dimensions: split a layer's Position into X, Y (and Z for 3D layers) Position with their own keyframes (temporal types and speeds are carried over per component), or recombine them into one spatial Position (keyframes at every component key time).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        for layer in &p.layers {
            tx.stage().require_prim(layer)?;
            let now = is_separated(tx.stage(), layer);
            let want = p.separated.unwrap_or(!now);
            if want == now {
                continue;
            }
            if want {
                separate(tx, layer)?;
            } else {
                recombine(tx, layer)?;
            }
            set(tx, layer, SEPARATE, Value::Bool(want))?;
        }
        Ok(())
    }
}

/// Whether the layer's Position is separated.
pub fn is_separated(stage: &bae_model::Stage, layer: &PrimPath) -> bool {
    stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, SEPARATE))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn three_d(stage: &bae_model::Stage, layer: &PrimPath) -> bool {
    stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, "three_d"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn vec3(v: &Value) -> [f64; 3] {
    match v {
        Value::Vec3(x) => *x,
        Value::Vec2(x) => [x[0], x[1], 0.0],
        _ => [0.0; 3],
    }
}

fn norm(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = norm(v);
    (n > 1e-9).then(|| v.map(|c| c / n))
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Position → X / Y / Z Position. Each component key keeps the temporal types and
/// influence; its speed is the spatial speed along the path direction at the key.
fn separate(tx: &mut Transaction<'_>, layer: &PrimPath) -> Result<(), CommandError> {
    let stage = tx.stage();
    let spec = query::attr_spec(stage, layer, POSITION)
        .cloned()
        .unwrap_or_default();
    let current = query::sample_attr(stage, layer, POSITION, Time::ZERO)
        .map(|v| vec3(&v))
        .unwrap_or_default();
    let fallback = spec.value.as_ref().map_or(current, vec3);
    let dims = if three_d(stage, layer) { 3 } else { 2 };
    let keys: Vec<(Time, &Keyframe)> = spec.keyframes.iter().map(|(t, k)| (*t, k)).collect();
    let points: Vec<[f64; 3]> = keys.iter().map(|(_, k)| vec3(&k.value)).collect();
    let mut out: [AttrSpec; 3] = Default::default();
    for (d, o) in out.iter_mut().enumerate() {
        o.value = Some(Value::Float(fallback[d]));
    }
    for (i, (t, k)) in keys.iter().enumerate() {
        // Path direction leaving / arriving at the key.
        let dir_out = k
            .spatial_out
            .and_then(unit)
            .or_else(|| points.get(i + 1).and_then(|n| unit(sub(*n, points[i]))));
        let dir_in = k
            .spatial_in
            .map(|v| v.map(|c| -c))
            .and_then(unit)
            .or_else(|| {
                i.checked_sub(1)
                    .and_then(|p| unit(sub(points[i], points[p])))
            });
        let per_dim = |eases: &[Ease], dir: Option<[f64; 3]>, d: usize| -> Vec<Ease> {
            eases
                .first()
                .map(|e| {
                    vec![Ease {
                        speed: e.speed * dir.map_or(0.0, |u| u[d]),
                        influence: e.influence,
                    }]
                })
                .unwrap_or_default()
        };
        for (d, o) in out.iter_mut().enumerate().take(dims) {
            let mut c = Keyframe {
                value: Value::Float(points[i][d]),
                interp_in: k.interp_in,
                interp_out: k.interp_out,
                ease_in: per_dim(&k.ease_in, dir_in, d),
                ease_out: per_dim(&k.ease_out, dir_out, d),
                spatial_in: None,
                spatial_out: None,
                flags: k.flags,
                label: k.label,
            };
            c.flags.spatial_auto_bezier = false;
            c.flags.spatial_continuous = false;
            c.flags.roving = false;
            o.keyframes.insert(*t, c);
        }
    }
    for (d, attr) in COMPONENTS.iter().enumerate() {
        let mut s = std::mem::take(&mut out[d]);
        refresh_auto(attr, &mut s);
        write_attr(tx, layer, attr, s)?;
    }
    Ok(())
}

/// X / Y / Z Position → Position: keyframes at every component key time with the
/// evaluated components; spatial tangents from the component slopes (Hermite → Bezier).
fn recombine(tx: &mut Transaction<'_>, layer: &PrimPath) -> Result<(), CommandError> {
    let stage = tx.stage();
    let specs: Vec<AttrSpec> = COMPONENTS
        .iter()
        .map(|a| {
            query::attr_spec(stage, layer, a)
                .cloned()
                .unwrap_or_default()
        })
        .collect();
    let mut times: Vec<Time> = specs
        .iter()
        .flat_map(|s| s.keyframes.keys().copied())
        .collect();
    times.sort();
    times.dedup();
    let sample = |t: Time| -> [f64; 3] {
        std::array::from_fn(|d| {
            query::sample_attr(stage, layer, COMPONENTS[d], t)
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
        })
    };
    let mut spec = query::attr_spec(stage, layer, POSITION)
        .cloned()
        .unwrap_or_default();
    spec.keyframes.clear();
    spec.value = Some(Value::Vec3(sample(
        times.first().copied().unwrap_or(Time::ZERO),
    )));
    let h = 1e-3;
    for (i, t) in times.iter().enumerate() {
        let p = sample(*t);
        let side = |out: bool| -> Interpolation {
            let interps: Vec<Interpolation> = specs
                .iter()
                .filter_map(|s| s.keyframes.get(t))
                .map(|k| if out { k.interp_out } else { k.interp_in })
                .collect();
            if !interps.is_empty() && interps.iter().all(|i| *i == Interpolation::Hold) {
                Interpolation::Hold
            } else if interps.iter().all(|i| *i == Interpolation::Linear) {
                Interpolation::Linear
            } else {
                Interpolation::Bezier
            }
        };
        let influence = |out: bool| -> f64 {
            specs
                .iter()
                .filter_map(|s| s.keyframes.get(t))
                .filter_map(|k| {
                    if out {
                        k.ease_out.first()
                    } else {
                        k.ease_in.first()
                    }
                })
                .map(|e| e.influence)
                .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))))
                .unwrap_or(Ease::DEFAULT_INFLUENCE)
        };
        let v_out = sub(sample(*t + Time::from_seconds(h)), p).map(|c| c / h);
        let v_in = sub(p, sample(*t - Time::from_seconds(h))).map(|c| c / h);
        let dt_next = times.get(i + 1).map(|n| (*n - *t).as_seconds());
        let dt_prev = i.checked_sub(1).map(|j| (*t - times[j]).as_seconds());
        let (interp_in, interp_out) = (side(false), side(true));
        let mut k = Keyframe {
            interp_in,
            interp_out,
            spatial_in: Some(dt_prev.map_or([0.0; 3], |dt| v_in.map(|c| -c * dt / 3.0))),
            spatial_out: Some(dt_next.map_or([0.0; 3], |dt| v_out.map(|c| c * dt / 3.0))),
            ..Keyframe::linear(Value::Vec3(p))
        };
        if interp_in == Interpolation::Bezier {
            k.ease_in = vec![Ease {
                speed: norm(v_in),
                influence: influence(false),
            }];
        }
        if interp_out == Interpolation::Bezier {
            k.ease_out = vec![Ease {
                speed: norm(v_out),
                influence: influence(true),
            }];
        }
        spec.keyframes.insert(*t, k);
    }
    write_attr(tx, layer, POSITION, spec)
}
