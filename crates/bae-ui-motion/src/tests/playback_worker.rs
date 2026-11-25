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
    let get = |worker: &mut PlaybackPreview, h: &super::Harness, frame| {
        let time = rate.frame_to_time(frame);
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            for e in registry.snapshot() {
                if e.texture.is_some() {
                    registry.set_display_handle(e.id, Some(e.id.raw()));
                }
            }
            if let Some((image, actual, _)) = worker.poll(
                &h.stage,
                &h.resources,
                &path,
                time,
                rate,
                range,
                false,
                1,
                &h.ctx,
            ) && actual == time
            {
                return image.handle;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    let first = get(&mut worker, &h, 0);
    worker.stop();
    get(&mut worker, &h, 29);
    assert_eq!(get(&mut worker, &h, 0), first);
    h.run("comp.create", serde_json::json!({"name":"Unrelated"}));
    assert_eq!(get(&mut worker, &h, 0), first);
    h.run("attr.set",serde_json::json!({"path":path,"attr":"bg_color","value":bae_model::Value::Color([0.1,0.2,0.3,1.0])}));
    assert_ne!(get(&mut worker, &h, 0), first);
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
        let loop_start = n + 3 - n.min(8);
        let cached = TimeRange::new(rate.frame_to_time(loop_start), range.end);
        draw_cached(&gpu, &registry, |i| {
            let time = rate.frame_to_time(loop_start + i % n.min(8));
            let (image, actual, _) = worker
                .poll(
                    &h.stage,
                    &h.resources,
                    &path,
                    time,
                    rate,
                    cached,
                    true,
                    1,
                    &h.ctx,
                )
                .expect("warm frame");
            assert_eq!(actual, time);
            image.handle
        });
    }
}

/// Sample each cached texture with egui's real GPU pipeline and complete a
/// full-resolution draw. Timestamp writes enclose only the textured pass.
fn draw_cached(
    gpu: &GpuContext,
    registry: &ViewportTextureRegistry,
    mut lookup: impl FnMut(i64) -> u64,
) {
    use bae_gpu::{timer::GpuTimer, wgpu};
    let device = gpu.device();
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let display = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("4K playback display"),
        size: wgpu::Extent3d {
            width: 3840,
            height: 2160,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target = display.create_view(&Default::default());
    let mut renderer = egui_wgpu::Renderer::new(device, format, Default::default());
    for e in registry.snapshot() {
        if let Some(texture) = e.texture {
            let egui::TextureId::User(handle) =
                renderer.register_native_texture(device, &texture.view, texture.filter)
            else {
                unreachable!()
            };
            registry.set_display_handle(e.id, Some(handle));
        }
    }
    let generation = registry.generation();
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [3840, 2160],
        pixels_per_point: 1.,
    };
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(3840., 2160.));
    let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.));
    const N: u32 = 240;
    let mut timer = GpuTimer::new(gpu, N).unwrap();
    let mut start = Instant::now();
    for i in 0..=N {
        if i == 1 {
            gpu.wait_idle().unwrap();
            start = Instant::now();
        }
        let mut mesh = egui::Mesh::with_texture(egui::TextureId::User(lookup(i64::from(i))));
        mesh.add_rect_with_uv(rect, uv, egui::Color32::WHITE);
        let jobs = [egui::ClippedPrimitive {
            clip_rect: rect,
            primitive: egui::epaint::Primitive::Mesh(mesh),
        }];
        let mut encoder = device.create_command_encoder(&Default::default());
        let mut commands =
            renderer.update_buffers(device, gpu.queue(), &mut encoder, &jobs, &screen);
        let scope = (i > 0).then(|| timer.scope("cached draw").unwrap());
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    timestamp_writes: scope.map(|s| timer.render_pass_writes(s)),
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, &jobs, &screen);
        }
        if i == N {
            timer.resolve(&mut encoder);
        }
        commands.push(encoder.finish());
        gpu.queue().submit(commands);
    }
    gpu.wait_idle().unwrap();
    let fps = f64::from(N) / start.elapsed().as_secs_f64();
    let ms = timer
        .read(gpu)
        .unwrap()
        .iter()
        .map(|s| s.millis())
        .sum::<f64>()
        / f64::from(N);
    assert_eq!(
        registry.generation(),
        generation,
        "cache hits must reuse registered textures"
    );
    // The benchmark must actually sample the cached composition. Inspect the
    // completed BGRA target once, outside both timing intervals.
    let pixels = bae_gpu::transfer::read_texture(gpu, &display).unwrap();
    let red = pixels
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .any(|p| p[2] > 64 && p[2] > p[0].saturating_add(16) && p[3] != 0);
    assert!(
        red,
        "display target did not contain any of the red shape pixels"
    );
    println!("cached 3840x2160 frames={N} gpu_pass_ms={ms:.4} completed_fps={fps:.2}");
}
