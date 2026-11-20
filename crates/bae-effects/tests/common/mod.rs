//! Shared GPU test harness: images as f32 pixels, upload/readback, running a
//! built-in effect through a `Standalone` target, comparisons.
//!
//! GPU tests skip (with a note) when the machine has no usable adapter; set
//! `BAE_REQUIRE_GPU=1` to make that a failure.

#![allow(dead_code)]

use std::sync::Arc;

use bae_effects::{Builtin, Standalone};
use bae_gpu::transfer;
use bae_gpu::wgpu;
use bae_gpu::{GpuContext, GpuContextDescriptor, GpuError};
use bae_model::{PrimPath, Time, Value};
use bae_render::{EffectParams, GpuImage, IRect, ImageAlpha, RenderEffect};

pub const F32: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
pub const F16: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// One GPU context per test binary: the built-in effects are process-wide
/// statics and cache pipelines (bae-plugin's per format, not per device).
pub fn context() -> Option<Arc<GpuContext>> {
    static CONTEXT: std::sync::OnceLock<Option<Arc<GpuContext>>> = std::sync::OnceLock::new();
    CONTEXT.get_or_init(create_context).clone()
}

fn create_context() -> Option<Arc<GpuContext>> {
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

/// An RGBA image (premultiplied unless stated) as f32 pixels covering `rect`.
#[derive(Clone, Debug)]
pub struct Pixels {
    pub rect: IRect,
    pub data: Vec<[f32; 4]>,
}

impl Pixels {
    pub fn new(rect: IRect, f: impl Fn(i32, i32) -> [f32; 4]) -> Self {
        let mut data = Vec::with_capacity(rect.area() as usize);
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                data.push(f(x, y));
            }
        }
        Self { rect, data }
    }

    /// Transparent outside the rect (AE semantics).
    pub fn get(&self, x: i32, y: i32) -> [f32; 4] {
        let r = self.rect;
        if x < r.x0 || y < r.y0 || x >= r.x1 || y >= r.y1 {
            return [0.0; 4];
        }
        self.data[((y - r.y0) as usize) * r.width() as usize + (x - r.x0) as usize]
    }

    /// Clamped to the rect (Repeat Edge Pixels).
    pub fn get_clamped(&self, x: i32, y: i32) -> [f32; 4] {
        let r = self.rect;
        self.get(x.clamp(r.x0, r.x1 - 1), y.clamp(r.y0, r.y1 - 1))
    }

    /// Applies `f` to every pixel.
    pub fn map(&self, f: impl Fn([f32; 4]) -> [f32; 4]) -> Self {
        Self {
            rect: self.rect,
            data: self.data.iter().map(|c| f(*c)).collect(),
        }
    }

    pub fn upload(&self, gpu: &GpuContext, format: wgpu::TextureFormat) -> GpuImage {
        let [w, h] = self.rect.size();
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("test input"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: bae_effects::IMAGE_USAGE,
            view_formats: &[],
        });
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
            &texture,
            transfer::TextureRegion::full(&texture),
            &bytes,
        )
        .unwrap();
        Standalone::image(texture, self.rect, ImageAlpha::Premultiplied)
    }

    pub fn download(gpu: &GpuContext, image: &GpuImage) -> Self {
        let data = transfer::read_texture(gpu, &image.texture).unwrap();
        let w = image.rect.width() as usize;
        let mut out = Vec::with_capacity(image.rect.area() as usize);
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

/// Largest absolute channel difference over `rect` (pixels outside an image's
/// rect count as transparent) and where it occurs.
pub fn max_diff(got: &Pixels, want: &Pixels, rect: IRect) -> (f32, (i32, i32), usize) {
    let mut worst = (0.0f32, (0, 0), 0);
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let (g, w) = (got.get(x, y), want.get(x, y));
            for k in 0..4 {
                let d = (g[k] - w[k]).abs();
                if d > worst.0 || d.is_nan() {
                    worst = (d, (x, y), k);
                }
            }
        }
    }
    worst
}

pub fn assert_close(got: &Pixels, want: &Pixels, rect: IRect, tol: f32, what: &str) {
    let (d, (x, y), k) = max_diff(got, want, rect);
    assert!(
        d <= tol && !d.is_nan(),
        "{what}: max diff {d} > {tol} at ({x}, {y}) channel {k}: got {:?}, want {:?}",
        got.get(x, y),
        want.get(x, y)
    );
}

pub fn instance(match_name: &str, params: Vec<(u32, Value)>) -> RenderEffect {
    RenderEffect {
        path: PrimPath::new("/comps/main/layers/a/effects/fx").unwrap(),
        effect: match_name.to_owned(),
        enabled: true,
        params,
    }
}

/// Renders `fx` on `input` for `roi` at `scale` and reads the result back.
pub fn run(
    gpu: &Arc<GpuContext>,
    format: wgpu::TextureFormat,
    fx: &Builtin,
    params: Vec<(u32, Value)>,
    input: &Pixels,
    roi: IRect,
    scale: f64,
) -> Pixels {
    let inst = instance(&fx.info.match_name, params);
    let p = EffectParams {
        effect: &inst,
        time: Time::ZERO,
        scale,
    };
    let mut target = Standalone::new(gpu.clone(), format);
    let image = input.upload(gpu, format);
    let out = fx.render(&mut target, &p, &image, roi).unwrap();
    target.finish().unwrap();
    Pixels::download(gpu, &out)
}

pub fn builtin(match_name: &str) -> &'static Builtin {
    bae_effects::get(match_name).unwrap_or_else(|| panic!("no built-in {match_name}"))
}

/// A deterministic test image: colour ramps, a hard-edged shape, partial
/// alpha, and a few HDR values (premultiplied).
pub fn test_image(rect: IRect) -> Pixels {
    Pixels::new(rect, |x, y| {
        let fx = (x - rect.x0) as f32 / rect.width().max(1) as f32;
        let fy = (y - rect.y0) as f32 / rect.height().max(1) as f32;
        let hash =
            (x.wrapping_mul(73856093) ^ y.wrapping_mul(19349663)).rem_euclid(1000) as f32 / 1000.0;
        let r = fx;
        let g = fy;
        let b = hash;
        let mut a = if (x / 7 + y / 5) % 4 == 0 { 0.5 } else { 1.0 };
        if (x + y) % 11 == 0 {
            a = 0.0;
        }
        let hdr = if x % 13 == 0 && y % 9 == 0 { 2.5 } else { 1.0 };
        [r * hdr * a, g * a, b * a, a]
    })
}

pub fn unpremultiply(c: [f32; 4]) -> [f32; 4] {
    if c[3] <= 0.0 {
        [0.0; 4]
    } else {
        [c[0] / c[3], c[1] / c[3], c[2] / c[3], c[3]]
    }
}

pub fn premultiply(c: [f32; 4]) -> [f32; 4] {
    [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
}

pub fn color(c: [f64; 4]) -> Value {
    Value::Color(c)
}
