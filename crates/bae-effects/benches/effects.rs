//! Effect render times on the GPU.
//!
//! `cargo bench -p bae-effects` (or `cargo xtask bench -p bae-effects`).
//! Optional arguments: a substring filter on the case name, `--json <path>`.
//!
//! For each case the effect is encoded and submitted `ITER` times back to back
//! (one submission per frame, textures reused from the target's pool) and the
//! wall time until the GPU is idle is divided by `ITER`: the steady-state
//! frame cost on the GPU, CPU encoding included. `latency` is one frame
//! encoded, submitted and waited for.

use std::sync::Arc;
use std::time::Instant;

use bae_effects::Standalone;
use bae_gpu::wgpu;
use bae_gpu::{GpuContext, GpuContextDescriptor};
use bae_model::{PrimPath, Value};
use bae_render::{EffectParams, GpuImage, IRect, ImageAlpha, RenderEffect};

const ITER: u32 = 60;

struct Case {
    name: String,
    match_name: &'static str,
    params: Vec<(u32, Value)>,
    size: [u32; 2],
}

fn input(gpu: &GpuContext, size: [u32; 2]) -> GpuImage {
    let [w, h] = size;
    let mut bytes = Vec::with_capacity((w * h * 8) as usize);
    for y in 0..h {
        for x in 0..w {
            let on = ((x / 64) + (y / 64)) % 2 == 0;
            let c: [f32; 4] = if on {
                [0.9, 0.5, 0.2, 1.0]
            } else {
                [0.1, 0.2, 0.4, 0.6]
            };
            for v in c {
                bytes.extend_from_slice(&half::f16::from_f32(v).to_le_bytes());
            }
        }
    }
    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("bench input"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: bae_effects::IMAGE_USAGE,
        view_formats: &[],
    });
    bae_gpu::transfer::upload_texture(
        gpu,
        &texture,
        bae_gpu::transfer::TextureRegion::full(&texture),
        &bytes,
    )
    .unwrap();
    Standalone::image(texture, IRect::from_size(w, h), ImageAlpha::Premultiplied)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json_path = args
        .iter()
        .position(|a| a == "--json")
        .and_then(|i| args.get(i + 1).cloned());
    let filter = args
        .iter()
        .find(|a| !a.starts_with("--") && Some(*a) != json_path.as_ref())
        .cloned();
    let steps = args.iter().any(|a| a == "--steps");

    let gpu = match GpuContext::new_headless(GpuContextDescriptor::default().with_env()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("no GPU: {e}");
            return;
        }
    };
    let t = Instant::now();
    bae_effects::prewarm(&gpu, wgpu::TextureFormat::Rgba16Float).unwrap();
    println!("adapter: {}", gpu.summary());
    println!(
        "prewarm (all pipelines, RGBA16F): {:.1} ms",
        t.elapsed().as_secs_f64() * 1e3
    );

    const UHD: [u32; 2] = [3840, 2160];
    const HD: [u32; 2] = [1920, 1080];
    let mut cases = Vec::new();
    for size in [UHD, HD] {
        let tag = if size == UHD { "4K" } else { "1080p" };
        for b in [10.0, 25.0, 100.0, 300.0] {
            cases.push(Case {
                name: format!("{tag} gaussian blurriness {b}"),
                match_name: "ADBE Gaussian Blur 2",
                params: vec![(1, Value::Float(b)), (3, Value::Bool(false))],
                size,
            });
        }
        for (r, n) in [(5.0, 1), (20.0, 3), (100.0, 3)] {
            cases.push(Case {
                name: format!("{tag} fast box blur r{r} x{n}"),
                match_name: "ADBE Box Blur2",
                params: vec![(1, Value::Float(r)), (2, Value::Int(n))],
                size,
            });
        }
        for (name, params) in [
            ("ADBE Tint", vec![(3, Value::Float(80.0))]),
            ("ADBE Tritone", vec![(4, Value::Float(20.0))]),
            ("ADBE Easy Levels2", vec![(5, Value::Float(1.4))]),
            ("ADBE Pro Levels2", vec![(6, Value::Float(1.4))]),
            (
                "ADBE HUE SATURATION",
                vec![(4, Value::Float(30.0)), (5, Value::Float(20.0))],
            ),
            (
                "ADBE Brightness & Contrast 2",
                vec![(1, Value::Float(30.0)), (2, Value::Float(20.0))],
            ),
            ("ADBE Exposure2", vec![(3, Value::Float(0.5))]),
        ] {
            cases.push(Case {
                name: format!("{tag} {name}"),
                match_name: name,
                params,
                size,
            });
        }
    }

    for (effect, params) in [
        ("ADBE Fractal Noise", vec![]),
        ("VISINF Grain Implant", vec![(21, Value::Int(3))]),
        ("ADBE Motion Blur", vec![(2, Value::Float(100.0))]),
        ("ADBE Radial Blur", vec![(1, Value::Float(25.0))]),
        (
            "ADBE Unsharp Mask2",
            vec![(1, Value::Float(100.0)), (2, Value::Float(50.0))],
        ),
        ("ADBE Glo2", vec![(3, Value::Float(100.0))]),
        ("ADBE Drop Shadow", vec![(5, Value::Float(100.0))]),
        ("ADBE Mosaic", vec![]),
    ] {
        cases.push(Case {
            name: format!("1080p {effect}"),
            match_name: effect,
            params,
            size: HD,
        });
    }

    for radius in [20.0, 100.0, 1000.0] {
        cases.push(Case {
            name: format!("1080p minimax r{radius}"),
            match_name: "ADBE Minimax",
            params: vec![(2, Value::Float(radius)), (3, Value::Int(2))],
            size: HD,
        });
    }
    for choke in [10.5, -100.0] {
        cases.push(Case {
            name: format!("1080p simple choker {choke}"),
            match_name: "ADBE Simple Choker",
            params: vec![(2, Value::Float(choke))],
            size: HD,
        });
    }

    let mut results = Vec::new();
    println!("{:<44} {:>10} {:>10}", "case", "ms/frame", "latency");
    for case in &cases {
        if filter
            .as_ref()
            .is_some_and(|f| !case.name.contains(f.as_str()))
        {
            continue;
        }
        let (per_frame, latency) = measure(&gpu, case);
        println!("{:<44} {:>10.3} {:>10.3}", case.name, per_frame, latency);
        if steps {
            profile_steps(&gpu, case);
        }
        results.push(format!(
            "{{\"case\": \"{}\", \"ms_per_frame\": {per_frame:.4}, \"latency_ms\": {latency:.4}}}",
            case.name
        ));
    }
    if let Some(path) = json_path {
        let body = format!(
            "{{\"adapter\": \"{}\", \"iterations\": {ITER}, \"results\": [\n  {}\n]}}\n",
            gpu.summary().replace('"', "'"),
            results.join(",\n  ")
        );
        std::fs::write(&path, body).unwrap();
        println!("wrote {path}");
    }
}

fn measure(gpu: &Arc<GpuContext>, case: &Case) -> (f64, f64) {
    let fx = bae_effects::get(case.match_name).unwrap();
    let inst = RenderEffect {
        path: PrimPath::new("/comps/bench/layers/a/effects/fx").unwrap(),
        effect: case.match_name.to_owned(),
        enabled: true,
        params: case.params.clone(),
    };
    let params = EffectParams {
        scale: 1.0,
        ..EffectParams::new(&inst)
    };
    let image = input(gpu, case.size);
    let effect = fx.effect();
    let roi = effect.output_bounds(&params, image.rect);
    let mut target = Standalone::new(gpu.clone(), wgpu::TextureFormat::Rgba16Float);
    // Warm up (pipelines, pool).
    for _ in 0..3 {
        let _ = fx.render(&mut target, &params, &image, roi).unwrap();
        target.finish().unwrap();
    }
    let t = Instant::now();
    let _ = fx.render(&mut target, &params, &image, roi).unwrap();
    target.finish().unwrap();
    let latency = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    for _ in 0..ITER {
        let _ = fx.render(&mut target, &params, &image, roi).unwrap();
        target.submit();
    }
    target.finish().unwrap();
    let per_frame = t.elapsed().as_secs_f64() * 1e3 / f64::from(ITER);
    (per_frame, latency)
}

/// Prints the GPU time of every dispatch of one frame of `case`.
fn profile_steps(gpu: &Arc<GpuContext>, case: &Case) {
    let Some(timer) = bae_gpu::timer::GpuTimer::new(gpu, 64) else {
        println!("    (no timestamp queries)");
        return;
    };
    bae_effects::profile::install(timer);
    let fx = bae_effects::get(case.match_name).unwrap();
    let inst = RenderEffect {
        path: PrimPath::new("/comps/bench/layers/a/effects/fx").unwrap(),
        effect: case.match_name.to_owned(),
        enabled: true,
        params: case.params.clone(),
    };
    let params = EffectParams::new(&inst);
    let image = input(gpu, case.size);
    let roi = fx.effect().output_bounds(&params, image.rect);
    let mut target = Standalone::new(gpu.clone(), wgpu::TextureFormat::Rgba16Float);
    // Warm the GPU up (clocks) without the timer, then time one frame.
    let timer = bae_effects::profile::take().unwrap();
    for _ in 0..30 {
        let _ = fx.render(&mut target, &params, &image, roi).unwrap();
        target.submit();
    }
    bae_effects::profile::install(timer);
    let _ = fx.render(&mut target, &params, &image, roi).unwrap();
    let timer = bae_effects::profile::take().unwrap();
    timer.resolve(target.encoder_mut());
    target.finish().unwrap();
    for sample in timer.read(gpu).unwrap() {
        println!("    {:<36} {:>8.3} ms", sample.label, sample.millis());
    }
}
