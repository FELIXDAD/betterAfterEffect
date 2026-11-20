//! AE's automatic keyframe values, for editing commands.
//!
//! Evaluation uses the values stored in each [`Keyframe`]. When keyframes change
//! (added, moved, values edited), AE recomputes some of those values itself; commands
//! that edit keyframes (12's graph editor, agents) call [`update_auto_values`] so the
//! stored data stays what AE would have stored. All rules were measured in AE 26.2.1
//! (`tests/fixtures/ae-26.2-auto.json`):
//!
//! - **Temporal auto-Bezier** (AE-ANI-040): an inner key's in and out speed are the
//!   slope between its neighbours, `(v[i+1] − v[i−1]) / (t[i+1] − t[i−1])`, per
//!   dimension; the first key's out speed and the last key's in speed are the slope of
//!   their segment; influence 16.67 %. Local extrema are not flattened.
//! - **Spatial auto-Bezier** (AE-ANI-042): tangents `±(P[i+1] − P[i−1]) / 6`, using the
//!   key itself for a missing neighbour; independent of key times and distances.
//! - **Roving keys** (AE-ANI-051): re-timed so that the span between the surrounding
//!   non-roving keys is one ease over the total arc length — a roving key's time is
//!   when that ease reaches the key's cumulative distance.

use std::collections::BTreeMap;

use bae_model::{Ease, Interpolation, Keyframe, Time, Value};

use crate::curve::{Curve, CurveKind};

/// Influence AE gives automatic eases.
const AUTO_INFLUENCE: f64 = 100.0 / 6.0;

/// Recompute AE's automatic values of `keys`: temporal auto-Bezier eases, spatial
/// auto-Bezier tangents (spatial curves) and roving key times (spatial curves). Returns
/// the updated keyframes (roving keys may move).
pub fn update_auto_values(
    keys: &BTreeMap<Time, Keyframe>,
    kind: CurveKind,
) -> BTreeMap<Time, Keyframe> {
    let mut out = keys.clone();
    if kind == CurveKind::Spatial {
        auto_bezier_spatial(&mut out);
        out = rove(&out);
    } else {
        auto_bezier_temporal(&mut out);
    }
    out
}

fn comps(v: &Value) -> Option<Vec<f64>> {
    Some(match v {
        Value::Float(x) => vec![*x],
        Value::Vec2(x) => x.to_vec(),
        Value::Vec3(x) => x.to_vec(),
        Value::Color(x) => x.to_vec(),
        Value::Time(t) => vec![t.as_seconds()],
        _ => return None,
    })
}

/// Temporal auto-Bezier eases of non-spatial curves (see the module docs).
pub fn auto_bezier_temporal(keys: &mut BTreeMap<Time, Keyframe>) {
    let list: Vec<(f64, Vec<f64>)> = keys
        .iter()
        .filter_map(|(t, k)| Some((t.as_seconds(), comps(&k.value)?)))
        .collect();
    if list.len() != keys.len() || list.len() < 2 {
        return;
    }
    let n = list.len();
    let dims = list[0].1.len();
    let slope = |a: usize, b: usize, d: usize| {
        let dt = list[b].0 - list[a].0;
        if dt > 0.0 {
            (list[b].1.get(d).copied().unwrap_or(0.0) - list[a].1.get(d).copied().unwrap_or(0.0))
                / dt
        } else {
            0.0
        }
    };
    // Colours share one ease (AE-ANI-034); its speed is a distance, left to the user.
    let shared = matches!(
        keys.values().next().map(|k| &k.value),
        Some(Value::Color(_))
    );
    for (i, k) in keys.values_mut().enumerate() {
        if !k.flags.temporal_auto_bezier || shared {
            continue;
        }
        let ease = |speed: f64| Ease {
            speed,
            influence: AUTO_INFLUENCE,
        };
        let (inn, out): (Vec<Ease>, Vec<Ease>) = (0..dims)
            .map(|d| {
                if i == 0 {
                    (ease(0.0), ease(slope(0, 1, d)))
                } else if i + 1 == n {
                    (ease(slope(n - 2, n - 1, d)), ease(0.0))
                } else {
                    let s = slope(i - 1, i + 1, d);
                    (ease(s), ease(s))
                }
            })
            .unzip();
        k.interp_in = Interpolation::Bezier;
        k.interp_out = Interpolation::Bezier;
        k.ease_in = inn;
        k.ease_out = out;
    }
}

/// Spatial auto-Bezier tangents (see the module docs).
pub fn auto_bezier_spatial(keys: &mut BTreeMap<Time, Keyframe>) {
    let points: Vec<[f64; 3]> = keys
        .values()
        .map(|k| match &k.value {
            Value::Vec2(v) => [v[0], v[1], 0.0],
            Value::Vec3(v) => *v,
            _ => [0.0; 3],
        })
        .collect();
    let n = points.len();
    for (i, k) in keys.values_mut().enumerate() {
        if !k.flags.spatial_auto_bezier {
            continue;
        }
        let prev = points[i.saturating_sub(1)];
        let next = points[(i + 1).min(n - 1)];
        let t: [f64; 3] = std::array::from_fn(|d| (next[d] - prev[d]) / 6.0);
        k.spatial_out = Some(t);
        k.spatial_in = Some(t.map(|v| -v));
    }
}

/// Re-time roving keys (see the module docs). Non-spatial or rove-free keyframes are
/// returned unchanged.
pub fn rove(keys: &BTreeMap<Time, Keyframe>) -> BTreeMap<Time, Keyframe> {
    if !keys.values().any(|k| k.flags.roving) {
        return keys.clone();
    }
    let Some(curve) = Curve::new(keys, CurveKind::Spatial) else {
        return keys.clone();
    };
    let times = curve.roving_times();
    keys.values()
        .zip(times)
        .map(|(k, t)| (t, k.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::KeyframeFlags;

    #[test]
    fn temporal_auto_bezier_unequal_spacing() {
        // AE 26.2.1: rotation keys (0 s, 0), (0.5, 100), (2, 30), (2.5, 200), (5, −50).
        let mut keys: BTreeMap<Time, Keyframe> = [
            (0.0, 0.0),
            (0.5, 100.0),
            (2.0, 30.0),
            (2.5, 200.0),
            (5.0, -50.0),
        ]
        .into_iter()
        .map(|(t, v)| {
            (
                Time::from_seconds(t),
                Keyframe {
                    flags: KeyframeFlags {
                        temporal_auto_bezier: true,
                        temporal_continuous: true,
                        ..Default::default()
                    },
                    ..Keyframe::linear(v)
                },
            )
        })
        .collect();
        auto_bezier_temporal(&mut keys);
        let speeds: Vec<(f64, f64)> = keys
            .values()
            .map(|k| (k.ease_in[0].speed, k.ease_out[0].speed))
            .collect();
        let want = [
            (0.0, 200.0),
            (15.0, 15.0),
            (50.0, 50.0),
            (-80.0 / 3.0, -80.0 / 3.0),
            (-100.0, 0.0),
        ];
        for (g, w) in speeds.iter().zip(want) {
            assert!(
                (g.0 - w.0).abs() < 1e-9 && (g.1 - w.1).abs() < 1e-9,
                "{g:?} {w:?}"
            );
        }
    }
}
