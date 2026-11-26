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
