mod common;

use bae_effects::Standalone;
use bae_gpu::wgpu::TextureFormat;
use bae_model::{Time, Value};
use bae_render::{EffectParams, IRect};
use common::{Pixels, builtin, context, instance, test_image};

#[test]
fn foam_draft_shows_the_selected_flow_map_before_emission() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(40, 32);
    let format = TextureFormat::Rgba16Float;
    let source = test_image(rect).upload(&gpu, format);
    let flow = Pixels::new(rect, |_, _| [0.125, 0.75, 0.5, 1.]).upload(&gpu, format);
    let emit = instance(
        "APC Foam",
        vec![
            (2, Value::Int(2)),
            (14, Value::Float(0.)),
            (
                64,
                Value::PrimRef(bae_model::PrimPath::new("/comps/main/layers/flow").unwrap()),
            ),
            (68, Value::Int(2)),
        ],
    );
    let p = EffectParams::new(&emit);
    let fx = builtin(&emit.effect);
    let mut target = Standalone::new(gpu.clone(), format);
    let dep = fx.effect().layer_dependencies(&p).pop().unwrap();
    target.set_effect_inputs(vec![(dep, flow)], vec![]);
    let out = fx.render(&mut target, &p, &source, rect).unwrap();
    target.finish().unwrap();
    assert_eq!(
        Pixels::download(&gpu, &out).get(16, 16),
        [0.125, 0.75, 0.5, 1.]
    );
}

#[test]
fn caustics_refracts_the_wave_world_height_map() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(128, 96);
    let format = TextureFormat::Rgba16Float;
    let input = test_image(rect).upload(&gpu, format);
    let mut target = Standalone::new(gpu.clone(), format);
    let wave = instance("APC Wave World", vec![(2, Value::Int(1))]);
    let p = EffectParams {
        time: Time::from_seconds(1.3),
        ..EffectParams::new(&wave)
    };
    let field = builtin(&wave.effect)
        .render(&mut target, &p, &input, rect)
        .unwrap();
    let layer = bae_model::PrimPath::new("/comps/main/layers/water").unwrap();
    let light = instance(
        "APC Caustics",
        vec![(10, Value::PrimRef(layer.clone())), (82, Value::Float(1.))],
    );
    let params = EffectParams::new(&light);
    let fx = builtin(&light.effect);
    let dep = fx
        .effect()
        .layer_dependencies(&params)
        .into_iter()
        .find(|d| d.layer == layer)
        .unwrap();
    assert_eq!(dep.stage, bae_render::effect::LayerStage::EffectsAndMasks);
    let flat = fx.render(&mut target, &params, &input, rect).unwrap();
    target.set_effect_inputs(vec![(dep, field)], vec![]);
    let refracted = fx.render(&mut target, &params, &input, rect).unwrap();
    target.finish().unwrap();
    let flat = Pixels::download(&gpu, &flat);
    let refracted = Pixels::download(&gpu, &refracted);
    assert!(
        flat.data
            .iter()
            .zip(refracted.data)
            .filter(|(a, b)| (a[0] - b[0]).abs() > 0.01)
            .count()
            > 100
    );
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
