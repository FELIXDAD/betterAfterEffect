//! Shape layers through bae-render's render graph: provider lookup, caching and
//! ROI negotiation for content drawn around the layer origin.

use std::sync::Arc;

use bae_model::PrimPath;
use bae_render::graph::{self, NodeKind, SourceResolver};
use bae_render::{
    CustomContent, EffectLibrary, FootageRef, LayerContent, LayerSource, RenderComp, RenderError,
    RenderLayer, RenderRequest, SourceProvider, SourceProviders,
};
use bae_vector::render::ShapeProvider;
use bae_vector::{
    BlendMode, Brush, Composite, Direction, FillRule, Paint, PaintKind, PathItem, PathSource,
    RectShape, SHAPE_PROVIDER, ShapeGroup, ShapeItem, VectorScene,
};
use glam::DAffine2;

struct Providers(SourceProviders);

impl SourceResolver for Providers {
    fn footage(&mut self, _: &FootageRef) -> Result<Option<Arc<dyn LayerSource>>, RenderError> {
        Ok(None)
    }

    fn custom(&mut self, content: &CustomContent) -> Result<Arc<dyn LayerSource>, RenderError> {
        self.0.resolve(content)
    }
}

fn square_scene() -> VectorScene {
    let root = ShapeGroup::new()
        .with(ShapeItem::Path(PathItem {
            source: PathSource::Rect(RectShape {
                size: [100.0, 100.0],
                position: [0.0, 0.0],
                roundness: 0.0,
            }),
            direction: Direction::Normal,
        }))
        .with(ShapeItem::Paint(Paint {
            kind: PaintKind::Fill(FillRule::NonZero),
            brush: Brush::Solid([1.0, 1.0, 1.0, 1.0]),
            opacity: 1.0,
            blend: BlendMode::Normal,
            composite: Composite::Below,
        }));
    VectorScene::from_group(&root)
}

#[test]
fn shape_layer_centred_on_its_position_is_not_clipped() {
    let path = PrimPath::new("/comps/main/layers/shape").unwrap();
    let scene = square_scene();
    let content = CustomContent {
        provider: SHAPE_PROVIDER.to_owned(),
        prim: path.clone(),
        size: None,
        hash: scene.content_hash(),
        data: Arc::new(scene),
    };
    let mut layer = RenderLayer::new(path, LayerContent::Custom(content));
    // AE default for a new shape layer: anchor (0, 0), position = comp centre.
    layer.transform = DAffine2::from_translation(glam::DVec2::new(320.0, 240.0));
    let mut comp = RenderComp::new(PrimPath::new("/comps/main").unwrap(), 640, 480);
    comp.layers.push(layer);

    let mut providers = SourceProviders::new();
    let provider = Arc::new(ShapeProvider::new());
    providers.register(provider.clone());
    let mut resolver = Providers(providers);
    let g = graph::build(
        &comp,
        &RenderRequest::full(),
        &mut resolver,
        &EffectLibrary::new(),
    )
    .unwrap();
    let request = g
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            NodeKind::Source(s) => Some(s.request),
            _ => None,
        })
        .expect("a source node");
    // The whole square around the layer origin is requested, negative half included.
    assert!(
        request.roi.x0 <= -50 && request.roi.y0 <= -50,
        "{request:?}"
    );
    assert!(request.roi.x1 >= 50 && request.roi.y1 >= 50, "{request:?}");

    // Sources are cached by content hash.
    let LayerContent::Custom(c) = &comp.layers[0].content else {
        unreachable!()
    };
    let a = provider.source(c).unwrap();
    let b = provider.source(c).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
}

#[test]
fn provider_rejects_unknown_data() {
    let provider = ShapeProvider::new();
    let content = CustomContent {
        provider: SHAPE_PROVIDER.to_owned(),
        prim: PrimPath::new("/x").unwrap(),
        size: None,
        hash: 1,
        data: Arc::new(42_u32),
    };
    assert!(provider.source(&content).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn overlapping_affine_images_keep_their_coverage_in_one_draw() {
    use bae_render::{OutputAlpha, Renderer};
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    if gpu.adapter_info().backend != bae_gpu::wgpu::Backend::Metal {
        return;
    }
    let mut renderer = Renderer::new(gpu);
    renderer
        .providers_mut()
        .register(Arc::new(ShapeProvider::new()));
    let path = PrimPath::new("/comps/main").unwrap();
    let mut comp = RenderComp::new(path, 180, 140);
    for (index, x) in [(0, 45.), (1, 125.)] {
        let path = PrimPath::new(&format!("/comps/main/layers/shape_{index}")).unwrap();
        let content = CustomContent {
            provider: SHAPE_PROVIDER.into(),
            prim: path.clone(),
            size: None,
            hash: index,
            data: Arc::new(square_scene()),
        };
        let mut layer = RenderLayer::new(path, LayerContent::Custom(content));
        layer.transform =
            DAffine2::from_translation(glam::DVec2::new(x, 70.)) * DAffine2::from_angle(0.1);
        layer.opacity = 0.5;
        comp.layers.push(layer);
    }
    let frame = renderer
        .render_frame(&comp, &RenderRequest::full())
        .unwrap();
    assert_eq!(frame.stats.draw_calls, 1);
    let image = renderer
        .read_rgba8(&frame, OutputAlpha::Premultiplied)
        .unwrap();
    for (x, y, alpha) in [(0, 0, 0), (30, 70, 128), (85, 70, 191), (145, 70, 128)] {
        assert!(
            image.pixel(x, y)[3].abs_diff(alpha) <= 1,
            "pixel ({x}, {y})"
        );
    }
}
