//! Layer transforms compared with After Effects.
//!
//! `tests/fixtures/ae-26.2-transforms.json` holds layer setups (2D, 3D orientation and
//! X/Y/Z rotation order, negative scale, parenting incl. mixed 2D/3D, auto-orient
//! along path) and, for each layer, AE's `toWorld(point)` (the world matrix) and
//! `toComp(point)` (through the default comp camera) at a few times, sampled with
//! `tests/fixtures/ae_transforms.py` (generates the AE script) and `tools/ae/aectl`.

use bae_anim::Evaluator;
use bae_anim::glam::DVec3;
use bae_model::{
    AttrSpec, AuthorId, Keyframe, Op, PrimPath, PrimType, Stage, Time, Transaction, Value,
};
use serde::Deserialize;

const TOLERANCE: f64 = 1e-3;
/// Auto-orient follows the motion path, so it inherits the spatial gap (see
/// ae_reference.rs) through the path tangent.
const AUTO_ORIENT_TOLERANCE: f64 = 0.05;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    layers: Vec<LayerDef>,
    points: Vec<[f64; 3]>,
    samples: Vec<Sample>,
}

#[derive(Deserialize)]
struct LayerDef {
    name: String,
    #[serde(default)]
    three_d: bool,
    parent: Option<String>,
    anchor: Option<[f64; 3]>,
    position: Option<[f64; 3]>,
    scale: Option<[f64; 3]>,
    rotation: Option<f64>,
    rotation_x: Option<f64>,
    rotation_y: Option<f64>,
    orientation: Option<[f64; 3]>,
    auto_orient: Option<String>,
    /// `[time s, value, in tangent, out tangent]`, linear in time.
    position_keys: Option<Vec<PositionKey>>,
}

/// `[time s, value, in tangent, out tangent]`.
type PositionKey = (f64, [f64; 3], [f64; 3], [f64; 3]);

#[derive(Deserialize)]
struct Sample {
    layer: String,
    point: usize,
    time: f64,
    world: [f64; 3],
    comp: [f64; 3],
}

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn layer_path(name: &str) -> String {
    format!("/comps/main/layers/{name}")
}

fn build(case: &Case) -> Stage {
    let mut stage = Stage::new();
    stage
        .transact(
            &AuthorId::new("user:test").unwrap(),
            "build",
            |tx: &mut Transaction<'_>| {
                let create = |tx: &mut Transaction<'_>, path: &str, t: PrimType| {
                    tx.apply(Op::CreatePrim {
                        path: p(path),
                        specifier: Default::default(),
                        prim_type: Some(t),
                        index: None,
                    })
                };
                let set = |tx: &mut Transaction<'_>, path: &str, attr: &str, spec: AttrSpec| {
                    tx.apply(Op::SetAttr {
                        path: p(path),
                        attr: attr.into(),
                        spec: Some(spec),
                    })
                };
                let v = |x: Value| AttrSpec::with_value(x);
                create(tx, "/comps", PrimType::FOLDER)?;
                create(tx, "/comps/main", PrimType::COMP)?;
                create(tx, "/comps/main/layers", PrimType::FOLDER)?;
                for l in &case.layers {
                    let lp = layer_path(&l.name);
                    create(tx, &lp, PrimType::LAYER)?;
                    set(tx, &lp, "kind", v(Value::Enum("null".into())))?;
                    set(tx, &lp, "three_d", v(Value::Bool(l.three_d)))?;
                    // Defaults the sampling script also sets in AE.
                    set(
                        tx,
                        &lp,
                        "transform:anchor_point",
                        v(Value::Vec3(l.anchor.unwrap_or([0.0; 3]))),
                    )?;
                    set(
                        tx,
                        &lp,
                        "transform:scale",
                        v(Value::Vec3(l.scale.unwrap_or([100.0; 3]))),
                    )?;
                    set(
                        tx,
                        &lp,
                        "transform:rotation",
                        v(Value::Float(l.rotation.unwrap_or(0.0))),
                    )?;
                    set(
                        tx,
                        &lp,
                        "transform:rotation_x",
                        v(Value::Float(l.rotation_x.unwrap_or(0.0))),
                    )?;
                    set(
                        tx,
                        &lp,
                        "transform:rotation_y",
                        v(Value::Float(l.rotation_y.unwrap_or(0.0))),
                    )?;
                    set(
                        tx,
                        &lp,
                        "transform:orientation",
                        v(Value::Vec3(l.orientation.unwrap_or([0.0; 3]))),
                    )?;
                    let mut pos = AttrSpec::with_value(Value::Vec3(
                        l.position.unwrap_or([960.0, 540.0, 0.0]),
                    ));
                    for (t, value, tin, tout) in l.position_keys.iter().flatten() {
                        pos.keyframes.insert(
                            Time::from_seconds(*t),
                            Keyframe {
                                spatial_in: Some(*tin),
                                spatial_out: Some(*tout),
                                ..Keyframe::linear(Value::Vec3(*value))
                            },
                        );
                    }
                    set(tx, &lp, "transform:position", pos)?;
                    if let Some(a) = &l.auto_orient {
                        set(tx, &lp, "auto_orient", v(Value::Enum(a.clone())))?;
                    }
                }
                for l in &case.layers {
                    if let Some(parent) = &l.parent {
                        set(
                            tx,
                            &layer_path(&l.name),
                            "parent",
                            v(Value::PrimRef(p(&layer_path(parent)))),
                        )?;
                    }
                }
                Ok::<_, bae_model::ModelError>(())
            },
        )
        .unwrap_or_else(|e| panic!("{}: {e}", case.id));
    stage
}

#[test]
fn transforms_match_after_effects() {
    let fx: Fixture =
        serde_json::from_str(include_str!("fixtures/ae-26.2-transforms.json")).expect("fixture");
    let mut failures = Vec::new();
    for case in &fx.cases {
        let stage = build(case);
        let mut ev = Evaluator::new();
        let mut max_err = 0.0_f64;
        let mut worst = String::new();
        for s in &case.samples {
            let ec = ev
                .evaluate_comp(&stage, &p("/comps/main"), Time::from_seconds(s.time))
                .unwrap();
            let layer = ec.layer(&p(&layer_path(&s.layer))).expect("layer");
            let got = layer
                .world_matrix
                .transform_point3(DVec3::from_array(case.points[s.point]));
            // 2D layers: AE's toWorld z is 0; only x/y matter.
            let dims = if layer.switches.three_d { 3 } else { 2 };
            let mut check = |what: &str, got: f64, want: f64, d: usize| {
                let err = (got - want).abs();
                if err > max_err {
                    max_err = err;
                    worst = format!(
                        "{} {what} t={} point {:?} dim {d}: got {got} want {want}",
                        s.layer, s.time, case.points[s.point]
                    );
                }
            };
            for d in 0..dims {
                check("world", got[d], s.world[d], d);
            }
            // AE's default comp camera (no camera layer): eye at (w/2, h/2, -zoom),
            // zoom = width * 50 / 36, perspective onto z = 0.
            if layer.switches.three_d {
                let zoom = bae_anim::default_camera_zoom(ec.width);
                let (cx, cy) = (ec.width as f64 / 2.0, ec.height as f64 / 2.0);
                let f = zoom / (got.z + zoom);
                check("comp", cx + (got.x - cx) * f, s.comp[0], 0);
                check("comp", cy + (got.y - cy) * f, s.comp[1], 1);
            }
        }
        println!("{:<24} max err {:.3e}  {}", case.id, max_err, worst);
        let tol = if case.id.starts_with("auto_orient") {
            AUTO_ORIENT_TOLERANCE
        } else {
            TOLERANCE
        };
        if max_err >= tol {
            failures.push(format!("{}: {:.3e} ({})", case.id, max_err, worst));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
