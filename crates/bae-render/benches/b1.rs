//! Benchmark B1 "motion graphics" (ARCHITECTURE §11; method in docs/perf.md).
//!
//! `cargo xtask bench -p bae-render` (or `cargo bench -p bae-render --bench b1`).
//! Environment: `BAE_BENCH_FRAMES` (default 300 = the whole 10 s comp),
//! `BAE_BENCH_OUT` (JSON report directory, default `target/bench`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bae_gpu::{GpuContext, GpuContextDescriptor};
use bae_render::fixture;
use bae_render::{
    AnimEvaluator, CompEvaluator, OutputAlpha, RenderComp, RenderRequest, Renderer, RendererConfig,
    Resolution,
};

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn main() {
    let frames: usize = std::env::var("BAE_BENCH_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let gpu = match GpuContext::new_headless(GpuContextDescriptor {
        // Benchmarks measure release behaviour: no backend validation.
        validation: false,
        panic_on_uncaptured_error: true,
        ..GpuContextDescriptor::default()
    }) {
        Ok(gpu) => gpu,
        Err(e) => {
            eprintln!("b1: no GPU ({e}); skipped");
            return;
        }
    };

    let assets = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("b1-assets");
    let t = Instant::now();
    let (builder, comp) = fixture::b1(&assets).expect("build B1");
    let stage = builder.into_stage();
    let build_scene = t.elapsed();

    // Evaluation (03's evaluator through the adapter), all frames.
    let mut eval = AnimEvaluator::new(None);
    let rate = bae_model::FrameRate::FPS_30;
    let mut comps: Vec<RenderComp> = Vec::with_capacity(frames);
    let mut eval_times = Vec::with_capacity(frames);
    for f in 0..frames {
        let t = Instant::now();
        comps.push(
            eval.evaluate(&stage, &comp, rate.frame_to_time(f as i64))
                .expect("evaluate"),
        );
        eval_times.push(ms(t.elapsed()));
    }

    let mut renderer = Renderer::with_config(
        gpu.clone(),
        RendererConfig {
            frame_cache_bytes: 0,
            ..RendererConfig::default()
        },
    );
    let full = RenderRequest::full();
    let half = RenderRequest {
        resolution: Resolution::Half,
        ..full
    };
    // Warm-up: pipelines, footage decode, pool.
    for c in comps.iter().take(3) {
        renderer.render_frame(c, &full).expect("render");
        renderer.render_frame(c, &half).expect("render");
    }
    renderer.wait_idle().expect("idle");
    let first = renderer.render_frame(&comps[0], &full).expect("render");

    // Preview throughput: frames back to back, GPU drained at the end.
    let throughput = |renderer: &mut Renderer, req: &RenderRequest| {
        let t = Instant::now();
        for c in &comps {
            renderer.render_frame(c, req).expect("render");
        }
        renderer.wait_idle().expect("idle");
        frames as f64 / t.elapsed().as_secs_f64()
    };
    let preview_fps = throughput(&mut renderer, &full);
    let preview_half_fps = throughput(&mut renderer, &half);

    // Latency: each frame rendered and finished on the GPU before the next.
    let mut latency: Vec<f64> = comps
        .iter()
        .map(|c| {
            let t = Instant::now();
            renderer.render_frame(c, &full).expect("render");
            renderer.wait_idle().expect("idle");
            ms(t.elapsed())
        })
        .collect();
    latency.sort_by(f64::total_cmp);

    // Final-render path: evaluate + render + 8-bit readback (no file encoding).
    let t = Instant::now();
    for f in 0..frames {
        let c = eval
            .evaluate(&stage, &comp, rate.frame_to_time(f as i64))
            .expect("evaluate");
        let frame = renderer.render_frame(&c, &full).expect("render");
        renderer
            .read_rgba8(&frame, OutputAlpha::Straight)
            .expect("readback");
    }
    let export_fps = frames as f64 / t.elapsed().as_secs_f64();

    eval_times.sort_by(f64::total_cmp);
    let eval_mean = eval_times.iter().sum::<f64>() / eval_times.len().max(1) as f64;
    let report = serde_json::json!({
        "scenario": "B1",
        "description": "1080p30 10 s: 50 animated solids (shape stand-ins, 1/5 Add/Screen/Multiply/Overlay) + 10 animated RGBA textures (text stand-ins) + background",
        "gpu": gpu.summary(),
        "frames": frames,
        "layers": comps[0].layers.len(),
        "draws_per_frame": first.stats.draws,
        "draw_calls_per_frame": first.stats.draw_calls,
        "passes_per_frame": first.stats.passes,
        "scene_build_ms": ms(build_scene),
        "eval_ms_mean": eval_mean,
        "eval_ms_p95": percentile(&eval_times, 0.95),
        "graph_build_ms": ms(first.stats.build),
        "encode_ms": ms(first.stats.encode),
        "preview_fps_full": preview_fps,
        "preview_fps_half": preview_half_fps,
        "latency_ms_p50": percentile(&latency, 0.5),
        "latency_ms_p95": percentile(&latency, 0.95),
        "export_fps_full": export_fps,
    });

    println!("B1 motion graphics — {}", gpu.summary());
    println!(
        "  {} frames, {} layers, {} draws → {} draw calls, {} passes per frame",
        frames,
        comps[0].layers.len(),
        first.stats.draws,
        first.stats.draw_calls,
        first.stats.passes
    );
    println!(
        "  evaluation (bae-anim)    mean {eval_mean:7.3} ms   p95 {:7.3} ms",
        percentile(&eval_times, 0.95)
    );
    println!(
        "  graph build / encode     {:7.3} ms / {:7.3} ms",
        ms(first.stats.build),
        ms(first.stats.encode)
    );
    println!("  preview full res         {preview_fps:8.1} fps");
    println!("  preview half res         {preview_half_fps:8.1} fps");
    println!(
        "  frame latency            p50 {:7.3} ms   p95 {:7.3} ms",
        percentile(&latency, 0.5),
        percentile(&latency, 0.95)
    );
    println!("  export (eval+render+readback) {export_fps:6.1} fps");

    let out_dir = std::env::var_os("BAE_BENCH_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("../bench"));
    if std::fs::create_dir_all(&out_dir).is_ok() {
        let path = out_dir.join("b1.json");
        if std::fs::write(
            &path,
            serde_json::to_string_pretty(&report).unwrap_or_default(),
        )
        .is_ok()
        {
            println!("  report: {}", path.display());
        }
    }
}
