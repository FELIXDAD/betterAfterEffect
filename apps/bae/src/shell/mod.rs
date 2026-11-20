//! The desktop shell: one winit window, the shared GPU context, egui on top.
//!
//! * The window's [`GpuContext`] is the process-wide one: it is inserted into the
//!   [`App`] as an `Arc<GpuContext>` resource and the egui renderer, the
//!   compositor and plugins all use its device.
//! * Redraws are event driven ([`ControlFlow::Wait`]): a frame is produced only
//!   for input, egui repaint requests (animations, async work) or window
//!   changes, so an idle window uses ~0% CPU.
//! * `--smoke-test N` renders N frames as fast as possible and exits 0; when the
//!   window cannot present (occluded, headless CI desktop) frames are rendered
//!   offscreen so the test still exercises the full UI path.
//!
//! See `docs/spec/app-shell.md`.

mod menu;
mod window_state;

use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use bae_app::{App, AppBuilder, AppModule};
use bae_gpu::viewport::{ViewportTextureId, ViewportTextureRegistry};
use bae_gpu::{
    DeviceLostSubscription, GpuContext, GpuContextDescriptor, GpuError, PresentModePreference,
    SurfaceOptions, WindowSurface, wgpu,
};
use bae_platform::single_instance::SingleInstanceError;
use bae_platform::{AppDirs, GlobalOptions, SettingsFile, SingleInstance, crash};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::ui::{self, StatusInfo, UiOutput, UiRenderer, Workspace};
use window_state::WindowState;

/// The shell's app module (depends on the GPU module).
#[derive(Debug, Default)]
pub struct ShellModule;

impl AppModule for ShellModule {
    fn name(&self) -> &str {
        "bae-shell"
    }

    fn build(&self, app: &mut AppBuilder) {
        app.add_module(bae_gpu::GpuModule);
    }
}

/// Events delivered to the event loop from other threads.
#[derive(Debug)]
enum UserEvent {
    /// egui asked for a repaint after `delay`.
    Repaint { delay: Duration },
    /// A native menu item was chosen.
    Menu(muda::MenuEvent),
    /// The GPU device was lost (not destroyed on purpose).
    DeviceLost,
}

/// A smoke test stops as failed if no frame completes for this long.
const SMOKE_STALL_TIMEOUT: Duration = Duration::from_secs(20);
/// Live-resize mode ends this long after the last `Resized` event.
const LIVE_RESIZE_SETTLE: Duration = Duration::from_millis(250);
/// How often device recovery is attempted before giving up.
const MAX_DEVICE_RECOVERIES: u32 = 3;

/// Runs the GUI until the window closes. Returns the process exit code.
pub fn run(app: App, global: &GlobalOptions) -> ExitCode {
    crate::i18n::init_from_env();
    let dirs = app.resource::<AppDirs>().cloned();

    // One GUI per user, except for smoke tests (CI, or next to a running instance).
    let _instance = match (&dirs, global.smoke_test) {
        (Some(dirs), None) => match SingleInstance::acquire(&dirs.state, "bae-gui") {
            Ok(guard) => Some(guard),
            Err(SingleInstanceError::AlreadyRunning { pid }) => {
                let pid = pid.map(|p| format!(" (pid {p})")).unwrap_or_default();
                eprintln!("{} is already running{pid}", bae_platform::APP_NAME);
                return ExitCode::FAILURE;
            }
            Err(error) => {
                tracing::warn!("single-instance check skipped: {error}");
                None
            }
        },
        _ => None,
    };

    let event_loop = match EventLoop::<UserEvent>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            tracing::error!("cannot create the event loop: {error}");
            eprintln!(
                "{}: cannot open a window: {error}",
                bae_platform::BINARY_NAME
            );
            return ExitCode::FAILURE;
        }
    };
    let proxy = event_loop.create_proxy();
    {
        let proxy = proxy.clone();
        muda::MenuEvent::set_event_handler(Some(move |event| {
            let _ = proxy.send_event(UserEvent::Menu(event));
        }));
    }

    let window_file = dirs
        .as_ref()
        .map(|dirs| SettingsFile::<WindowState>::new(dirs.state_file(window_state::FILE_NAME)));
    let window_state = window_file
        .as_ref()
        .map(|file| file.load().value)
        .unwrap_or_default();

    let mut shell = Shell {
        app,
        proxy,
        smoke: global.smoke_test.map(SmokeTest::new),
        window_file,
        window_state,
        menu: None,
        workspace: Workspace::new(None),
        viewer: None,
        view: None,
        repaint_at: None,
        last_resize: None,
        recoveries: 0,
        exit_code: ExitCode::SUCCESS,
    };
    if let Err(error) = event_loop.run_app(&mut shell) {
        tracing::error!("event loop error: {error}");
        return ExitCode::FAILURE;
    }
    muda::MenuEvent::set_event_handler(None::<fn(muda::MenuEvent)>);
    shell.exit_code
}

struct SmokeTest {
    target: u32,
    rendered: u32,
    offscreen: u32,
    started: Instant,
    last_progress: Instant,
}

impl SmokeTest {
    fn new(target: u32) -> Self {
        let now = Instant::now();
        Self {
            target,
            rendered: 0,
            offscreen: 0,
            started: now,
            last_progress: now,
        }
    }
}

struct Shell {
    app: App,
    proxy: EventLoopProxy<UserEvent>,
    smoke: Option<SmokeTest>,
    window_file: Option<SettingsFile<WindowState>>,
    window_state: WindowState,
    menu: Option<menu::AppMenu>,
    workspace: Workspace,
    viewer: Option<ViewportTextureId>,
    view: Option<WindowView>,
    repaint_at: Option<Instant>,
    /// Time of the last `Resized` event while live-resize mode is on.
    last_resize: Option<Instant>,
    recoveries: u32,
    exit_code: ExitCode,
}

/// Everything tied to the window and the current GPU device.
struct WindowView {
    window: Arc<Window>,
    gpu: Arc<GpuContext>,
    surface: WindowSurface,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    viewport_info: egui::ViewportInfo,
    renderer: UiRenderer,
    status: StatusInfo,
    lost_subscription: DeviceLostSubscription,
    /// Smoke-test fallback target when the window cannot present.
    offscreen: Option<(wgpu::Texture, wgpu::TextureView)>,
}

impl Drop for WindowView {
    fn drop(&mut self) {
        self.gpu.remove_device_lost_callback(self.lost_subscription);
    }
}

impl Shell {
    fn create_window(&self, event_loop: &ActiveEventLoop) -> anyhow::Result<Arc<Window>> {
        let attributes = Window::default_attributes().with_title(bae_platform::APP_NAME);
        let attributes = self.window_state.apply(attributes, event_loop);
        let window = event_loop
            .create_window(attributes)
            .context("cannot create the main window")?;
        Ok(Arc::new(window))
    }

    /// Creates the GPU context, surface, egui state and renderer for `window`.
    fn create_view(&mut self, window: Arc<Window>) -> anyhow::Result<WindowView> {
        let desc = self
            .app
            .resource::<GpuContextDescriptor>()
            .cloned()
            .unwrap_or_default();
        let (gpu, surface) = GpuContext::new_with_surface(desc, window.clone())?;
        let size = window.inner_size();
        let options = SurfaceOptions {
            present_mode: if self.smoke.is_some() {
                PresentModePreference::NoVsync
            } else {
                PresentModePreference::Vsync
            },
            ..SurfaceOptions::default()
        };
        let surface = WindowSurface::new(&gpu, surface, size.width, size.height, options)?;
        crash::set_context("gpu", gpu.summary());
        tracing::info!(
            "window surface: {:?}, {:?}",
            surface.format(),
            surface.config().present_mode
        );

        let lost_subscription = {
            let proxy = self.proxy.clone();
            gpu.on_device_lost(move |info| {
                if !info.destroyed {
                    let _ = proxy.send_event(UserEvent::DeviceLost);
                }
            })
        };
        self.app.insert_resource(gpu.clone());

        let egui_ctx = egui::Context::default();
        // Extra egui viewports (floating panels) render embedded in the main
        // window until native multi-window support lands.
        egui_ctx.set_embed_viewports(true);
        {
            let proxy = self.proxy.clone();
            egui_ctx.set_request_repaint_callback(move |info| {
                if info.viewport_id == egui::ViewportId::ROOT {
                    let _ = proxy.send_event(UserEvent::Repaint { delay: info.delay });
                }
            });
        }
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(gpu.limits().max_texture_dimension_2d as usize),
        );
        let mut viewport_info = egui::ViewportInfo::default();
        egui_winit::update_viewport_info(&mut viewport_info, &egui_ctx, &window, true);
        let renderer = UiRenderer::new(&gpu, surface.format());

        // Placeholder composition until the renderer (workstream 04) publishes one.
        if let Some(registry) = self.app.resource::<Arc<ViewportTextureRegistry>>().cloned() {
            if let Some(old) = self.viewer.take() {
                registry.remove(old);
            }
            self.viewer = ui::publish_placeholder_comp(&gpu, &registry);
            self.workspace.set_viewer(self.viewer);
        }

        let status = StatusInfo {
            gpu: gpu.summary(),
            size_px: [size.width, size.height],
            pixels_per_point: window.scale_factor() as f32,
            ..StatusInfo::default()
        };
        Ok(WindowView {
            window,
            gpu,
            surface,
            egui_ctx,
            egui_state,
            viewport_info,
            renderer,
            status,
            lost_subscription,
            offscreen: None,
        })
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: &anyhow::Error) {
        tracing::error!("{error:#}");
        eprintln!("{}: {error:#}", bae_platform::BINARY_NAME);
        self.exit_code = ExitCode::FAILURE;
        event_loop.exit();
    }

    fn save_window_state(&mut self) {
        let Some(view) = &self.view else { return };
        if self.smoke.is_some() {
            return; // Never let automated runs overwrite the user's layout.
        }
        self.window_state = WindowState::capture(&view.window, &self.window_state);
        if let Some(file) = &self.window_file
            && let Err(error) = file.save(&self.window_state)
        {
            tracing::warn!("cannot save window state: {error}");
        }
    }

    fn schedule_repaint(&mut self, delay: Duration) {
        let Some(view) = &self.view else { return };
        if delay.is_zero() {
            view.window.request_redraw();
            return;
        }
        if let Some(at) = Instant::now().checked_add(delay) {
            self.repaint_at = Some(self.repaint_at.map_or(at, |current| current.min(at)));
        }
    }

    fn on_menu(&mut self, event_loop: &ActiveEventLoop, id: &str) {
        match id {
            menu::ids::QUIT => {
                self.save_window_state();
                event_loop.exit();
            }
            menu::ids::ABOUT => self.workspace.show_about(),
            menu::ids::RESET_WORKSPACE => self.workspace.reset_layout(),
            other => tracing::info!("menu command `{other}` is not implemented yet"),
        }
        if let Some(view) = &self.view {
            view.window.request_redraw();
        }
    }

    /// Recreates the device after a loss, keeping the window and UI state.
    fn recover_device(&mut self, event_loop: &ActiveEventLoop) {
        let Some(view) = self.view.take() else { return };
        if !view.gpu.is_lost() {
            self.view = Some(view);
            return;
        }
        self.recoveries += 1;
        let info = view.gpu.lost_info();
        tracing::warn!(
            "GPU device lost ({}); recreating (attempt {})",
            info.map(|i| i.message).unwrap_or_default(),
            self.recoveries
        );
        if self.recoveries > MAX_DEVICE_RECOVERIES {
            let _ = crash::run_emergency_saves();
            self.fail(
                event_loop,
                &anyhow::anyhow!("the GPU device was lost repeatedly; giving up"),
            );
            return;
        }
        let window = view.window.clone();
        drop(view); // Release every resource of the dead device first.
        match self.create_view(window) {
            Ok(view) => {
                view.window.request_redraw();
                self.view = Some(view);
            }
            Err(error) => self.fail(event_loop, &error),
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let Some(view) = self.view.as_mut() else {
            return;
        };
        let frame_start = Instant::now();

        egui_winit::update_viewport_info(
            &mut view.viewport_info,
            &view.egui_ctx,
            &view.window,
            false,
        );
        let mut input = view.egui_state.take_egui_input(&view.window);
        input
            .viewports
            .insert(egui::ViewportId::ROOT, view.viewport_info.clone());
        view.viewport_info.events.clear();

        let registry = self.app.resource::<Arc<ViewportTextureRegistry>>().cloned();
        let workspace = &mut self.workspace;
        let status = &view.status;
        let output = view.egui_ctx.run_ui(input, |ui| {
            workspace.ui(ui, status, registry.as_deref());
        });
        view.egui_state.handle_platform_output_with_event_loop(
            &view.window,
            event_loop,
            output.platform_output,
        );

        // Root-viewport commands from the UI (custom title bar: drag, minimize,
        // maximize, close …).
        let mut close_requested = false;
        if let Some(root) = output.viewport_output.get(&egui::ViewportId::ROOT) {
            let cancel = root
                .commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::CancelClose));
            let mut actions = Vec::new();
            egui_winit::process_viewport_commands(
                &view.egui_ctx,
                &mut view.viewport_info,
                root.commands.iter().cloned(),
                &view.window,
                &mut actions,
            );
            close_requested = !cancel
                && view
                    .viewport_info
                    .events
                    .iter()
                    .any(|e| matches!(e, egui::ViewportEvent::Close));
        }

        if let Some(registry) = registry.as_deref() {
            view.renderer.sync_viewports(&view.gpu, registry);
        }
        let ui_output = UiOutput {
            shapes: output.shapes,
            textures_delta: output.textures_delta,
            pixels_per_point: output.pixels_per_point,
        };
        let (width, height) = view.surface.size();
        let background = view.egui_ctx.global_style().visuals.panel_fill;

        let mut presented = false;
        let mut device_lost = false;
        match view.surface.acquire() {
            Ok(Some(frame)) => {
                view.renderer.render(
                    &view.gpu,
                    &view.egui_ctx,
                    ui_output,
                    frame.view(),
                    [width, height],
                    background,
                );
                view.window.pre_present_notify();
                let suboptimal = frame.is_suboptimal();
                frame.present();
                if suboptimal {
                    view.surface.handle_suboptimal();
                }
                presented = true;
            }
            result => {
                match result {
                    Err(GpuError::SurfaceLost) => {
                        match view.gpu.create_surface(view.window.clone()) {
                            Ok(surface) => view.surface.replace_surface(surface),
                            Err(error) => tracing::error!("cannot recreate the surface: {error}"),
                        }
                        view.window.request_redraw();
                    }
                    Err(GpuError::DeviceLost(_)) => device_lost = true,
                    Err(error) => tracing::warn!("frame skipped: {error}"),
                    Ok(_) => {}
                }
                // Keep egui's texture state in sync even without a frame; smoke
                // tests render offscreen so they keep exercising the full path.
                if self.smoke.is_some() && !device_lost {
                    let target = offscreen_target(view, [width, height]);
                    view.renderer.render(
                        &view.gpu,
                        &view.egui_ctx,
                        ui_output,
                        &target,
                        [width, height],
                        background,
                    );
                    if let Some(smoke) = &mut self.smoke {
                        smoke.offscreen += 1;
                    }
                    presented = true;
                } else {
                    view.renderer
                        .apply_textures(&view.gpu, ui_output.textures_delta);
                }
            }
        }
        view.gpu.poll();

        if presented {
            let ms = frame_start.elapsed().as_secs_f32() * 1000.0;
            let status = &mut view.status;
            status.frame_ms = if status.frames == 0 {
                ms
            } else {
                status.frame_ms * 0.9 + ms * 0.1
            };
            status.frames += 1;
            status.size_px = [width, height];
            status.pixels_per_point = output.pixels_per_point;
        }

        if device_lost {
            self.recover_device(event_loop);
            return;
        }
        if close_requested {
            self.save_window_state();
            event_loop.exit();
            return;
        }
        if presented && let Some(smoke) = &mut self.smoke {
            smoke.rendered += 1;
            smoke.last_progress = Instant::now();
            if smoke.rendered >= smoke.target {
                let elapsed = smoke.started.elapsed();
                let summary = format!(
                    "smoke test passed: {} frames in {:.1} ms ({} offscreen) on {}",
                    smoke.rendered,
                    elapsed.as_secs_f64() * 1000.0,
                    smoke.offscreen,
                    view.gpu.summary()
                );
                let errors = view.gpu.uncaptured_error_count();
                if errors > 0 {
                    self.fail(
                        event_loop,
                        &anyhow::anyhow!("smoke test: {errors} wgpu validation errors"),
                    );
                } else {
                    println!("{summary}");
                    tracing::info!("{summary}");
                    event_loop.exit();
                }
            }
        }
    }
}

/// The smoke test's fallback render target, (re)created at the surface size.
fn offscreen_target(view: &mut WindowView, size: [u32; 2]) -> wgpu::TextureView {
    let format = view.renderer.format();
    let reuse = view
        .offscreen
        .as_ref()
        .is_some_and(|(texture, _)| [texture.width(), texture.height()] == size);
    if !reuse {
        let texture = view.gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("bae.smoke-offscreen"),
            size: wgpu::Extent3d {
                width: size[0].max(1),
                height: size[1].max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let target = texture.create_view(&Default::default());
        view.offscreen = Some((texture, target));
    }
    view.offscreen
        .as_ref()
        .map(|(_, target)| target.clone())
        .unwrap_or_else(|| unreachable!("offscreen target was just created"))
}

impl ApplicationHandler<UserEvent> for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.view.is_some() {
            return;
        }
        let window = match self.create_window(event_loop) {
            Ok(window) => window,
            Err(error) => return self.fail(event_loop, &error),
        };
        match self.create_view(window) {
            Ok(view) => {
                if self.menu.is_none() {
                    match menu::AppMenu::new() {
                        Ok(menu) => {
                            menu.attach(&view.window);
                            self.menu = Some(menu);
                        }
                        Err(error) => tracing::warn!("native menu unavailable: {error}"),
                    }
                }
                view.window.request_redraw();
                self.view = Some(view);
            }
            Err(error) => self.fail(event_loop, &error),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(view) = self.view.as_mut() else {
            return;
        };
        let response = view.egui_state.on_window_event(&view.window, &event);
        if response.repaint {
            view.window.request_redraw();
        }
        match event {
            WindowEvent::CloseRequested => {
                self.save_window_state();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                // A minimize also arrives as a zero-size resize; that is not a
                // live resize.
                if size.width > 0 && size.height > 0 && self.smoke.is_none() {
                    view.surface.set_live_resize(true);
                    self.last_resize = Some(Instant::now());
                }
                view.surface.resize(size.width, size.height);
                view.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } | WindowEvent::Occluded(false) => {
                view.window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Repaint { delay } => self.schedule_repaint(delay),
            UserEvent::Menu(event) => self.on_menu(event_loop, event.id.as_ref()),
            UserEvent::DeviceLost => self.recover_device(event_loop),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(smoke) = &self.smoke {
            if smoke.last_progress.elapsed() > SMOKE_STALL_TIMEOUT {
                let rendered = smoke.rendered;
                self.fail(
                    event_loop,
                    &anyhow::anyhow!("smoke test stalled after {rendered} frames"),
                );
                return;
            }
            if let Some(view) = &self.view {
                view.window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::Poll);
            return;
        }
        // Sleep until the earliest pending deadline (egui repaint, end of live
        // resize); with none pending, sleep until the next event.
        let now = Instant::now();
        let mut deadline: Option<Instant> = None;
        if let Some(last) = self.last_resize {
            let settle = last + LIVE_RESIZE_SETTLE;
            if now >= settle {
                self.last_resize = None;
                if let Some(view) = &mut self.view {
                    view.surface.set_live_resize(false);
                }
            } else {
                deadline = Some(settle);
            }
        }
        if let Some(at) = self.repaint_at {
            if at <= now {
                self.repaint_at = None;
                if let Some(view) = &self.view {
                    view.window.request_redraw();
                }
            } else {
                deadline = Some(deadline.map_or(at, |d| d.min(at)));
            }
        }
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.save_window_state();
        // Drop GPU objects while the window still exists.
        self.view = None;
    }
}
