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

mod common;
#[test]
fn fill_uses_visible_temporal_samples_or_spatial_boundaries_and_keeps_the_exterior_clear() {
    use bae_vision::{FillMethod, FillSettings};
    use common::{F16, Pixels};
    let Some(gpu) = common::context() else {
        return;
    };
    let rect = bae_render::IRect::new(0, 0, 32, 16);
    let pattern = |x: i32| {
        if x < 16 {
            [1., 0., 0., 1.]
        } else {
            [0., 0., 1., 1.]
        }
    };
    let src = Pixels::new(rect, |x, _| {
        if (11..21).contains(&x) {
            [0.; 4]
        } else {
            pattern(x)
        }
    })
    .upload(&gpu, F16);
    let other = Pixels::new(rect, |x, _| {
        if (11..21).contains(&x) {
            [0., 1., 0., 1.]
        } else {
            pattern(x)
        }
    })
    .upload(&gpu, F16);
    let mut target = bae_effects::Standalone::new(gpu.clone(), F16);
    for method in [
        FillMethod::Object,
        FillMethod::Surface,
        FillMethod::EdgeBlend,
    ] {
        let patch = bae_effects::content_aware_fill(
            &mut target,
            &src,
            &other,
            &other,
            FillSettings {
                method,
                expansion: 1,
                lighting: 0.,
            },
        )
        .unwrap();
        target.submit();
        let px = Pixels::download(&gpu, &patch);
        assert_eq!(px.get(3, 8), [0.; 4]);
        assert_eq!(px.get(10, 8)[3], 1., "one-pixel expansion");
        let p = px.get(15, 8);
        assert_eq!(p[3], 1.);
        match method {
            FillMethod::Object => assert!(p[1] > 0.9, "temporal donor: {p:?}"),
            FillMethod::Surface => assert!(p[0] > 0.9 || p[2] > 0.9, "intact patch: {p:?}"),
            FillMethod::EdgeBlend => assert!(p[0] > 0.02 && p[2] > 0.02, "boundary blend: {p:?}"),
        }
    }
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
