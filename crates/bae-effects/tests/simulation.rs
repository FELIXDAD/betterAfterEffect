mod common;

use bae_effects::Standalone;
use bae_gpu::wgpu::TextureFormat;
use bae_model::{Time, Value};
use bae_render::{EffectParams, IRect};
use common::{Pixels, builtin, context, instance, test_image};

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
