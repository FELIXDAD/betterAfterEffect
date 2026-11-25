use std::sync::Arc;

use bae_model::{PrimPath, Time, Value};
use bae_render::{
    LayerContent, OutputAlpha, RenderComp, RenderEffect, RenderError, RenderLayer, RenderRequest,
    Renderer, graph::CompFrames,
};

#[path = "ae/categories/mod.rs"]
mod categories;
#[path = "ae/harness.rs"]
mod harness;

const CLIP: &str = "/comps/main/layers/clip";

fn path(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn effect(name: &str, match_name: &str, params: Vec<(u32, Value)>) -> RenderEffect {
    RenderEffect {
        path: path(&format!("{CLIP}/effects/{name}")),
        enabled: true,
        effect: match_name.into(),
        params,
    }
}

fn solid(prim: &str, width: u32, color: [f32; 4]) -> RenderLayer {
    RenderLayer::new(
        path(prim),
        LayerContent::Solid {
            color,
            width,
            height: 8,
        },
    )
}

struct Frames<F>(F);
impl<F: Fn(Time) -> RenderComp> CompFrames for Frames<F> {
    fn comp_at(&mut self, _comp: &PrimPath, time: Time) -> Result<Option<RenderComp>, RenderError> {
        Ok(Some((self.0)(time)))
    }
}

fn renderer() -> Option<Renderer> {
    let mut renderer = Renderer::new(bae_gpu::testing::shared_context()?);
    bae_effects::register(
        &mut bae_model::EffectRegistry::new(),
        renderer.effects_mut(),
    );
    Some(renderer)
}

fn pixels(renderer: &mut Renderer, scene: impl Fn(Time) -> RenderComp, t: f64) -> Vec<u8> {
    let t = Time::from_seconds(t);
    let comp = scene(t);
    let frame = renderer
        .render_frame_with(&comp, &RenderRequest::default(), Some(&mut Frames(scene)))
        .unwrap();
    renderer
        .read_rgba8(&frame, OutputAlpha::Premultiplied)
        .unwrap()
        .data
}

#[test]
fn particles_keep_their_birth_position_on_an_animated_producer() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let scene = |time: Time| {
        let mut comp = RenderComp::new(path("/comps/main"), 120, 8);
        comp.time = time;
        let mut layer = solid(CLIP, 120, [1.; 4]);
        layer.time = time;
        layer.effects.push(effect(
            "particles",
            "CC Particle Systems II",
            vec![
                (4, Value::Vec2([20. + time.as_seconds() * 60., 4.])),
                (5, Value::Float(0.)),
                (6, Value::Float(0.)),
                (10, Value::Float(0.)),
                (12, Value::Float(0.)),
                (18, Value::Int(4)),
                (19, Value::Float(1.)),
                (20, Value::Float(1.)),
            ],
        ));
        comp.layers.push(layer);
        comp
    };
    let frame = pixels(&mut renderer, scene, 1.);
    assert!(
        frame
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .any(|(i, c)| (28..35).contains(&(i % 120)) && c[3] > 32)
    );
    let _ = pixels(&mut renderer, scene, 2.4);
    assert_eq!(frame, pixels(&mut renderer, scene, 1.));
}

#[test]
fn particle_texture_clocks_match_in_playback_and_after_a_seek() {
    let Some(mut r) = renderer() else { return };
    for mode in [1, 2, 3] {
        let scene = |t: Time| {
            let mut comp = RenderComp::new(path("/comps/main"), 64, 8);
            comp.time = t;
            let mut emitter = solid(CLIP, 64, [1.; 4]);
            emitter.time = t;
            // Exactly one particle, born at 0.5 s, stationary for its whole life.
            let mut params = vec![
                (4, Value::Float(0.001)),
                (23, Value::Int(24)),
                (24, Value::Float(4.)),
                (25, Value::Float(4.)),
                (29, Value::Color([1.; 4])),
                (30, Value::Color([1.; 4])),
                (45, Value::PrimRef(path("/comps/main/layers/texture"))),
                (47, Value::Int(mode)),
            ];
            params.extend([10, 11, 12, 16, 18, 26, 42, 43].map(|id| (id, Value::Float(0.))));
            emitter
                .effects
                .push(effect("particles", "CC Particle World", params));
            let color = match t.as_seconds() {
                s if s < 0.5 => [1., 0., 0., 1.],
                s if s < 0.9 => [0., 1., 0., 1.],
                _ => [0., 0., 1., 1.],
            };
            let mut texture = solid("/comps/main/layers/texture", 64, color);
            texture.visible = false;
            texture.time = t;
            comp.layers = vec![emitter, texture];
            comp
        };
        let render = |r: &mut Renderer, frame| pixels(r, scene, f64::from(frame) / 24.);
        let cold = render(&mut r, 30);
        let channel = match mode {
            1 => 0,
            2 => 2,
            _ => 1,
        };
        assert!(
            cold.as_chunks::<4>().0.iter().any(|p| p[channel] > 128),
            "texture mode {mode}"
        );
        for frame in 0..30 {
            let image = render(&mut r, frame);
            if mode == 1 && frame > 12 {
                assert_eq!(image, cold, "birth texture changed at frame {frame}");
            }
        }
        assert_eq!(render(&mut r, 30), cold);
        let _ = render(&mut r, 180);
        assert_eq!(render(&mut r, 30), cold);
    }
}

#[test]
fn posterize_freezes_earlier_effects_in_layer_time() {
    let Some(mut r) = renderer() else { return };
    let result = pixels(
        &mut r,
        |time| {
            let mut comp = RenderComp::new(path("/comps/main"), 16, 8);
            comp.time = time;
            let mut layer = solid(CLIP, 16, [1.; 4]);
            let local = time.as_seconds() / 2.;
            layer.time = Time::from_seconds(local);
            layer.time_scale = 2.;
            layer.effects = vec![
                effect(
                    "fill",
                    "ADBE Fill",
                    vec![(2, Value::Color([local, 0., 0., 1.]))],
                ),
                effect("time", "ADBE Posterize Time", vec![(1, Value::Float(2.))]),
            ];
            comp.layers.push(layer);
            comp
        },
        1.5,
    );
    assert_eq!(&result[..4], &[128, 0, 0, 255]);
}

#[test]
fn echo_and_time_displacement_sample_separate_frames() {
    let Some(mut r) = renderer() else { return };
    let color = |t: Time| {
        if t.as_seconds() < 0.99 {
            [1., 0., 0., 1.]
        } else if t.as_seconds() > 1.01 {
            [0., 0., 1., 1.]
        } else {
            [0., 1., 0., 1.]
        }
    };
    let echo = pixels(
        &mut r,
        |time| {
            let mut comp = RenderComp::new(path("/comps/main"), 16, 8);
            comp.time = time;
            let mut layer = solid(CLIP, 16, color(time));
            layer.time = time;
            layer
                .effects
                .push(effect("echo", "ADBE Echo", vec![(1, Value::Float(-0.25))]));
            comp.layers.push(layer);
            comp
        },
        1.,
    );
    assert_eq!(&echo[..4], &[255, 255, 0, 255]);

    let displacement = pixels(
        &mut r,
        |time| {
            let mut comp = RenderComp::new(path("/comps/main"), 16, 8);
            comp.time = time;
            let mut layer = solid(CLIP, 16, color(time));
            layer.time = time;
            layer.effects.push(effect(
                "displace",
                "ADBE Time Displacement",
                vec![
                    (1, Value::PrimRef(path("/comps/main/layers/map"))),
                    (2, Value::Float(0.25)),
                    (3, Value::Float(8.)),
                ],
            ));
            let mut map = RenderComp::new(path("/comps/control"), 16, 8);
            map.layers = vec![
                solid("/comps/control/layers/black", 8, [0., 0., 0., 1.]),
                solid("/comps/control/layers/white", 16, [1.; 4]),
            ];
            let mut map = RenderLayer::new(
                path("/comps/main/layers/map"),
                LayerContent::Comp(Arc::new(map)),
            );
            map.visible = false;
            comp.layers = vec![layer, map];
            comp
        },
        1.,
    );
    assert_eq!(&displacement[0..4], &[255, 0, 0, 255]);
    assert_eq!(&displacement[48..52], &[0, 0, 255, 255]);
}

#[test]
fn scene_detection_stops_at_a_cut_in_both_directions() {
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    for (name, smooth, detect) in [
        ("ADBE AutoLevels", 1, 2),
        ("ADBE AutoContrast", 1, 2),
        ("ADBE AutoColor", 1, 2),
        ("ADBE ShadowHighlight", 4, 5),
    ] {
        let text = serde_json::json!({"cases":[{
            "name":"scene", "effect":name, "input":"frame-mid", "time":2. / 30.,
            "frames":["frame-cut","frame-cut","frame-mid","frame-mid","frame-cut"],
            "params":{}
        }]});
        let mut case = harness::parse_cases(&text.to_string()).remove(0);
        let single = harness::render(&gpu, &case);
        case.params.push((smooth, Value::Float(0.16)));
        let crossed = harness::render(&gpu, &case);
        case.params.push((detect, Value::Bool(true)));
        let gated = harness::render(&gpu, &case);
        let delta = |image: &[[u8; 4]]| {
            image
                .iter()
                .zip(&single)
                .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)))
                .max()
                .unwrap()
        };
        assert!(delta(&crossed) > 2, "{name} includes the other scene");
        assert!(delta(&gated) <= 1, "{name} excludes the other scene");
    }
}

#[test]
fn wide_time_averages_frames_and_time_difference_compares_them() {
    let Some(mut r) = renderer() else { return };
    // Red before 1 s, blue from then on.
    let scene = |match_name: &'static str, params: Vec<(u32, Value)>| {
        move |time: Time| {
            let mut comp = RenderComp::new(path("/comps/main"), 16, 8);
            comp.time = time;
            let color = if time.as_seconds() < 0.99 {
                [1., 0., 0., 1.]
            } else {
                [0., 0., 1., 1.]
            };
            let mut layer = solid(CLIP, 16, color);
            layer.time = time;
            layer
                .effects
                .push(effect("time", match_name, params.clone()));
            comp.layers.push(layer);
            comp
        }
    };
    // One frame back (red), now and one frame on (blue), a third each.
    let wide = pixels(&mut r, scene("CC Wide Time", Vec::new()), 1.);
    let near = |a: &[u8], b: [u8; 4]| a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2);
    assert!(near(&wide[..4], [85, 0, 170, 255]), "{:?}", &wide[..4]);
    // Blue now against red one frame earlier: 50 % grey ± the difference.
    let difference = pixels(
        &mut r,
        scene("ADBE Difference", vec![(2, Value::Float(-1. / 30.))]),
        1.,
    );
    assert!(
        near(&difference[..4], [0, 128, 255, 255]),
        "{:?}",
        &difference[..4]
    );
}
