//! Mask benchmark (docs/perf.md §4): 1080p, 50 layers with feathered masks whose
//! paths animate every frame (so every mask is rasterised and feathered again in
//! every frame – the worst case for the mask passes).
//!
//! `cargo bench -p bae-render --bench masks`. Environment: `BAE_BENCH_FRAMES`
//! (default 120), `BAE_BENCH_OUT` (JSON report directory, default `target/bench`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bae_gpu::{GpuContext, GpuContextDescriptor};
use bae_model::{PrimPath, Time};
use bae_render::fixture::{Lcg, ellipse_path, rect_path};
use bae_render::{
    LayerContent, MaskMode, RenderComp, RenderLayer, RenderMask, RenderRequest, Renderer,
    RendererConfig, Resolution,
};
use glam::{DAffine2, DVec2};

const W: u32 = 1920;
const H: u32 = 1080;
const LAYERS: usize = 50;

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

/// Layer `i` at frame `f`: a full-frame solid with a feathered ellipse mask that
/// drifts and breathes; every 5th layer adds a subtracted rectangle (two masks,
/// the accumulator path), every 7th is inverted (draws the whole frame), every
/// 3rd has an expansion and different x / y feathers.
fn layer(i: usize, f: usize, seed: &[f64; 6]) -> RenderLayer {
    let t = f as f64 / 30.0;
    let path = PrimPath::new(&format!("/comps/masks/layers/l{i}")).unwrap();
    let mut l = RenderLayer::new(
        path.clone(),
        LayerContent::Solid {
            color: [seed[0] as f32, seed[1] as f32, seed[2] as f32, 1.0],
            width: W,
            height: H,
        },
    );
    l.opacity = 0.7;
    l.transform =
        DAffine2::from_translation(DVec2::new((t * 40.0 * (seed[3] - 0.5)).sin() * 30.0, 0.0));
    let cx = 200.0 + seed[3] * 1500.0 + (t + seed[4] * 6.0).sin() * 120.0;
    let cy = 150.0 + seed[4] * 780.0 + (t * 1.3 + seed[5] * 6.0).cos() * 80.0;
    let r = 120.0 + seed[5] * 260.0 + (t * 2.0).sin() * 30.0;
    let feather = 20.0 + seed[0] as f32 * 100.0;
    let mut m = RenderMask {
        path: PrimPath::new(&format!("{path}/masks/m1")).unwrap(),
        shape: ellipse_path(cx - r, cy - r * 0.7, cx + r, cy + r * 0.7),
        mode: MaskMode::Add,
        inverted: i.is_multiple_of(7),
        opacity: 0.9,
        feather: if i.is_multiple_of(3) {
            [feather, feather * 0.25]
        } else {
            [feather, feather]
        },
        expansion: if i.is_multiple_of(3) { 8.0 } else { 0.0 },
        feather_points: Vec::new(),
    };
    if m.inverted {
        m.opacity = 0.5;
    }
    l.masks.push(m);
    if i.is_multiple_of(5) {
        l.masks.push(RenderMask {
            path: PrimPath::new(&format!("{path}/masks/m2")).unwrap(),
            shape: rect_path(cx - r * 0.3, cy - r * 0.3, cx + r * 0.3, cy + r * 0.3),
            mode: MaskMode::Subtract,
            inverted: false,
            opacity: 1.0,
            feather: [10.0, 10.0],
            expansion: 0.0,
            feather_points: Vec::new(),
        });
    }
    l
}

fn main() {
    let frames: usize = std::env::var("BAE_BENCH_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let gpu = match GpuContext::new_headless(GpuContextDescriptor {
        validation: false,
        panic_on_uncaptured_error: true,
        ..GpuContextDescriptor::default()
    }) {
        Ok(gpu) => gpu,
        Err(e) => {
            eprintln!("masks: no GPU ({e}); skipped");
            return;
        }
    };
    let mut rng = Lcg::new(4);
    let seeds: Vec<[f64; 6]> = (0..LAYERS)
        .map(|_| std::array::from_fn(|_| rng.next_f64()))
        .collect();
    let comp_at = |f: usize| {
        let mut c = RenderComp::new(PrimPath::new("/comps/masks").unwrap(), W, H);
        c.time = Time::from_seconds(f as f64 / 30.0);
        c.background = [0.0, 0.0, 0.0, 1.0];
        c.layers = (0..LAYERS).map(|i| layer(i, f, &seeds[i])).collect();
        c
    };
    let comps: Vec<RenderComp> = (0..frames).map(comp_at).collect();
    // The same layers without masks, for the masks' share of the frame time.
    let unmasked: Vec<RenderComp> = comps
        .iter()
        .map(|c| {
            let mut c = c.clone();
            for l in &mut c.layers {
                l.masks.clear();
            }
            c
        })
        .collect();

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
    for c in comps.iter().take(3) {
        renderer.render_frame(c, &full).expect("render");
        renderer.render_frame(c, &half).expect("render");
    }
    renderer.wait_idle().expect("idle");
    let first = renderer.render_frame(&comps[0], &full).expect("render");

    let throughput = |renderer: &mut Renderer, comps: &[RenderComp], req: &RenderRequest| {
        let t = Instant::now();
        for c in comps {
            renderer.render_frame(c, req).expect("render");
        }
        renderer.wait_idle().expect("idle");
        comps.len() as f64 / t.elapsed().as_secs_f64()
    };
    let fps_full = throughput(&mut renderer, &comps, &full);
    let fps_half = throughput(&mut renderer, &comps, &half);
    let fps_unmasked = throughput(&mut renderer, &unmasked, &full);
    let mut build: Vec<f64> = Vec::with_capacity(frames);
    let mut encode: Vec<f64> = Vec::with_capacity(frames);
    let mut latency: Vec<f64> = comps
        .iter()
        .map(|c| {
            let t = Instant::now();
            let frame = renderer.render_frame(c, &full).expect("render");
            build.push(ms(frame.stats.build));
            encode.push(ms(frame.stats.encode));
            renderer.wait_idle().expect("idle");
            ms(t.elapsed())
        })
        .collect();
    latency.sort_by(f64::total_cmp);
    build.sort_by(f64::total_cmp);
    encode.sort_by(f64::total_cmp);

    let masks: usize = comps[0].layers.iter().map(|l| l.masks.len()).sum();
    let report = serde_json::json!({
        "scenario": "masks",
        "description": "1080p: 50 full-frame solids, feathered (20-120 px) ellipse masks animated every frame; 1/5 with a second (subtract) mask, 1/7 inverted, 1/3 expanded with x != y feather",
        "gpu": gpu.summary(),
        "frames": frames,
        "layers": LAYERS,
        "masks": masks,
        "nodes_per_frame": first.stats.nodes,
        "draw_calls_per_frame": first.stats.draw_calls,
        "passes_per_frame": first.stats.passes,
        "graph_build_ms_p50": percentile(&build, 0.5),
        "graph_build_ms_p95": percentile(&build, 0.95),
        "encode_ms_p50": percentile(&encode, 0.5),
        "preview_fps_full": fps_full,
        "preview_fps_half": fps_half,
        "preview_fps_full_without_masks": fps_unmasked,
        "latency_ms_p50": percentile(&latency, 0.5),
        "latency_ms_p95": percentile(&latency, 0.95),
    });
    println!("masks — {}", gpu.summary());
    println!(
        "  {frames} frames, {LAYERS} layers, {masks} masks, {} nodes → {} draw calls, {} passes per frame",
        first.stats.nodes, first.stats.draw_calls, first.stats.passes
    );
    println!(
        "  graph build              p50 {:7.3} ms   p95 {:7.3} ms",
        percentile(&build, 0.5),
        percentile(&build, 0.95)
    );
    println!(
        "  encode (CPU)             p50 {:7.3} ms",
        percentile(&encode, 0.5)
    );
    println!("  preview full res         {fps_full:8.1} fps");
    println!("  preview half res         {fps_half:8.1} fps");
    println!("  same layers, no masks    {fps_unmasked:8.1} fps");
    println!(
        "  frame latency            p50 {:7.3} ms   p95 {:7.3} ms",
        percentile(&latency, 0.5),
        percentile(&latency, 0.95)
    );
    let out = std::env::var_os("BAE_BENCH_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("../bench"));
    if std::fs::create_dir_all(&out).is_ok() {
        let _ = std::fs::write(
            out.join("masks.json"),
            serde_json::to_string_pretty(&report).unwrap_or_default(),
        );
    }
}
