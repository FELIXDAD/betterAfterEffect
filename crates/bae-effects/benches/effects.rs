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

mod cases;

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

    let cases = cases::all();
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
    effect_inputs(&mut target, &*effect, &params, &image);
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
    let effect = fx.effect();
    let roi = effect.output_bounds(&params, image.rect);
    let mut target = Standalone::new(gpu.clone(), wgpu::TextureFormat::Rgba16Float);
    effect_inputs(&mut target, &*effect, &params, &image);
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

fn effect_inputs(
    target: &mut Standalone,
    effect: &dyn bae_render::Effect,
    params: &EffectParams<'_>,
    image: &GpuImage,
) {
    let layers = effect
        .layer_dependencies(params)
        .into_iter()
        .map(|dep| (dep, image.clone()))
        .collect();
    target.set_effect_inputs(layers, Vec::new());
}
