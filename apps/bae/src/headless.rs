//! `bae --headless`: no window. Creates the shared GPU context, then runs the
//! requested batch work. `--smoke-test N` renders N frames of the UI offscreen
//! through the same egui renderer and viewport bridge the window uses.
//! `bae --headless <project>` opens the project (`.bae` folder or `.baez` file)
//! and prints a one-line summary; with `--smoke-test N` the UI is rendered with
//! that project open. A project that cannot be opened exits with status 1.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use bae_app::App;
use bae_gpu::viewport::ViewportTextureRegistry;
use bae_gpu::{GpuContext, GpuContextDescriptor, transfer, wgpu};
use bae_platform::GlobalOptions;
use bae_platform::crash;

use crate::document::Document;
use crate::ui::{self, PlaceholderViewer, StatusInfo, UiOutput, UiRenderer, Workspace};

/// Offscreen size of the headless smoke test.
const SMOKE_SIZE: [u32; 2] = [1280, 720];
/// When set, the headless smoke test writes its last frame to this path (BMP), for
/// screenshots in PRs and CI artifacts.
const SCREENSHOT_ENV: &str = "BAE_SMOKE_SCREENSHOT";

/// Runs headless mode.
pub fn run(app: &mut App, global: &GlobalOptions) -> ExitCode {
    crate::i18n::init_from_env();
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

    let project = global.open.first().map(std::path::PathBuf::as_path);
    if global.open.len() > 1 {
        tracing::warn!("only the first project is opened");
    }
    if let Some(frames) = global.smoke_test {
        return smoke_test(app, &gpu, frames, project);
    }
    if let Some(path) = project {
        let mut document = Document::new(None);
        return match document.open(path) {
            Ok(()) => {
                let stage = document.stage();
                println!(
                    "opened {}: {} sublayer(s), {} prim(s)",
                    path.display(),
                    stage.layer_stack().len(),
                    stage.sublayers().map(|l| l.len()).sum::<usize>()
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{}: {error:#}", bae_platform::BINARY_NAME);
                ExitCode::FAILURE
            }
        };
    }
    tracing::info!("headless mode ready on {}; nothing to do", gpu.summary());
    ExitCode::SUCCESS
}

fn smoke_test(
    app: &mut App,
    gpu: &Arc<GpuContext>,
    frames: u32,
    project: Option<&std::path::Path>,
) -> ExitCode {
    match render_offscreen(app, gpu, frames, project) {
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

fn render_offscreen(
    app: &mut App,
    gpu: &Arc<GpuContext>,
    frames: u32,
    project: Option<&std::path::Path>,
) -> anyhow::Result<String> {
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
    if let Some(viewer) = viewer {
        app.insert_resource(PlaceholderViewer(viewer));
    }
    let egui_ctx = egui::Context::default();
    bae_ui::theme::apply(&egui_ctx);
    let mut renderer = UiRenderer::new(gpu, format);
    // Always the default layout: smoke tests never read or write user state.
    let mut workspace = Workspace::new(app, bae_ui::WorkbenchState::default());
    let workspaces = workspace.workspaces();
    let mut document = Document::new(app.resource::<Arc<bae_ui::history::HistoryLog>>().cloned());
    document.add_module_commands(app);
    if let Some(path) = project {
        document.open(path)?;
    }
    // `BAE_SMOKE_DEMO=1`: a demo project open in the Motion workspace (screenshots).
    let demo = std::env::var_os("BAE_SMOKE_DEMO").is_some();
    if demo {
        for inv in bae_ui_motion::demo::steps() {
            if let crate::document::Outcome::Failed(e) = document.run(&inv) {
                anyhow::bail!("demo step `{}` failed: {e}", inv.id);
            }
        }
        bae_ui_motion::demo::open(app.resources(), document.stage());
    }
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
        let mut invocations = Vec::new();
        let resources = app.resources();
        let output = egui_ctx.run_ui(input, |ui| {
            invocations = workspace.ui(ui, &status, &document, resources);
        });
        // Cycle through every workspace so each preset's panels get drawn.
        if !demo && frame % 10 == 9 && !workspaces.is_empty() {
            let next = &workspaces[(frame as usize / 10 + 1) % workspaces.len()].0;
            invocations.push(bae_ui::Invocation::with_params(
                bae_ui::action::ids::SWITCH_WORKSPACE,
                serde_json::json!({ "id": next }),
            ));
        }
        for inv in invocations {
            if let Some(rest) = workspace.handle(inv, &document, resources) {
                tracing::debug!("headless: unhandled `{}`", rest.id);
            }
        }
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
    if let Some(path) = std::env::var_os(SCREENSHOT_ENV) {
        write_bmp(std::path::Path::new(&path), &image)?;
    }
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

/// Writes an RGBA8 image as an uncompressed 32-bit BMP (no image codec needed).
fn write_bmp(path: &std::path::Path, image: &transfer::ImageData) -> anyhow::Result<()> {
    let (w, h) = (image.width, image.height);
    let size = 54 + w * h * 4;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(b"BM");
    for v in [size, 0, 54, 40] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes()); // top-down rows
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    for v in [0u32, w * h * 4, 2835, 2835, 0, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for y in 0..h {
        for x in 0..w {
            let p = image.pixel(x, y);
            out.extend_from_slice(&[p[2], p[1], p[0], 255]);
        }
    }
    std::fs::write(path, out)?;
    Ok(())
}
