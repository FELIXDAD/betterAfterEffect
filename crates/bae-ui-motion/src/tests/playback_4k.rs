//! Run explicitly in release mode on a hardware adapter (see docs/perf.md).

use super::Harness;
use crate::gpu_preview::{GpuPreview, PreviewEffects, PreviewSources};
use bae_anim::Evaluator;
use bae_app::AppBuilder;
use bae_gpu::{
    GpuContext, GpuContextDescriptor, timer::GpuTimer, viewport::ViewportTextureRegistry,
};
use bae_model::{FrameRate, PrimPath};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;

pub(super) fn scene(count: usize, effects: bool) -> (Harness, PrimPath) {
    let mut h = Harness::new();
    let mut modules = AppBuilder::new();
    modules.add_module(bae_effects::EffectsModule);
    modules
        .register_arc::<dyn bae_render::SourceProvider>(
            bae_vector::SHAPE_PROVIDER,
            Arc::new(bae_vector::render::ShapeProvider::new()),
        )
        .unwrap();
    let mut app = modules.build();
    bae_motion::effect::register_effect_commands(&mut h.reg, app.registry());
    h.resources.insert(PreviewEffects(app.registry()));
    h.resources.insert(PreviewSources(app.registry()));
    let comp = h.run(
        "comp.create",
        json!({"name":"Playback","width":3840,"height":2160,"fps":60}),
    );
    let comp = PrimPath::new(comp["path"].as_str().unwrap()).unwrap();
    let cols = if count == 100 { 10 } else { 40 };
    let rows = count.div_ceil(cols);
    for i in 0..count {
        let position = [
            (i % cols * 3840 + 1920) / cols,
            (i / cols * 2160 + 1080) / rows,
        ];
        let layer = h.run(
            "layer.create",
            json!({"comp":comp,"name":format!("R{i:04}"),
            "kind":"shape","shape":"rect","size":[72,36],"position":position}),
        );
        let path = &layer["path"];
        h.run(
            "attr.set_expression",
            json!({"path":path,"attr":"transform:rotation","source":"time*1000"}),
        );
        if effects && i % 10 == 0 {
            for name in ["ADBE Gaussian Blur 2", "ADBE Glo2"] {
                h.run("effect.apply", json!({"layers":[path],"match_name":name}));
            }
        }
    }
    (h, comp)
}

// WDDM accumulates scheduled GPU-engine execution in 100 ns ticks. Query
// outside the measured loop, with the queue idle. The timestamp-query span
// also includes gaps between the many Vello submissions in the baseline.
#[cfg(windows)]
fn engine_ms() -> Option<f64> {
    use std::os::windows::process::CommandExt;
    let script = format!(
        "(Get-CimInstance Win32_PerfRawData_GPUPerformanceCounters_GPUEngine -Filter \"Name LIKE 'pid_{}_%'\" | Measure-Object RunningTime -Sum).Sum",
        std::process::id()
    );
    let out = std::process::Command::new("powershell.exe")
        .creation_flags(0x08000000)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    out.status.success().then_some(())?;
    Some(
        std::str::from_utf8(&out.stdout)
            .ok()?
            .trim()
            .parse::<f64>()
            .ok()?
            / 10_000.0,
    )
}

#[cfg(not(windows))]
fn engine_ms() -> Option<f64> {
    None
}

#[test]
#[ignore = "4K hardware playback benchmark"]
fn playback_4k() {
    let frames: u32 = std::env::var("BAE_BENCH_FRAMES").map_or(24, |v| v.parse().unwrap());
    assert!((1..=240).contains(&frames));
    let gpu = GpuContext::new_headless(GpuContextDescriptor {
        validation: false,
        allow_software_fallback: false,
        panic_on_uncaptured_error: true,
        ..GpuContextDescriptor::default().with_env()
    })
    .unwrap();
    let info = gpu.adapter_info();
    println!(
        "adapter={} backend={:?} driver={} {}",
        info.name, info.backend, info.driver, info.driver_info
    );
    let mut timer = GpuTimer::new(&gpu, frames).expect("GPU timestamps required");
    assert!(timer.supports_encoder_timestamps());
    for (layers, effects) in [(100, false), (1000, false), (100, true)] {
        let (h, path) = scene(layers, effects);
        let registry = Arc::new(ViewportTextureRegistry::new());
        let mut preview = GpuPreview::with(gpu.clone(), registry.clone());
        registry.set_display_handle(preview.viewport(), Some(1));
        let mut eval = Evaluator::new();
        let mut cpu_ms = 0.0;
        let mut eval_ms = 0.0;
        let mut render = |frame: u32, timed: bool, timer: &mut GpuTimer| {
            let start = Instant::now();
            let time = FrameRate::FPS_60.frame_to_time(i64::from(frame));
            let comp = eval.evaluate_comp(&h.stage, &path, time).unwrap();
            let evaluation = start.elapsed();
            assert_eq!(comp.layers.len(), layers);
            assert!(comp.errors.is_empty());
            let scope = timed.then(|| {
                let scope = timer.scope("preview queue").unwrap();
                let mut encoder = gpu.device().create_command_encoder(&Default::default());
                timer.write_begin(&mut encoder, scope);
                gpu.queue().submit([encoder.finish()]);
                scope
            });
            let encode = Instant::now();
            preview
                .frame(&h.resources, &h.stage, &comp, 1, None)
                .expect("published preview");
            if let Some(scope) = scope {
                cpu_ms += (evaluation + encode.elapsed()).as_secs_f64() * 1000.0;
                eval_ms += evaluation.as_secs_f64() * 1000.0;
                let mut encoder = gpu.device().create_command_encoder(&Default::default());
                timer.write_end(&mut encoder, scope);
                gpu.queue().submit([encoder.finish()]);
            }
        };
        for f in 0..3 {
            render(f, false, &mut timer);
        }
        gpu.wait_idle().unwrap();
        let engine_start = engine_ms();
        let wall = Instant::now();
        for f in 3..3 + frames {
            render(f, false, &mut timer);
        }
        gpu.wait_idle().unwrap();
        let fps = f64::from(frames) / wall.elapsed().as_secs_f64();
        let busy = engine_start
            .zip(engine_ms())
            .map(|(a, b)| (b - a) / f64::from(frames));
        timer.reset();
        for f in 3 + frames..3 + frames * 2 {
            render(f, true, &mut timer);
        }
        let mut encoder = gpu.device().create_command_encoder(&Default::default());
        timer.resolve(&mut encoder);
        gpu.queue().submit([encoder.finish()]);
        let n = f64::from(frames);
        let queue_ms = timer
            .read(&gpu)
            .unwrap()
            .iter()
            .map(|s| s.millis())
            .sum::<f64>()
            / n;
        assert_eq!(registry.get(preview.viewport()).unwrap().size, [3840, 2160]);
        println!(
            "layers={layers} effects={effects} frames={frames} cpu_ms={:.3} eval_ms={:.3} gpu_queue_ms={queue_ms:.3} engine_ms={busy:?} fps={fps:.2}",
            cpu_ms / n,
            eval_ms / n
        );
    }
}
