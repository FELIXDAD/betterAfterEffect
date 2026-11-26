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
    let get = |worker: &mut PlaybackPreview, frame, range, looping| {
        let time = rate.frame_to_time(frame);
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            for e in registry.snapshot() {
                if e.texture.is_some() {
                    registry.set_display_handle(e.id, Some(e.id.raw()));
                }
            }
            if let Some((_, actual, _)) = worker.poll(
                &h.stage,
                &h.resources,
                &path,
                time,
                rate,
                range,
                looping,
                1,
                &h.ctx,
            ) {
                assert!(
                    actual <= time && range.contains(actual),
                    "stale frame {actual:?} at {time:?}"
                );
                if actual == time {
                    return;
                }
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    get(&mut worker, 0, range, false);
    worker.stop();
    get(&mut worker, 29, range, false);
    get(&mut worker, 7, range, false);
    let short = TimeRange::new(rate.frame_to_time(5), rate.frame_to_time(8));
    get(&mut worker, 7, short, true);
    get(&mut worker, 5, short, true);
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
        measure_cadence(&h, &path, &gpu, &registry, &mut worker);
    }
}

/// Sample the viewer at composition deadlines. A new frame counts when its
/// published texture is copied to the offscreen display target. Repeated
/// draws of a held frame do not increase the numerator.
fn measure_cadence(
    h: &super::Harness,
    path: &bae_model::PrimPath,
    gpu: &GpuContext,
    registry: &ViewportTextureRegistry,
    worker: &mut PlaybackPreview,
) {
    use bae_gpu::wgpu;
    let seconds: u32 = std::env::var("BAE_BENCH_PLAY_SECONDS").map_or(30, |v| v.parse().unwrap());
    assert!((1..=120).contains(&seconds));
    let rate = FrameRate::FPS_60;
    let info = bae_motion::query::comp_info(&h.stage, path).unwrap();
    let end = rate.time_to_frame(info.work_area.end);
    let target = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("paced 4K viewer output"),
        size: wgpu::Extent3d {
            width: 3840,
            height: 2160,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let start = Instant::now();
    let mut samples = Vec::new();
    let mut last = None;
    let mut fresh = 0;
    let mut next = 0u64;
    let warm = 180;
    while next < warm + u64::from(seconds) * 60 {
        let at = start + Duration::from_nanos((next * 1_000_000_000).div_ceil(60));
        std::thread::sleep(at.saturating_duration_since(Instant::now()));
        let index = (start.elapsed().as_nanos() * 60 / 1_000_000_000) as u64;
        if index >= warm + u64::from(seconds) * 60 {
            break;
        }
        let time = rate.frame_to_time(index as i64 % end);
        let entries = registry.snapshot();
        for e in &entries {
            if e.texture.is_some() && e.display_handle.is_none() {
                registry.set_display_handle(e.id, Some(e.id.raw()));
            }
        }
        let poll = Instant::now();
        let selected = worker.poll(
            &h.stage,
            &h.resources,
            path,
            time,
            rate,
            info.work_area,
            true,
            1,
            &h.ctx,
        );
        if index >= warm {
            samples.push(poll.elapsed());
        }
        if let Some((image, actual, _)) = selected
            && let Some(tex) = entries
                .iter()
                .find(|e| e.id.raw() == image.handle)
                .and_then(|e| e.texture.as_ref())
        {
            let mut encoder = gpu.device().create_command_encoder(&Default::default());
            encoder.copy_texture_to_texture(
                tex.view.texture().as_image_copy(),
                target.as_image_copy(),
                target.size(),
            );
            gpu.queue().submit([encoder.finish()]);
            if last != Some(actual) && index >= warm {
                fresh += 1;
            }
            last = Some(actual);
        }
        next = index + 1;
    }
    gpu.wait_idle().unwrap();
    samples.sort_unstable();
    let elapsed = (start.elapsed() - Duration::from_secs(3)).as_secs_f64();
    println!(
        "paced 4K elapsed_s={elapsed:.3} samples={} fresh={fresh} fresh_fps={:.3} ui_p95_ms={:.4}",
        samples.len(),
        f64::from(fresh) / elapsed,
        samples[samples.len() * 95 / 100].as_secs_f64() * 1000.
    );
}
