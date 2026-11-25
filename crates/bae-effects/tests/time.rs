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
fn write_on_evaluates_subframe_history_from_the_layer_in_point() {
    let Some(mut r) = renderer() else { return };
    let scene = |time: Time| {
        let mut comp = RenderComp::new(path("/comps/main"), 32, 8);
        comp.time = time;
        let mut layer = solid(CLIP, 32, [1.; 4]);
        layer.in_point = Time::from_seconds(0.25);
        let elapsed = (time - layer.in_point).as_seconds();
        layer.time = Time::from_seconds(elapsed);
        layer.effects.push(effect(
            "write",
            "ADBE Write-on",
            vec![
                (1, Value::Vec2([4. + elapsed * 160., 4.])),
                (7, Value::Float(0.05)),
                (10, Value::Int(2)),
            ],
        ));
        comp.layers.push(layer);
        comp
    };
    let result = pixels(&mut r, scene, 0.36);
    let alpha = |x: usize| result[(3 * 32 + x) * 4 + 3];
    assert!(alpha(4) > 200 && alpha(12) > 200 && alpha(20) > 200);
    assert_eq!(alpha(8), 0);
}
