//! Keying and generative effects, including nonzero ROI origins and scanline edges.
mod common;
use bae_gpu::wgpu::TextureFormat::{Rgba16Float, Rgba32Float};
use bae_model::{ParamKind, Value};
use bae_render::IRect;
use common::*;

#[test]
fn matching_plate_clears_alpha_and_fade_only_cuts_along_the_wire() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(0, 0, 32, 24);
    let input = Pixels::new(rect, |_, _| [0.25, 0.25, 0.25, 0.5]);
    let matte = run(
        &gpu,
        Rgba16Float,
        builtin("ADBE Difference Matte2"),
        vec![(6, Value::Float(3.))],
        &input,
        rect,
        1.,
    );
    assert_eq!(matte.get(16, 12), [0.; 4]);
    let wire = run(
        &gpu,
        Rgba16Float,
        builtin("CC Simple Wire Removal"),
        vec![
            (1, Value::Vec2([16.5, 4.5])),
            (2, Value::Vec2([16.5, 20.5])),
            (3, Value::Int(1)),
            (4, Value::Float(4.)),
        ],
        &input,
        rect,
        1.,
    );
    assert_eq!(wire.get(16, 12), [0.; 4]);
    assert_eq!(wire.get(8, 12), input.get(8, 12));
}

#[test]
fn keying_definitions_match_the_ae_disk_ids() {
    let spill = builtin("ADBE Spill2").def();
    assert_eq!(
        spill.params.iter().map(|p| p.id).collect::<Vec<_>>(),
        [1, 1001, 2, 3, 4, 5, 6, 7, 8, 9]
    );
    assert_eq!(spill.param(2).unwrap().kind, ParamKind::GroupStart);
    assert_eq!(spill.param(9).unwrap().kind, ParamKind::GroupEnd);
    assert_eq!(
        builtin("ADBE PS Median").def().param(1).unwrap().default,
        Some(Value::Int(0))
    );
    assert_eq!(
        builtin("ADBE Linear Color Key2")
            .def()
            .param(3)
            .unwrap()
            .default,
        Some(color([0., 0., 1., 1.]))
    );
}

#[test]
fn blue_key_and_standard_spill_preserve_premultiplication() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-2, 6, 20, 16);
    let input = Pixels::new(rect, |x, _| {
        if x < 8 {
            [0., 0., 0.5, 0.5]
        } else {
            [0.1, 0.5, 0.2, 0.5]
        }
    });
    let keyed = run(
        &gpu,
        Rgba16Float,
        builtin("ADBE Linear Color Key2"),
        vec![],
        &input,
        rect,
        1.,
    );
    assert_close(
        &keyed,
        &input.map(|c| if c[0] == 0. { [0.; 4] } else { c }),
        rect,
        0.001,
        "linear key",
    );
    let suppressed = run(
        &gpu,
        Rgba16Float,
        builtin("ADBE Spill2"),
        vec![],
        &input,
        rect,
        1.,
    );
    assert!(suppressed.get(10, 8)[1] < 0.5);
    assert_eq!(suppressed.get(10, 8)[3], 0.5);
}

#[test]
fn median_selects_each_channel_across_workgroups() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-3, 1, 67, 14);
    let roi = IRect::new(1, 3, 64, 12);
    let input = Pixels::new(rect, |x, y| {
        let n = (x + 8 + y * 5) as u32;
        let v = |factor| ((n * factor) % 255u32) as f32 / 127.0;
        [v(37) - 0.9, 1.5 + v(19), v(83), 1.0]
    });
    for r in [1, 3] {
        let got = run(
            &gpu,
            Rgba32Float,
            builtin("ADBE PS Median"),
            vec![(1, Value::Int(r.into()))],
            &input,
            roi,
            1.,
        );
        let expected = Pixels::new(roi, |x, y| {
            std::array::from_fn(|ch| {
                let mut v = Vec::new();
                for dy in -r..=r {
                    for dx in -r..=r {
                        v.push(input.get_clamped(x + dx, y + dy)[ch]);
                    }
                }
                v.sort_by(f32::total_cmp);
                v[v.len() / 2]
            })
        });
        assert_close(&got, &expected, roi, 0.000001, "median");
    }
}

#[test]
fn choker_full_gray_softness_preserves_clear_pixels_and_applies_choke() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(16, 16);
    for (radius, choke, alpha, expected) in [(8., 0., 0., 0.), (0., 51., 0.5, 0.3)] {
        let input = Pixels::new(rect, |_, _| [alpha; 4]);
        let got = run(
            &gpu,
            Rgba32Float,
            builtin("ADBE Matte Choker"),
            vec![
                (1, Value::Float(radius)),
                (2, Value::Float(choke)),
                (3, Value::Float(1.)),
            ],
            &input,
            rect,
            1.,
        );
        assert!((got.get(8, 8)[3] - expected).abs() < 1e-6);
    }
}

#[test]
fn choker_tile_and_cell_evolution_are_repeatable() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-10, -6, 82, 44);
    let roi = IRect::new(2, 1, 61, 37);
    let input = test_image(rect);
    let choker = builtin("ADBE Matte Choker");
    let stages = vec![
        (4, Value::Float(5.)),
        (5, Value::Float(-30.)),
        (7, Value::Int(2)),
    ];
    let full = run(&gpu, Rgba16Float, choker, stages.clone(), &input, rect, 1.);
    let tile = run(&gpu, Rgba16Float, choker, stages, &input, roi, 1.);
    assert_close(&full, &tile, roi, 0.003, "repeated choker ROI");
    let cells = builtin("ADBE Cell Pattern");
    let render = |angle| {
        run(
            &gpu,
            Rgba32Float,
            cells,
            vec![
                (13, Value::Float(angle)),
                (15, Value::Bool(true)),
                (16, Value::Float(2.)),
                (17, Value::Int(7)),
            ],
            &input,
            rect,
            1.,
        )
    };
    let start = render(0.);
    assert_close(&start, &render(720.), rect, 0.0001, "cell cycle");
    assert!(max_diff(&start, &render(95.), rect).0 > 0.05);
}

#[test]
fn cell_pattern_tiling_and_contrast() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(256, 128);
    let input = Pixels::new(rect, |_, _| [0.; 4]);
    let cells = builtin("ADBE Cell Pattern");
    let image = run(&gpu, Rgba32Float, cells, vec![], &input, rect, 1.);
    let count = image.data.len() as f32;
    let mean = image.data.iter().map(|p| p[0]).sum::<f32>() / count;
    let variance = image
        .data
        .iter()
        .map(|p| (p[0] - mean).powi(2))
        .sum::<f32>()
        / count;
    // AE's default on this frame measures 218.3 ± 25.2 (8-bit levels).
    assert!((mean * 255. - 218.3).abs() < 8.);
    assert!((variance.sqrt() * 255. - 25.2).abs() < 9.);
    let tiled = run(
        &gpu,
        Rgba32Float,
        cells,
        vec![
            (6, Value::Float(12.)),
            (9, Value::Bool(true)),
            (10, Value::Int(3)),
            (11, Value::Int(4)),
        ],
        &input,
        rect,
        1.,
    );
    for y in 0..80 {
        for x in 0..220 {
            let c = tiled.get(x, y)[0];
            assert!((tiled.get(x + 36, y)[0] - c).abs() < 0.0001);
            assert!((tiled.get(x, y + 48)[0] - c).abs() < 0.0001);
        }
    }
}

#[test]
fn screen_colour_removes_green_and_preserves_red() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(2, 3, 16, 9);
    let input = Pixels::new(rect, |x, _| {
        if x < 8 {
            [0., 1., 0., 1.]
        } else {
            [0.5, 0., 0., 0.5]
        }
    });
    let out = run(
        &gpu,
        Rgba32Float,
        builtin("Keylight 906"),
        vec![(4, color([0., 1., 0., 1.]))],
        &input,
        rect,
        1.,
    );
    assert_eq!(out.get(4, 4), [0.; 4]);
    assert_eq!(out.get(12, 4), [0.5, 0., 0., 0.5]);
}

#[test]
fn keying_gpu_paths_accept_partial_layer_regions() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(2, 3, 16, 9);
    let input = Pixels::new(rect, |_, _| [0.2, 0.3, 0.1, 1.]);
    let roi = IRect::new(4, 4, 12, 8);
    for name in [
        "ADBE KeyCleaner",
        "ADBE RefineMatte2",
        "ADBE RefineRBMatte",
        "ADBE Unmult",
        "ADBE Extract",
    ] {
        let out = run(&gpu, Rgba16Float, builtin(name), vec![], &input, roi, 1.);
        assert!(out.get(6, 5).iter().all(|n| n.is_finite()), "{name}");
    }
}

#[test]
fn screen_morphology_fills_neighbourhoods_and_removes_isolated_spots() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(-4, 2, 59, 36);
    let green = [0., 1., 0., 1.];
    let red = [1., 0., 0., 1.];
    let island = Pixels::new(rect, |x, y| if (x, y) == (22, 18) { red } else { green });
    let render = |id, amount, input: &Pixels, roi| {
        run(
            &gpu,
            Rgba16Float,
            builtin("Keylight 906"),
            vec![
                (4, color([0., 1., 0., 1.])),
                (2, Value::Int(5)),
                (id, Value::Float(amount)),
            ],
            input,
            roi,
            1.,
        )
    };
    let tile = IRect::new(15, 11, 31, 26);
    let grown = render(15, 5., &island, tile);
    for y in tile.y0..tile.y1 {
        for x in tile.x0..tile.x1 {
            let inside = (x - 22).abs().max((y - 18).abs()) <= 5;
            assert_eq!(grown.get(x, y)[0], f32::from(inside), "grow at {x},{y}");
        }
    }
    let removed = render(18, 4., &island, tile);
    assert_eq!(removed.get(22, 18)[0], 0.);
    let hole = Pixels::new(rect, |x, y| if (x, y) == (22, 18) { green } else { red });
    assert_eq!(render(17, 4., &hole, tile).get(22, 18)[0], 1.);
    let softened = render(16, 4., &island, tile);
    assert!(softened.get(22, 18)[0] > softened.get(23, 18)[0]);
    assert!(softened.get(24, 19)[0] > 0.);
    assert_close(
        &render(16, 4., &island, rect),
        &softened,
        tile,
        0.0001,
        "tiled screen softness",
    );
}

#[test]
fn colour_guidance_preserves_an_equal_luminance_edge() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(3, 1, 64, 25);
    let input = Pixels::new(rect, |x, _| {
        if x < 30 {
            [0.2, 0., 0., 0.2]
        } else {
            [0., 0.8 * 0.299 / 0.587, 0., 0.8]
        }
    });
    let tile = IRect::new(22, 6, 38, 20);
    let render = |roi| {
        run(
            &gpu,
            Rgba32Float,
            builtin("ADBE KeyCleaner"),
            vec![(1, Value::Float(100.)), (6, Value::Float(6.))],
            &input,
            roi,
            1.,
        )
    };
    let result = render(tile);
    assert!(result.get(29, 10)[3] < 0.22 && result.get(30, 10)[3] > 0.78);
    assert_close(&render(rect), &result, tile, 0.0001, "tiled RGB guidance");
}

#[test]
fn wire_displace_horizontal_clears_a_thick_slanted_wire() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(40, 40);
    // A white wire 10 px thick along the diagonal over blue.
    let input = Pixels::new(rect, |x, y| {
        let d = (x as f32 - y as f32).abs() / std::f32::consts::SQRT_2;
        if d < 5. {
            [1., 1., 1., 1.]
        } else {
            [0., 0., 1., 1.]
        }
    });
    let out = run(
        &gpu,
        Rgba16Float,
        builtin("CC Simple Wire Removal"),
        vec![
            (1, Value::Vec2([4.5, 4.5])),
            (2, Value::Vec2([35.5, 35.5])),
            (3, Value::Int(4)),
            (4, Value::Float(10.)),
            (6, Value::Float(0.)),
        ],
        &input,
        rect,
        1.,
    );
    let [r, g, b, _] = out.get(20, 20);
    assert!(
        r < 0.05 && g < 0.05 && b > 0.95,
        "wire centre: {:?}",
        out.get(20, 20)
    );
}
