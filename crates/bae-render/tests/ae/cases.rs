//! AE comparison cases (`cases.json`), shared by the `ae_compare` example and the
//! `ae_golden` test: deterministic footage, building every case as a BAE project
//! through commands, and the pixel diff.

#![allow(dead_code)]

use std::path::Path;

use bae_model::{FrameRate, PrimPath, Time, Value};
use bae_render::fixture::{SceneBuilder, ellipse_path, rect_path, write_label_png};
use bae_render::png_out::{read_rgba8, write_rgba8};
use serde_json::Value as Json;

pub fn cases() -> Json {
    serde_json::from_str(include_str!("cases.json")).expect("cases.json")
}

pub fn write_image(path: &Path, w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) {
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            data.extend_from_slice(&f(x, y));
        }
    }
    write_rgba8(
        path,
        &bae_gpu::transfer::ImageData {
            width: w,
            height: h,
            format: bae_gpu::wgpu::TextureFormat::Rgba8Unorm,
            bytes_per_row: w * 4,
            data,
        },
    )
    .expect("write asset");
}

/// Footage used by the cases (deterministic, generated on both sides).
pub fn prepare(dir: &Path) {
    std::fs::create_dir_all(dir).expect("assets dir");
    write_image(&dir.join("ramp_x.png"), 256, 256, |x, _| {
        [x as u8, x as u8, x as u8, 255]
    });
    write_image(&dir.join("ramp_y.png"), 256, 256, |_, y| {
        [y as u8, y as u8, y as u8, 255]
    });
    write_image(&dir.join("color_ramp.png"), 256, 256, |x, y| {
        [x as u8, y as u8, 255 - x as u8, 255]
    });
    write_label_png(&dir.join("label.png"), 64, 24, 0.1).expect("label");
}

pub fn num(v: &Json) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

pub fn vec2(v: &Json) -> [f64; 2] {
    [num(&v[0]), num(&v[1])]
}

/// Builds every case into one stage (inner comps first, as listed).
pub fn build(assets: &Path) -> (SceneBuilder, Vec<(String, u8)>) {
    let mut b = SceneBuilder::new();
    let mut outer = Vec::new();
    let spec = cases();
    let mut comp_sizes = std::collections::HashMap::new();
    for case in spec["cases"].as_array().expect("cases") {
        let name = case["name"].as_str().expect("name");
        let [w, h] = vec2(&case["size"]);
        comp_sizes.insert(name.to_owned(), [w, h]);
        let comp = b
            .comp(
                name,
                w as u32,
                h as u32,
                FrameRate::FPS_30,
                Time::ONE_SECOND,
            )
            .expect("comp");
        let layers = case["layers"].as_array().expect("layers");
        let mut paths = Vec::new();
        for l in layers {
            let lname = l["name"].as_str().expect("layer name");
            let (layer, size) = if let Some(c) = l["solid"].as_array() {
                let [sw, sh] = vec2(&l["size"]);
                let color = [
                    num(&c[0]) / 255.0,
                    num(&c[1]) / 255.0,
                    num(&c[2]) / 255.0,
                    1.0,
                ];
                let asset = b
                    .solid_asset(&format!("{name}_{lname}"), color, sw as u32, sh as u32)
                    .expect("solid");
                (
                    b.layer(&comp, lname, "solid", Some(&asset)).expect("layer"),
                    [sw, sh],
                )
            } else if let Some(file) = l["footage"].as_str() {
                let path = assets.join(file);
                let (fw, fh, _) = read_rgba8(&path).expect("footage");
                let asset_name = format!("{name}_{}", file.trim_end_matches(".png"));
                let asset = b
                    .footage_asset(&asset_name, &path, None, Some([fw, fh]))
                    .expect("footage");
                (
                    b.layer(&comp, lname, "footage", Some(&asset))
                        .expect("layer"),
                    [f64::from(fw), f64::from(fh)],
                )
            } else {
                let inner = l["comp"].as_str().expect("solid, footage or comp");
                let src = PrimPath::new(&format!("/comps/{inner}")).expect("path");
                (
                    b.layer(&comp, lname, "pre_comp", Some(&src))
                        .expect("layer"),
                    comp_sizes[inner],
                )
            };
            b.set(
                &layer,
                "transform:anchor_point",
                Value::Vec3([size[0] / 2.0, size[1] / 2.0, 0.0]),
            )
            .expect("anchor");
            let [x, y] = vec2(&l["pos"]);
            b.set(&layer, "transform:position", Value::Vec3([x, y, 0.0]))
                .expect("position");
            if l["scale"].is_array() {
                let [sx, sy] = vec2(&l["scale"]);
                b.set(&layer, "transform:scale", Value::Vec3([sx, sy, 100.0]))
                    .expect("scale");
            }
            if let Some(r) = l["rot"].as_f64() {
                b.set(&layer, "transform:rotation", Value::Float(r))
                    .expect("rotation");
            }
            if let Some(o) = l["opacity"].as_f64() {
                b.set(&layer, "transform:opacity", Value::Float(o))
                    .expect("opacity");
            }
            if let Some(m) = l["mode"].as_str() {
                b.set(&layer, "blend_mode", Value::Enum(m.into()))
                    .expect("mode");
            }
            if l["preserve"].as_bool() == Some(true) {
                b.set(&layer, "preserve_transparency", Value::Bool(true))
                    .expect("preserve");
            }
            if l["enabled"].as_bool() == Some(false) {
                b.set(&layer, "enabled", Value::Bool(false))
                    .expect("enabled");
            }
            for (i, m) in l["masks"].as_array().into_iter().flatten().enumerate() {
                add_mask(&mut b, &layer, i, m);
            }
            paths.push(layer);
        }
        for (i, l) in layers.iter().enumerate() {
            if let Some(m) = l.get("matte").filter(|m| m.is_object()) {
                let target = &paths[m["layer"].as_u64().expect("matte layer") as usize];
                b.set(
                    &paths[i],
                    "track_matte",
                    Value::Enum(m["mode"].as_str().expect("mode").into()),
                )
                .expect("matte");
                b.set(
                    &paths[i],
                    "track_matte_layer",
                    Value::PrimRef(target.clone()),
                )
                .expect("matte layer");
            }
        }
        if case["inner"].as_bool() != Some(true) {
            outer.push((
                name.to_owned(),
                case["tolerance"].as_u64().unwrap_or(1) as u8,
            ));
        }
    }
    (b, outer)
}

/// Mask `i` of a case layer: `rect` / `ellipse` `[x0, y0, x1, y1]` or `poly`
/// `[[x, y], …]` in layer pixels, `mode`, `inverted`, `opacity` (%), `feather`
/// `[x, y]`, `expansion` – the same keys `render_cases.jsx` reads.
fn add_mask(b: &mut SceneBuilder, layer: &PrimPath, i: usize, m: &Json) {
    let r = |k: &str| {
        let a = m[k].as_array().expect("rect");
        [num(&a[0]), num(&a[1]), num(&a[2]), num(&a[3])]
    };
    let shape = if m["rect"].is_array() {
        let [x0, y0, x1, y1] = r("rect");
        rect_path(x0, y0, x1, y1)
    } else if m["ellipse"].is_array() {
        let [x0, y0, x1, y1] = r("ellipse");
        ellipse_path(x0, y0, x1, y1)
    } else {
        let pts: Vec<[f64; 2]> = m["poly"]
            .as_array()
            .expect("rect, ellipse or poly")
            .iter()
            .map(vec2)
            .collect();
        bae_model::BezierPath {
            in_tangents: vec![[0.0, 0.0]; pts.len()],
            out_tangents: vec![[0.0, 0.0]; pts.len()],
            vertices: pts,
            closed: true,
            feather: Vec::new(),
        }
    };
    let mode = m["mode"].as_str().unwrap_or("add");
    let mask = b
        .mask(layer, &format!("mask_{}", i + 1), shape, mode)
        .expect("mask");
    if m["inverted"].as_bool() == Some(true) {
        b.set(&mask, "inverted", Value::Bool(true))
            .expect("inverted");
    }
    if let Some(o) = m["opacity"].as_f64() {
        b.set(&mask, "opacity", Value::Float(o)).expect("opacity");
    }
    if m["feather"].is_array() {
        b.set(&mask, "feather", Value::Vec2(vec2(&m["feather"])))
            .expect("feather");
    }
    if let Some(e) = m["expansion"].as_f64() {
        b.set(&mask, "expansion", Value::Float(e))
            .expect("expansion");
    }
}

/// Cases whose AE golden has not been rendered yet (`pending_golden`).
pub fn pending(name: &str) -> bool {
    cases()["cases"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|c| c["name"].as_str() == Some(name) && c["pending_golden"].as_bool() == Some(true))
}

#[derive(Default)]
pub struct Diff {
    pub max: u8,
    pub mean: f64,
    pub over1: usize,
    pub over3: usize,
    pub pixels: usize,
    pub worst: (u32, u32, [u8; 4], [u8; 4]),
}

pub fn diff(w: u32, a: &[u8], b: &[u8]) -> Diff {
    let mut d = Diff::default();
    let mut sum = 0u64;
    for (i, (pa, pb)) in a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .enumerate()
    {
        let m = pa
            .iter()
            .zip(pb)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        sum += pa
            .iter()
            .zip(pb)
            .map(|(x, y)| u64::from(x.abs_diff(*y)))
            .sum::<u64>();
        if m > 1 {
            d.over1 += 1;
        }
        if m > 3 {
            d.over3 += 1;
        }
        if m > d.max {
            d.max = m;
            let (x, y) = (i as u32 % w, i as u32 / w);
            d.worst = (x, y, *pa, *pb);
        }
        d.pixels += 1;
    }
    d.mean = sum as f64 / (d.pixels.max(1) * 4) as f64;
    d
}
