mod common;
use bae_model::Value;
use bae_render::{EffectParams, IRect};
use common::*;

#[test]
fn optics_render_an_offset_tile_and_channel_blur_preserves_other_channels() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(2, 3, 26, 25);
    let roi = IRect::new(4, 5, 22, 20);
    let input = Pixels::new(rect, |x, _| {
        if x < 12 {
            [0.8, 0.2, 0.1, 1.]
        } else {
            [0.1, 0.7, 0.2, 1.]
        }
    });
    let out = run(
        &gpu,
        F16,
        builtin("ADBE Channel Blur"),
        vec![(1, Value::Float(12.))],
        &input,
        roi,
        1.,
    );
    assert!(out.get(11, 10)[0] < 0.78);
    assert!((out.get(11, 10)[1] - 0.2).abs() < 0.001);
    for (name, id, amount) in [
        ("ADBE Camera Lens Blur", 1, 8.),
        ("ADBE Bilateral", 1, 3.),
        ("ADBE Smart Blur", 1, 3.),
        ("CC Radial Blur", 2, 30.),
        ("CC Radial Fast Blur", 2, 30.),
        ("CS CrossBlur", 1, 4.),
        ("CC Vector Blur", 2, 20.),
    ] {
        let out = run(
            &gpu,
            F16,
            builtin(name),
            vec![(id, Value::Float(amount))],
            &input,
            roi,
            1.,
        );
        assert!(out.get(10, 10).iter().all(|v| v.is_finite()), "{name}");
    }
}

#[test]
fn lens_map_at_the_focal_distance_keeps_detail() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(24, 24);
    let input = Pixels::new(rect, |x, _| {
        if x == 12 {
            [1., 1., 1., 1.]
        } else {
            [0., 0., 0., 1.]
        }
    });
    let fx = builtin("ADBE Camera Lens Blur");
    let inst = instance(
        &fx.info.match_name,
        vec![(
            10,
            Value::PrimRef(bae_model::PrimPath::new("/Comp/Depth").unwrap()),
        )],
    );
    let p = EffectParams::new(&inst);
    let dep = fx.effect().layer_dependencies(&p).remove(0);
    let image = input.upload(&gpu, F16);
    let mut target = bae_effects::Standalone::new(gpu.clone(), F16);
    for (depth, focused) in [(0., true), (1., false)] {
        let map = Pixels::new(rect, |_, _| [depth, depth, depth, 1.]);
        target.set_effect_inputs(vec![(dep.clone(), map.upload(&gpu, F16))], vec![]);
        let out = fx.render(&mut target, &p, &image, rect).unwrap();
        target.submit();
        let result = Pixels::download(&gpu, &out);
        if focused {
            assert_close(&result, &input, rect, 0.001, "in-focus depth");
        } else {
            assert!(result.get(12, 12)[0] < 0.8);
            assert!(result.get(11, 12)[0] > 0.01);
        }
    }
}

#[test]
fn wide_optics_preserve_focus_foreground_scatter_and_repeated_edges() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-5, -3, 60, 40);
    let tile = IRect::new(13, 7, 41, 32);
    let input = Pixels::new(rect, |x, _| {
        if x >= 30 {
            [1., 0., 0., 1.]
        } else if x == 0 {
            [0., 0.25, 0., 0.5]
        } else {
            [0., 0., 0., 0.5]
        }
    });
    let depth = Pixels::new(rect, |x, _| {
        let d = f32::from(x >= 30);
        [d, d, d, 1.]
    });
    let fx = builtin("ADBE Camera Lens Blur");
    let inst = instance(
        &fx.info.match_name,
        vec![
            (1, Value::Float(20.)),
            (21, Value::Bool(true)),
            (
                10,
                Value::PrimRef(bae_model::PrimPath::new("/Comp/Depth").unwrap()),
            ),
        ],
    );
    let p = EffectParams {
        layer_bounds: rect,
        float: true,
        ..EffectParams::new(&inst)
    };
    let mut target = bae_effects::Standalone::new(gpu.clone(), F16);
    target.set_effect_inputs(
        vec![(
            fx.effect().layer_dependencies(&p).remove(0),
            depth.upload(&gpu, F16),
        )],
        vec![],
    );
    let image = input.upload(&gpu, F16);
    let mut render = |roi| {
        let out = fx.render(&mut target, &p, &image, roi).unwrap();
        target.finish().unwrap();
        Pixels::download(&gpu, &out)
    };
    let full = render(rect);
    assert_close(
        &full,
        &input,
        IRect::new(-4, 2, 5, 33),
        0.001,
        "focused line",
    );
    let spill = full.get(29, 18);
    assert!(
        spill[0] > 0.05 && spill[3] > 0.51,
        "foreground scatter {spill:?}"
    );
    assert_close(&render(tile), &full, tile, 0.001, "depth tile");

    let flat = Pixels::new(rect, |_, _| [0.75, 0.375, 0., 0.75]);
    for name in ["ADBE Camera Lens Blur", "ADBE Bilateral", "ADBE Smart Blur"] {
        let mut params = vec![(1, Value::Float(24.))];
        if name == "ADBE Camera Lens Blur" {
            params.push((21, Value::Bool(true)));
        }
        let out = run(&gpu, F16, builtin(name), params, &flat, rect, 1.);
        assert_close(&out, &flat, rect, 0.001, name);
    }
}

#[test]
fn channel_blur_matches_independent_gaussians_with_partial_coverage() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-5, 7, 49, 44);
    let input = test_image(rect);
    let radii = [0., 8., 32., 80.];
    for (dimensions, repeat) in [(1, false), (2, true), (3, true)] {
        let roi = if repeat { rect } else { rect.inflate(3) };
        let gaussians = radii.map(|r| {
            if r == 0. {
                input.clone()
            } else {
                let gaussian = builtin("ADBE Gaussian Blur 2");
                let instance = instance(
                    &gaussian.info.match_name,
                    vec![
                        (1, Value::Float(r)),
                        (2, Value::Int(dimensions)),
                        (3, Value::Bool(repeat)),
                    ],
                );
                // The channel effect filters in the working colour space.
                let p = EffectParams {
                    linear: true,
                    float: true,
                    ..EffectParams::new(&instance)
                };
                let mut target = bae_effects::Standalone::new(gpu.clone(), F32);
                let result = gaussian
                    .render(&mut target, &p, &input.upload(&gpu, F32), roi)
                    .unwrap();
                target.finish().unwrap();
                Pixels::download(&gpu, &result)
            }
        });
        let want = Pixels::new(roi, |x, y| {
            let alpha = gaussians[3].get(x, y)[3];
            let [r, g, b] = [0, 1, 2].map(|c| unpremultiply(gaussians[c].get(x, y))[c] * alpha);
            [r, g, b, alpha]
        });
        let mut params = vec![(5, Value::Bool(repeat)), (6, Value::Int(dimensions))];
        params.extend((1..=4).zip(radii.map(Value::Float)));
        let out = run(
            &gpu,
            F32,
            builtin("ADBE Channel Blur"),
            params.clone(),
            &input,
            roi,
            1.,
        );
        assert_close(&out, &want, roi, 0.0001, "per-channel Gaussian");
        let tile = IRect::new(-1, 13, 22, 30);
        let part = run(
            &gpu,
            F32,
            builtin("ADBE Channel Blur"),
            params,
            &input,
            tile,
            1.,
        );
        assert_close(&part, &out, tile, 0.0001, "channel tile");
    }
}
