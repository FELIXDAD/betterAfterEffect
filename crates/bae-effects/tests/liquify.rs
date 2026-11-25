//! Liquify: every pixel reads the layer at its position plus the Distortion
//! Mesh's offset there.

use bae_effects::effects::liquify::{MATCH_NAME, MESH, Mesh, PERCENT};
use bae_model::{PrimPath, Value};
use bae_render::{
    LayerContent, OutputAlpha, RenderComp, RenderEffect, RenderLayer, RenderRequest, Renderer,
};
use glam::DVec2;

fn path(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

/// Alpha along row 16 of a 32 × 32 red solid in a 48 × 32 comp, liquified by `mesh`.
fn row(mesh: Option<&Mesh>) -> Option<Vec<u8>> {
    let mut renderer = Renderer::new(bae_gpu::testing::shared_context()?);
    bae_effects::register(
        &mut bae_model::EffectRegistry::new(),
        renderer.effects_mut(),
    );
    let mut comp = RenderComp::new(path("/comps/main"), 48, 32);
    let mut layer = RenderLayer::new(
        path("/comps/main/layers/solid"),
        LayerContent::Solid {
            color: [1.0, 0.0, 0.0, 1.0],
            width: 32,
            height: 32,
        },
    );
    if let Some(m) = mesh {
        layer.effects = vec![RenderEffect {
            path: path("/comps/main/layers/solid/effects/Liquify"),
            enabled: true,
            effect: MATCH_NAME.into(),
            params: vec![(MESH, m.to_value()), (PERCENT, Value::Float(100.0))],
        }];
    }
    comp.layers.push(layer);
    let frame = renderer
        .render_frame(&comp, &RenderRequest::default())
        .ok()?;
    let px = renderer
        .read_rgba8(&frame, OutputAlpha::Premultiplied)
        .ok()?
        .data;
    Some((0..48).map(|x| px[(16 * 48 + x) * 4 + 3]).collect())
}

#[test]
fn the_mesh_moves_where_pixels_are_read() {
    let Some(plain) = row(None) else {
        return;
    };
    assert_eq!((plain[4], plain[28], plain[40]), (255, 255, 0));
    // Every pixel reads 8 px to the right: the right edge comes in by 8 px.
    let mut m = Mesh::new(DVec2::ZERO, DVec2::new(32.0, 32.0));
    m.offsets.iter_mut().for_each(|o| *o = DVec2::new(8.0, 0.0));
    let moved = row(Some(&m)).unwrap();
    assert_eq!((moved[4], moved[20], moved[28]), (255, 255, 0), "{moved:?}");
}
