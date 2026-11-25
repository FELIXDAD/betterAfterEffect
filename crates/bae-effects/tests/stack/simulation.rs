use super::*;

#[test]
fn producer_point_follows_layer_size_on_apply_and_reset() {
    assert_reference_points(&[("CC Particle Systems II", 4, [15.5, 6.5])]);
}

#[test]
fn stopped_emitter_keeps_birth_position_and_lifespan_on_seek() {
    use bae_model::{Keyframe, Value};
    use bae_render::eval::EvaluatorFrames;
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.solid(&comp, [1.; 3], [64, 32]);
    let emitter = fx.apply(&layer, "CC Particle Systems II");
    for (id, value) in [(5, 0.), (6, 0.), (10, 0.), (12, 0.), (19, 1.), (20, 1.)] {
        fx.param(&emitter, id, json!({"type":"float","value":value}));
    }
    fx.param(&emitter, 18, json!({"type":"int","value":4}));
    for (id, start, end, t, hold) in [
        (1, Value::Float(1.), Value::Float(0.), 0.5, true),
        (2, Value::Float(1.), Value::Float(0.06), 0.5, true),
        (
            4,
            Value::Vec2([8., 16.]),
            Value::Vec2([48., 16.]),
            1.,
            false,
        ),
    ] {
        for (time, value) in [(0., start), (t, end)] {
            let key = if hold {
                Keyframe::hold(value)
            } else {
                Keyframe::linear(value)
            };
            fx.run(
                "key.set",
                json!({"path":emitter,"attr":format!("param:{id:04}"),
                "time":Time::from_seconds(time).flicks(),"keyframe":key}),
            );
        }
    }
    let Some(renderer) = fx.renderer.as_mut() else {
        return;
    };
    let mut render = |t, roi| {
        let rc = fx
            .eval
            .evaluate(&fx.stage, &comp, Time::from_seconds(t))
            .unwrap();
        let mut samples = EvaluatorFrames {
            stage: &fx.stage,
            evaluator: &mut fx.eval,
        };
        let frame = renderer
            .render_frame_with(
                &rc,
                &RenderRequest {
                    roi,
                    ..RenderRequest::full()
                },
                Some(&mut samples),
            )
            .unwrap();
        let data = renderer
            .read_rgba8(&frame, OutputAlpha::Premultiplied)
            .unwrap()
            .data;
        (data, renderer.frame_cache().stats().hits)
    };
    let alive = render(0.8, None).0;
    let visible: Vec<_> = alive
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, p)| p[3] > 32)
        .map(|(i, _)| i % 64)
        .collect();
    assert!(
        visible.iter().any(|&x| (12..19).contains(&x)),
        "early births retain their producer position"
    );
    assert!(
        visible.iter().all(|&x| x < 32),
        "emission stops at the hold key"
    );
    assert!(
        render(1.6, None)
            .0
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[3] == 0)
    );
    render(0.2, None);
    assert_eq!(
        alive,
        render(0.8, None).0,
        "direct seek and forward evaluation must agree"
    );
    let (_, hits) = render(0.8, Some(bae_render::IRect::new(0, 0, 20, 32)));
    let (second, next_hits) = render(0.8, Some(bae_render::IRect::new(12, 0, 32, 32)));
    assert!(
        next_hits > hits,
        "adjacent preview tiles share a particle surface"
    );
    let expected: Vec<_> = alive
        .as_chunks::<256>()
        .0
        .iter()
        .flat_map(|row| row[12 * 4..32 * 4].iter().copied())
        .collect();
    assert_eq!(second, expected);
}
