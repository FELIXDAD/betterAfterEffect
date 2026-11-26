//! `sampleImage` read-backs (`Renderer::read_samples`): a layer's pixels in its
//! layer space, with or without its masks, averaged over the sample area.

use bae_anim::{ImageSamples, SampleKey};
use bae_model::{PrimPath, Time};
use bae_render::fixture::rect_path;
use bae_render::{
    LayerContent, MaskMode, RenderComp, RenderLayer, RenderMask, RenderRequest, Renderer,
};

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

#[test]
fn samples_a_layer_with_and_without_its_masks() {
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let mut comp = RenderComp::new(p("/comps/main"), 64, 32);
    let mut a = RenderLayer::new(
        p("/comps/main/layers/a"),
        LayerContent::Solid {
            color: [1.0, 0.0, 0.0, 1.0],
            width: 64,
            height: 32,
        },
    );
    // Moved and hidden: samples are in the layer's own space, whatever is shown.
    a.visible = false;
    a.transform = glam::DAffine2::from_translation(glam::DVec2::new(100.0, 0.0));
    a.masks = vec![RenderMask {
        path: p("/comps/main/layers/a/masks/m"),
        shape: rect_path(0.0, 0.0, 32.0, 32.0),
        mode: MaskMode::Add,
        inverted: false,
        opacity: 1.0,
        feather: [0.0, 0.0],
        expansion: 0.0,
    }];
    comp.layers = vec![a];

    let key = |center: [f64; 2], radius: [f64; 2], post: bool| {
        SampleKey::new(
            p("/comps/main"),
            p("/comps/main/layers/a"),
            Time::ZERO,
            center,
            radius,
            post,
        )
    };
    let cases = [
        (key([10.5, 10.5], [0.5, 0.5], true), [1.0, 0.0, 0.0, 1.0]),
        (key([40.5, 10.5], [0.5, 0.5], true), [0.0; 4]),
        (key([40.5, 10.5], [0.5, 0.5], false), [1.0, 0.0, 0.0, 1.0]),
        (key([32.0, 10.5], [1.0, 0.5], true), [1.0, 0.0, 0.0, 0.5]),
        (key([70.0, 10.0], [2.0, 2.0], false), [0.0; 4]),
    ];
    let samples = ImageSamples::new();
    for (k, _) in &cases {
        assert_eq!(samples.get(k), None);
    }
    let mut r = Renderer::new(gpu);
    r.read_samples(&samples, &comp, &RenderRequest::default(), None)
        .unwrap();
    assert!(samples.is_waiting());
    r.wait_samples(&samples).unwrap();
    assert!(!samples.is_waiting());
    for (k, want) in &cases {
        let got = samples.get(k).expect("filled in");
        let close = got.iter().zip(want).all(|(g, w)| (g - w).abs() < 2e-3);
        assert!(close, "{:?}: {got:?}, want {want:?}", k.area());
    }
}

/// The whole round trip: an expression samples a layer, the renderer reads it
/// back, and the next evaluation uses the value (what export does per frame).
#[test]
fn expression_samples_round_trip() {
    use bae_model::{AuthorId, CommandRegistry, FrameRate};
    use bae_render::fixture::SceneBuilder;
    use bae_render::{AnimEvaluator, CompEvaluator};
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        eprintln!("no GPU adapter; skipping");
        return;
    };
    let mut b = SceneBuilder::new();
    let comp = b
        .comp("main", 100, 100, FrameRate::FPS_30, Time::from_seconds(2.0))
        .unwrap();
    b.solid_layer(&comp, "a", [1.0, 0.5, 0.0, 1.0], [100, 100], [0.0, 0.0])
        .unwrap();
    let probe = b
        .solid_layer(&comp, "probe", [0.0, 0.0, 1.0, 1.0], [10, 10], [0.0, 0.0])
        .unwrap();
    let mut stage = b.into_stage();
    CommandRegistry::with_builtins()
        .execute(
            &mut stage,
            &AuthorId::system(),
            "attr.set_expression",
            serde_json::json!({
                "path": probe.as_str(),
                "attr": "transform:opacity",
                "source": r#"thisComp.layer("a").sampleImage([50, 50], [4, 4])[1] * 100"#,
            }),
        )
        .unwrap();
    let samples = std::sync::Arc::new(ImageSamples::new());
    let mut eval = AnimEvaluator::new(None);
    eval.set_image_samples(Some(samples.clone()));
    let opacity = |rc: &RenderComp| {
        rc.layers
            .iter()
            .find(|l| l.path == probe)
            .map(|l| l.opacity)
    };
    let rc = eval.evaluate(&stage, &comp, Time::ZERO).unwrap();
    assert_eq!(
        opacity(&rc),
        Some(0.0),
        "transparent black before the read-back"
    );
    assert!(samples.has_requests());
    let mut r = Renderer::new(gpu);
    r.read_samples(&samples, &rc, &RenderRequest::default(), None)
        .unwrap();
    r.wait_samples(&samples).unwrap();
    let rc = eval.evaluate(&stage, &comp, Time::ZERO).unwrap();
    let got = opacity(&rc).expect("probe layer");
    assert!((got - 0.5).abs() < 2e-3, "{got}");
    assert!(!samples.has_requests() && !samples.is_waiting());
}
