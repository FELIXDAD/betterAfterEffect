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
