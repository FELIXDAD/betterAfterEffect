#[path = "common/time_scene.rs"]
mod scene;
use bae_model::{Time, Value};
use bae_render::RenderComp;
use scene::*;

#[test]
fn roto_foreground_and_background_propagate_on_both_sides_of_a_seed() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let stamp = Time::from_seconds(1.).0 as f64;
    let scene = |t: Time| {
        let mut comp = RenderComp::new(path("/comps/main"), 64, 8);
        comp.time = t;
        let mut layer = solid(CLIP, 64, [1.; 4]);
        layer.time = t;
        layer.effects.push(effect(
            "roto",
            bae_vision::ROTO,
            vec![(
                0,
                Value::FloatList(vec![
                    stamp, 2., 3., 4., 6., 4., stamp, -2., 44., 4., 48., 4.,
                ]),
            )],
        ));
        comp.layers.push(layer);
        comp
    };
    for t in [1., 0.96, 1.04] {
        let frame = pixels(&mut renderer, scene, t);
        let alpha = |x| frame[(4 * 64 + x) * 4 + 3];
        assert!(
            alpha(4usize) > 245 && alpha(12usize) > 200 && alpha(45usize) < 8,
            "matte at {t}"
        );
    }
}

#[test]
fn refine_edge_brush_recovers_partial_coverage_on_a_colour_ramp() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let scene = |edge: bool| {
        let mut comp = RenderComp::new(path("/comps/main"), 64, 8);
        let mut layer = solid(CLIP, 64, [1.; 4]);
        layer.effects.push(effect(
            "ramp",
            "ADBE Ramp",
            vec![
                (1, Value::Vec2([0., 4.])),
                (2, Value::Color([0., 0., 0., 1.])),
                (3, Value::Vec2([63., 4.])),
                (4, Value::Color([1.; 4])),
            ],
        ));
        let mut params = vec![(
            0,
            Value::FloatList(vec![0., 3., 48., 3., 48., 6., 0., -3., 16., 3., 16., 6.]),
        )];
        if edge {
            params.push((
                bae_vision::EDGE_STROKES,
                Value::FloatList(vec![0., 8., 32., 1., 32., 7.]),
            ));
        }
        layer.effects.push(effect("roto", bae_vision::ROTO, params));
        comp.layers.push(layer);
        comp
    };
    let raw = pixels(&mut renderer, |_| scene(false), 0.);
    let refined = pixels(&mut renderer, |_| scene(true), 0.);
    assert!((26usize..38).any(|x| {
        let i = (4 * 64 + x) * 4 + 3;
        refined[i] > 10 && refined[i] < 245 && refined[i].abs_diff(raw[i]) > 10
    }));
    assert_eq!(&raw[..16], &refined[..16]);
}

/// A seed frame that does not render (the layer is trimmed away there) leaves
/// the layer as it is instead of failing the whole frame.
#[test]
fn roto_without_its_seed_frame_still_renders() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let seed = Time::from_seconds(1.);
    let stamp = seed.0 as f64;
    let scene = |t: Time| {
        let mut comp = RenderComp::new(path("/comps/main"), 64, 8);
        comp.time = t;
        if t == seed {
            return comp;
        }
        let mut layer = solid(CLIP, 64, [1.; 4]);
        layer.time = t;
        layer.effects.push(effect(
            "roto",
            bae_vision::ROTO,
            vec![(0, Value::FloatList(vec![stamp, 2., 3., 4., 6., 4.]))],
        ));
        comp.layers.push(layer);
        comp
    };
    let frame = pixels(&mut renderer, scene, 1.04);
    assert_eq!(frame[(4 * 64 + 45) * 4 + 3], 255);
}

/// Freezing before any stroke stores "nothing masked out", not the layer's
/// own pixels as a matte.
#[test]
fn roto_bake_without_strokes_keeps_the_whole_layer() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let scene = |t: Time| {
        let mut comp = RenderComp::new(path("/comps/main"), 64, 8);
        comp.time = t;
        let mut layer = solid(CLIP, 64, [0.2, 0.9, 0.1, 1.]);
        layer.time = t;
        layer.effects.push(effect(
            "roto",
            bae_vision::ROTO,
            vec![(bae_vision::BAKE, Value::Bool(true))],
        ));
        comp.layers.push(layer);
        comp
    };
    let frame = pixels(&mut renderer, scene, 1.);
    let at = (4 * 64 + 45) * 4;
    assert_eq!(&frame[at..at + 4], &[255, 0, 255, 255]);
}
