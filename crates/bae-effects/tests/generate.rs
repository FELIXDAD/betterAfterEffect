mod common;
use bae_model::{Time, Value};
use bae_render::{IRect, RenderComp};
use common::*;

#[test]
fn layer_and_mask_controls_are_enabled() {
    for id in [1, 7, 3, 4] {
        assert!(!builtin("ADBE Fill").def().param(id).unwrap().flags.disabled);
    }
}

#[test]
fn fill_respects_project_depth() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(3, 2);
    let source = Pixels::new(r, |_, _| [0.0, 0.0, 0.0, 0.5]);
    for (float, red) in [(true, 1.0), (false, 0.5)] {
        let result = run_depth(
            &gpu,
            F32,
            builtin("ADBE Fill"),
            vec![(2, Value::Color([2., 0., 0., 1.]))],
            &source,
            r,
            1.,
            float,
        );
        assert_eq!(result.get(0, 0), [red, 0., 0., 0.5]);
    }
}

#[test]
fn gradient_points_stay_with_layer_across_tiles() {
    use bae_effects::Standalone;
    use bae_render::EffectParams;
    let Some(gpu) = context() else { return };
    let bounds = IRect::from_size(128, 128);
    let tile = IRect::new(8, 16, 48, 32);
    let region = Pixels::new(tile, |_, _| [1.; 4]);
    let mut image = region.upload(&gpu, F32);
    // The half-resolution tile covers this region in layer pixels.
    image.rect = IRect::new(16, 32, 96, 64);
    image.pixel_scale = 2.0;
    let fx = builtin("ADBE Ramp");
    let inst = instance(&fx.info.match_name, vec![]);
    let p = EffectParams {
        layer_bounds: bounds,
        scale: 0.5,
        ..EffectParams::new(&inst)
    };
    let mut target = Standalone::new(gpu.clone(), F32);
    let out = fx.render(&mut target, &p, &image, tile).unwrap();
    target.finish().unwrap();
    assert_eq!(out.rect, image.rect);
    assert_eq!(out.texel_size(), tile.size());
    let data = bae_gpu::transfer::read_texture(&gpu, &out.texture).unwrap();
    for (x, y) in [(0, 0), (39, 15)] {
        let value = (tile.y0 as f32 + y as f32) / 64.0;
        let pixel = data.pixel(x, y);
        let rgba: [f32; 4] = std::array::from_fn(|i| {
            f32::from_le_bytes(pixel[4 * i..4 * i + 4].try_into().unwrap())
        });
        assert_eq!(rgba, [value, value, value, 1.]);
    }
}

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
