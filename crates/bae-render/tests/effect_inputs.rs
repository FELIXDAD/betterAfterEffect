//! Effects reading other layers, their own masks and other frames
//! (`Effect::layer_dependencies` / `mask_dependencies`, `CompFrames`): stages
//! Source / Masks / Effects & Masks, circular references, mask coverage, and
//! another comp time.

use std::sync::Arc;

use bae_gpu::{GpuContext, wgpu};
use bae_model::{EffectDef, PrimPath, Time, Value};
use bae_render::effect::{LayerDependency, LayerStage, MaskSelect};
use bae_render::fixture::rect_path;
use bae_render::graph::CompFrames;
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, LayerContent, MaskMode, OutputAlpha,
    RenderComp, RenderEffect, RenderError, RenderLayer, RenderMask, RenderRequest, Renderer,
};

fn gpu() -> Option<Arc<GpuContext>> {
    bae_gpu::testing::shared_context()
}

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn def(match_name: &str) -> EffectDef {
    EffectDef {
        match_name: match_name.into(),
        name: match_name.into(),
        category: String::new(),
        plugin: None,
        params: Vec::new(),
        thread_safe: true,
        gpu: true,
    }
}

/// Test effect: outputs the image it asked for. Param 1: layer, 2: stage
/// (0 source, 1 masks, 2 effects & masks), 3: time offset in seconds, 4: read
/// all of its own masks instead.
struct Read(EffectDef);

fn dependency(params: &EffectParams<'_>) -> Option<LayerDependency> {
    let Some(Value::PrimRef(layer)) = params.value(1) else {
        return None;
    };
    let stage = match params.popup(2) {
        Some(1) => LayerStage::Masks,
        Some(2) => LayerStage::EffectsAndMasks,
        _ => LayerStage::Source,
    };
    let offset = params.float(3).unwrap_or(0.0);
    Some(LayerDependency {
        layer: layer.clone(),
        time: params.comp_time + Time::from_seconds(offset),
        stage,
    })
}

impl Effect for Read {
    fn def(&self) -> &EffectDef {
        &self.0
    }

    fn layer_dependencies(&self, params: &EffectParams<'_>) -> Vec<LayerDependency> {
        dependency(params).into_iter().collect()
    }

    fn mask_dependencies(&self, params: &EffectParams<'_>) -> Vec<MaskSelect> {
        if params.bool(4) == Some(true) {
            vec![MaskSelect::All]
        } else {
            Vec::new()
        }
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        _output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let got = if params.bool(4) == Some(true) {
            ctx.effect_mask(&MaskSelect::All)
        } else {
            dependency(params).and_then(|d| ctx.effect_layer(&d))
        };
        Ok(got.unwrap_or(input).clone())
    }
}

/// Test effect: fills its output opaque white.
struct White(EffectDef);

impl Effect for White {
    fn def(&self) -> &EffectDef {
        &self.0
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        _params: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let s = input.pixel_scale;
        let px = |v: i32| (f64::from(v) * s).round() as i32;
        let out = ctx.allocate(
            IRect::new(px(roi.x0), px(roi.y0), px(roi.x1), px(roi.y1)),
            s,
        )?;
        ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("test.white"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &out.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        Ok(out)
    }
}

fn renderer(gpu: Arc<GpuContext>) -> Renderer {
    let mut r = Renderer::new(gpu);
    r.effects_mut().register(Arc::new(Read(def("test.read"))));
    r.effects_mut().register(Arc::new(White(def("test.white"))));
    r
}

fn solid(name: &str, color: [f32; 4]) -> RenderLayer {
    RenderLayer::new(
        p(&format!("/comps/main/layers/{name}")),
        LayerContent::Solid {
            color,
            width: 64,
            height: 32,
        },
    )
}

/// A mask over the left half of a layer.
fn left_half(layer: &str) -> RenderMask {
    RenderMask {
        path: p(&format!("/comps/main/layers/{layer}/masks/m")),
        shape: rect_path(0.0, 0.0, 32.0, 32.0),
        mode: MaskMode::Add,
        inverted: false,
        opacity: 1.0,
        feather: [0.0, 0.0],
        expansion: 0.0,
    }
}

fn read(layer: &str, stage: i64, offset: f64, masks: bool) -> RenderEffect {
    RenderEffect {
        path: p("/comps/main/layers/b/effects/read"),
        effect: "test.read".into(),
        enabled: true,
        params: vec![
            (1, Value::PrimRef(p(&format!("/comps/main/layers/{layer}")))),
            (2, Value::Int(stage)),
            (3, Value::Float(offset)),
            (4, Value::Bool(masks)),
        ],
    }
}

/// `a`: a hidden red solid with a mask over its left half; `b`: a visible
/// solid whose effect shows what it read.
fn scene(a_color: [f32; 4], b_effect: RenderEffect) -> RenderComp {
    let mut comp = RenderComp::new(p("/comps/main"), 64, 32);
    let mut a = solid("a", a_color);
    a.visible = false;
    a.masks = vec![left_half("a")];
    let mut b = solid("b", [0.0, 0.0, 1.0, 1.0]);
    b.effects = vec![b_effect];
    comp.layers = vec![b, a];
    comp
}

fn pixel(r: &mut Renderer, comp: &RenderComp, frames: Option<&mut dyn CompFrames>) -> [[u8; 4]; 2] {
    let frame = r
        .render_frame_with(comp, &RenderRequest::default(), frames)
        .unwrap();
    let img = r.read_rgba8(&frame, OutputAlpha::Straight).unwrap();
    let at = |x: u32| {
        let i = ((16 * img.width + x) * 4) as usize;
        [
            img.data[i],
            img.data[i + 1],
            img.data[i + 2],
            img.data[i + 3],
        ]
    };
    // Left half (inside a's mask) and right half (outside).
    [at(16), at(48)]
}

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

#[test]
fn stages_follow_ae_layer_popup() {
    let Some(gpu) = gpu() else { return };
    let mut r = renderer(gpu);
    // Source: the whole red solid, its mask ignored; its own visibility too.
    let [l, rt] = pixel(&mut r, &scene(RED, read("a", 0, 0.0, false)), None);
    assert_eq!((l, rt), ([255, 0, 0, 255], [255, 0, 0, 255]));
    // Masks: only the left half.
    let [l, rt] = pixel(&mut r, &scene(RED, read("a", 1, 0.0, false)), None);
    assert_eq!(l, [255, 0, 0, 255]);
    assert_eq!(rt[3], 0, "{rt:?}");
    // Effects & Masks: a's effect runs after its mask, so a generator that
    // ignores its input fills the whole layer (as in AE).
    let mut comp = scene(RED, read("a", 2, 0.0, false));
    comp.layers[1].effects = vec![RenderEffect {
        path: p("/comps/main/layers/a/effects/white"),
        effect: "test.white".into(),
        enabled: true,
        params: Vec::new(),
    }];
    let [l, rt] = pixel(&mut r, &comp, None);
    assert_eq!((l, rt), ([255; 4], [255; 4]));
}

#[test]
fn circular_references_fall_back_to_masks() {
    let Some(gpu) = gpu() else { return };
    let mut r = renderer(gpu);
    // b reads a (effects & masks), a reads b (effects & masks).
    let mut comp = scene(RED, read("a", 2, 0.0, false));
    let mut back = read("b", 2, 0.0, false);
    back.path = p("/comps/main/layers/a/effects/read");
    comp.layers[1].effects = vec![back];
    let frame = r.render_frame(&comp, &RenderRequest::default()).unwrap();
    assert!(
        frame.warnings.iter().any(|w| w.contains("circular")),
        "{:?}",
        frame.warnings
    );
    // A layer reading itself with its effects also stops at its masks.
    let mut comp = scene(RED, read("b", 2, 0.0, false));
    comp.layers[0].masks = vec![left_half("b")];
    let frame = r.render_frame(&comp, &RenderRequest::default()).unwrap();
    assert!(frame.warnings.iter().any(|w| w.contains("circular")));
}

#[test]
fn mask_coverage_reaches_the_effect() {
    let Some(gpu) = gpu() else { return };
    let mut r = renderer(gpu);
    let mut comp = scene(RED, read("a", 0, 0.0, true));
    // b's own mask: the effect reads its coverage (white where covered).
    comp.layers[0].masks = vec![RenderMask {
        inverted: true,
        ..left_half("b")
    }];
    let [l, rt] = pixel(&mut r, &comp, None);
    assert_eq!(l[3], 0, "{l:?}");
    assert_eq!(rt, [255, 255, 255, 255]);
}

/// Another frame: `a` is green one second later.
struct Later;

impl CompFrames for Later {
    fn comp_at(&mut self, _comp: &PrimPath, time: Time) -> Result<Option<RenderComp>, RenderError> {
        let mut comp = scene([0.0, 1.0, 0.0, 1.0], read("a", 0, 0.0, false));
        comp.time = time;
        Ok(Some(comp))
    }
}

#[test]
fn other_frames_come_from_the_frame_source() {
    let Some(gpu) = gpu() else { return };
    let mut r = renderer(gpu);
    let comp = scene(RED, read("a", 0, 1.0, false));
    let [l, _] = pixel(&mut r, &comp, Some(&mut Later));
    assert_eq!(l, [0, 255, 0, 255]);
    // Such frames are not cached (the other frame may change on its own).
    let again = r
        .render_frame_with(&comp, &RenderRequest::default(), Some(&mut Later))
        .unwrap();
    assert!(!again.from_cache);
    // Without a frame source the effect gets nothing and says so.
    let frame = r.render_frame(&comp, &RenderRequest::default()).unwrap();
    assert!(
        frame.warnings.iter().any(|w| w.contains("other frames")),
        "{:?}",
        frame.warnings
    );
}
