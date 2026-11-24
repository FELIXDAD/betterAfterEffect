//! Pixel and render-graph coverage for expression controls and channel effects.

mod common;

use bae_effects::get;
use bae_model::{PrimPath, Value};
use bae_render::{
    IRect, LayerContent, MaskMode, RenderComp, RenderLayer, RenderMask, RenderRequest, Renderer,
    Resolution, fixture::rect_path,
};

use common::{F16, F32, Pixels, assert_close, context, instance, run, run_depth};

#[test]
fn channel_remapping_uses_straight_color_and_premultiplies_once() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(2, 2);
    let input = Pixels::new(r, |_, _| [0.1, 0.2, 0.3, 0.5]);
    for format in [F16, F32] {
        let got = run(
            &gpu,
            format,
            get("ADBE Shift Channels").unwrap(),
            vec![
                (1, Value::Int(4)),
                (2, Value::Int(1)),
                (3, Value::Int(9)),
                (4, Value::Int(10)),
            ],
            &input,
            r,
            1.0,
        );
        assert_close(
            &got,
            &Pixels::new(r, |_, _| [0.3, 0.6, 0.0, 0.6]),
            r,
            0.001,
            "shift",
        );
        let got = run(
            &gpu,
            format,
            get("ADBE Invert").unwrap(),
            vec![(1, Value::Int(2)), (2, Value::Float(25.0))],
            &input,
            r,
            1.0,
        );
        assert_close(
            &got,
            &Pixels::new(r, |_, _| [0.325, 0.2, 0.3, 0.5]),
            r,
            0.001,
            "invert red",
        );
    }

    let hdr = Pixels::new(r, |_, _| [1.25, -0.25, 0.5, 1.0]);
    let fx = get("ADBE Invert").unwrap();
    for (float, expected) in [
        (true, [-0.25, 1.25, 0.5, 1.0]),
        (false, [0.0, 1.0, 0.5, 1.0]),
    ] {
        let got = run_depth(&gpu, F32, fx, vec![], &hdr, r, 1.0, float);
        assert_close(
            &got,
            &Pixels::new(r, |_, _| expected),
            r,
            1e-5,
            "invert depth",
        );
    }
}

#[test]
fn color_effects_preserve_alpha() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(3, 1);
    let input = Pixels::new(r, |x, _| {
        [
            [0.5, 0.0, 0.0, 0.5],
            [0.5, 0.5, 0.0, 0.5],
            [0.125, 0.125, 0.125, 0.5],
        ][x as usize]
    });
    let black_white = run(
        &gpu,
        F32,
        get("ADBE Black&White").unwrap(),
        vec![],
        &input,
        r,
        1.0,
    );
    let gray = Pixels::new(r, |x, _| {
        let l = [0.2, 0.3, 0.125][x as usize];
        [l, l, l, 0.5]
    });
    assert_close(&black_white, &gray, r, 1e-5, "black-white weights");
    for (name, params) in [
        ("ADBE Vibrance", vec![(2, Value::Float(-100.0))]),
        (
            "ADBE Color Balance 2",
            vec![(1, Value::Float(40.0)), (10, Value::Bool(true))],
        ),
        (
            "ADBE PhotoFilterPS",
            vec![(1, Value::Int(22)), (2, Value::Color([0.2, 1.0, 0.4, 1.0]))],
        ),
    ] {
        let got = run(&gpu, F16, get(name).unwrap(), params, &input, r, 1.0);
        for i in 0..3 {
            assert_eq!(got.get(i, 0)[3], 0.5, "{name}");
        }
        assert_ne!(got.data, input.data, "{name}");
    }
}

#[test]
fn minimax_matches_a_direct_extremum_and_is_roi_invariant() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(35, 23);
    let input = Pixels::new(r, |x, y| {
        let a = ((x * 71 + y * 7) % 103) as f32 / 103.0;
        [0.5 * a, 0.25 * a, a, a]
    });
    let fx = get("ADBE Minimax").unwrap();
    for (op, repeat, radius) in [
        (1, false, 5),
        (2, false, 5),
        (3, true, 5),
        (4, false, 5),
        (1, true, 100),
    ] {
        let extrema = |src: &Pixels, minimum: bool| {
            Pixels::new(r, |x, y| {
                let mut a = if minimum { 1.0f32 } else { 0.0f32 };
                for iy in y - radius..=y + radius {
                    for ix in x - radius..=x + radius {
                        let px = if repeat {
                            src.get_clamped(ix, iy)
                        } else {
                            src.get(ix, iy)
                        };
                        a = if minimum { a.min(px[3]) } else { a.max(px[3]) };
                    }
                }
                [0.5 * a, 0.25 * a, a, a]
            })
        };
        let first = extrema(&input, matches!(op, 1 | 3));
        let want = if op >= 3 {
            extrema(&first, op == 4)
        } else {
            first
        };
        let params = vec![
            (1, Value::Int(op)),
            (2, Value::Float(f64::from(radius))),
            (3, Value::Int(2)),
            (5, Value::Bool(repeat)),
        ];
        for roi in [r, IRect::new(17, 7, 25, 14)] {
            let got = run(&gpu, F32, fx, params.clone(), &input, roi, 1.0);
            // Transparent texels have zero straight colour, so a minimum can
            // darken RGB even when its separately computed alpha stays positive.
            let compare_alpha = |p: [f32; 4]| [0.0, 0.0, 0.0, p[3]];
            assert_close(
                &got.map(compare_alpha),
                &want.map(compare_alpha),
                roi,
                1e-5,
                "minimax",
            );
        }
    }
}

#[test]
fn choker_spreads_fractional_pixels_and_has_an_opaque_matte_view() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(7, 5);
    let input = Pixels::new(r, |x, _| {
        if x == 3 {
            [1.0, 0.5, 0.0, 1.0]
        } else {
            [0.0; 4]
        }
    });
    let fx = get("ADBE Simple Choker").unwrap();
    let out = run(&gpu, F32, fx, vec![(2, Value::Float(-0.5))], &input, r, 1.0);
    let spread = out.get(2, 2);
    assert!(spread[3] > 0.1 && spread[3] < 0.5);
    assert!((spread[0] - spread[3]).abs() < 1e-6);
    assert!((spread[1] - 0.5 * spread[3]).abs() < 1e-6);
    assert!(out.get(1, 2)[3] < 0.01);
    let matte = run(&gpu, F32, fx, vec![(1, Value::Int(2))], &input, r, 1.0);
    assert_eq!(matte.get(3, 2), [1.0; 4]);
    assert_eq!(matte.get(2, 2), [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn set_matte_checks_out_the_source_of_a_hidden_layer_in_layer_space() {
    let Some(gpu) = context() else { return };
    let mut comp = RenderComp::new(PrimPath::new("/comps/main").unwrap(), 24, 16);
    let solid = |name: &str, width, height, color| {
        RenderLayer::new(
            PrimPath::new(&format!("/comps/main/layers/{name}")).unwrap(),
            LayerContent::Solid {
                color,
                width,
                height,
            },
        )
    };
    let mut layer = solid("a", 24, 16, [0.8, 0.2, 0.4, 0.5]);
    let mut matte = solid("matte", 12, 8, [1.0, 1.0, 1.0, 0.5]);
    matte.visible = false;
    matte.opacity = 0.0;
    matte
        .masks
        .push(rectangle_mask(&matte.path, "left", [0., 0., 6., 8.]));
    matte
        .effects
        .push(instance("ADBE Fill", vec![(5, Value::Float(0.25))]));
    layer.effects.push(instance(
        "ADBE Set Matte3",
        vec![
            (1, Value::PrimRef(matte.path.clone())),
            (4, Value::Bool(false)),
        ],
    ));
    comp.layers = vec![layer, matte];
    let mut renderer = Renderer::new(gpu.clone());
    bae_effects::register(
        &mut bae_model::EffectRegistry::new(),
        renderer.effects_mut(),
    );

    for resolution in [Resolution::Full, Resolution::Half] {
        for (stage, left, right) in [(1, 0.25, 0.25), (2, 0.25, 0.), (3, 0.0625, 0.)] {
            comp.layers[0].effects[0].params.truncate(2);
            comp.layers[0].effects[0]
                .params
                .push((0x10001, Value::Int(stage)));
            let frame = renderer
                .render_frame(
                    &comp,
                    &RenderRequest {
                        resolution,
                        ..RenderRequest::full()
                    },
                )
                .unwrap();
            let raw = bae_gpu::transfer::read_texture(&gpu, &frame.image.texture).unwrap();
            let alpha =
                |x, y| half::f16::from_le_bytes(raw.pixel(x, y)[6..8].try_into().unwrap()).to_f32();
            let factor = resolution.factor();
            let actual = [
                alpha(8 / factor, 8 / factor),
                alpha(16 / factor, 8 / factor),
            ];
            assert!(
                (actual[0] - left).abs() < 0.002 && (actual[1] - right).abs() < 0.002,
                "{resolution:?} stage {stage}: {actual:?}, expected {left}, {right}"
            );
            assert!(alpha(1 / factor, 8 / factor) < 0.002);
        }
    }
}

fn rectangle_mask(layer: &PrimPath, name: &str, rect: [f64; 4]) -> RenderMask {
    RenderMask {
        path: PrimPath::new(&format!("{layer}/masks/{name}")).unwrap(),
        shape: rect_path(rect[0], rect[1], rect[2], rect[3]),
        mode: MaskMode::Add,
        inverted: false,
        opacity: 1.,
        feather: [0.; 2],
        expansion: 0.,
    }
}

#[test]
fn fill_uses_one_mask_or_both_through_the_render_graph() {
    let Some(gpu) = context() else { return };
    let mut comp = RenderComp::new(PrimPath::new("/comps/main").unwrap(), 16, 8);
    let mut layer = RenderLayer::new(
        PrimPath::new("/comps/main/layers/a").unwrap(),
        LayerContent::Solid {
            color: [0.75, 0.75, 0.75, 1.],
            width: 16,
            height: 8,
        },
    );
    layer.masks = vec![
        rectangle_mask(&layer.path, "left", [0., 0., 4., 8.]),
        rectangle_mask(&layer.path, "right", [12., 0., 16., 8.]),
    ];
    layer.effects = vec![instance(
        "ADBE Fill",
        vec![(1, Value::PrimRef(layer.masks[0].path.clone()))],
    )];
    comp.layers.push(layer);
    let mut renderer = Renderer::new(gpu.clone());
    bae_effects::register(
        &mut bae_model::EffectRegistry::new(),
        renderer.effects_mut(),
    );
    for all in [false, true] {
        comp.layers[0].effects[0].params.truncate(1);
        comp.layers[0].effects[0].params.push((7, Value::Bool(all)));
        let frame = renderer
            .render_frame(&comp, &RenderRequest::full())
            .unwrap();
        let raw = bae_gpu::transfer::read_texture(&gpu, &frame.image.texture).unwrap();
        let alpha =
            |x| half::f16::from_le_bytes(raw.pixel(x, 4)[6..8].try_into().unwrap()).to_f32();
        assert_eq!(alpha(2), 1.);
        assert_eq!(alpha(8), 0.);
        assert_eq!(alpha(14), 1.);
        let green = half::f16::from_le_bytes(raw.pixel(14, 4)[2..4].try_into().unwrap()).to_f32();
        assert_eq!(green, if all { 0. } else { 0.75 });
    }
}

#[test]
fn pixel_effects_cover_the_render_roi_at_half_resolution() {
    let Some(gpu) = context() else { return };
    let mut comp = RenderComp::new(PrimPath::new("/comps/main").unwrap(), 16, 12);
    let mut layer = RenderLayer::new(
        PrimPath::new("/comps/main/layers/a").unwrap(),
        LayerContent::Solid {
            color: [0.25, 0.5, 0.75, 0.5],
            width: 16,
            height: 12,
        },
    );
    layer.effects.push(instance("ADBE Invert", vec![]));
    layer.effects.push(instance(
        "ADBE Shift Channels",
        vec![(2, Value::Int(4)), (4, Value::Int(2))],
    ));
    comp.layers.push(layer);
    let mut renderer = Renderer::new(gpu.clone());
    bae_effects::register(
        &mut bae_model::EffectRegistry::new(),
        renderer.effects_mut(),
    );
    let frame = renderer
        .render_frame(
            &comp,
            &RenderRequest {
                resolution: Resolution::Half,
                roi: Some(IRect::new(4, 2, 14, 10)),
                ..RenderRequest::full()
            },
        )
        .unwrap();
    let rgba = bae_gpu::transfer::read_texture(&gpu, &frame.image.texture).unwrap();
    for y in 0..4 {
        for x in 0..5 {
            let b = rgba.pixel(x, y);
            let px: [f32; 4] = std::array::from_fn(|c| {
                half::f16::from_le_bytes(b[c * 2..c * 2 + 2].try_into().unwrap()).to_f32()
            });
            assert_eq!(px, [0.125, 0.25, 0.375, 0.5]);
        }
    }
}

#[test]
fn photo_filter_restores_luminosity_after_gamut_clipping() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(3, 1);
    let pixels = Pixels::new(rect, |x, _| {
        [
            [0.0, 0.0, 0.5, 0.5],
            [0.5, 0.25, 0.1, 0.5],
            [0.25, 0.25, 0.25, 0.5],
        ][x as usize]
    });
    let params = vec![(1, Value::Int(4)), (3, Value::Float(65.0))];
    let filtered = run_depth(
        &gpu,
        F32,
        get("ADBE PhotoFilterPS").unwrap(),
        params,
        &pixels,
        rect,
        1.0,
        false,
    );
    let lum = |p: [f32; 4]| 0.30 * p[0] + 0.59 * p[1] + 0.11 * p[2];
    for (original, got) in pixels.data.iter().zip(filtered.data) {
        assert!((lum(*original) - lum(got)).abs() < 1e-5);
        assert_eq!(original[3], got[3]);
        assert_ne!(*original, got);
    }
}
