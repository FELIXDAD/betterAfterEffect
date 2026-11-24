//! Shared by `examples/ae_compare.rs` and `tests/ae_golden.rs`: the generated
//! inputs, the case list and rendering a case with BAE at 8 bpc.

#![allow(dead_code)]

use std::sync::Arc;

use bae_effects::Standalone;
use bae_gpu::GpuContext;
use bae_gpu::wgpu;
use bae_model::{PrimPath, Value};
use bae_render::{EffectParams, IRect, ImageAlpha, RenderEffect};

pub const CASES: &str = include_str!("cases.json");
pub const SIZE: [u32; 2] = [256, 128];

/// One comparison case.
#[derive(Clone, Debug)]
pub struct Case {
    pub name: String,
    pub input: String,
    pub bpc: u64,
    pub effect: String,
    pub time: f64,
    pub params: Vec<(u32, Value)>,
    /// Allowed max per-channel difference (8-bit levels).
    pub tolerance: u8,
    /// Allowed mean difference (8-bit levels).
    pub mean: f64,
}

pub fn cases() -> Vec<Case> {
    parse_cases(CASES)
}

/// Cases of a `cases.json` text.
pub fn parse_cases(text: &str) -> Vec<Case> {
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let params = c["params"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| {
                    let id: u32 = k.parse().unwrap();
                    let value = match v {
                        serde_json::Value::Array(a) => {
                            let n: Vec<f64> = a.iter().map(|x| x.as_f64().unwrap()).collect();
                            match n.as_slice() {
                                [x, y] => Value::Vec2([*x, *y]),
                                [r, g, b] => Value::Color([*r, *g, *b, 1.0]),
                                [r, g, b, a] => Value::Color([*r, *g, *b, *a]),
                                _ => panic!("invalid AE array parameter {id}: {n:?}"),
                            }
                        }
                        other => Value::Float(other.as_f64().unwrap()),
                    };
                    (id, value)
                })
                .collect();
            Case {
                name: c["name"].as_str().unwrap().to_owned(),
                bpc: c["bpc"].as_u64().unwrap_or(8),
                input: c["input"].as_str().unwrap().to_owned(),
                effect: c["effect"].as_str().unwrap().to_owned(),
                time: c["time"].as_f64().unwrap_or(0.0),
                params,
                tolerance: c["tolerance"].as_u64().unwrap_or(2) as u8,
                mean: c["mean"].as_f64().unwrap_or(0.5),
            }
        })
        .collect()
}

/// Straight RGBA8 input images, generated (no binary files in the repository).
pub fn input(name: &str) -> Vec<[u8; 4]> {
    let [w, h] = SIZE;
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let fx = x as f64 / (w - 1) as f64;
            let fy = y as f64 / (h - 1) as f64;
            let px = match name {
                // Probes: a hard vertical edge (blur kernels) and grey ramps
                // (tone curves; the lower half at half range).
                "fine" => [
                    (x % 16) as f64 * 16. / 255.,
                    (y % 16) as f64 * 16. / 255.,
                    ((x / 16 + y / 16) % 2) as f64 * 128. / 255.,
                    1.,
                ],
                "coords" => [fx, y as f64 * 2.0 / 255.0, 0.25, 1.0],
                "step" => {
                    let v = if x >= w / 2 { 1.0 } else { 0.0 };
                    [v, v, v, 1.0]
                }
                "line" => {
                    let v = if x == w / 2 { 1.0 } else { 0.0 };
                    [v, v, v, 1.0]
                }
                "lineg" => {
                    // A white line on 50 % grey (negative kernel lobes show).
                    let v = if x == w / 2 { 1.0 } else { 0.5 };
                    [v, v, v, 1.0]
                }
                "ramp" => {
                    let v = x as f64 / (w - 1) as f64;
                    let v = if y < h / 2 { v } else { 0.5 * v };
                    [v, v, v, 1.0]
                }
                "chart" => {
                    if y < h / 2 {
                        // Hue ramp with a brightness ramp downwards.
                        let hsv = hsv_to_rgb(fx, 1.0 - 0.5 * fy, 1.0);
                        [hsv[0], hsv[1], hsv[2], 1.0]
                    } else if x < w / 2 {
                        // Grey ramp with hard 16 px blocks of white.
                        let g = fx * 2.0;
                        let block = (x / 16 + y / 16) % 5 == 0;
                        let v = if block { 1.0 } else { g };
                        [v, v, v, 1.0]
                    } else {
                        // Skin-ish, sky-ish and saturated patches.
                        let patches = [
                            [0.85, 0.62, 0.50],
                            [0.35, 0.55, 0.85],
                            [0.10, 0.65, 0.20],
                            [0.95, 0.85, 0.10],
                            [0.60, 0.10, 0.45],
                            [0.05, 0.05, 0.05],
                        ];
                        let p = patches[((x - w / 2) / 22 + (y / 22)) as usize % patches.len()];
                        [p[0], p[1], p[2], 1.0]
                    }
                }
                _ => {
                    // "soft": a coloured disc with a feathered alpha edge on transparency.
                    let dx = x as f64 + 0.5 - w as f64 / 2.0;
                    let dy = y as f64 + 0.5 - h as f64 / 2.0;
                    let d = (dx * dx + dy * dy).sqrt();
                    let a = ((52.0 - d) / 12.0).clamp(0.0, 1.0);
                    [0.9 * fx + 0.1, 0.4, 1.0 - fy, a]
                }
            };
            out.push(px.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    out
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> [f64; 3] {
    let k = |n: f64| {
        let k = (n + h * 6.0) % 6.0;
        v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)
    };
    [k(5.0), k(3.0), k(1.0)]
}

/// Renders `case` with BAE (RGBA32F working buffer, effect at full resolution,
/// output = the comp rect) and returns premultiplied RGBA8.
pub fn render(gpu: &Arc<GpuContext>, case: &Case) -> Vec<[u8; 4]> {
    let fx =
        bae_effects::get(&case.effect).unwrap_or_else(|| panic!("no built-in {}", case.effect));
    let [w, h] = SIZE;
    let rect = IRect::from_size(w, h);
    // These channel effects can reveal the original RGB of transparent
    // texels. A straight source keeps that data available, as the AE input does.
    let straight = matches!(
        case.effect.as_str(),
        "ADBE Invert"
            | "ADBE Shift Channels"
            | "ADBE Set Matte3"
            | "ADBE Simple Choker"
            | "ADBE Minimax"
    );
    let data: Vec<u8> = input(&case.input)
        .iter()
        .flat_map(|p| {
            let a = f32::from(p[3]) / 255.0;
            let gain = if straight { 1.0 } else { a };
            [
                f32::from(p[0]) / 255.0 * gain,
                f32::from(p[1]) / 255.0 * gain,
                f32::from(p[2]) / 255.0 * gain,
                a,
            ]
        })
        .flat_map(f32::to_le_bytes)
        .collect();
    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("ae compare input"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: bae_effects::IMAGE_USAGE,
        view_formats: &[],
    });
    bae_gpu::transfer::upload_texture(
        gpu,
        &texture,
        bae_gpu::transfer::TextureRegion::full(&texture),
        &data,
    )
    .unwrap();
    let image = Standalone::image(
        texture,
        rect,
        if straight {
            ImageAlpha::Straight
        } else {
            ImageAlpha::Premultiplied
        },
    );
    let inst = RenderEffect {
        path: PrimPath::new("/comps/ae/layers/input/effects/fx").unwrap(),
        effect: case.effect.clone(),
        enabled: true,
        params: case.params.clone(),
    };
    let p = EffectParams {
        time: bae_model::Time::from_seconds(case.time),
        scale: 1.0,
        float: case.bpc == 32,
        sixteen_bpc: case.bpc == 16,
        ..EffectParams::new(&inst)
    };
    let mut target = Standalone::new(gpu.clone(), wgpu::TextureFormat::Rgba32Float);
    let out = fx.render(&mut target, &p, &image, rect).unwrap();
    target.finish().unwrap();
    let back = bae_gpu::transfer::read_texture(gpu, &out.texture).unwrap();
    let mut px = vec![[0u8; 4]; (w * h) as usize];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let (tx, ty) = (x - out.rect.x0, y - out.rect.y0);
            if tx < 0 || ty < 0 || tx >= out.rect.width() as i32 || ty >= out.rect.height() as i32 {
                continue;
            }
            let b = back.pixel(tx as u32, ty as u32);
            let mut c: [f32; 4] = std::array::from_fn(|k| {
                f32::from_le_bytes(b[4 * k..4 * k + 4].try_into().unwrap())
            });
            if out.alpha == ImageAlpha::Straight {
                c = [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]];
            }
            px[(y as u32 * w + x as u32) as usize] =
                c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    px
}

/// Max and mean absolute difference, and pixels differing by more than 1 / 3.
pub fn diff(a: &[[u8; 4]], b: &[[u8; 4]]) -> (u8, f64, usize, usize) {
    let mut max = 0u8;
    let mut sum = 0u64;
    let (mut over1, mut over3) = (0, 0);
    for (p, q) in a.iter().zip(b) {
        let d = (0..4).map(|k| p[k].abs_diff(q[k])).max().unwrap_or(0);
        max = max.max(d);
        sum += u64::from(d);
        over1 += usize::from(d > 1);
        over3 += usize::from(d > 3);
    }
    (max, sum as f64 / a.len().max(1) as f64, over1, over3)
}

pub fn write_png(path: &std::path::Path, px: &[[u8; 4]]) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), SIZE[0], SIZE[1]);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    writer.write_image_data(px.as_flattened()).unwrap();
}

pub fn read_png(path: &std::path::Path, bpc: u64) -> Option<Vec<[u8; 4]>> {
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba {
        return None;
    }
    let data = &buf[..info.buffer_size()];
    match info.bit_depth {
        png::BitDepth::Eight => Some(data.as_chunks::<4>().0.to_vec()),
        png::BitDepth::Sixteen => Some(
            data.as_chunks::<8>()
                .0
                .iter()
                .map(|p| {
                    std::array::from_fn(|i| {
                        // AE 26 saveFrameToPng maps 32-bpc reference white
                        // to 6553/65535. Alpha retains its full range.
                        let value = u32::from(u16::from_be_bytes([p[2 * i], p[2 * i + 1]]));
                        let scale = if bpc == 32 && i < 3 { 10 } else { 1 };
                        ((value * scale + 128) / 257).min(255) as u8
                    })
                })
                .collect(),
        ),
        _ => None,
    }
}
