use crate::common::*;
use bae_model::Value;
use bae_render::IRect;

#[test]
fn spherical_generators_render_the_same_field_to_both_eyes() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(96, 48);
    let input = Pixels::new(rect, |_, _| [0., 0., 0., 1.]);
    for name in [
        "Mettle SkyBox Color Gradients",
        "Mettle SkyBox Fractal Noise",
    ] {
        let out = run(
            &gpu,
            F32,
            builtin(name),
            vec![(2, Value::Int(3))],
            &input,
            rect,
            1.,
        );
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        for y in 0..48 {
            for x in 0..48 {
                let a = out.get(x, y);
                let b = out.get(x + 48, y);
                assert!(a.iter().all(|v| v.is_finite()), "{name}");
                for ch in 0..4 {
                    assert!((a[ch] - b[ch]).abs() < 0.0001, "{name}");
                }
                lo = lo.min(a[0]);
                hi = hi.max(a[0]);
            }
        }
        assert!(hi - lo > 0.1, "{name}");
    }
}

#[test]
fn chromatic_aberration_separates_channels_and_glitch_is_repeatable() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(96, 48);
    let input = Pixels::new(rect, |x, y| {
        let a = if (x / 3 + y / 4) % 2 == 0 { 0.15 } else { 0.85 };
        [a, a, a, 1.]
    });
    let chroma = run(
        &gpu,
        F32,
        builtin("Mettle SkyBox Chromatic Aberrat"),
        vec![],
        &input,
        rect,
        1.,
    );
    assert!(chroma.data.iter().any(|p| (p[0] - p[2]).abs() > 0.1));
    let glitch = builtin("Mettle SkyBox Digital Glitch");
    let render = |seed| {
        run(
            &gpu,
            F32,
            glitch,
            vec![(38, Value::Int(seed))],
            &input,
            rect,
            1.,
        )
    };
    let a = render(2);
    assert_eq!(a.data, render(2).data);
    let b = render(7);
    assert!(
        a.data
            .iter()
            .zip(b.data)
            .any(|(a, b)| (a[0] - b[0]).abs() > 0.1)
    );
}
