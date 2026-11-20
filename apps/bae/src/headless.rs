//! `bae --headless`: no window. Creates the shared GPU context, then runs the
//! requested batch work. `--smoke-test N` renders N frames of the UI offscreen
//! through the same egui renderer and viewport bridge the window uses.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use bae_app::App;
use bae_gpu::viewport::ViewportTextureRegistry;
use bae_gpu::{GpuContext, GpuContextDescriptor, transfer, wgpu};
use bae_platform::GlobalOptions;
use bae_platform::cli::EXIT_NOT_IMPLEMENTED;
use bae_platform::crash;

use crate::ui::{self, StatusInfo, UiOutput, UiRenderer, Workspace};

/// Offscreen size of the headless smoke test.
const SMOKE_SIZE: [u32; 2] = [1280, 720];

/// Runs headless mode.
pub fn run(app: &mut App, global: &GlobalOptions) -> ExitCode {
    let desc = app
        .resource::<GpuContextDescriptor>()
        .cloned()
        .unwrap_or_default();
    let gpu = match GpuContext::new_headless(desc) {
        Ok(gpu) => gpu,
        Err(error) => {
            tracing::error!("{error}");
            eprintln!("{}: {error}", bae_platform::BINARY_NAME);
            return ExitCode::FAILURE;
        }
    };
    crash::set_context("gpu", gpu.summary());
    app.insert_resource(gpu.clone());

    if let Some(frames) = global.smoke_test {
        return smoke_test(app, &gpu, frames);
    }
    if !global.open.is_empty() {
        eprintln!(
            "{} --headless: opening projects is not implemented yet",
            bae_platform::BINARY_NAME
        );
        return ExitCode::from(EXIT_NOT_IMPLEMENTED);
    }
    tracing::info!("headless mode ready on {}; nothing to do", gpu.summary());
    ExitCode::SUCCESS
}

fn smoke_test(app: &App, gpu: &Arc<GpuContext>, frames: u32) -> ExitCode {
    match render_offscreen(app, gpu, frames) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            tracing::error!("smoke test failed: {error:#}");
            eprintln!("smoke test failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn render_offscreen(app: &App, gpu: &Arc<GpuContext>, frames: u32) -> anyhow::Result<String> {
    let [width, height] = SMOKE_SIZE;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("bae.smoke-test"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let registry = app.resource::<Arc<ViewportTextureRegistry>>().cloned();
    let viewer = registry
        .as_deref()
        .and_then(|registry| ui::publish_placeholder_comp(gpu, registry));
    let egui_ctx = egui::Context::default();
    let mut renderer = UiRenderer::new(gpu, format);
    let mut workspace = Workspace::new(viewer);
    let mut status = StatusInfo {
        gpu: gpu.summary(),
        size_px: SMOKE_SIZE,
        pixels_per_point: 1.0,
        ..StatusInfo::default()
    };

    let start = Instant::now();
    for frame in 0..frames {
        let frame_start = Instant::now();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width as f32, height as f32),
            )),
            time: Some(start.elapsed().as_secs_f64()),
            max_texture_side: Some(gpu.limits().max_texture_dimension_2d as usize),
            ..Default::default()
        };
        let output = egui_ctx.run_ui(input, |ui| {
            workspace.ui(ui, &status, registry.as_deref());
        });
        if let Some(registry) = registry.as_deref() {
            renderer.sync_viewports(gpu, registry);
        }
        let background = egui_ctx.global_style().visuals.panel_fill;
        renderer.render(
            gpu,
            &egui_ctx,
            UiOutput {
                shapes: output.shapes,
                textures_delta: output.textures_delta,
                pixels_per_point: output.pixels_per_point,
            },
            &view,
            SMOKE_SIZE,
            background,
        );
        gpu.poll();
        status.frames = u64::from(frame) + 1;
        let ms = frame_start.elapsed().as_secs_f32() * 1000.0;
        status.frame_ms = if frame == 0 {
            ms
        } else {
            status.frame_ms * 0.9 + ms * 0.1
        };
    }
    gpu.wait_idle()?;
    let elapsed = start.elapsed();

    let image = transfer::read_texture(gpu, &target)?;
    let first = image.pixel(0, 0).to_vec();
    let varied = (0..height)
        .step_by(7)
        .flat_map(|y| (0..width).step_by(13).map(move |x| (x, y)))
        .any(|(x, y)| image.pixel(x, y) != first.as_slice());
    anyhow::ensure!(varied, "the rendered UI is a uniform color");
    anyhow::ensure!(
        gpu.uncaptured_error_count() == 0,
        "{} wgpu validation errors",
        gpu.uncaptured_error_count()
    );
    anyhow::ensure!(!gpu.is_lost(), "the GPU device was lost");
    Ok(format!(
        "smoke test passed: {frames} frames {width}×{height} offscreen in {:.1} ms ({:.2} ms/frame) on {}",
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_secs_f64() * 1000.0 / f64::from(frames),
        gpu.summary()
    ))
}
