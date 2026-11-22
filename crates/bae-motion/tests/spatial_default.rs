//! Preferences > Composition > Default Spatial Interpolation to Linear: new keyframes
//! of spatial properties are spatially linear while it is on (the default) and Auto
//! Bezier when it is off. A test binary of its own: the setting is process-wide.

use bae_model::{AuthorId, CommandRegistry, PrimPath, Stage, Time};
use bae_motion::query;
use serde_json::json;

const SEC: i64 = Time::FLICKS_PER_SECOND;

#[test]
fn new_spatial_keyframes_follow_the_preference() {
    let mut reg = CommandRegistry::with_builtins();
    bae_motion::register_commands(&mut reg);
    let mut stage = Stage::new();
    let author = AuthorId::new("user:test").unwrap();
    let mut run = |stage: &mut Stage, id: &str, p: serde_json::Value| {
        reg.execute(stage, &author, id, p)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .output
    };
    let comp = run(
        &mut stage,
        "comp.create",
        json!({"width": 640, "height": 360, "fps": 30}),
    );
    let layer = run(
        &mut stage,
        "layer.create",
        json!({"comp": comp["path"], "kind": "solid"}),
    );
    let layer = PrimPath::new(layer["path"].as_str().unwrap()).unwrap();
    let pos = "transform:position";
    let add = |stage: &mut Stage,
               run: &mut dyn FnMut(&mut Stage, &str, serde_json::Value) -> serde_json::Value,
               t: i64,
               x: f64,
               y: f64| {
        run(
            stage,
            "attr.set_at",
            json!({"path": layer, "attr": pos, "time": t, "value": {"type": "vec3", "value": [x, y, 0.0]}}),
        );
    };
    run(
        &mut stage,
        "attr.set_animated",
        json!({"path": layer, "attr": pos, "animated": true, "time": 0}),
    );
    // On (default): linear.
    add(&mut stage, &mut run, SEC, 100.0, 0.0);
    let k = |stage: &Stage, t: i64| {
        query::attr_spec(stage, &layer, pos).unwrap().keyframes[&Time(t)].clone()
    };
    assert!(!k(&stage, SEC).flags.spatial_auto_bezier);
    // Off: Auto Bezier, with tangents once it has neighbours on both sides.
    bae_motion::set_default_spatial_linear(false);
    add(&mut stage, &mut run, 2 * SEC, 100.0, 100.0);
    add(&mut stage, &mut run, SEC / 2, 50.0, 50.0);
    let mid = k(&stage, SEC / 2);
    assert!(mid.flags.spatial_auto_bezier && mid.flags.spatial_continuous);
    assert!(mid.spatial_out.is_some_and(|t| t != [0.0; 3]), "{mid:?}");
    // Rotation is not spatial: unaffected.
    run(
        &mut stage,
        "attr.set_animated",
        json!({"path": layer, "attr": "transform:rotation", "animated": true, "time": 0}),
    );
    let rot = query::attr_spec(&stage, &layer, "transform:rotation").unwrap();
    assert!(!rot.keyframes[&Time::ZERO].flags.spatial_auto_bezier);
    bae_motion::set_default_spatial_linear(true);
}
