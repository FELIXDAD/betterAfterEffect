//! Keying and generative effects, including nonzero ROI origins and scanline edges.
mod common;
use bae_gpu::wgpu::TextureFormat::{Rgba16Float, Rgba32Float};
use bae_model::{ParamKind, Value};
use bae_render::IRect;
use common::*;

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
        [n * 37 % 255, n * 19 % 255, n * 83 % 255, 255].map(|v| v as f32 / 255.)
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
        assert_close(&got, &expected, roi, 0.002, "median");
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
