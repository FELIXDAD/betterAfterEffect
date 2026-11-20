//! Performance budget (acceptance: 1000 keyframed layers, one frame < 2 ms without
//! expressions).
//!
//! `cargo test --release -p bae-anim --test perf -- --nocapture`. The budget is only
//! asserted in release builds; debug builds just print the timing.

use std::time::Instant;

use bae_anim::Evaluator;
use bae_model::{
    AttrSpec, AuthorId, Ease, Interpolation, Keyframe, Op, PrimPath, PrimType, Stage, Time, Value,
};

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn bezier(v: impl Into<Value>, dims: usize) -> Keyframe {
    Keyframe {
        interp_in: Interpolation::Bezier,
        interp_out: Interpolation::Bezier,
        ease_in: vec![Ease::default(); dims],
        ease_out: vec![Ease::default(); dims],
        ..Keyframe::linear(v)
    }
}

fn spatial(v: [f64; 3], tangent: [f64; 3]) -> Keyframe {
    Keyframe {
        spatial_in: Some([-tangent[0], -tangent[1], -tangent[2]]),
        spatial_out: Some(tangent),
        ..bezier(v, 1)
    }
}

fn build(n: usize) -> Stage {
    let mut stage = Stage::new();
    stage
        .transact(&AuthorId::new("user:test").unwrap(), "build", |tx| {
            let mut create = |path: &str, t: PrimType| {
                tx.apply(Op::CreatePrim {
                    path: p(path),
                    specifier: Default::default(),
                    prim_type: Some(t),
                    index: None,
                })
            };
            create("/comps", PrimType::FOLDER)?;
            create("/comps/main", PrimType::COMP)?;
            create("/comps/main/layers", PrimType::FOLDER)?;
            for i in 0..n {
                create(&format!("/comps/main/layers/l{i}"), PrimType::LAYER)?;
            }
            for i in 0..n {
                let lp = p(&format!("/comps/main/layers/l{i}"));
                let mut set = |attr: &str, spec: AttrSpec| {
                    tx.apply(Op::SetAttr {
                        path: lp.clone(),
                        attr: attr.into(),
                        spec: Some(spec),
                    })
                };
                set("kind", AttrSpec::with_value(Value::Enum("solid".into())))?;
                let o = i as f64;
                let mut pos = AttrSpec::default();
                pos.keyframes
                    .insert(Time::ZERO, spatial([o, 0.0, 0.0], [50.0, 80.0, 0.0]));
                pos.keyframes.insert(
                    Time::from_seconds(2.0),
                    spatial([o + 900.0, 600.0, 0.0], [60.0, -40.0, 0.0]),
                );
                pos.keyframes.insert(
                    Time::from_seconds(4.0),
                    spatial([o + 100.0, 900.0, 0.0], [0.0; 3]),
                );
                set("transform:position", pos)?;
                let mut scale = AttrSpec::default();
                scale
                    .keyframes
                    .insert(Time::ZERO, bezier([100.0, 100.0, 100.0], 3));
                scale
                    .keyframes
                    .insert(Time::from_seconds(3.0), bezier([50.0, 150.0, 100.0], 3));
                set("transform:scale", scale)?;
                let mut rot = AttrSpec::default();
                rot.keyframes.insert(Time::ZERO, bezier(0.0, 1));
                rot.keyframes
                    .insert(Time::from_seconds(4.0), bezier(360.0 + o, 1));
                set("transform:rotation", rot)?;
                let mut op = AttrSpec::default();
                op.keyframes.insert(Time::ZERO, Keyframe::linear(0.0));
                op.keyframes
                    .insert(Time::from_seconds(1.0), Keyframe::linear(100.0));
                set("transform:opacity", op)?;
                if i > 0 && i % 10 != 0 {
                    let parent = format!("/comps/main/layers/l{}", i - i % 10);
                    set("parent", AttrSpec::with_value(Value::PrimRef(p(&parent))))?;
                }
            }
            Ok::<_, bae_model::ModelError>(())
        })
        .unwrap();
    stage
}

#[test]
fn thousand_keyframed_layers_per_frame() {
    let stage = build(1000);
    let comp = p("/comps/main");
    let mut ev = Evaluator::new();
    let t0 = Instant::now();
    ev.evaluate_comp(&stage, &comp, Time::ZERO).unwrap();
    let compile = t0.elapsed();

    let frames = 120;
    let rate = bae_model::FrameRate::FPS_30;
    let t1 = Instant::now();
    for f in 0..frames {
        let ec = ev
            .evaluate_comp(&stage, &comp, rate.frame_to_time(f))
            .unwrap();
        assert_eq!(ec.layers.len(), 1000);
    }
    let per_frame = t1.elapsed() / frames as u32;
    println!("compile (first frame): {compile:?}; per frame: {per_frame:?}");
    if !cfg!(debug_assertions) {
        assert!(
            per_frame.as_secs_f64() < 2e-3,
            "per-frame evaluation {per_frame:?} exceeds 2 ms"
        );
    }
}

/// Expressions are outside the 2 ms budget; this prints their cost for reference
/// (100 layers, each with `wiggle` on Position and a cross-layer read on Rotation).
#[test]
fn expressions_per_frame() {
    let mut stage = build(100);
    stage
        .transact(&AuthorId::new("user:test").unwrap(), "exprs", |tx| {
            for i in 0..100 {
                let lp = p(&format!("/comps/main/layers/l{i}"));
                for (attr, src) in [
                    ("transform:position", "wiggle(2, 30)".to_owned()),
                    (
                        "transform:rotation",
                        format!("thisComp.layer({}).rotation + time * 10", (i + 1) % 100 + 1),
                    ),
                ] {
                    tx.apply(Op::SetExpression {
                        path: lp.clone(),
                        attr: attr.into(),
                        expression: Some(bae_model::Expression::new(src)),
                    })?;
                }
            }
            Ok::<_, bae_model::ModelError>(())
        })
        .unwrap();
    let comp = p("/comps/main");
    let mut ev = Evaluator::new();
    ev.evaluate_comp(&stage, &comp, Time::ZERO).unwrap();
    let frames = 30;
    let t = Instant::now();
    for f in 0..frames {
        let ec = ev
            .evaluate_comp(&stage, &comp, bae_model::FrameRate::FPS_30.frame_to_time(f))
            .unwrap();
        assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    }
    println!(
        "100 layers × 2 expressions: {:?} per frame",
        t.elapsed() / frames as u32
    );
}
