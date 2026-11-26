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
