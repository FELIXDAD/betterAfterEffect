//! Motion blur on the GPU (docs/spec/ae/compositing-3d-color.md AE-CMP-064,
//! transform-keyframes-expressions.md AE-ANI-102 … 105): equal-weight averages
//! of the layer at every sample placement against a CPU reference, AE's
//! adaptive sample count for 2D layers, camera motion on 3D layers, and the
//! layer + comp switches through the evaluator.

use std::sync::Arc;

use bae_gpu::GpuContext;
use bae_model::{FrameRate, PrimPath, Time, Value};
use bae_render::camera::Camera;
use bae_render::fixture::SceneBuilder;
use bae_render::{
    AnimEvaluator, CompEvaluator, CompMotionBlur, LayerContent, LayerSample, OutputAlpha,
    RenderComp, RenderLayer, RenderRequest, Renderer,
};
use glam::{DAffine2, DMat4, DVec2, DVec3};

fn gpu() -> Option<Arc<GpuContext>> {
    bae_gpu::testing::shared_context()
}

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

struct Image {
    w: u32,
    data: Vec<u8>,
}

impl Image {
    fn px(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.w + x) * 4) as usize;
        self.data[i..i + 4].try_into().unwrap()
    }
}

fn render(r: &mut Renderer, comp: &RenderComp) -> Image {
    let frame = r.render_frame(comp, &RenderRequest::full()).unwrap();
    let img = r.read_rgba8(&frame, OutputAlpha::Premultiplied).unwrap();
    Image {
        w: img.width,
        data: img.data,
    }
}

fn solid(size: [u32; 2]) -> RenderLayer {
    RenderLayer::new(
        p("/comps/main/layers/a"),
        LayerContent::Solid {
            color: [1.0, 1.0, 1.0, 1.0],
            width: size[0],
            height: size[1],
        },
    )
}

fn comp_with_samples(n: usize) -> RenderComp {
    let mut comp = RenderComp::new(p("/comps/main"), 100, 40);
    comp.motion_blur = Some(Arc::new(CompMotionBlur {
        times: (0..n as i64).map(Time::from_flicks).collect(),
        cameras: vec![None; n],
        adaptive_limit: 128,
    }));
    comp
}

/// Length of `[a0, a1) ∩ [b0, b1)`.
fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// Expected alpha (0–255) of pixel column `x` for a 20 px wide layer whose left
/// edge sits at each of `edges`.
fn expected(x: u32, edges: &[f64]) -> f64 {
    let x = f64::from(x);
    edges
        .iter()
        .map(|e| overlap(x, x + 1.0, *e, e + 20.0))
        .sum::<f64>()
        / edges.len() as f64
        * 255.0
}

#[test]
fn horizontal_motion_averages_every_placement() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = comp_with_samples(16);
    let mut layer = solid([20, 20]);
    // 16 samples 2 px apart (shutter positions k/16 of 32 px).
    let at = |x: f64| DAffine2::from_translation(DVec2::new(x, 10.0));
    layer.transform = at(26.0);
    layer.motion_samples = (0..16)
        .map(|k| LayerSample {
            transform: at(10.0 + 32.0 * f64::from(k) / 16.0),
            matrix_3d: None,
        })
        .collect();
    comp.layers.push(layer);
    let img = render(&mut r, &comp);
    // 32 px of travel: AE samples a 2D layer every 1.9 px or closer, so 17
    // placements at shutter positions j/17 instead of 16.
    let edges: Vec<f64> = (0..17).map(|j| 10.0 + 32.0 * f64::from(j) / 17.0).collect();
    let mut worst = 0.0f64;
    for x in 0..100 {
        let a = f64::from(img.px(x, 20)[3]);
        worst = worst.max((a - expected(x, &edges)).abs());
        // Rows outside the layer stay empty.
        assert_eq!(img.px(x, 5)[3], 0);
    }
    assert!(worst <= 1.5, "max alpha error {worst}");
    // Smeared over the travel: partial coverage on both sides.
    assert!(img.px(15, 20)[3] > 0 && img.px(15, 20)[3] < 128);
    // The smear (32 px) is longer than the layer (20 px): no pixel is ever
    // covered for the whole shutter.
    assert!(img.px(40, 20)[3] < 200);
}

#[test]
fn layers_that_do_not_move_stay_sharp() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = comp_with_samples(8);
    let mut layer = solid([20, 20]);
    layer.transform = DAffine2::from_translation(DVec2::new(10.3, 10.0));
    let plain = {
        let mut c = comp.clone();
        c.layers.push(layer.clone());
        render(&mut r, &c)
    };
    layer.motion_samples = vec![
        LayerSample {
            transform: layer.transform,
            matrix_3d: None
        };
        8
    ]
    .into();
    comp.layers.push(layer);
    assert_eq!(render(&mut r, &comp).data, plain.data);
}

#[test]
fn camera_motion_blurs_3d_layers_with_fixed_samples() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let n = 16;
    let mut comp = comp_with_samples(n);
    let zoom = Camera::default_for(100, 40).zoom;
    let cam = |dx: f64| {
        Camera::new(
            DMat4::from_translation(DVec3::new(50.0 + dx, 20.0, -zoom)),
            zoom,
        )
    };
    // The camera pans 32 px to the left over the shutter; a static 3D layer at
    // z = 0 slides 32 px to the right in the frame.
    let mb = CompMotionBlur {
        times: (0..n as i64).map(Time::from_flicks).collect(),
        cameras: (0..n)
            .map(|k| Some(cam(-32.0 * (k as f64 + 0.5) / n as f64)))
            .collect(),
        adaptive_limit: 128,
    };
    comp.motion_blur = Some(Arc::new(mb));
    comp.camera = Some(cam(-16.0));
    let mut layer = solid([20, 20]);
    let m = DMat4::from_translation(DVec3::new(10.0, 10.0, 0.0));
    layer.matrix_3d = Some(m);
    layer.transform = DAffine2::from_translation(DVec2::new(10.0, 10.0));
    layer.motion_samples = vec![
        LayerSample {
            transform: layer.transform,
            matrix_3d: Some(m),
        };
        n
    ]
    .into();
    comp.layers.push(layer);
    let img = render(&mut r, &comp);
    // 3D layers keep Samples Per Frame (AE-ANI-104): 16 placements 2 px apart.
    let edges: Vec<f64> = (0..n)
        .map(|k| 10.0 + 32.0 * (k as f64 + 0.5) / n as f64)
        .collect();
    let mut worst = 0.0f64;
    for x in 0..100 {
        let a = f64::from(img.px(x, 20)[3]);
        worst = worst.max((a - expected(x, &edges)).abs());
    }
    assert!(worst <= 1.5, "max alpha error {worst}");
}

/// A 20×20 white solid moving 60 px to the right over frames 0 → 2.
fn moving_scene(comp_switch: bool, layer_switch: bool) -> (bae_model::Stage, PrimPath) {
    let mut b = SceneBuilder::new();
    let comp = b
        .comp("main", 120, 40, FrameRate::FPS_30, Time::ONE_SECOND)
        .unwrap();
    b.set(&comp, "motion_blur", Value::Bool(comp_switch))
        .unwrap();
    let layer = b
        .solid_layer(&comp, "a", [1.0, 1.0, 1.0, 1.0], [20, 20], [10.0, 10.0])
        .unwrap();
    b.set(&layer, "motion_blur", Value::Bool(layer_switch))
        .unwrap();
    let frame = FrameRate::FPS_30.frame_duration();
    b.key(
        &layer,
        "transform:position",
        Time::ZERO,
        Value::Vec3([20.0, 20.0, 0.0]),
    )
    .unwrap();
    b.key(
        &layer,
        "transform:position",
        Time::from_flicks(2 * frame.flicks()),
        Value::Vec3([80.0, 20.0, 0.0]),
    )
    .unwrap();
    (b.into_stage(), comp)
}

#[test]
fn both_switches_gate_motion_blur() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let frame1 = FrameRate::FPS_30.frame_duration();
    let partial = |img: &Image| {
        (0..120)
            .filter(|&x| (1..255).contains(&img.px(x, 20)[3]))
            .count()
    };
    for (comp_on, layer_on, blurred) in [
        (true, true, true),
        (true, false, false),
        (false, true, false),
    ] {
        let (stage, comp) = moving_scene(comp_on, layer_on);
        let mut eval = AnimEvaluator::new(None);
        let rc = eval.evaluate(&stage, &comp, frame1).unwrap();
        assert_eq!(rc.motion_blur.is_some(), blurred);
        let img = render(&mut r, &rc);
        // 30 px per frame, 180° shutter: a 15 px smear at both edges.
        let n = partial(&img);
        if blurred {
            assert!(
                (26..=34).contains(&n),
                "comp {comp_on} layer {layer_on}: {n}"
            );
            // Phase −90°: 16 samples from 7.5 px before the frame-time position
            // (left edge 40) on, 15/16 px apart (AE's t_open + dur·k/N).
            assert_eq!(img.px(50, 20)[3], 255);
            let edges: Vec<f64> = (0..16).map(|k| 32.5 + 15.0 * f64::from(k) / 16.0).collect();
            for x in [40, 59] {
                let a = f64::from(img.px(x, 20)[3]);
                let want = expected(x, &edges);
                assert!((a - want).abs() <= 1.5, "x = {x}: {a} vs {want}");
            }
        } else {
            assert_eq!(n, 0, "comp {comp_on} layer {layer_on}: sharp");
        }
    }
}
