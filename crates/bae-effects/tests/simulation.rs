mod common;

use bae_effects::Standalone;
use bae_gpu::wgpu::TextureFormat;
use bae_model::{Time, Value};
use bae_render::{EffectParams, IRect};
use common::{Pixels, builtin, context, instance, test_image};

#[test]
fn playground_grid_emits_with_the_cannon_off() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(64, 48);
    let input = test_image(rect).upload(&gpu, TextureFormat::Rgba16Float);
    let mut target = Standalone::new(gpu.clone(), TextureFormat::Rgba16Float);
    let emit = instance(
        "ADBE Playgnd",
        vec![
            (103, Value::Float(0.)),
            (132, Value::Float(32.)),
            (133, Value::Float(24.)),
            (134, Value::Float(2.)),
            (135, Value::Float(2.)),
            (
                501,
                Value::PrimRef(bae_model::PrimPath::new("/comps/main/layers/map").unwrap()),
            ),
            (502, Value::Int(3)),
        ],
    );
    let params = EffectParams::new(&emit);
    let fx = builtin(&emit.effect);
    let dep = fx.effect().layer_dependencies(&params).pop().unwrap();
    let map = Pixels::new(rect, |_, _| [0.75, 0., 0., 1.]).upload(&gpu, TextureFormat::Rgba16Float);
    target.set_effect_inputs(vec![(dep, map)], vec![]);
    let out = fx.render(&mut target, &params, &input, rect).unwrap();
    target.finish().unwrap();
    let pixels = Pixels::download(&gpu, &out);
    for y in [18, 30] {
        for x in [24, 40] {
            assert!(pixels.get(x, y)[3] > 0.9);
            assert!((pixels.get(x, y)[1] - 0.75).abs() < 0.01);
        }
    }
    assert_eq!(pixels.get(32, 24), [0.; 4]);
}

#[test]
fn particle_world_seek_reproduces_pixels_and_cropped_tiles() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(128, 96);
    let input = test_image(rect).upload(&gpu, TextureFormat::Rgba16Float);
    let inst = instance(
        "CC Particle World",
        vec![
            (4, Value::Float(5.)),
            (3, Value::Float(0.08)),
            (66, Value::Int(4)),
        ],
    );
    let mut target = Standalone::new(gpu.clone(), TextureFormat::Rgba16Float);
    let mut render = |frame, roi| {
        let mut p = EffectParams::new(&inst);
        p.time = p.frame_rate.frame_to_time(frame);
        let out = builtin(&inst.effect)
            .render(&mut target, &p, &input, roi)
            .unwrap();
        target.finish().unwrap();
        Pixels::download(&gpu, &out)
    };
    let a = render(74, rect);
    assert!(a.data.iter().any(|px| px[3] > 0.1));
    assert_ne!(a.data, render(75, rect).data);
    for frame in (0..18)
        .map(|i| i * 90)
        .chain([45, 12, 845, 60, 70, 71, 72, 73])
    {
        let _ = render(frame, rect);
    }
    assert_eq!(a.data, render(74, rect).data);
    let tile = IRect::new(41, 38, 87, 66);
    common::assert_close(&render(74, tile), &a, tile, 0.004, "particle tile");
}

#[test]
fn long_lived_floor_particles_match_after_a_backward_seek() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(80, 64);
    let input = test_image(rect).upload(&gpu, TextureFormat::Rgba16Float);
    let inst = instance(
        "CC Particle World",
        vec![
            (4, Value::Float(0.00001)),
            (5, Value::Float(3000.)),
            (16, Value::Float(0.)),
            (18, Value::Float(0.02)),
            (23, Value::Int(4)),
            (24, Value::Float(2.)),
            (25, Value::Float(2.)),
            (66, Value::Int(4)),
        ],
    );
    let mut target = Standalone::new(gpu.clone(), TextureFormat::Rgba16Float);
    let mut render = |seconds| {
        let p = EffectParams {
            time: Time::from_seconds(seconds),
            ..EffectParams::new(&inst)
        };
        let out = builtin(&inst.effect)
            .render(&mut target, &p, &input, rect)
            .unwrap();
        target.finish().unwrap();
        Pixels::download(&gpu, &out).data
    };
    let late = render(2999.);
    assert!(late.iter().any(|p| p[3] > 0.1));
    assert_ne!(render(10.), late);
    assert_eq!(render(2999.), late);
}

#[test]
fn simulation_presets_render_finite_pixels_in_both_working_formats() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(80, 64);
    for format in [TextureFormat::Rgba16Float, TextureFormat::Rgba32Float] {
        let input = test_image(rect).upload(&gpu, format);
        let mut target = Standalone::new(gpu.clone(), format);
        for info in bae_effects::catalog().filter(|fx| fx.category == "Simulation") {
            let values = if info.match_name == "CC Scatterize" {
                vec![(1, Value::Float(8.))]
            } else {
                vec![]
            };
            let inst = instance(&info.match_name, values);
            let p = EffectParams {
                time: Time::from_seconds(0.375),
                ..EffectParams::new(&inst)
            };
            let out = builtin(&inst.effect)
                .render(&mut target, &p, &input, rect)
                .unwrap();
            target.finish().unwrap();
            let image = Pixels::download(&gpu, &out);
            assert!(
                image.data.iter().flatten().all(|v| v.is_finite()),
                "{}",
                inst.effect
            );
            assert!(image.data.iter().any(|p| p[3] > 0.), "{}", inst.effect);
        }
    }
}
