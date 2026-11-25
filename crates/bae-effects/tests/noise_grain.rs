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
fn unsupported_sample_controls_are_disabled() {
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
