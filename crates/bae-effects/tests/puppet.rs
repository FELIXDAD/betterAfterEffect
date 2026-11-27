//! Puppet: the mesh follows its pins; pixels off the mesh are transparent.

use bae_effects::effects::puppet::MATCH_NAME;
use bae_model::PrimPath;
use bae_render::puppet::{PINS_PARAM, Pin, PinKind, pack};
use bae_render::{
    LayerContent, OutputAlpha, RenderComp, RenderEffect, RenderLayer, RenderRequest, Renderer,
};
use glam::DVec2;

fn path(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn pin(rest: [f64; 2], position: [f64; 2]) -> Pin {
    Pin {
        kind: PinKind::Position,
        rest: DVec2::from_array(rest),
        position: DVec2::from_array(position),
        scale: 100.0,
        rotation: 0.0,
        amount: 0.3,
        extent: 15.0,
        in_front: 0.0,
    }
}

/// Alpha along row 16 of a 32 × 32 red solid in a 64 × 32 comp, with `pins`.
fn row(pins: &[Pin]) -> Option<Vec<u8>> {
    let mut renderer = Renderer::new(bae_gpu::testing::shared_context()?);
    bae_effects::register(
        &mut bae_model::EffectRegistry::new(),
        renderer.effects_mut(),
    );
    let mut comp = RenderComp::new(path("/comps/main"), 64, 32);
    let mut layer = RenderLayer::new(
        path("/comps/main/layers/solid"),
        LayerContent::Solid {
            color: [1.0, 0.0, 0.0, 1.0],
            width: 32,
            height: 32,
        },
    );
    layer.effects = vec![RenderEffect {
        path: path("/comps/main/layers/solid/effects/Puppet"),
        enabled: true,
        effect: MATCH_NAME.into(),
        params: vec![(PINS_PARAM, pack(pins))],
    }];
    comp.layers.push(layer);
    let frame = renderer
        .render_frame(&comp, &RenderRequest::default())
        .ok()?;
    let px = renderer
        .read_rgba8(&frame, OutputAlpha::Premultiplied)
        .ok()?
        .data;
    Some((0..64).map(|x| px[(16 * 64 + x) * 4 + 3]).collect())
}

#[test]
fn one_pin_moves_the_layer_with_it() {
    let Some(still) = row(&[pin([16.0, 16.0], [16.0, 16.0])]) else {
        return;
    };
    assert_eq!((still[4], still[28], still[40]), (255, 255, 0), "{still:?}");
    // Dragged 16 px right: the layer follows, its left part uncovered.
    let moved = row(&[pin([16.0, 16.0], [32.0, 16.0])]).unwrap();
    assert_eq!((moved[4], moved[24], moved[44]), (0, 255, 255), "{moved:?}");
}

/// The first render reads the layer's alpha back for its outline, without
/// waiting: a little later it can be taken (a solid: all inside).
#[test]
fn the_layer_alpha_is_traced_in_the_background() {
    let Some(ctx) = bae_gpu::testing::shared_context() else {
        return;
    };
    let mut renderer = Renderer::new(ctx);
    bae_effects::register(
        &mut bae_model::EffectRegistry::new(),
        renderer.effects_mut(),
    );
    let effect = path("/comps/trace/layers/solid/effects/Puppet");
    let mut comp = RenderComp::new(path("/comps/trace"), 64, 32);
    let mut layer = RenderLayer::new(
        path("/comps/trace/layers/solid"),
        LayerContent::Solid {
            color: [1.0, 0.0, 0.0, 1.0],
            width: 32,
            height: 32,
        },
    );
    layer.effects = vec![RenderEffect {
        path: effect.clone(),
        enabled: true,
        effect: MATCH_NAME.into(),
        params: vec![(PINS_PARAM, pack(&[pin([16.0, 16.0], [20.0, 16.0])]))],
    }];
    comp.layers.push(layer);
    let mut traced = None;
    // One render records the read-back; asking for it moves it along.
    renderer
        .render_frame(&comp, &RenderRequest::default())
        .unwrap();
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        traced = bae_effects::effects::puppet::take_trace(&effect);
        if traced.is_some() {
            break;
        }
    }
    let mask =
        bae_effects::effects::puppet::Mask::from_value(&traced.expect("traced")).expect("a mask");
    assert!(mask.on.iter().all(|b| *b), "a solid is opaque everywhere");
    assert!(
        bae_effects::effects::puppet::take_trace(&effect).is_none(),
        "taken once"
    );
}
