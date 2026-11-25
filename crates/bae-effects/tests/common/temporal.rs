use bae_model::{PrimPath, Time, Value};
use bae_render::{
    LayerContent, OutputAlpha, RenderComp, RenderEffect, RenderError, RenderLayer, RenderRequest,
    Renderer, graph::CompFrames,
};

pub(super) const CLIP: &str = "/comps/main/layers/clip";

pub(super) fn path(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

pub(super) fn effect(name: &str, match_name: &str, params: Vec<(u32, Value)>) -> RenderEffect {
    RenderEffect {
        path: path(&format!("{CLIP}/effects/{name}")),
        enabled: true,
        effect: match_name.into(),
        params,
    }
}

pub(super) fn solid(prim: &str, width: u32, color: [f32; 4]) -> RenderLayer {
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

pub(super) fn renderer() -> Option<Renderer> {
    let mut renderer = Renderer::new(bae_gpu::testing::shared_context()?);
    bae_effects::register(
        &mut bae_model::EffectRegistry::new(),
        renderer.effects_mut(),
    );
    Some(renderer)
}

pub(super) fn pixels(
    renderer: &mut Renderer,
    scene: impl Fn(Time) -> RenderComp,
    t: f64,
) -> Vec<u8> {
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
