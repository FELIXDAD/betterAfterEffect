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
    // Two emission intervals share a draw but have distinct birth times and IDs.
    fx.run(
        "key.set",
        json!({"path":emitter, "attr":"param:0001",
        "time":Time::from_seconds(0.25).flicks(), "keyframe":Keyframe::hold(Value::Float(0.5))}),
    );
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
        visible.iter().any(|&x| (24..28).contains(&x)),
        "births after the rate change keep their positions"
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

/// A stationary Particle Playground cannon (no speed, spread or gravity) firing
/// 60 particles per second until 0.5 s (AE's usual stop: the rate keyed to 0),
/// optionally moving from x 16 to 48 meanwhile. RGBA8 frames at `times`.
fn cannon_frames(moving: bool, times: &[f64]) -> Option<Vec<Vec<u8>>> {
    use bae_model::{Keyframe, Value};
    use bae_render::eval::EvaluatorFrames;
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.solid(&comp, [0.; 3], [64, 32]);
    let cannon = fx.apply(&layer, "ADBE Playgnd");
    fx.param(&cannon, 101, json!({"type":"vec2","value":[32.0, 16.0]}));
    for (id, value) in [(106, 0.), (107, 0.), (108, 0.), (110, 2.), (331, 0.)] {
        fx.param(&cannon, id, json!({"type":"float","value":value}));
    }
    let mut key = |attr: &str, time: f64, keyframe: Keyframe| {
        fx.run(
            "key.set",
            json!({"path":cannon, "attr":attr,
            "time":Time::from_seconds(time).flicks(), "keyframe":keyframe}),
        );
    };
    for (time, rate) in [(0., 60.), (0.5, 0.)] {
        key("param:0103", time, Keyframe::hold(Value::Float(rate)));
    }
    if moving {
        for (time, x) in [(0., 16.), (0.5, 48.)] {
            key("param:0101", time, Keyframe::linear(Value::Vec2([x, 16.])));
        }
    }
    let renderer = fx.renderer.as_mut()?;
    Some(
        times
            .iter()
            .map(|&t| {
                let rc = fx
                    .eval
                    .evaluate(&fx.stage, &comp, Time::from_seconds(t))
                    .unwrap();
                let mut samples = EvaluatorFrames {
                    stage: &fx.stage,
                    evaluator: &mut fx.eval,
                };
                let frame = renderer
                    .render_frame_with(&rc, &RenderRequest::full(), Some(&mut samples))
                    .unwrap();
                renderer
                    .read_rgba8(&frame, OutputAlpha::Premultiplied)
                    .unwrap()
                    .data
            })
            .collect(),
    )
}

#[test]
fn playground_keeps_fired_particles_after_the_rate_is_keyed_to_zero() {
    let Some(frames) = cannon_frames(false, &[0.8, 1.2]) else {
        return;
    };
    assert!(
        frames[0].as_chunks::<4>().0.iter().any(|px| px[3] > 128),
        "fired particles are gone"
    );
    assert_eq!(
        frames[1], frames[0],
        "fired particles stay after the cannon stops"
    );
}

#[test]
fn playground_particles_stay_where_a_moving_cannon_fired_them() {
    let Some(frames) = cannon_frames(true, &[1.0]) else {
        return;
    };
    let lit: Vec<usize> = frames[0]
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, px)| px[3] > 128)
        .map(|(i, _)| i % 64)
        .collect();
    let (left, right) = (lit.iter().min(), lit.iter().max());
    assert!(
        left < Some(&24) && right > Some(&40),
        "particles span the path: {left:?}..{right:?}"
    );
}
