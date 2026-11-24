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
fn deferred_inputs_are_retained_and_disabled() {
    for (name, ids) in [
        ("ADBE Fill", &[1, 7, 3, 4][..]),
        ("ADBE Gradient Wipe", &[3, 4]),
        ("VISINF Grain Implant", &[60, 61, 62, 63]),
    ] {
        for &id in ids {
            assert!(
                builtin(name).def().param(id).unwrap().flags.disabled,
                "{name} {id}"
            );
        }
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
    let image = region.upload(&gpu, F32);
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
    assert_eq!(
        Pixels::download(&gpu, &out).get(8, 16),
        [0.25, 0.25, 0.25, 1.]
    );
}
