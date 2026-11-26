//! GPU pixel and parameter checks for Color Correction and Utility.
mod common;
use bae_gpu::wgpu::TextureFormat::Rgba32Float;
use bae_model::{ParamKind, Value};
use bae_render::IRect;
use common::*;

#[test]
fn ae_disk_ids_and_nested_groups() {
    let selective = builtin("ADBE SelectiveColor").def();
    assert_eq!(selective.params.len(), 62);
    assert!((2..=6).all(|id| selective.param(id).unwrap().flags.cannot_time_vary));
    assert_eq!(selective.param(62).unwrap().kind, ParamKind::GroupEnd);
    let wheel = builtin("APC Colorama").def();
    assert_eq!(wheel.params[0].id, 3);
    assert!(wheel.param(10).unwrap().flags.cannot_time_vary);
    assert_eq!(wheel.param(12).unwrap().kind, ParamKind::ArbitraryData);
    assert_eq!(wheel.param(38).unwrap().default, Some(Value::Float(1.)));
    assert_eq!(builtin("ADBE Apply Color LUT2").info.category, "Utility");
}

#[test]
fn embedded_luts_use_the_same_transform_as_bae_color() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-2, 7, 35, 27);
    let input = test_image(rect);
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../bae-color/tests/data/ocio");
    for name in ["curve1d.cube", "shaper3d.cube", "gamma.spi1d", "look.spi3d"] {
        let bytes = bae_color::lutfile::embed(&fixtures.join(name)).unwrap();
        assert_eq!(bae_color::lutfile::embedded_name(&bytes), Some(name));
        let transform = bae_color::lutfile::from_embedded(&bytes).unwrap();
        let expected = input.map(|mut c| {
            transform.apply_premultiplied(std::slice::from_mut(&mut c));
            c
        });
        let got = run(
            &gpu,
            Rgba32Float,
            builtin("ADBE Apply Color LUT2"),
            vec![(1, Value::Bytes(bytes))],
            &input,
            rect,
            1.,
        );
        assert_close(&got, &expected, rect, 0.0001, name);
    }
}

#[test]
fn lumetri_input_and_creative_look_use_straight_color_in_order() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-2, 7, 35, 27);
    let input = test_image(rect);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../bae-color/tests/data/ocio");
    let assets = ["curve1d.cube", "look.spi3d"]
        .map(|name| bae_color::lutfile::embed(&dir.join(name)).unwrap());
    let [first, look] = assets
        .each_ref()
        .map(|b| bae_color::lutfile::from_embedded(b).unwrap());
    let expected = input.map(|mut pixel| {
        first.apply_premultiplied(std::slice::from_mut(&mut pixel));
        let mut graded = pixel;
        look.apply_premultiplied(std::slice::from_mut(&mut graded));
        std::array::from_fn(|i| pixel[i] + 0.35 * (graded[i] - pixel[i]))
    });
    let [first, look] = assets;
    let params = vec![
        (125, Value::Bytes(first)),
        (126, Value::Bytes(look)),
        (26, Value::Float(35.)),
    ];
    let got = run(
        &gpu,
        Rgba32Float,
        builtin("ADBE Lumetri"),
        params,
        &input,
        rect,
        1.,
    );
    assert_close(
        &got,
        &expected,
        rect,
        0.0001,
        "Lumetri LUT order and intensity",
    );
}

#[test]
fn lumetri_secondary_curves_change_only_enabled_channels() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(0, 0, 3, 2);
    let input = test_image(rect).map(|_| [0.25, 0., 0., 0.5]);
    let gray = 0.25 * 0.2126_f32.powf(1. / 2.4);
    for (id, enable, amount, expected) in [
        (106, 105, 1. / 3., [0., 0.25, 0., 0.5]),
        (108, 107, 1., [0.25 * 2_f32.powf(1. / 2.4), 0., 0., 0.5]),
        (110, 109, -1., [gray, gray, gray, 0.5]),
        (112, 111, -1., [gray, gray, gray, 0.5]),
    ] {
        for enabled in [true, false] {
            let params = vec![
                (id, Value::FloatList(vec![0., 0., amount])),
                (enable, Value::Bool(enabled)),
            ];
            let got = run(
                &gpu,
                Rgba32Float,
                builtin("ADBE Lumetri"),
                params,
                &input,
                rect,
                1.,
            );
            let want = input.map(|p| if enabled { expected } else { p });
            assert_close(
                &got,
                &want,
                rect,
                0.0006,
                "Lumetri secondary curve and bypass",
            );
        }
    }
}

#[test]
fn lumetri_mask_refinement_and_sharpen_match_full_frame_on_a_tile() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-11, 6, 89, 58);
    let roi = IRect::new(12, 17, 54, 35);
    let input = test_image(rect);
    let fx = builtin("ADBE Lumetri");
    let params = vec![
        (11, Value::Float(-0.5)),
        (29, Value::Float(-67.)),
        (70, Value::Color([0.4, 0.6, 0.5, 1.])),
        (1007, Value::Float(80.)),
        (1008, Value::Float(80.)),
        (81, Value::Float(40.)),
        (82, Value::Float(5.)),
        (89, Value::Float(50.)),
        (90, Value::Float(45.)),
        (51, Value::Float(-2.)),
    ];
    let full = run(&gpu, Rgba32Float, fx, params.clone(), &input, rect, 1.);
    let tile = run(&gpu, Rgba32Float, fx, params, &input, roi, 1.);
    assert_close(&tile, &full, roi, 0.0001, "Lumetri tile");
}

#[test]
fn full_layer_histogram_is_stable_across_tiles() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-7, 3, 92, 54);
    let roi = IRect::new(5, 10, 50, 32);
    let input = Pixels::new(rect, |x, y| {
        let f = (x + 7) as f32 / 100.;
        let a = if y < 10 { 0. } else { 0.5 };
        [
            (0.1 + 0.6 * f) * a,
            (0.2 + 0.6 * f) * a,
            (0.15 + 0.35 * f) * a,
            a,
        ]
    });
    for name in [
        "ADBE AutoLevels",
        "ADBE AutoContrast",
        "ADBE AutoColor",
        "ADBE Equalize",
    ] {
        let fx = builtin(name);
        let full = run(&gpu, Rgba32Float, fx, vec![], &input, rect, 1.);
        let tile = run(&gpu, Rgba32Float, fx, vec![], &input, roi, 1.);
        assert_close(&tile, &full, roi, 1e-6, name);
        assert!(max_diff(&full, &input, rect).0 > 0.05, "{name}");
        assert_eq!(full.get(15, 5), [0.; 4]);
        assert_eq!(full.get(15, 25)[3], 0.5);
    }
}

#[test]
fn snap_neutral_uses_the_bottom_right_of_complete_areas() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(128, 64);
    let fx = builtin("ADBE AutoColor");
    for spacing in [4, 16] {
        let input = Pixels::new(rect, |x, y| {
            let rgb = if y < 6 {
                [0, 0, 0]
            } else if y < 12 {
                [255, 255, 255]
            } else if y >= 16 && x % spacing < 2 && y % spacing < 2 {
                if x % spacing == 1 && y % spacing == 1 {
                    [143, 114, 126]
                } else {
                    [130, 137, 117]
                }
            } else {
                [240, 25, 235]
            };
            [
                rgb[0] as f32 / 255.,
                rgb[1] as f32 / 255.,
                rgb[2] as f32 / 255.,
                1.,
            ]
        });
        let render = |snap| {
            run(
                &gpu,
                Rgba32Float,
                fx,
                vec![
                    (3, Value::Float(0.)),
                    (4, Value::Float(0.)),
                    (5, Value::Bool(snap)),
                ],
                &input,
                rect,
                1.,
            )
        };
        let on = render(true);
        if spacing == 16 {
            assert_close(&on, &render(false), rect, 1e-6, "under one percent");
        } else {
            let [r, g, b, _] = on.get(1, 17);
            assert!(r.max(g).max(b) - r.min(g).min(b) < 2. / 255.);
            let [r, g, _, _] = on.get(0, 16);
            assert!((r - g).abs() > 15. / 255.);
        }
    }
}

#[test]
fn wheel_uses_custom_stops_and_phase_wraps() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(8, 4);
    let input = Pixels::new(rect, |_, _| [0.25, 0.25, 0.25, 1.0]);
    let render = |angle| {
        run(
            &gpu,
            Rgba32Float,
            builtin("APC Colorama"),
            vec![
                (2, Value::Int(2)),
                (8, Value::Float(angle)),
                (12, Value::Bytes(b"[[0,1,0,0,1],[0.5,0,0,1,1]]".to_vec())),
            ],
            &input,
            rect,
            1.,
        )
    };
    let a = render(0.);
    assert_close(&a, &render(360.), rect, 1e-6, "phase wraps");
    assert_close(
        &a,
        &Pixels::new(rect, |_, _| [0.5, 0., 0.5, 1.]),
        rect,
        0.006,
        "interpolated custom wheel",
    );
}

#[test]
fn colorama_reads_phase_and_mask_from_a_hidden_layer() {
    use bae_model::PrimPath;
    use bae_render::{LayerContent, RenderComp, RenderLayer, RenderRequest, Renderer};

    let Some(gpu) = context() else { return };
    let mut comp = RenderComp::new(PrimPath::new("/comps/main").unwrap(), 8, 8);
    let solid = |name: &str, color| {
        RenderLayer::new(
            PrimPath::new(&format!("/comps/main/layers/{name}")).unwrap(),
            LayerContent::Solid {
                color,
                width: 8,
                height: 8,
            },
        )
    };
    let mut layer = solid("a", [0.0, 0.0, 0.0, 1.0]);
    let mut control = solid("control", [0.25, 0.0, 0.0, 0.5]);
    control.visible = false;
    layer.effects.push(instance(
        "APC Colorama",
        vec![
            (2, Value::Int(10)),
            (4, Value::PrimRef(control.path.clone())),
            (6, Value::Int(2)),
            (28, Value::PrimRef(control.path.clone())),
            (30, Value::Int(3)),
            (32, Value::Bool(false)),
        ],
    ));
    comp.layers = vec![layer, control];
    let mut renderer = Renderer::new(gpu.clone());
    bae_effects::register(
        &mut bae_model::EffectRegistry::new(),
        renderer.effects_mut(),
    );
    let frame = renderer
        .render_frame(&comp, &RenderRequest::full())
        .unwrap();
    let image = bae_gpu::transfer::read_texture(&gpu, &frame.image.texture).unwrap();
    for (channel, expected) in image
        .pixel(4, 4)
        .as_chunks::<2>()
        .0
        .iter()
        .zip([0.25, 0.5, 0.0, 0.5])
    {
        let actual = half::f16::from_le_bytes(*channel).to_f32();
        assert!((actual - expected).abs() < 0.01, "{actual} vs {expected}");
    }
}

#[test]
fn legacy_corrections_clip_straight_hdr_before_premultiplying() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(3, 2);
    let input = Pixels::new(rect, |_, _| [-0.25, 0.75, 0.25, 0.5]);
    let expected = Pixels::new(rect, |_, _| [0.0, 0.5, 0.25, 0.5]);
    for name in [
        "ADBE Gamma/Pedestal/Gain2",
        "ADBE Leave Color",
        "ADBE SelectiveColor",
    ] {
        let actual = run(&gpu, Rgba32Float, builtin(name), vec![], &input, rect, 1.);
        assert_close(&actual, &expected, rect, 1e-5, name);
    }
    for (name, id, value) in [
        ("APC Colorama", 36, 100.),
        ("ADBE AutoLevels", 5, 100.),
        ("ADBE AutoContrast", 5, 100.),
        ("ADBE AutoColor", 6, 100.),
        ("ADBE Equalize", 2, 0.),
    ] {
        let original = run(
            &gpu,
            Rgba32Float,
            builtin(name),
            vec![(id, Value::Float(value))],
            &input,
            rect,
            1.,
        );
        assert_close(&original, &expected, rect, 0.005, name);
    }
    let overwhite = Pixels::new(rect, |_, _| [0.75, 0.75, 0.75, 0.5]);
    let white = Pixels::new(rect, |_, _| [0.5; 4]);
    let actual = run(
        &gpu,
        Rgba32Float,
        builtin("ADBE Broadcast Colors"),
        vec![],
        &overwhite,
        rect,
        1.,
    );
    assert_close(&actual, &white, rect, 1e-5, "broadcast white");
}

#[test]
fn ocio_file_and_cdl_apply_embedded_grades_on_gpu() {
    let Some(gpu) = context() else {
        return;
    };
    let rect = IRect::from_size(4, 4);
    let pixels = Pixels::new(rect, |_, _| [0.15, 0.1, 0.05, 0.5]);
    let cdl =
        b"test.cc\0<ColorCorrection><SOPNode><Slope>2 1 1</Slope></SOPNode></ColorCorrection>"
            .to_vec();
    let cube = b"test.cube\0LUT_1D_SIZE 2\n0 0 0\n2 1 1\n".to_vec();
    for (name, id, bytes) in [
        ("ADBE OCIO CDL Transform", 6, cdl),
        ("ADBE OCIO FILE Transform", 5, cube),
    ] {
        let out = run(
            &gpu,
            F32,
            builtin(name),
            vec![(id, Value::Bytes(bytes))],
            &pixels,
            rect,
            1.,
        );
        for (actual, expected) in out.get(2, 2).into_iter().zip([0.3, 0.1, 0.05, 0.5]) {
            assert!((actual - expected).abs() < 0.0001, "{name}: {actual}");
        }
    }
}
