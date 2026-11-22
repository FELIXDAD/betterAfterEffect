//! Headless GPU tests. They skip (pass with a note) when the machine has no usable
//! adapter, e.g. CI runners without a GPU; set `BAE_REQUIRE_GPU=1` to make a
//! missing adapter a failure instead.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bae_gpu::transfer::{self, TextureReadback, TextureRegion};
use bae_gpu::wgpu;
use bae_gpu::{GpuContext, GpuContextDescriptor, GpuError, GpuReport, timer::GpuTimer};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Tests share the process-wide context (`bae_gpu::testing`, #154); a test that
/// destroys a device takes [`exclusive_context`] and runs alone.
static SHARED: RwLock<()> = RwLock::new(());

/// The shared context, held for the test's duration.
struct Gpu {
    ctx: Arc<GpuContext>,
    _shared: RwLockReadGuard<'static, ()>,
}

impl std::ops::Deref for Gpu {
    type Target = Arc<GpuContext>;
    fn deref(&self) -> &Arc<GpuContext> {
        &self.ctx
    }
}

fn context() -> Option<Gpu> {
    let shared = SHARED.read();
    bae_gpu::testing::shared_context().map(|ctx| Gpu {
        ctx,
        _shared: shared,
    })
}

/// A context of its own, while no other test of this binary uses the GPU.
fn exclusive_context() -> Option<(Arc<GpuContext>, RwLockWriteGuard<'static, ()>)> {
    let exclusive = SHARED.write();
    match GpuContext::new_headless(GpuContextDescriptor::default().with_env()) {
        Ok(ctx) => Some((ctx, exclusive)),
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

fn texture(
    ctx: &GpuContext,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    ctx.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("test"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

#[test]
fn headless_device_and_report() {
    let Some(ctx) = context() else { return };
    assert!(!ctx.is_lost());
    let info = ctx.adapter_info();
    assert!(!info.name.is_empty());
    assert!(ctx.limits().max_texture_dimension_2d >= 8192);
    let report = GpuReport::new(&ctx);
    let text = report.to_string();
    assert!(text.contains(&info.name), "{text}");
    let json: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();
    assert_eq!(json["selected"]["name"], info.name.as_str());
    assert!(json["limits"]["maxTextureDimension2D"].as_u64().unwrap() >= 8192);
    assert!(!ctx.summary().is_empty());
}

#[test]
fn clear_and_readback_with_row_padding() {
    let Some(ctx) = context() else { return };
    // 37 px * 4 B = 148 B per row: padded to 256 in the staging buffer.
    let (width, height) = (37, 19);
    let target = texture(
        &ctx,
        width,
        height,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let view = target.create_view(&Default::default());
    let mut encoder = ctx.device().create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 1.0,
                        g: 0.5,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    ctx.queue().submit([encoder.finish()]);
    let image = transfer::read_texture(&ctx, &target).unwrap();
    assert_eq!((image.width, image.height), (width, height));
    assert_eq!(image.bytes_per_row, width * 4);
    assert_eq!(image.data.len(), (width * height * 4) as usize);
    for (x, y) in [
        (0, 0),
        (width - 1, 0),
        (0, height - 1),
        (width - 1, height - 1),
        (18, 9),
    ] {
        let px = image.pixel(x, y);
        assert_eq!(px[0], 255, "red at {x},{y}");
        assert!((127..=128).contains(&px[1]), "green {} at {x},{y}", px[1]);
        assert_eq!(px[2], 0, "blue at {x},{y}");
        assert_eq!(px[3], 255, "alpha at {x},{y}");
    }
    assert_eq!(ctx.uncaptured_error_count(), 0);
}

#[test]
fn upload_readback_round_trip() {
    let Some(ctx) = context() else { return };
    for (format, bpp) in [
        (wgpu::TextureFormat::Rgba8Unorm, 4u32),
        (wgpu::TextureFormat::Rgba16Float, 8),
        (wgpu::TextureFormat::Rgba32Float, 16),
    ] {
        let (width, height) = (61, 23);
        let data: Vec<u8> = (0..width * height * bpp)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        // Avoid NaN payload canonicalisation in float formats: clear exponent bits
        // that would produce NaN/Inf by keeping values small.
        let data: Vec<u8> = match bpp {
            4 => data,
            8 => data
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|c| [c[0], c[1] & 0x3b])
                .collect(),
            _ => data
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|c| [c[0], c[1], c[2], c[3] & 0x3f])
                .collect(),
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("upload"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = transfer::create_texture_with_data(&ctx, &desc, &data).unwrap();
        let image = transfer::read_texture(&ctx, &texture).unwrap();
        assert_eq!(image.format, format);
        assert_eq!(image.data, data, "{format:?} round trip");

        // Partial region readback through an explicit encoder.
        let mut encoder = ctx.device().create_command_encoder(&Default::default());
        let region = TextureRegion {
            mip_level: 0,
            origin: wgpu::Origin3d { x: 5, y: 7, z: 0 },
            size: wgpu::Extent3d {
                width: 9,
                height: 4,
                depth_or_array_layers: 1,
            },
        };
        let readback = TextureReadback::record(&ctx, &mut encoder, &texture, region).unwrap();
        ctx.queue().submit([encoder.finish()]);
        let pending = readback.map();
        let part = pending.wait(&ctx).unwrap();
        for y in 0..4 {
            let src = ((y + 7) * width * bpp + 5 * bpp) as usize;
            assert_eq!(part.row(y), &data[src..src + (9 * bpp) as usize]);
        }
    }
    // Size mismatch is an error, not a panic.
    let tex = texture(
        &ctx,
        4,
        4,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::COPY_DST,
    );
    assert!(transfer::upload_texture(&ctx, &tex, TextureRegion::full(&tex), &[0; 3]).is_err());
    assert_eq!(ctx.uncaptured_error_count(), 0);
}

#[test]
fn async_readback_completes_by_polling() {
    let Some(ctx) = context() else { return };
    let tex = texture(
        &ctx,
        16,
        16,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
    );
    transfer::upload_texture(&ctx, &tex, TextureRegion::full(&tex), &[42; 16 * 16 * 4]).unwrap();
    let pending = TextureReadback::start(&ctx, &tex).unwrap();
    let mut spins = 0;
    let image = loop {
        ctx.poll();
        if let Some(result) = pending.try_finish() {
            break result.unwrap();
        }
        spins += 1;
        assert!(spins < 10_000, "readback never completed");
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert!(image.data.iter().all(|&b| b == 42));
}

#[test]
fn timer_smoke() {
    let Some(ctx) = context() else { return };
    let Some(mut timer) = GpuTimer::new(&ctx, 4) else {
        eprintln!("skipping: no timestamp queries on {}", ctx.summary());
        return;
    };
    let target = texture(
        &ctx,
        256,
        256,
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let view = target.create_view(&Default::default());
    let mut encoder = ctx.device().create_command_encoder(&Default::default());
    let scope = timer.scope("clear").unwrap();
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("timed clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            timestamp_writes: Some(timer.render_pass_writes(scope)),
            ..Default::default()
        });
    }
    let encoder_scope = timer.scope("encoder").unwrap();
    if timer.write_begin(&mut encoder, encoder_scope) {
        timer.write_end(&mut encoder, encoder_scope);
    }
    timer.resolve(&mut encoder);
    ctx.queue().submit([encoder.finish()]);
    let samples = timer.read(&ctx).unwrap();
    assert_eq!(samples.len(), 2);
    assert_eq!(&*samples[0].label, "clear");
    assert!(samples[0].millis() < 1000.0);
    timer.reset();
    assert!(timer.read(&ctx).unwrap().is_empty());
}

#[test]
fn device_lost_callback_fires_on_destroy() {
    let Some((ctx, _exclusive)) = exclusive_context() else {
        return;
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let removed = ctx.on_device_lost(|_| unreachable!("removed callbacks must not run"));
    assert!(ctx.remove_device_lost_callback(removed));
    {
        let calls = calls.clone();
        ctx.on_device_lost(move |info| {
            assert!(info.destroyed);
            calls.fetch_add(1, Ordering::SeqCst);
        });
    }
    ctx.device().destroy();
    // The callback runs during destroy or the next poll.
    let _ = ctx.device().poll(wgpu::PollType::Poll);
    assert!(ctx.is_lost());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(ctx.lost_info().unwrap().destroyed);
    // Recovery: a fresh device from the same descriptor.
    let fresh = ctx.recreate().unwrap();
    assert!(!fresh.is_lost());
    assert_eq!(fresh.adapter_info().name, ctx.adapter_info().name);
}

#[test]
fn native_handles_are_exposed() {
    let Some(ctx) = context() else { return };
    let tex = texture(
        &ctx,
        8,
        8,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING,
    );
    // SAFETY: the handles are only compared against null, never dereferenced.
    let (device, native_tex) = unsafe { (ctx.native_device(), ctx.native_texture(&tex)) };
    match ctx.native_api() {
        bae_gpu::NativeApi::Metal | bae_gpu::NativeApi::Direct3D12 => {
            let device = device.expect("native device");
            assert!(!device.device.is_null());
            assert!(!device.queue.is_null());
            assert!(!native_tex.expect("native texture").texture.is_null());
        }
        _ => assert!(device.is_none()),
    }
}

#[test]
fn viewport_registry_publishes_views() {
    let Some(ctx) = context() else { return };
    let registry = bae_gpu::ViewportTextureRegistry::new();
    let id = registry.create("viewer");
    let tex = texture(
        &ctx,
        320,
        180,
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let before = registry.generation();
    registry
        .publish(id, &tex, wgpu::FilterMode::Linear)
        .unwrap();
    assert!(registry.generation() > before);
    let published = registry.get(id).unwrap();
    assert_eq!(published.size, [320, 180]);
    assert_eq!(published.format, wgpu::TextureFormat::Rgba16Float);
    // Not sampleable → rejected.
    let not_sampleable = texture(
        &ctx,
        4,
        4,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    assert!(
        registry
            .publish(id, &not_sampleable, wgpu::FilterMode::Linear)
            .is_err()
    );
}
