mod common;
use bae_model::Value;
use bae_render::IRect;
use common::*;

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
fn layer_and_mask_controls_are_enabled() {
    for id in [3, 4] {
        assert!(
            !builtin("ADBE Gradient Wipe")
                .def()
                .param(id)
                .unwrap()
                .flags
                .disabled
        );
    }
}

#[test]
fn gradient_wipe_thresholds_follow_project_depth() {
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
}

#[test]
fn card_camera_projects_the_grid_and_flips_to_the_back_layer() {
    use bae_effects::Standalone;
    use bae_model::PrimPath;
    use bae_render::{EffectParams, RenderEffect, effect::LayerDependency};
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(40, 24);
    let mut a = test_image(rect);
    a.data.fill([1., 0., 0., 1.]);
    let mut b = test_image(rect);
    b.data.fill([0., 0., 1., 1.]);
    let raw = a.upload(&gpu, F32);
    let layer = PrimPath::new("/comp/back").unwrap();
    for (progress, expected) in [(0., [1., 0., 0., 1.]), (100., [0., 0., 1., 1.])] {
        let instance = RenderEffect {
            path: PrimPath::new("/comp/front/effects/wipe").unwrap(),
            effect: "APC CardWipeCam".into(),
            enabled: true,
            params: vec![
                (2, Value::Float(progress)),
                (6, Value::PrimRef(layer.clone())),
                (8, Value::Int(1)),
                (10, Value::Float(3.)),
                (12, Value::Float(5.)),
                (56, Value::Float(0.)),
                (66, Value::Float(1.)),
            ],
        };
        let params = EffectParams::new(&instance);
        let mut target = Standalone::new(gpu.clone(), F32);
        target.set_effect_inputs(
            vec![(
                LayerDependency::source(layer.clone(), params.comp_time),
                b.upload(&gpu, F32),
            )],
            vec![],
        );
        let output = builtin("APC CardWipeCam")
            .render(&mut target, &params, &raw, rect)
            .unwrap();
        target.submit();
        let got = Pixels::download(&gpu, &output);
        assert_eq!(got.get(14, 11), expected);
    }
}

#[test]
fn cc_wipes_have_exact_source_and_reveal_endpoints() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(32, 16);
    let image = test_image(r);
    let clear = Pixels::new(r, |_, _| [0.; 4]);
    for (name, id, end, want) in [
        ("CC Jaws", 1, 1., &clear),
        ("CS LineSweep", 1, 100., &clear),
        ("CC Twister", 2, 1., &image),
        ("CC Glass Wipe", 1, 100., &image),
        ("CC WarpoMatic", 1, 100., &image),
    ] {
        for (v, expected) in [(0., &image), (end, want)] {
            let result = run(
                &gpu,
                F32,
                builtin(name),
                vec![(id, Value::Float(v))],
                &image,
                r,
                1.,
            );
            assert_close(&result, expected, r, 0.0001, &format!("{name} at {v}"));
        }
    }
}

#[test]
fn glass_refracts_between_two_distinct_layers() {
    use bae_effects::Standalone;
    use bae_model::PrimPath;
    use bae_render::{EffectParams, effect::LayerDependency};
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(8, 8);
    let first = Pixels::new(r, |_, _| [1., 0., 0., 1.]).upload(&gpu, F32);
    let second = Pixels::new(r, |_, _| [0., 1., 0., 1.]).upload(&gpu, F32);
    let gradient = Pixels::new(r, |_, _| [0.5, 0.5, 0.5, 1.]).upload(&gpu, F32);
    let reveal = PrimPath::new("/comps/main/layers/b").unwrap();
    let control = PrimPath::new("/comps/main/layers/c").unwrap();
    let inst = instance(
        "CC Glass Wipe",
        vec![
            (1, Value::Float(50.)),
            (2, Value::PrimRef(reveal.clone())),
            (3, Value::PrimRef(control.clone())),
        ],
    );
    let p = EffectParams {
        layer_bounds: r,
        ..EffectParams::new(&inst)
    };
    let mut target = Standalone::new(gpu.clone(), F32);
    target.set_effect_inputs(
        vec![
            (LayerDependency::source(reveal, p.comp_time), second),
            (LayerDependency::source(control, p.comp_time), gradient),
        ],
        vec![],
    );
    let out = builtin("CC Glass Wipe")
        .render(&mut target, &p, &first, r)
        .unwrap();
    target.finish().unwrap();
    let pixel = Pixels::download(&gpu, &out).get(4, 4);
    assert!(
        (pixel[0] - 0.5).abs() < 0.01 && (pixel[1] - 0.5).abs() < 0.01,
        "{pixel:?}"
    );
}

#[test]
fn line_sweep_default_direction_sweeps_top_to_bottom() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(64, 64);
    let input = Pixels::new(r, |_, _| [1., 1., 1., 1.]);
    // AE: 0° is up and clockwise, so the default 180° erases from the top.
    let out = run(
        &gpu,
        F32,
        builtin("CS LineSweep"),
        vec![(1, Value::Float(40.)), (4, Value::Float(99.))],
        &input,
        r,
        1.,
    );
    assert_eq!(out.get(32, 4)[3], 0., "top");
    assert_eq!(out.get(32, 60)[3], 1., "bottom");
}
