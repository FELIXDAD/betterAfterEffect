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

#[test]
fn vegas_selects_connected_image_contours_in_scan_order() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-4, 2, 60, 38);
    let image = Pixels::new(rect, |x, y| {
        let inside = (4..18).contains(&x) && (6..20).contains(&y)
            || (32..48).contains(&x) && (18..33).contains(&y);
        let l = f32::from(inside);
        [l, l, l, 1.]
    });
    for (index, here, other) in [(1, (4, 12), (32, 25)), (2, (32, 25), (4, 12))] {
        let output = run(
            &gpu,
            bae_gpu::wgpu::TextureFormat::Rgba16Float,
            builtin("APC Vegas"),
            vec![
                (8, Value::Int(1)),
                (44, Value::Int(2)),
                (46, Value::Int(index)),
                (24, Value::Float(1.)),
                (28, Value::Int(1)),
                (36, Value::Float(1.)),
                (38, Value::Float(0.)),
                (42, Value::Float(1.)),
            ],
            &image,
            rect,
            1.,
        );
        assert!(output.get(here.0, here.1)[3] > 0.4);
        assert_eq!(output.get(other.0, other.1)[3], 0.);
    }
}
