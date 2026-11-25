//! Structural and GPU invariants for generators, noise and transitions.
mod common;
use bae_model::{ParamKind, Value};
use bae_render::IRect;
use common::*;

#[test]
fn nested_grain_end_markers_and_fractal_disk_ids() {
    let def = builtin("VISINF Grain Implant").def();
    let ids: Vec<_> = def.params.iter().map(|p| p.id).collect();
    assert!(
        ids.windows(8)
            .any(|ids| ids == [32, 33, 34, 115, 111, 122, 5, 6])
    );
    assert!(matches!(def.param(111).unwrap().kind, ParamKind::GroupEnd));
    let def = builtin("ADBE Fractal Noise").def();
    assert_eq!(def.param(31).unwrap().name, "Perspective Offset");
    assert_eq!(def.param(27).unwrap().default, Some(Value::Int(0)));
}

#[test]
fn fractal_loop_seed_and_fractional_octaves() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(64, 40);
    let src = test_image(rect);
    for format in [F32, F16] {
        let render = |angle: f64, cycle: bool, seed: i64, octaves: f64| {
            run(
                &gpu,
                format,
                builtin("ADBE Fractal Noise"),
                vec![
                    (23, Value::Float(angle)),
                    (25, Value::Bool(cycle)),
                    (26, Value::Float(2.)),
                    (27, Value::Int(seed)),
                    (15, Value::Float(octaves)),
                ],
                &src,
                rect,
                1.,
            )
            .data
        };
        let first = render(-90., true, 42, 3.5);
        assert_eq!(first, render(630., true, 42, 3.5));
        assert_ne!(first, render(90., true, 42, 3.5));
        assert_ne!(first, render(-90., true, 43, 3.5));
        assert_ne!(first, render(-90., true, 42, 3.));
        assert_ne!(render(0., false, 0, 6.), render(360., false, 0, 6.));
    }
}

#[test]
fn all_wipes_have_exact_endpoints() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(32, 32);
    let src = test_image(r);
    for (name, end) in [
        ("ADBE Linear Wipe", 100.),
        ("ADBE Radial Wipe", 100.),
        ("ADBE Block Dissolve", 100.),
        ("ADBE Venetian Blinds", 100.),
        ("ADBE Gradient Wipe", 1.),
    ] {
        for completion in [0., end] {
            let result = run(
                &gpu,
                F32,
                builtin(name),
                vec![(1, Value::Float(completion))],
                &src,
                r,
                1.,
            );
            if completion == 0. {
                assert_eq!(result.data, src.data);
            } else {
                assert!(result.data.iter().all(|&px| px == [0.; 4]), "{name}");
            }
        }
    }
}

#[test]
fn implemented_layer_and_mask_controls_are_enabled() {
    for (name, ids) in [
        ("ADBE Fill", &[1, 7, 3, 4][..]),
        ("ADBE Gradient Wipe", &[3, 4]),
    ] {
        for &id in ids {
            assert!(
                !builtin(name).def().param(id).unwrap().flags.disabled,
                "{name} {id}"
            );
        }
    }
    for id in [60, 61, 62, 63] {
        assert!(
            builtin("VISINF Grain Implant")
                .def()
                .param(id)
                .unwrap()
                .flags
                .disabled
        );
    }
}

#[test]
fn hdr_fill_and_eight_sixteen_bit_wipe_thresholds() {
    use bae_effects::Standalone;
    use bae_render::EffectParams;
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(3, 2);
    let input = Pixels::new(r, |_, _| [0.499, 0.499, 0.499, 1.0]);
    let fx = builtin("ADBE Gradient Wipe");
    let inst = instance(&fx.info.match_name, vec![(1, Value::Float(0.5))]);
    let image = input.upload(&gpu, F32);
    for (sixteen_bpc, float, expected) in
        [(false, false, 0.5), (true, false, 0.0), (false, true, 0.0)]
    {
        let p = EffectParams {
            sixteen_bpc,
            float,
            ..EffectParams::new(&inst)
        };
        let mut target = Standalone::new(gpu.clone(), F32);
        let out = fx.render(&mut target, &p, &image, r).unwrap();
        target.finish().unwrap();
        assert_eq!(Pixels::download(&gpu, &out).get(0, 0)[3], expected);
    }
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

#[test]
fn paint_bucket_connectivity_and_jump_flood_stroke() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(17, 13);
    let mut src = test_image(rect);
    for (i, pixel) in src.data.iter_mut().enumerate() {
        *pixel = if i % 17 == 8 {
            [1.; 4]
        } else {
            [0., 0., 0., 1.]
        };
    }
    for (stroke, last_red) in [(1, 7), (3, 9)] {
        let got = run(
            &gpu,
            F32,
            builtin("ADBE Paint Bucket"),
            vec![
                (2, Value::Vec2([3., 6.])),
                (8, Value::Int(stroke)),
                (5, Value::Float(2.)),
            ],
            &src,
            rect,
            1.,
        );
        assert_eq!(got.get(last_red, 6), [1., 0., 0., 1.]);
        assert_eq!(got.get(12, 6), [0., 0., 0., 1.]);
    }
}

#[test]
fn noise_variants_share_the_hls_phase_and_legacy_median() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(0, 0, 12, 12);
    let input = Pixels::new(rect, |x, y| {
        let z = if (x + y) % 2 == 0 { 0.25 } else { 0.75 };
        [z, z, z, 1.]
    });
    let values = vec![(3, Value::Float(35.))];
    let fixed = run(
        &gpu,
        F16,
        builtin("ADBE Noise HLS2"),
        values.clone(),
        &input,
        rect,
        1.,
    );
    let automatic = run(
        &gpu,
        F16,
        builtin("ADBE Noise HLS Auto2"),
        values,
        &input,
        rect,
        1.,
    );
    assert_close(&fixed, &automatic, rect, 0.001, "shared HLS phase");
    assert_ne!(fixed.get(2, 2), input.get(2, 2));
    for name in [
        "ADBE AIF Perlin Noise 3D",
        "ADBE Median",
        "ADBE Noise Alpha2",
        "VISINF Grain Removal",
    ] {
        let out = run(&gpu, F16, builtin(name), vec![], &input, rect, 1.);
        assert!(out.get(2, 2).iter().all(|v| v.is_finite()), "{name}");
    }
}
#[test]
fn match_grain_uses_the_selected_reference_on_the_gpu() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(20, 20);
    let flat = Pixels::new(rect, |_, _| [0.5, 0.5, 0.5, 1.]);
    let noise = Pixels::new(rect, |x, y| {
        let n = if (x + y) % 2 == 0 { 0.6 } else { 0.4 };
        [n, n, n, 1.]
    });
    let fx = builtin("VISINF Grain Duplication");
    let layer = bae_model::PrimPath::new("/Comp/Reference").unwrap();
    let inst = instance(
        &fx.info.match_name,
        vec![(13, Value::PrimRef(layer)), (2, Value::Int(5))],
    );
    let p = bae_render::EffectParams::new(&inst);
    let dep = fx.effect().layer_dependencies(&p).remove(0);
    let mut target = bae_effects::Standalone::new(gpu.clone(), F16);
    target.set_effect_inputs(vec![(dep, noise.upload(&gpu, F16))], vec![]);
    let result = fx
        .render(&mut target, &p, &flat.upload(&gpu, F16), rect)
        .unwrap();
    target.finish().unwrap();
    let out = Pixels::download(&gpu, &result);
    assert!(max_diff(&out, &flat, rect).0 > 0.02);
}
