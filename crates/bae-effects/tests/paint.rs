//! The Paint effect: brush strokes are painted on the layer, newest on top,
//! and an Eraser stroke removes what is under it.

use bae_model::{PrimPath, Value};
use bae_render::paint::{Erase, PAINT_EFFECT, STROKES_PARAM, Stroke, Tool, pack};
use bae_render::{
    LayerContent, OutputAlpha, RenderComp, RenderEffect, RenderLayer, RenderRequest, Renderer,
};
use glam::DVec2;

fn path(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn stroke(tool: Tool, from: DVec2, to: DVec2, color: [f32; 4]) -> Stroke {
    Stroke {
        tool,
        erase: Erase::LayerAndPaint,
        points: vec![from, to],
        color,
        diameter: 8.0,
        angle: 0.0,
        hardness: 1.0,
        roundness: 1.0,
        spacing: 0.25,
        opacity: 1.0,
        flow: 1.0,
        channels: 0,
        clone_offset: DVec2::ZERO,
    }
}

/// RGBA8 of a 32 × 32 white solid with `strokes` painted on it.
fn paint(strokes: &[Stroke]) -> Option<Vec<u8>> {
    let mut renderer = Renderer::new(bae_gpu::testing::shared_context()?);
    bae_effects::register(
        &mut bae_model::EffectRegistry::new(),
        renderer.effects_mut(),
    );
    let mut comp = RenderComp::new(path("/comps/main"), 32, 32);
    let mut layer = RenderLayer::new(
        path("/comps/main/layers/solid"),
        LayerContent::Solid {
            color: [1.0; 4],
            width: 32,
            height: 32,
        },
    );
    layer.effects = vec![RenderEffect {
        path: path("/comps/main/layers/solid/effects/Paint"),
        enabled: true,
        effect: PAINT_EFFECT.into(),
        params: vec![(1, Value::Bool(false)), (STROKES_PARAM, pack(strokes))],
    }];
    comp.layers.push(layer);
    let frame = renderer
        .render_frame(&comp, &RenderRequest::default())
        .ok()?;
    Some(
        renderer
            .read_rgba8(&frame, OutputAlpha::Premultiplied)
            .ok()?
            .data,
    )
}

fn at(px: &[u8], x: usize, y: usize) -> [u8; 4] {
    let i = (y * 32 + x) * 4;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

#[test]
fn brush_strokes_paint_and_the_eraser_removes() {
    let red = stroke(
        Tool::Brush,
        DVec2::new(4.0, 16.0),
        DVec2::new(28.0, 16.0),
        [1.0, 0.0, 0.0, 1.0],
    );
    let Some(px) = paint(std::slice::from_ref(&red)) else {
        return;
    };
    assert_eq!(at(&px, 16, 16), [255, 0, 0, 255], "on the stroke");
    assert_eq!(at(&px, 16, 4), [255, 255, 255, 255], "away from it");
    // A blue stroke across it is on top; an eraser stroke clears the layer.
    let blue = stroke(
        Tool::Brush,
        DVec2::new(16.0, 4.0),
        DVec2::new(16.0, 28.0),
        [0.0, 0.0, 1.0, 1.0],
    );
    let eraser = stroke(
        Tool::Eraser,
        DVec2::new(4.0, 26.0),
        DVec2::new(28.0, 26.0),
        [1.0; 4],
    );
    let px = paint(&[red, blue, eraser]).unwrap();
    assert_eq!(at(&px, 16, 16), [0, 0, 255, 255], "newest on top");
    assert_eq!(at(&px, 8, 26)[3], 0, "erased");
}
