//! Shape layer preview benchmark (docs/perf.md): a 1080p comp with N shape
//! layers (a rectangle or a star with a fill), each spinning through the
//! expression `time*1000`, played through the viewer's GPU preview path:
//! `bae_anim::Evaluator` (expressions) → `AnimEvaluator::convert` →
//! `Renderer::render_frame_output` (frame + 8-bit conversion, one submit).
//!
//! Prints the time per frame of each stage and the sustained frame rate with
//! the GPU included. `cargo bench -p bae-vector --bench shapes [-- <layers>…]`
//! (default 100 and 1000). `BAE_BENCH_FRAMES` (default 120), `BAE_BENCH_4K=1`
//! (3840 × 2160 instead of 1920 × 1080), `BAE_BENCH_FX=<n>` (the first n
//! layers also get Gaussian Blur 20 and Glow with radius 30).

use std::sync::Arc;
use std::time::{Duration, Instant};

use bae_anim::Evaluator;
use bae_gpu::{GpuContext, GpuContextDescriptor};
use bae_model::{AuthorId, CommandRegistry, FrameRate, PrimPath, Stage, Time, Value};
use bae_render::fixture::SceneBuilder;
use bae_render::{
    AnimEvaluator, OutputAlpha, RenderPurpose, RenderRequest, Renderer, RendererConfig,
};
use bae_vector::render::ShapeProvider;
use serde_json::json;

/// The comp size (`BAE_BENCH_4K`).
fn size() -> (u32, u32) {
    if std::env::var_os("BAE_BENCH_4K").is_some() {
        (3840, 2160)
    } else {
        (1920, 1080)
    }
}

fn run(stage: &mut Stage, registry: &CommandRegistry, id: &str, params: serde_json::Value) {
    registry
        .execute(stage, &AuthorId::system(), id, params)
        .unwrap_or_else(|e| panic!("{id}: {e}"));
}

fn set(stage: &mut Stage, registry: &CommandRegistry, prim: &str, attr: &str, value: Value) {
    let value = serde_json::to_value(&value).unwrap();
    run(
        stage,
        registry,
        "attr.set",
        json!({"path": prim, "attr": attr, "value": value}),
    );
}

/// Layers with blur and glow (`BAE_BENCH_FX`).
fn fx_layers() -> usize {
    std::env::var("BAE_BENCH_FX")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// The comp and its N spinning shape layers.
fn project(layers: usize) -> (Stage, PrimPath) {
    let (w, h) = size();
    let mut b = SceneBuilder::new();
    let comp = b
        .comp(
            "main",
            w,
            h,
            FrameRate::FPS_30,
            Time::from_flicks(10 * Time::FLICKS_PER_SECOND),
        )
        .unwrap();
    let paths: Vec<PrimPath> = (0..layers)
        .map(|i| b.layer(&comp, &format!("shape{i}"), "shape", None).unwrap())
        .collect();
    let mut stage = b.into_stage();
    let registry = CommandRegistry::with_builtins();
    let cols = (layers as f64).sqrt().ceil() as usize;
    for (i, layer) in paths.iter().enumerate() {
        let l = layer.as_str();
        let (x, y) = (i % cols, i / cols);
        let pos = [
            (x as f64 + 0.5) * f64::from(w) / cols as f64,
            (y as f64 + 0.5) * f64::from(h) / cols as f64,
            0.0,
        ];
        set(
            &mut stage,
            &registry,
            l,
            "transform:position",
            Value::Vec3(pos),
        );
        let create = |stage: &mut Stage, path: String, ty: &str| {
            run(
                stage,
                &registry,
                "prim.create",
                json!({"path": path, "prim_type": ty}),
            );
            path
        };
        create(&mut stage, format!("{l}/contents"), "Folder");
        create(&mut stage, format!("{l}/contents/group"), "ShapeGroup");
        if i % 2 == 0 {
            let rect = create(&mut stage, format!("{l}/contents/group/rect"), "Rect");
            set(
                &mut stage,
                &registry,
                &rect,
                "size",
                Value::Vec2([80.0, 50.0]),
            );
        } else {
            create(&mut stage, format!("{l}/contents/group/star"), "Star");
        }
        let fill = create(&mut stage, format!("{l}/contents/group/fill"), "Fill");
        let c = i as f64 / layers as f64;
        set(
            &mut stage,
            &registry,
            &fill,
            "color",
            Value::Color([c, 1.0 - c, 0.5, 1.0]),
        );
        run(
            &mut stage,
            &registry,
            "attr.set_expression",
            json!({"path": l, "attr": "transform:rotation", "source": "time*1000"}),
        );
        if i < fx_layers() {
            create(&mut stage, format!("{l}/effects"), "Folder");
            for (name, match_name, id, value) in [
                ("blur", "ADBE Gaussian Blur 2", 1, 20.0),
                ("glow", "ADBE Glo2", 3, 30.0),
            ] {
                let fx = create(&mut stage, format!("{l}/effects/{name}"), "Effect");
                set(
                    &mut stage,
                    &registry,
                    &fx,
                    "match_name",
                    Value::String(match_name.into()),
                );
                set(
                    &mut stage,
                    &registry,
                    &fx,
                    &format!("param:{id:04}"),
                    Value::Float(value),
                );
            }
        }
    }
    (stage, comp)
}

#[derive(Default)]
struct Times {
    eval: Duration,
    convert: Duration,
    render: Duration,
    build: Duration,
    encode: Duration,
    wait: Duration,
}

fn ms(d: Duration, frames: usize) -> f64 {
    d.as_secs_f64() * 1000.0 / frames as f64
}

fn bench(gpu: &Arc<GpuContext>, layers: usize, frames: usize) {
    let (stage, comp) = project(layers);
    let mut eval = Evaluator::new();
    let mut anim = AnimEvaluator::new(None).with_purpose(RenderPurpose::Preview);
    let mut renderer = Renderer::with_config(
        gpu.clone(),
        RendererConfig {
            frame_cache_bytes: 512 << 20,
            pool_idle_bytes: 128 << 20,
        },
    );
    renderer
        .providers_mut()
        .register(Arc::new(ShapeProvider::new()));
    bae_effects::register(
        &mut bae_model::EffectRegistry::new(),
        renderer.effects_mut(),
    );
    let request = RenderRequest {
        purpose: RenderPurpose::Preview,
        ..RenderRequest::full()
    };
    // `BAE_BENCH_LAYER_TIMES=1`: the Render Time column's per-layer timing on.
    let layer_times = std::env::var_os("BAE_BENCH_LAYER_TIMES").is_some();
    renderer.set_layer_times(layer_times);
    {
        let ec = eval
            .evaluate_comp(&stage, &comp, Time::ZERO)
            .expect("evaluate");
        let rc = anim.convert(&stage, &ec, 0).expect("convert");
        let out = renderer.render_frame(&rc, &request).expect("render");
        println!(
            "  {layers:5} layers: {} nodes, {} draw calls, {} passes, warnings {:?}",
            out.stats.nodes, out.stats.draw_calls, out.stats.passes, out.warnings
        );
    }
    let rate = FrameRate::FPS_30;
    let mut frame = |f: usize, times: &mut Times, wait: bool| {
        let time = rate.frame_to_time(f as i64);
        let t = Instant::now();
        let ec = eval.evaluate_comp(&stage, &comp, time).expect("evaluate");
        times.eval += t.elapsed();
        let t = Instant::now();
        let rc = anim.convert(&stage, &ec, 0).expect("convert");
        times.convert += t.elapsed();
        let t = Instant::now();
        let mut frames = bae_render::eval::EvaluatorFrames {
            stage: &stage,
            evaluator: &mut anim,
        };
        let (out, _texture) = renderer
            .render_frame_output(&rc, &request, Some(&mut frames), OutputAlpha::Premultiplied)
            .expect("render");
        times.render += t.elapsed();
        times.build += out.stats.build;
        times.encode += out.stats.encode;
        if wait {
            let t = Instant::now();
            renderer.wait_idle().expect("idle");
            times.wait += t.elapsed();
        }
    };
    // Warm-up (pipelines, expression contexts, caches).
    for f in 0..5 {
        frame(f, &mut Times::default(), true);
    }
    let mut times = Times::default();
    for f in 0..frames {
        frame(5 + f, &mut times, true);
    }
    // Sustained: no wait between frames (as the viewer plays).
    let start = Instant::now();
    for f in 0..frames {
        frame(5 + frames + f, &mut Times::default(), false);
    }
    renderer.wait_idle().expect("idle");
    let fps = frames as f64 / start.elapsed().as_secs_f64();
    if layer_times {
        let ec = eval
            .evaluate_comp(&stage, &comp, rate.frame_to_time(1))
            .expect("evaluate");
        let rc = anim.convert(&stage, &ec, 0).expect("convert");
        let out = renderer.render_frame(&rc, &request).expect("render");
        let total = |t: &[(PrimPath, Duration)]| t.iter().map(|(_, d)| *d).sum::<Duration>();
        println!(
            "  layer times: {} layers, CPU {:?}; GPU {} layers, {:?}",
            out.layer_times.len(),
            total(&out.layer_times),
            out.layer_gpu_times.len(),
            total(&out.layer_gpu_times),
        );
    }
    println!(
        "  {layers:5} layers: eval {:6.2} ms  convert {:6.2} ms  render+submit {:6.2} ms (graph {:5.2}, encode {:5.2})  gpu wait {:6.2} ms  |  sustained {fps:6.1} fps",
        ms(times.eval, frames),
        ms(times.convert, frames),
        ms(times.render, frames),
        ms(times.build, frames),
        ms(times.encode, frames),
        ms(times.wait, frames),
    );
}

fn main() {
    let frames: usize = std::env::var("BAE_BENCH_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let counts: Vec<usize> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let counts = if counts.is_empty() {
        vec![100, 1000]
    } else {
        counts
    };
    let gpu = match GpuContext::new_headless(GpuContextDescriptor {
        validation: false,
        panic_on_uncaptured_error: true,
        ..GpuContextDescriptor::default()
    }) {
        Ok(gpu) => gpu,
        Err(e) => {
            eprintln!("shapes: no GPU ({e}); skipped");
            return;
        }
    };
    println!(
        "shapes — {}×{}, spinning shape layers (time*1000), {} with blur + glow — {}",
        size().0,
        size().1,
        fx_layers(),
        gpu.summary()
    );
    for layers in counts {
        bench(&gpu, layers, frames);
    }
}
