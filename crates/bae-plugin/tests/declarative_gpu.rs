//! Declarative effects on a real GPU, compared with CPU references.
//!
//! Skipped (with a note) when the machine has no usable adapter; set
//! `BAE_REQUIRE_GPU=1` to make that a failure.

use std::path::PathBuf;
use std::sync::Arc;

use bae_gpu::transfer;
use bae_gpu::wgpu;
use bae_gpu::{GpuContext, GpuContextDescriptor, GpuError};
use bae_model::{PrimPath, Time, Value};
use bae_plugin::declarative::create_image;
use bae_plugin::{DeclarativeEffect, DirSource, MemorySource, Package};
use bae_render::{Effect, EffectParams, GpuImage, IRect, RenderEffect, RenderError};

fn context() -> Option<Arc<GpuContext>> {
    match GpuContext::new_headless(GpuContextDescriptor::default().with_env()) {
        Ok(ctx) => Some(ctx),
        Err(error @ GpuError::NoAdapter { .. }) => {
            if std::env::var_os("BAE_REQUIRE_GPU").is_some_and(|v| v == "1") {
                panic!("BAE_REQUIRE_GPU=1 but {error}");
            }
            eprintln!("skipping GPU test: {error}");
            None
        }
        Err(error) => panic!("GPU context creation failed: {error}"),
    }
}

const F32: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
const F16: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// An RGBA image as f32 pixels covering `rect`.
#[derive(Clone, Debug)]
struct Pixels {
    rect: IRect,
    data: Vec<[f32; 4]>,
}

impl Pixels {
    fn new(rect: IRect, f: impl Fn(i32, i32) -> [f32; 4]) -> Self {
        let mut data = Vec::new();
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                data.push(f(x, y));
            }
        }
        Self { rect, data }
    }

    /// Transparent outside the rect (AE semantics).
    fn get(&self, x: i32, y: i32) -> [f32; 4] {
        let r = self.rect;
        if x < r.x0 || y < r.y0 || x >= r.x1 || y >= r.y1 {
            return [0.0; 4];
        }
        self.data[((y - r.y0) as usize) * r.width() as usize + (x - r.x0) as usize]
    }

    fn upload(&self, gpu: &GpuContext, format: wgpu::TextureFormat) -> GpuImage {
        let image = create_image(gpu, self.rect, 1.0, format).unwrap();
        let bytes: Vec<u8> = match format {
            F32 => self
                .data
                .iter()
                .flatten()
                .flat_map(|x| x.to_le_bytes())
                .collect(),
            F16 => self
                .data
                .iter()
                .flatten()
                .flat_map(|x| half::f16::from_f32(*x).to_le_bytes())
                .collect(),
            other => panic!("{other:?}"),
        };
        transfer::upload_texture(
            gpu,
            &image.texture,
            transfer::TextureRegion::full(&image.texture),
            &bytes,
        )
        .unwrap();
        image
    }

    fn download(gpu: &GpuContext, image: &GpuImage) -> Self {
        let data = transfer::read_texture(gpu, &image.texture).unwrap();
        let w = image.rect.width() as usize;
        let mut out = Vec::new();
        for y in 0..image.rect.height() as usize {
            for x in 0..w {
                let px = data.pixel(x as u32, y as u32);
                let c: [f32; 4] = match data.format {
                    F32 => std::array::from_fn(|k| {
                        f32::from_le_bytes(px[4 * k..4 * k + 4].try_into().unwrap())
                    }),
                    F16 => std::array::from_fn(|k| {
                        half::f16::from_le_bytes(px[2 * k..2 * k + 2].try_into().unwrap()).to_f32()
                    }),
                    other => panic!("{other:?}"),
                };
                out.push(c);
            }
        }
        Self {
            rect: image.rect,
            data: out,
        }
    }
}

fn assert_close(got: &Pixels, want: &Pixels, tol: f32) {
    assert_eq!(got.rect, want.rect);
    for y in got.rect.y0..got.rect.y1 {
        for x in got.rect.x0..got.rect.x1 {
            let (g, w) = (got.get(x, y), want.get(x, y));
            for k in 0..4 {
                assert!(
                    (g[k] - w[k]).abs() <= tol,
                    "pixel ({x}, {y}) channel {k}: got {g:?}, want {w:?}"
                );
            }
        }
    }
}

fn instance(match_name: &str, params: Vec<(u32, Value)>) -> RenderEffect {
    RenderEffect {
        path: PrimPath::new("/comps/main/layers/a/effects/fx").unwrap(),
        effect: match_name.to_owned(),
        enabled: true,
        params,
    }
}

/// Renders one effect into a fresh encoder and waits for the result.
fn run(
    gpu: &Arc<GpuContext>,
    effect: &DeclarativeEffect,
    instance: &RenderEffect,
    scale: f64,
    input: &GpuImage,
    output_roi: IRect,
    format: wgpu::TextureFormat,
) -> GpuImage {
    let params = EffectParams {
        effect: instance,
        time: Time::from_seconds(1.0),
        scale,
    };
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("test"),
        });
    let mut alloc = |rect: IRect,
                     s: f64,
                     f: wgpu::TextureFormat|
     -> Result<GpuImage, RenderError> { create_image(gpu, rect, s, f) };
    let out = effect
        .render_with(
            gpu,
            &mut encoder,
            format,
            &mut alloc,
            &params,
            input,
            output_roi,
        )
        .unwrap();
    gpu.queue().submit([encoder.finish()]);
    out
}

fn memory_effect(manifest: &str, shader: &str) -> Arc<DeclarativeEffect> {
    let pkg = Package::load(
        &MemorySource::new("test")
            .with_file("bae-plugin.toml", manifest)
            .with_file("fx.wgsl", shader),
    )
    .unwrap_or_else(|d| panic!("{d:#?}"));
    DeclarativeEffect::from_package(&pkg).unwrap().remove(0)
}

const TINT_MANIFEST: &str = r#"schema_version = 1
[plugin]
id = "com.example.tint"
version = "1.0.0"
api = "0.1"
name = "Tint"
[[effect]]
id = "tint"
name = "Tint"
implementation = "wgsl"
shader = "fx.wgsl"
identity_when = "params.amount == 0"
[[effect.param]]
id = 1
key = "amount"
type = "float"
name = "Amount"
default = 1.0
range = [0.0, 1.0]
[[effect.param]]
id = 2
key = "color"
type = "color"
name = "Color"
default = [1.0, 0.5, 0.25, 1.0]
[[effect.param]]
id = 3
key = "mode"
type = "popup"
name = "Mode"
choices = ["Multiply", "Replace"]
[[effect.param]]
id = 4
key = "offset"
type = "point"
name = "Offset"
relative = false
"#;

/// mode 1: multiply by the color; mode 2: replace with the color (keeping alpha);
/// then add `offset.x / 1000` to red so the point conversion is visible.
const TINT_SHADER: &str = "
fn tint(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    var t: vec4<f32>;
    if (params.mode == 1u) {
        t = vec4<f32>(c.rgb * params.color.rgb, c.a);
    } else {
        t = vec4<f32>(params.color.rgb * c.a, c.a);
    }
    let r = mix(c, t, params.amount);
    return r + vec4<f32>(params.offset.x / 1000.0, 0.0, 0.0, 0.0);
}
";

#[test]
fn pixel_effect_matches_cpu_reference() {
    let Some(gpu) = context() else { return };
    let fx = memory_effect(TINT_MANIFEST, TINT_SHADER);
    assert_eq!(fx.def().match_name, "com.example.tint/tint");
    let rect = IRect::new(-3, 2, 29, 21);
    let src = Pixels::new(rect, |x, y| {
        let a = ((x + y).rem_euclid(5)) as f32 / 4.0;
        [0.3 * a, (x as f32 / 32.0).abs() * a, 0.7 * a, a]
    });
    for format in [F32, F16] {
        let input = src.upload(&gpu, format);
        let inst = instance(
            "com.example.tint/tint",
            vec![
                (1, Value::Float(0.5)),
                (2, Value::Color([0.5, 1.0, 2.0, 1.0])),
                (3, Value::Int(1)),
                (4, Value::Vec2([100.0, 0.0])),
            ],
        );
        // A sub-rect ROI: only that region is produced.
        let roi = IRect::new(0, 4, 20, 15);
        let out = run(&gpu, &fx, &inst, 1.0, &input, roi, format);
        assert_eq!(out.rect, roi);
        let want = Pixels::new(roi, |x, y| {
            let c = src.get(x, y);
            let m = [c[0] * 0.5, c[1], c[2] * 2.0, c[3]];
            std::array::from_fn(|k| c[k] + (m[k] - c[k]) * 0.5 + if k == 0 { 0.1 } else { 0.0 })
        });
        let tol = if format == F32 { 1e-5 } else { 4e-3 };
        assert_close(&Pixels::download(&gpu, &out), &want, tol);

        // Half resolution: points arrive in render pixels (×0.5).
        let out = run(&gpu, &fx, &inst, 0.5, &input, roi, format);
        let want_half = Pixels::new(roi, |x, y| {
            let c = src.get(x, y);
            let m = [c[0] * 0.5, c[1], c[2] * 2.0, c[3]];
            std::array::from_fn(|k| c[k] + (m[k] - c[k]) * 0.5 + if k == 0 { 0.05 } else { 0.0 })
        });
        assert_close(&Pixels::download(&gpu, &out), &want_half, tol);
    }
}

#[test]
fn identity_and_defaults() {
    let Some(gpu) = context() else { return };
    let fx = memory_effect(TINT_MANIFEST, TINT_SHADER);
    let src = Pixels::new(IRect::from_size(8, 8), |x, _| {
        [x as f32 / 8.0, 0.5, 0.25, 1.0]
    });
    let input = src.upload(&gpu, F32);
    // amount == 0 → the input itself, no GPU work.
    let inst = instance("com.example.tint/tint", vec![(1, Value::Float(0.0))]);
    let out = run(&gpu, &fx, &inst, 1.0, &input, input.rect, F32);
    assert_eq!(out.texture, input.texture);
    // No values at all → manifest defaults (amount 1, color (1, .5, .25), multiply, offset 0).
    let inst = instance("com.example.tint/tint", vec![]);
    let out = run(&gpu, &fx, &inst, 1.0, &input, input.rect, F32);
    let want = Pixels::new(input.rect, |x, y| {
        let c = src.get(x, y);
        [c[0], c[1] * 0.5, c[2] * 0.25, c[3]]
    });
    assert_close(&Pixels::download(&gpu, &out), &want, 1e-6);
    // Values outside the valid range are clamped (amount 5 → 1).
    let inst = instance("com.example.tint/tint", vec![(1, Value::Float(5.0))]);
    let out = run(&gpu, &fx, &inst, 1.0, &input, input.rect, F32);
    assert_close(&Pixels::download(&gpu, &out), &want, 1e-6);
    assert_eq!(fx.compiled_formats(), 1);
}

fn example(dir: &str) -> Arc<DeclarativeEffect> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/examples")
        .join(dir);
    let pkg = Package::load(&DirSource::new(root)).unwrap_or_else(|d| panic!("{d:#?}"));
    DeclarativeEffect::from_package(&pkg).unwrap().remove(0)
}

/// CPU box blur with transparent edges: horizontal over `x ± n`, then vertical.
fn box_blur(src: &Pixels, n: i32, out: IRect) -> Pixels {
    let k = (2 * n + 1) as f32;
    let tmp_rect = IRect::new(out.x0, out.y0 - n, out.x1, out.y1 + n);
    let tmp = Pixels::new(tmp_rect, |x, y| {
        let mut s = [0.0f32; 4];
        for i in -n..=n {
            let c = src.get(x + i, y);
            (0..4).for_each(|j| s[j] += c[j]);
        }
        s.map(|v| v / k)
    });
    Pixels::new(out, |x, y| {
        let mut s = [0.0f32; 4];
        for i in -n..=n {
            let c = tmp.get(x, y + i);
            (0..4).for_each(|j| s[j] += c[j]);
        }
        s.map(|v| v / k)
    })
}

#[test]
fn box_blur_example_matches_cpu_and_negotiates_roi() {
    let Some(gpu) = context() else { return };
    let fx = example("box-blur");
    let layer = IRect::from_size(24, 16);
    let src = Pixels::new(layer, |x, y| {
        let v = if (x / 4 + y / 4) % 2 == 0 { 1.0 } else { 0.2 };
        [v, v * 0.5, 0.0, 1.0]
    });
    let input = src.upload(&gpu, F32);
    let inst = instance(
        "com.example.box-blur/box-blur",
        vec![(1, Value::Float(2.6))],
    );
    let params = EffectParams {
        effect: &inst,
        time: Time::ZERO,
        scale: 1.0,
    };
    // radius 2.6 → read margin ceil(2.6) = 3 on every side, result grows by 3.
    assert_eq!(fx.output_bounds(&params, layer), IRect::new(-3, -3, 27, 19));
    let roi = IRect::new(-2, 1, 20, 14);
    assert_eq!(fx.input_roi(&params, roi), IRect::new(-5, -2, 23, 17));
    let out = run(&gpu, &fx, &inst, 1.0, &input, roi, F32);
    assert_eq!(out.rect, roi);
    // The shader uses round(radius) = 3 taps per side.
    assert_close(&Pixels::download(&gpu, &out), &box_blur(&src, 3, roi), 1e-5);

    // Half resolution: radius 2.6 full-res → round(1.3) = 1 tap per side.
    let out = run(&gpu, &fx, &inst, 0.5, &input, roi, F32);
    assert_close(&Pixels::download(&gpu, &out), &box_blur(&src, 1, roi), 1e-5);

    // radius < 0.5 → identity.
    let inst = instance(
        "com.example.box-blur/box-blur",
        vec![(1, Value::Float(0.2))],
    );
    let out = run(&gpu, &fx, &inst, 1.0, &input, roi, F32);
    assert_eq!(out.texture, input.texture);
}

#[test]
fn vignette_example_renders() {
    let Some(gpu) = context() else { return };
    let fx = example("vignette");
    let layer = IRect::from_size(64, 32);
    let src = Pixels::new(layer, |_, _| [0.8, 0.8, 0.8, 1.0]);
    for format in [F16, F32] {
        let input = src.upload(&gpu, format);
        // Default params (amount 50 %, relative center = layer center).
        let inst = instance("com.example.vignette/vignette", vec![]);
        let out = Pixels::download(&gpu, &run(&gpu, &fx, &inst, 1.0, &input, layer, format));
        let center = out.get(32, 16);
        let corner = out.get(0, 0);
        assert!((center[0] - 0.8).abs() < 2e-3, "center {center:?}");
        assert!(corner[0] < 0.7, "corner {corner:?}");
        assert!((corner[3] - 1.0).abs() < 1e-3, "alpha kept {corner:?}");
        // Symmetric around the center.
        let (a, b) = (out.get(4, 4), out.get(59, 27));
        assert!((a[0] - b[0]).abs() < 2e-3, "{a:?} vs {b:?}");
    }
}
