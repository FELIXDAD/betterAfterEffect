mod common;
use bae_model::Value;
use bae_render::IRect;
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
