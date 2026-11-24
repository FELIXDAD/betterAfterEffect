//! Curve points, localized correction, scanline histogram and alpha handling.
mod common;
use bae_gpu::wgpu::TextureFormat::{Rgba16Float, Rgba32Float};
use bae_model::{ParamKind, Value};
use bae_render::IRect;
use common::*;

#[test]
fn curves_and_tolerance_follow_ae_disk_ids() {
    let curves = builtin("ADBE CurvesCustom").def();
    assert_eq!(
        curves.params.iter().map(|p| p.id).collect::<Vec<_>>(),
        [2, 1]
    );
    assert_eq!(curves.param(1).unwrap().kind, ParamKind::CurvePoints);
    let to = builtin("ADBE Change To Color").def();
    assert_eq!(
        to.params.iter().map(|p| p.id).collect::<Vec<_>>(),
        [1, 2, 3, 4, 10, 5, 6, 7, 11, 8, 9]
    );
}

#[test]
fn curves_compose_channels_in_straight_color_and_remap_alpha() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-3, 5, 10, 14);
    let roi = IRect::new(0, 6, 5, 10);
    let image = Pixels::new(rect, |_, _| [0.2, 0.1, 0.3, 0.4]);
    // RGB halves the range; red is inverted; alpha maps [0,1] to [0.5,1].
    let points = vec![
        0., 0., 0., 0., 1., 0.5, 1., 0., 1., 1., 1., 0., 4., 0., 0.5, 4., 1., 1.,
    ];
    let expected = Pixels::new(roi, |_, _| [0.175, 0.0875, 0.2625, 0.7]);
    let got = run(
        &gpu,
        Rgba32Float,
        builtin("ADBE CurvesCustom"),
        vec![(1, Value::FloatList(points))],
        &image,
        roi,
        1.,
    );
    assert_close(&got, &expected, roi, 0.0001, "curves");
}

#[test]
fn primary_color_changes_preserve_premultiplication() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(12, 3);
    let src = Pixels::new(rect, |x, _| [x as f32 / 12., 0., 0., x as f32 / 12.]);
    let expected = src.map(|c| [0., c[0], 0., c[3]]);
    for (mn, params) in [
        ("ADBE Color Balance (HLS)", vec![(1, Value::Float(120.))]),
        (
            "ADBE Change Color",
            vec![(2, Value::Float(120.)), (5, color([1., 0., 0., 1.]))],
        ),
        (
            "ADBE Change To Color",
            vec![(1, color([1., 0., 0., 1.])), (2, color([0., 1., 0., 1.]))],
        ),
        (
            "ADBE CHANNEL MIXER",
            vec![(1, Value::Float(0.)), (5, Value::Float(100.))],
        ),
    ] {
        let got = run(&gpu, Rgba16Float, builtin(mn), params, &src, rect, 1.);
        assert_close(&got, &expected, rect, 0.002, mn);
    }
}

#[test]
fn shadow_histogram_and_blur_are_tile_independent() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-10, 4, 121, 73);
    let roi = IRect::new(13, 12, 75, 47);
    let input = test_image(rect);
    let effect = builtin("ADBE ShadowHighlight");
    let p = vec![(10, Value::Float(75.))];
    let full = run(&gpu, Rgba16Float, effect, p.clone(), &input, rect, 1.);
    assert!(max_diff(&full, &input, rect).0 > 0.03);
    let tile = run(&gpu, Rgba16Float, effect, p, &input, roi, 1.);
    assert_close(&tile, &full, roi, 0.004, "shadow tile");
    let original = run(
        &gpu,
        Rgba32Float,
        effect,
        vec![(16, Value::Float(100.))],
        &input,
        roi,
        1.,
    );
    assert_close(&original, &input, roi, 0.00001, "blend with original");
}

#[test]
fn channel_mixer_preserves_hdr_and_clips_integer_channels_before_premultiplying() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(4, 4);
    let input = Pixels::new(rect, |_, _| [0.5, 0.25, 0., 0.5]);
    let effect = builtin("ADBE CHANNEL MIXER");
    let params = vec![(4, Value::Float(50.0)), (12, Value::Float(-25.0))];
    for (float, pixel) in [
        (true, [0.75, 0.25, -0.125, 0.5]),
        (false, [0.5, 0.25, 0., 0.5]),
    ] {
        let actual = run_depth(
            &gpu,
            Rgba32Float,
            effect,
            params.clone(),
            &input,
            rect,
            1.,
            float,
        );
        assert_close(
            &actual,
            &Pixels::new(rect, |_, _| pixel),
            rect,
            1e-5,
            "mixer range",
        );
    }
}
