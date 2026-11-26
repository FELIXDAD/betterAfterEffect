mod common;
#[path = "immersive/generators.rs"]
mod generators;
use bae_model::Value;
use bae_render::{EffectParams, IRect};
use common::*;

#[test]
fn rotation_wraps_each_eye_independently() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(64, 32);
    let input = Pixels::new(rect, |x, y| {
        [x as f32 / 64., f32::from(u8::from(y >= 16)), 0., 1.]
    });
    let output = run(
        &gpu,
        F32,
        builtin("Mettle SkyBox Rotate Sphere"),
        vec![(6, Value::Int(2)), (3, Value::Float(180.))],
        &input,
        rect,
        1.,
    );
    for y in [0, 7, 15, 16, 24, 31] {
        for x in [0, 15, 32, 63] {
            let expected = input.get((x + 32) % 64, y);
            let pixel = output.get(x, y);
            assert!(
                (pixel[0] - expected[0]).abs() < 0.003 && (pixel[1] - expected[1]).abs() < 0.001
            );
        }
    }
}

#[test]
fn converter_reads_the_whole_panorama_for_a_cube_tile() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(128, 64);
    let tile = IRect::new(96, 96, 99, 99);
    let fx = builtin("Mettle SkyBox Converter");
    let params = vec![(2, Value::Int(4)), (3, Value::Int(1)), (4, Value::Int(256))];
    let inst = instance(&fx.info.match_name, params.clone());
    let p = EffectParams::new(&inst);
    assert_eq!(
        fx.effect().output_bounds(&p, rect),
        IRect::from_size(256, 192)
    );
    assert_eq!(fx.effect().required_input(&p, tile, rect), rect);
    let input = Pixels::new(rect, |x, y| [x as f32 / 127., y as f32 / 63., 0., 1.]);
    let output = run(&gpu, F32, fx, params, &input, tile, 1.);
    let center = output.get(96, 96);
    assert!((center[0] - 0.5).abs() < 0.02 && (center[1] - 0.5).abs() < 0.02);
}

#[test]
fn plane_projection_and_viewer_share_the_forward_direction() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(256, 128);
    let input = Pixels::new(rect, |_, _| [0.7, 0.2, 0.1, 1.]);
    let sphere = run(
        &gpu,
        F32,
        builtin("Mettle SkyBox Project 2D"),
        vec![],
        &input,
        rect,
        1.,
    );
    assert!(sphere.get(128, 64)[3] > 0.99);
    assert_eq!(sphere.get(0, 64)[3], 0.);
    let view = run(
        &gpu,
        F32,
        builtin("Mettle SkyBox Viewer"),
        vec![(3, Value::Int(256))],
        &sphere,
        rect,
        1.,
    );
    assert!((view.get(128, 64)[0] - 0.7).abs() < 0.001);
    assert_eq!(view.get(0, 64)[3], 0.);
}

#[path = "immersive/filters.rs"]
mod filters;
