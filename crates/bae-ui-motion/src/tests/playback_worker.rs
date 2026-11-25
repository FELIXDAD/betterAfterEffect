use super::playback_4k::scene;
use crate::playback::PlaybackPreview;
use bae_gpu::{GpuContext, GpuContextDescriptor, viewport::ViewportTextureRegistry};
use bae_model::{FrameRate, Time, TimeRange};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn playhead_has_priority_after_preview_stops() {
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    let mut h = super::Harness::new();
    let (path, _, _) = h.comp_with_layers();
    let registry = Arc::new(ViewportTextureRegistry::new());
    let mut worker = PlaybackPreview::new(gpu, registry.clone());
    let rate = FrameRate::FPS_60;
    let range = TimeRange::new(Time::ZERO, rate.frame_to_time(60));
    let get = |worker: &mut PlaybackPreview, frame| {
        let time = rate.frame_to_time(frame);
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            for e in registry.snapshot() {
                if e.texture.is_some() {
                    registry.set_display_handle(e.id, Some(e.id.raw()));
                }
            }
            if worker
                .poll(
                    &h.stage,
                    &h.resources,
                    &path,
                    time,
                    rate,
                    range,
                    false,
                    1,
                    &h.ctx,
                )
                .is_some_and(|(_, actual, _)| actual == time)
            {
                return;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    get(&mut worker, 0);
    worker.stop();
    get(&mut worker, 29);
}

/// Consume consecutive worker frames as fast as possible. Shell registration is
/// emulated here. No window or monitor refresh rate enters this measurement.
#[test]
#[ignore = "4K hardware playback benchmark"]
fn playback_worker_4k() {
    let n: i64 = std::env::var("BAE_BENCH_FRAMES").map_or(24, |v| v.parse().unwrap());
    assert!((1..=240).contains(&n));
    let gpu = GpuContext::new_headless(GpuContextDescriptor {
        validation: false,
        allow_software_fallback: false,
        panic_on_uncaptured_error: true,
        ..GpuContextDescriptor::default().with_env()
    })
    .unwrap();
    let rate = FrameRate::FPS_60;
    for (layers, effects) in [(100, false), (1000, false), (100, true)] {
        let (h, path) = scene(layers, effects);
        let registry = Arc::new(ViewportTextureRegistry::new());
        let mut worker = PlaybackPreview::new(gpu.clone(), registry.clone());
        let range = TimeRange::new(Time::ZERO, rate.frame_to_time(n + 3));
        let mut wall = Instant::now();
        let mut ui_times = Vec::new();
        for frame in 0..n + 3 {
            let time = rate.frame_to_time(frame);
            let deadline = Instant::now() + Duration::from_secs(120);
            if frame == 3 {
                wall = Instant::now();
            }
            loop {
                for entry in registry.snapshot() {
                    if entry.texture.is_some() && entry.display_handle.is_none() {
                        registry.set_display_handle(entry.id, Some(entry.id.raw()));
                    }
                }
                let start = Instant::now();
                let ready = worker
                    .poll(
                        &h.stage,
                        &h.resources,
                        &path,
                        time,
                        rate,
                        range,
                        false,
                        1,
                        &h.ctx,
                    )
                    .is_some_and(|(_, actual, errors)| errors.is_empty() && actual == time);
                if frame >= 3 {
                    ui_times.push(start.elapsed());
                }
                if ready {
                    break;
                }
                assert!(Instant::now() < deadline, "frame {frame} never arrived");
                std::thread::sleep(Duration::from_micros(200));
            }
        }
        let fps = n as f64 / wall.elapsed().as_secs_f64();
        ui_times.sort_unstable();
        println!(
            "worker layers={layers} effects={effects} frames={n} ui_p95_ms={:.4} fps={fps:.2}",
            ui_times[ui_times.len() * 95 / 100].as_secs_f64() * 1000.
        );
    }
}
