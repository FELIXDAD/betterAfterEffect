use bae_model::{Time, Value};
use bae_render::RenderComp;
#[path = "common/time_scene.rs"]
mod time_scene;
use time_scene::*;

#[test]
fn write_on_evaluates_subframe_history_from_the_layer_in_point() {
    let Some(mut r) = renderer() else { return };
    let scene = |time: Time| {
        let mut comp = RenderComp::new(path("/comps/main"), 32, 8);
        comp.time = time;
        let mut layer = solid(CLIP, 32, [1.; 4]);
        layer.in_point = Time::from_seconds(0.25);
        let elapsed = (time - layer.in_point).as_seconds();
        layer.time = Time::from_seconds(elapsed);
        layer.effects.push(effect(
            "write",
            "ADBE Write-on",
            vec![
                (1, Value::Vec2([4. + elapsed * 160., 4.])),
                (7, Value::Float(0.05)),
                (10, Value::Int(2)),
            ],
        ));
        comp.layers.push(layer);
        comp
    };
    let result = pixels(&mut r, scene, 0.36);
    let alpha = |x: usize| result[(3 * 32 + x) * 4 + 3];
    assert!(alpha(4) > 200 && alpha(12) > 200 && alpha(20) > 200);
    assert_eq!(alpha(8), 0);
}
