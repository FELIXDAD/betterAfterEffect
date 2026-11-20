//! `bae_anim::edit` (AE's automatic keyframe values) compared with AE 26.2.1.
//!
//! `tests/fixtures/ae-26.2-auto.json` is written by `tests/fixtures/ae-auto.jsx`
//! (run with `tools/ae/aectl run`): temporal and spatial auto-Bezier keys with unequal
//! spacing, and the same path with roving inner keys. The keyframe-case fixture adds
//! AE's equal-spacing cases (`temporal_auto_bezier`, `spatial_auto_bezier`,
//! `spatial_roving`).

use std::collections::BTreeMap;

use bae_anim::CurveKind;
use bae_anim::edit::{auto_bezier_spatial, auto_bezier_temporal, update_auto_values};
use bae_model::{AttrSpec, Ease, Interpolation, Keyframe, KeyframeFlags, Time, Value};
use serde::Deserialize;

#[derive(Deserialize)]
struct Auto {
    /// `[t, value, in speed, in influence, out speed, out influence]`
    temporal: Vec<[f64; 6]>,
    /// `[t, value, in tangent, out tangent]`
    spatial: Vec<SpatialKey>,
    /// `[t, value, in tangent, out tangent, out speed, out infl, in speed, in infl, roving]`
    roving: Vec<RovingKey>,
}

type SpatialKey = (f64, [f64; 3], [f64; 3], [f64; 3]);
type RovingKey = (f64, [f64; 3], [f64; 3], [f64; 3], f64, f64, f64, f64, bool);

fn auto() -> Auto {
    serde_json::from_str(include_str!("fixtures/ae-26.2-auto.json")).unwrap()
}

#[test]
fn temporal_auto_bezier_matches_ae() {
    let a = auto();
    let mut keys: BTreeMap<Time, Keyframe> = a
        .temporal
        .iter()
        .map(|k| {
            (
                Time::from_seconds(k[0]),
                Keyframe {
                    flags: KeyframeFlags {
                        temporal_auto_bezier: true,
                        temporal_continuous: true,
                        ..Default::default()
                    },
                    ..Keyframe::linear(k[1])
                },
            )
        })
        .collect();
    auto_bezier_temporal(&mut keys);
    for (k, want) in keys.values().zip(&a.temporal) {
        assert!(
            (k.ease_in[0].speed - want[2]).abs() < 1e-9,
            "{:?} {want:?}",
            k.ease_in
        );
        assert!((k.ease_in[0].influence - want[3]).abs() < 1e-6);
        assert!(
            (k.ease_out[0].speed - want[4]).abs() < 1e-9,
            "{:?} {want:?}",
            k.ease_out
        );
        assert!((k.ease_out[0].influence - want[5]).abs() < 1e-6);
    }
}

#[test]
fn spatial_auto_bezier_matches_ae() {
    let a = auto();
    let mut keys: BTreeMap<Time, Keyframe> = a
        .spatial
        .iter()
        .map(|(t, v, _, _)| {
            (
                Time::from_seconds(*t),
                Keyframe {
                    flags: KeyframeFlags {
                        spatial_auto_bezier: true,
                        spatial_continuous: true,
                        ..Default::default()
                    },
                    ..Keyframe::linear(Value::Vec3(*v))
                },
            )
        })
        .collect();
    auto_bezier_spatial(&mut keys);
    for (k, (_, _, tin, tout)) in keys.values().zip(&a.spatial) {
        let (gi, go) = (k.spatial_in.unwrap(), k.spatial_out.unwrap());
        for d in 0..3 {
            // AE stores tangents as f32.
            assert!(
                (gi[d] - tin[d]).abs() < 1e-4 && (go[d] - tout[d]).abs() < 1e-4,
                "{gi:?} {tin:?}"
            );
        }
    }
}

#[test]
fn roving_times_match_ae() {
    let a = auto();
    let keys: BTreeMap<Time, Keyframe> = a
        .roving
        .iter()
        .enumerate()
        .map(|(i, (_, v, tin, tout, so, io, si, ii, roving))| {
            // Feed equally spaced times: the roving ones must be recomputed.
            let t = Time::from_seconds(i as f64 * 5.0 / (a.roving.len() - 1) as f64);
            let k = Keyframe {
                interp_in: Interpolation::Bezier,
                interp_out: Interpolation::Bezier,
                ease_in: vec![Ease {
                    speed: *si,
                    influence: *ii,
                }],
                ease_out: vec![Ease {
                    speed: *so,
                    influence: *io,
                }],
                spatial_in: Some(*tin),
                spatial_out: Some(*tout),
                flags: KeyframeFlags {
                    roving: *roving,
                    ..Default::default()
                },
                ..Keyframe::linear(Value::Vec3(*v))
            };
            (t, k)
        })
        .collect();
    let out = update_auto_values(&keys, CurveKind::Spatial);
    for (got, want) in out.keys().zip(&a.roving) {
        // AE's own arc lengths differ by ~0.01 px (see ae_reference.rs).
        assert!(
            (got.as_seconds() - want.0).abs() < 1e-4,
            "{} vs {}",
            got.as_seconds(),
            want.0
        );
    }
}

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    attr: AttrSpec,
}

/// AE's stored auto values in the keyframe fixture are reproduced from the values alone.
#[test]
fn keyframe_fixture_auto_values_are_reproduced() {
    let fx: Fixture = serde_json::from_str(include_str!("fixtures/ae-26.2-anim.json")).unwrap();
    for id in [
        "temporal_auto_bezier",
        "spatial_auto_bezier",
        "spatial_roving",
    ] {
        let case = fx.cases.iter().find(|c| c.id == id).unwrap();
        let kind = if id.starts_with("spatial") {
            CurveKind::Spatial
        } else {
            CurveKind::PerDimension
        };
        let mut scrambled = case.attr.keyframes.clone();
        for k in scrambled.values_mut() {
            k.ease_in.iter_mut().for_each(|e| e.speed = 999.0);
            k.ease_out.iter_mut().for_each(|e| e.speed = 999.0);
            if kind == CurveKind::Spatial {
                k.spatial_in = Some([1.0; 3]);
                k.spatial_out = Some([1.0; 3]);
                k.ease_in.iter_mut().for_each(|e| e.speed = 0.0);
                k.ease_out.iter_mut().for_each(|e| e.speed = 0.0);
            }
        }
        let out = update_auto_values(&scrambled, kind);
        for ((gt, g), (wt, w)) in out.iter().zip(&case.attr.keyframes) {
            assert!(
                (gt.as_seconds() - wt.as_seconds()).abs() < 1e-4,
                "{id}: time {gt} vs {wt}"
            );
            if kind == CurveKind::Spatial {
                for d in 0..3 {
                    assert!(
                        (g.spatial_out.unwrap()[d] - w.spatial_out.unwrap()[d]).abs() < 1e-4,
                        "{id}"
                    );
                }
            } else {
                assert!(
                    (g.ease_in[0].speed - w.ease_in[0].speed).abs() < 1e-6,
                    "{id}"
                );
                assert!(
                    (g.ease_out[0].speed - w.ease_out[0].speed).abs() < 1e-6,
                    "{id}"
                );
            }
        }
    }
}
