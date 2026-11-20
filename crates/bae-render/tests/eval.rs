//! MockEvaluator against stages built through bae-model commands.

use bae_model::{FrameRate, PrimPath, Time, Value};
use bae_render::fixture::SceneBuilder;
use bae_render::{BlendMode, CompEvaluator, LayerContent, MockEvaluator, TrackMatteMode};
use glam::DVec2;

const S: i64 = Time::FLICKS_PER_SECOND;

fn t(seconds: f64) -> Time {
    Time::from_seconds(seconds)
}

#[test]
fn evaluates_solids_transforms_keyframes_and_layer_time() {
    let mut b = SceneBuilder::new();
    let comp = b
        .comp(
            "main",
            400,
            300,
            FrameRate::FPS_30,
            Time::from_flicks(4 * S),
        )
        .unwrap();
    b.set(&comp, "bg_color", Value::Color([0.1, 0.2, 0.3, 1.0]))
        .unwrap();
    let a = b
        .solid_layer(&comp, "a", [1.0, 0.0, 0.0, 1.0], [100, 100], [150.0, 100.0])
        .unwrap();
    b.set(&a, "start_time", Value::Time(t(0.5))).unwrap();
    b.set(&a, "blend_mode", Value::Enum("multiply".into()))
        .unwrap();
    // Opacity 0 → 100 % over the first second of *layer* time.
    b.key(&a, "transform:opacity", Time::ZERO, Value::Float(0.0))
        .unwrap();
    b.key(&a, "transform:opacity", t(1.0), Value::Float(100.0))
        .unwrap();
    b.set(&a, "transform:rotation", Value::Float(90.0)).unwrap();

    let stage = b.into_stage();
    let mut eval = MockEvaluator::new(&stage, None);
    let rc = eval.evaluate(&comp, t(1.0)).unwrap();
    assert_eq!((rc.width, rc.height), (400, 300));
    assert_eq!(rc.background, [0.1, 0.2, 0.3, 1.0]);
    assert_eq!(rc.layers.len(), 1);
    let l = &rc.layers[0];
    assert_eq!(l.time, t(0.5), "layer time = comp time − start_time");
    assert!((l.opacity - 0.5).abs() < 1e-6);
    assert_eq!(l.blend_mode, BlendMode::Multiply);
    assert!(l.active && l.visible);
    assert!(matches!(
        l.content,
        LayerContent::Solid {
            width: 100,
            height: 100,
            ..
        }
    ));
    // Anchor (50, 50) lands on position (200, 150); rotation is clockwise.
    let c = l.transform.transform_point2(DVec2::new(50.0, 50.0));
    assert!((c - DVec2::new(200.0, 150.0)).length() < 1e-9);
    let right = l.transform.transform_point2(DVec2::new(100.0, 50.0));
    assert!(
        (right - DVec2::new(200.0, 200.0)).length() < 1e-9,
        "{right}"
    );

    // Outside the comp duration the layer is inactive (out_point = duration).
    let rc = eval.evaluate(&comp, t(4.0)).unwrap();
    assert!(!rc.layers[0].active);
}

#[test]
fn evaluates_precomps_parents_and_track_mattes() {
    let mut b = SceneBuilder::new();
    let inner = b
        .comp(
            "inner",
            200,
            200,
            FrameRate::FPS_30,
            Time::from_flicks(10 * S),
        )
        .unwrap();
    let dot = b
        .solid_layer(&inner, "dot", [0.0, 1.0, 0.0, 1.0], [20, 20], [0.0, 0.0])
        .unwrap();
    b.key(
        &dot,
        "transform:position",
        Time::ZERO,
        Value::Vec3([10.0, 10.0, 0.0]),
    )
    .unwrap();
    b.key(
        &dot,
        "transform:position",
        t(2.0),
        Value::Vec3([110.0, 10.0, 0.0]),
    )
    .unwrap();

    let comp = b
        .comp(
            "main",
            640,
            360,
            FrameRate::FPS_30,
            Time::from_flicks(10 * S),
        )
        .unwrap();
    let matte = b
        .solid_layer(&comp, "matte", [1.0, 1.0, 1.0, 1.0], [100, 100], [0.0, 0.0])
        .unwrap();
    b.set(&matte, "enabled", Value::Bool(false)).unwrap();
    let pre = b.layer(&comp, "pre", "pre_comp", Some(&inner)).unwrap();
    b.set(&pre, "start_time", Value::Time(t(1.0))).unwrap();
    b.set(&pre, "track_matte", Value::Enum("luma_inverted".into()))
        .unwrap();
    b.set(&pre, "transform:position", Value::Vec3([100.0, 50.0, 0.0]))
        .unwrap();
    let child = b
        .solid_layer(&comp, "child", [0.0, 0.0, 1.0, 1.0], [10, 10], [0.0, 0.0])
        .unwrap();
    b.set(&child, "parent", Value::PrimRef(pre.clone()))
        .unwrap();

    let stage = b.into_stage();
    let mut eval = MockEvaluator::new(&stage, None);
    let rc = eval.evaluate(&comp, t(2.0)).unwrap();
    let names: Vec<_> = rc.layers.iter().map(|l| l.path.name().to_owned()).collect();
    assert_eq!(
        names,
        ["matte", "pre", "child"],
        "first child is the top layer"
    );

    assert!(!rc.layers[0].visible);
    let tm = rc.layers[1].track_matte.expect("matte");
    assert_eq!(
        (tm.layer, tm.mode),
        (0, TrackMatteMode::LumaInverted),
        "none = layer above"
    );

    let LayerContent::Comp(nested) = &rc.layers[1].content else {
        panic!("pre-comp content")
    };
    assert_eq!(
        nested.time,
        t(1.0),
        "nested comp is evaluated at layer time"
    );
    let dot = &nested.layers[0];
    // Halfway between the keyframes: position (60, 10), anchor (10, 10).
    let p = dot.transform.transform_point2(DVec2::new(10.0, 10.0));
    assert!((p - DVec2::new(60.0, 10.0)).length() < 1e-6, "{p}");

    // The child follows its parent's transform: parent translates by (100, 50).
    let c = rc.layers[2]
        .transform
        .transform_point2(DVec2::new(5.0, 5.0));
    assert!((c - DVec2::new(105.0, 55.0)).length() < 1e-9, "{c}");

    let deps = rc.dependencies();
    assert!(
        deps.iter()
            .any(|d| d.path == PrimPath::new("/comps/inner").unwrap() && d.time == t(1.0))
    );
}
