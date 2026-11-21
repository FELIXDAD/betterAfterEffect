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
//! * File commands (new / open / close / save / save as) run through
//!   [`crate::files`] with the native dialogs ([`dialogs::Native`]); closing the
//!   window or quitting asks about unsaved changes first. `bae <project>` opens a
//!   project at startup.
//!
//! See `docs/spec/app-shell.md`.

mod dialogs;
mod menu;
#[cfg(target_os = "macos")]
mod open_documents;
mod window_state;

#[cfg(test)]
pub(crate) use menu::undo_redo_text;

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

use crate::document::{Document, Outcome};
use crate::files::{self, CommitGate, FileDialogs, NoDialogs};
use crate::ui::{self, PlaceholderViewer, StatusInfo, UiOutput, UiRenderer, Workspace};
use bae_ui::Invocation;
use bae_ui::action::ids;
use bae_ui::history::HistoryLog;
use window_state::{WindowState, WorkbenchSettings};

/// The shell's app module (depends on the GPU module).
#[derive(Debug, Default)]
pub struct ShellModule;

impl AppModule for ShellModule {
    fn name(&self) -> &str {
        "bae-shell"
    }

    fn build(&self, app: &mut AppBuilder) {
        app.add_module(bae_gpu::GpuModule);
        app.add_module(bae_ui::UiModule);
        // Agent panels: tool-call log and proposals (bae-agent, workstream 17).
        let audit = Arc::new(bae_agent::AuditLog::new());
        let feed: Arc<dyn bae_ui::agent::AgentFeed> =
            Arc::new(crate::agent_feed::BaeAgentFeed::new(audit.clone()));
        app.insert_resource(audit);
        app.insert_resource(feed);
    }

    fn finish(&self, app: &mut AppBuilder) {
        ui::register_panels(app);
        // File menu items: registered like any module's menu items, so the native
        // menu shows the shortcut the active keymap binds to each.
        let owner = app.current_owner();
        let menu = app.registry::<bae_ui::MenuItemSpec>();
        menu.batch(|tx| {
            for (id, item) in menu::file_items().into_iter().chain(menu::window_items()) {
                if !tx.contains(id) {
                    let _ = tx.register(owner.clone(), id, item);
                }
            }
        });
    }
}

/// English titles of the File menu items and their menu path (i18n test).
#[cfg(test)]
pub fn file_menu_titles() -> Vec<String> {
    menu::file_items()
        .into_iter()
        .flat_map(|(_, spec)| {
            spec.path
                .iter()
                .cloned()
                .chain([spec.title.clone()])
                .collect::<Vec<_>>()
        })
        .collect()
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
    /// A screen reader or UI automation client asked for the accessibility
    /// tree or for an action (accesskit).
    AccessKit(egui_winit::accesskit_winit::Event),
    /// Finder asked to open projects (double-click, Dock drop, `open`): macOS only,
    /// see `open_documents`.
    #[cfg(target_os = "macos")]
    OpenFiles(Vec<std::path::PathBuf>),
}

impl From<egui_winit::accesskit_winit::Event> for UserEvent {
    fn from(event: egui_winit::accesskit_winit::Event) -> Self {
        UserEvent::AccessKit(event)
    }
}

/// A smoke test stops as failed if no frame completes for this long.
const SMOKE_STALL_TIMEOUT: Duration = Duration::from_secs(20);
/// Live-resize mode ends this long after the last `Resized` event.
const LIVE_RESIZE_SETTLE: Duration = Duration::from_millis(250);
/// How long file commands wait for the frame that commits a value being typed
/// before they run anyway (a window that does not redraw, e.g. minimized).
const COMMIT_WAIT: Duration = Duration::from_millis(300);
/// How often device recovery is attempted before giving up.
const MAX_DEVICE_RECOVERIES: u32 = 3;

/// Runs the GUI until the window closes. Returns the process exit code.
pub fn run(mut app: App, global: &GlobalOptions) -> ExitCode {
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
    // After the event loop exists (it registers winit's app delegate class), before
    // `run_app` (so a project double-clicked to launch BAE is not missed).
    #[cfg(target_os = "macos")]
    {
        let proxy = std::sync::Mutex::new(proxy.clone());
        open_documents::install(move |paths| {
            let proxy = proxy
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let _ = proxy.send_event(UserEvent::OpenFiles(paths));
        });
    }

    let window_file = dirs
        .as_ref()
        .map(|dirs| SettingsFile::<WindowState>::new(dirs.state_file(window_state::FILE_NAME)));
    let window_state = window_file
        .as_ref()
        .map(|file| file.load().value)
        .unwrap_or_default();
    // Smoke tests always start from the default layout and never save it.
    let workbench_file = dirs
        .as_ref()
        .filter(|_| global.smoke_test.is_none())
        .map(|dirs| {
            SettingsFile::<WorkbenchSettings>::new(
                dirs.state_file(window_state::WORKBENCH_FILE_NAME),
            )
        });
    let loaded = workbench_file.as_ref().map(|file| file.load());
    // First launch: no saved workspaces yet, so ask for the user's role.
    let first_run = loaded
        .as_ref()
        .is_some_and(|l| l.status == bae_platform::settings::LoadStatus::Missing);
    let workbench_state = loaded.map(|l| l.value.0).unwrap_or_default();
    let mut document = Document::new(app.resource::<Arc<HistoryLog>>().cloned());
    document.add_module_commands(&app);
    // `bae <project>`: open it now. One project at a time, like AE.
    let mut startup_error = None;
    if let Some(path) = global.open.first() {
        if global.open.len() > 1 {
            tracing::warn!("only the first project is opened: {}", path.display());
        }
        match document.open(path) {
            Ok(()) => {
                tracing::info!("opened {}", path.display());
                crate::view_state::restore(&mut app, &document);
            }
            Err(error) if global.smoke_test.is_some() => {
                eprintln!(
                    "{}: {}",
                    bae_platform::BINARY_NAME,
                    crate::file_error::describe_open(path, &error)
                );
                return ExitCode::FAILURE;
            }
            Err(error) => {
                startup_error = Some(format!(
                    "{}\n\n{}",
                    crate::i18n::tr("Could not open the project."),
                    crate::file_error::describe_open(path, &error)
                ));
            }
        }
    }
    let mut workspace = Workspace::new(&mut app, workbench_state);
    workspace.set_extra_commands(document.command_titles());
    if first_run {
        workspace.show_role_picker();
    }

    let mut shell = Shell {
        app,
        proxy,
        smoke: global.smoke_test.map(SmokeTest::new),
        window_file,
        window_state,
        menu: None,
        workspace,
        workbench_file,
        document,
        startup_error,
        window_title: String::new(),
        pending: Vec::new(),
        viewer: None,
        view: None,
        repaint_at: None,
        last_resize: None,
        recoveries: 0,
        gate: CommitGate::default(),
        gate_deadline: None,
        ui_passes: 0,
        accesskit_active: false,
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
    workbench_file: Option<SettingsFile<WorkbenchSettings>>,
    document: Document,
    /// `bae <project>` failed; reported once the window is up.
    startup_error: Option<String>,
    /// Title last given to the native window.
    window_title: String,
    /// Invocations collected during the last frame, run in `about_to_wait`.
    pending: Vec<Invocation>,
    viewer: Option<ViewportTextureId>,
    view: Option<WindowView>,
    repaint_at: Option<Instant>,
    /// Time of the last `Resized` event while live-resize mode is on.
    last_resize: Option<Instant>,
    recoveries: u32,
    /// Holds file commands and Quit until the edits before them are committed.
    gate: CommitGate,
    /// When held commands run even without the committing frame.
    gate_deadline: Option<Instant>,
    /// UI frames run so far (egui passes, presented or not).
    ui_passes: u64,
    /// A screen reader / automation client is reading the accesskit tree.
    accesskit_active: bool,
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
        // `bae-ui` draws its own title bar on macOS; the window keeps native
        // resizing, rounded corners and shadow.
        let attributes = egui_winit::create_winit_window_attributes(
            &egui::Context::default(),
            bae_ui::titlebar::viewport_builder(bae_platform::APP_NAME),
        );
        // Hidden until the accesskit adapter exists (it must be created before
        // the window is first shown); `resumed` shows it.
        let attributes = self
            .window_state
            .apply(attributes, event_loop)
            .with_visible(false);
        #[cfg(windows)]
        let attributes = with_exe_icon(attributes);
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
        bae_ui::theme::apply(&egui_ctx);
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
            match self.viewer {
                Some(id) => {
                    self.app.insert_resource(PlaceholderViewer(id));
                }
                None => {
                    self.app.remove_resource::<PlaceholderViewer>();
                }
            }
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
        if let Some(file) = &self.workbench_file
            && let Err(error) = file.save(&WorkbenchSettings(self.workspace.state()))
        {
            tracing::warn!("cannot save the workspace layout: {error}");
        }
        self.window_state = WindowState::capture(&view.window, &self.window_state);
        if let Some(file) = &self.window_file
            && let Err(error) = file.save(&self.window_state)
        {
            tracing::warn!("cannot save window state: {error}");
        }
    }

    fn schedule_repaint(&mut self, delay: Duration) {
        tracing::trace!(?delay, "egui repaint request");
        let Some(view) = &self.view else { return };
        if delay.is_zero() {
            view.window.request_redraw();
            return;
        }
        if let Some(at) = Instant::now().checked_add(delay) {
            self.repaint_at = Some(self.repaint_at.map_or(at, |current| current.min(at)));
        }
    }

    /// Runs a command id from a menu, shortcut, toolbar or panel: the focused
    /// panel and the workbench first, then the shell's own actions, then the
    /// document (undo/redo and `bae-model` commands).
    fn dispatch(&mut self, event_loop: &ActiveEventLoop, inv: Invocation) {
        let Some(inv) = self
            .workspace
            .handle(inv, &self.document, self.app.resources())
        else {
            self.request_redraw();
            return;
        };
        match inv.id.as_str() {
            ids::QUIT => {
                self.quit(event_loop);
                return;
            }
            ids::ABOUT => self.workspace.show_about(),
            id if files::is_file_command(id) => {
                // Saves record the open comps; another project restores its own.
                let mut dialogs = self.dialogs();
                crate::view_state::run_file_command(
                    &mut self.app,
                    &mut self.document,
                    &mut *dialogs,
                    &inv,
                );
            }
            ids::HELP => tracing::info!("`{}` is not implemented yet", inv.id),
            _ => match self.document.run(&inv) {
                Outcome::Changed => self.set_notice(String::new()),
                Outcome::Unchanged => {}
                Outcome::Unknown => tracing::debug!("no handler for command `{}`", inv.id),
                Outcome::Failed(error) => {
                    tracing::warn!("command `{}` failed: {error}", inv.id);
                    self.set_notice(ui::failure_notice(&error));
                }
            },
        }
        if let Some(menu) = &mut self.menu {
            menu.set_undo_state(self.document.undo_state());
        }
        self.request_redraw();
    }

    /// Show `text` in the status bar (empty clears it).
    fn set_notice(&mut self, text: String) {
        if let Some(view) = &mut self.view {
            view.status.notice = text;
        }
    }

    fn request_redraw(&self) {
        if let Some(view) = &self.view {
            view.window.request_redraw();
        }
    }

    /// Runs the invocations collected since the last call: edits first, file
    /// commands and Quit after them — and, while a value is being typed, only
    /// after the frame in which ending the edit commits it ([`CommitGate`]).
    fn run_pending(&mut self, event_loop: &ActiveEventLoop) {
        let batch = std::mem::take(&mut self.pending);
        let editing = self
            .view
            .as_ref()
            .is_some_and(|v| v.egui_ctx.egui_wants_keyboard_input());
        let mut admitted = self.gate.admit(batch, editing, self.ui_passes);
        if admitted.stop_editing {
            if let Some(view) = &self.view {
                // Fields commit when they lose focus (Enter or a click elsewhere).
                view.egui_ctx.memory_mut(|m| m.stop_text_input());
            }
            self.gate_deadline = Instant::now().checked_add(COMMIT_WAIT);
        }
        if self.gate.is_waiting() {
            if self.gate_deadline.is_none_or(|at| Instant::now() >= at) {
                admitted.run.extend(self.gate.release());
            } else {
                self.request_redraw();
            }
        }
        if !self.gate.is_waiting() {
            self.gate_deadline = None;
        }
        for inv in admitted.run {
            self.dispatch(event_loop, inv);
        }
    }

    /// The dialogs file commands use: native ones owned by the window, none in
    /// smoke tests (they must never block).
    fn dialogs(&self) -> Box<dyn FileDialogs> {
        if self.smoke.is_some() {
            Box::new(NoDialogs)
        } else {
            Box::new(dialogs::Native::new(
                self.view.as_ref().map(|v| v.window.clone()),
            ))
        }
    }

    /// Quit (menu, Cmd/Ctrl+Q, closing the window) unless the user keeps the
    /// project open at the unsaved-changes question.
    fn quit(&mut self, event_loop: &ActiveEventLoop) {
        if self.smoke.is_none() {
            let mut dialogs = self.dialogs();
            if !crate::view_state::confirm_quit(&self.app, &mut self.document, &mut *dialogs) {
                self.request_redraw();
                return;
            }
        }
        self.save_window_state();
        event_loop.exit();
    }

    /// Keeps the native window title in step with the project name and the
    /// unsaved-changes mark (Windows title bar, macOS Window menu and Mission
    /// Control; macOS also gets the "edited" state).
    fn sync_window_title(&mut self) {
        let Some(view) = &self.view else { return };
        let title = self.document.title();
        if title != self.window_title {
            view.window.set_title(&title);
            #[cfg(target_os = "macos")]
            {
                use winit::platform::macos::WindowExtMacOS as _;
                view.window.set_document_edited(self.document.is_dirty());
            }
            self.window_title = title;
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
        // The accesskit adapter cannot be created again for a visible window:
        // move it to the new egui state.
        let mut view = view;
        let accesskit = view.egui_state.accesskit.take();
        drop(view); // Release every resource of the dead device first.
        match self.create_view(window) {
            Ok(mut view) => {
                view.egui_state.accesskit = accesskit;
                if self.accesskit_active {
                    view.egui_ctx.enable_accesskit();
                }
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
        tracing::trace!(frame = view.status.frames, "redraw");

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
        let document = &self.document;
        let resources = self.app.resources();
        let mut invocations = Vec::new();
        let output = view.egui_ctx.run_ui(input, |ui| {
            invocations = workspace.ui(ui, status, document, resources);
        });
        self.ui_passes += 1;
        if let Some(menu) = &mut self.menu {
            let cx = bae_ui::menu::MenuContext {
                stage: document.stage(),
                resources,
                workspace: workspace.active_workspace(),
            };
            menu.sync(workspace.keymap(), &cx);
        }
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

        // Run after the frame (and outside the borrow of the view).
        self.pending.extend(invocations);
        if device_lost {
            self.recover_device(event_loop);
            return;
        }
        if close_requested {
            // Asked (unsaved changes) after the frame, outside the redraw.
            self.pending.push(Invocation::new(ids::QUIT));
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

/// The command for projects Finder asked to open: one project at a time, like AE and
/// like `bae <project>`, so only the first is opened. Goes through the normal
/// `file.open_project` (unsaved-changes question, error dialog).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn open_files_invocation(paths: &[std::path::PathBuf]) -> Option<Invocation> {
    let (first, rest) = paths.split_first()?;
    if !rest.is_empty() {
        tracing::warn!("only the first project is opened: {}", first.display());
    }
    Some(Invocation::with_params(
        ids::OPEN_PROJECT,
        serde_json::json!({ "path": first.to_string_lossy() }),
    ))
}

/// Window and taskbar icon from the icon group linked into `bae.exe`
/// (`apps/bae/build.rs`, `assets/icons/app/bae.res`).
#[cfg(windows)]
fn with_exe_icon(attributes: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    use winit::platform::windows::{IconExtWindows as _, WindowAttributesExtWindows as _};
    /// `GROUP_ICON_ID` in `xtask/src/icon.rs`.
    const APP_ICON_RESOURCE: u16 = 1;
    match winit::window::Icon::from_resource(APP_ICON_RESOURCE, None) {
        Ok(icon) => attributes
            .with_window_icon(Some(icon.clone()))
            .with_taskbar_icon(Some(icon)),
        Err(error) => {
            tracing::warn!("app icon resource not found: {error}");
            attributes
        }
    }
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
            Ok(mut view) => {
                // Screen readers (VoiceOver, Narrator) and UI automation read the
                // labelled widgets through accesskit (issue #116).
                view.egui_state
                    .init_accesskit(event_loop, &view.window, self.proxy.clone());
                view.window.set_visible(true);
                if self.menu.is_none() {
                    let contributed = self.workspace.contributed_menus(&self.app);
                    match menu::AppMenu::new(&self.workspace.workspaces(), &contributed) {
                        Ok(menu) => {
                            menu.attach(&view.window);
                            self.menu = Some(menu);
                        }
                        Err(error) => tracing::warn!("native menu unavailable: {error}"),
                    }
                }
                view.window.request_redraw();
                self.view = Some(view);
                if let Some(message) = self.startup_error.take() {
                    self.dialogs().error(&message);
                }
            }
            Err(error) => self.fail(event_loop, &error),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(view) = self.view.as_mut() else {
            return;
        };
        if matches!(event, WindowEvent::RedrawRequested) {
            // Not an input event: egui-winit answers it with `repaint: true`, and
            // honouring that would redraw forever (no idle).
            self.redraw(event_loop);
            return;
        }
        let response = view.egui_state.on_window_event(&view.window, &event);
        tracing::trace!(
            event = egui_winit::short_window_event_description(&event),
            repaint = response.repaint,
            "window event"
        );
        if response.repaint {
            view.window.request_redraw();
        }
        match event {
            WindowEvent::CloseRequested => {
                // Through the dispatch: unsaved changes are asked about first.
                self.pending.push(Invocation::new(ids::QUIT));
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
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Repaint { delay } => self.schedule_repaint(delay),
            UserEvent::Menu(event) => {
                let id = event.id.as_ref();
                let inv = self
                    .menu
                    .as_ref()
                    .and_then(|m| m.contributed_invocation(id))
                    .unwrap_or_else(|| menu::invocation(id));
                // A modal dialog is open: menu commands (and their key
                // equivalents) do not reach the project behind it; Quit still works.
                if self.workspace.modal_open() && inv.id != ids::QUIT {
                    return;
                }
                // Undo / Redo (their native key equivalents too) while a text
                // field is focused undo the typing, as in any Mac / Windows app.
                if (inv.id == ids::UNDO || inv.id == ids::REDO)
                    && let Some(view) = self.view.as_mut()
                    && view.egui_ctx.text_edit_focused()
                {
                    let modifiers = egui::Modifiers {
                        shift: inv.id == ids::REDO,
                        ..egui::Modifiers::COMMAND
                    };
                    view.egui_state
                        .egui_input_mut()
                        .events
                        .push(egui::Event::Key {
                            key: egui::Key::Z,
                            physical_key: Some(egui::Key::Z),
                            pressed: true,
                            repeat: false,
                            modifiers,
                        });
                    view.window.request_redraw();
                    return;
                }
                // Queued behind the edits the last frame produced (about_to_wait
                // runs them in order), not ahead of them.
                self.pending.push(inv);
            }
            UserEvent::DeviceLost => self.recover_device(event_loop),
            #[cfg(target_os = "macos")]
            UserEvent::OpenFiles(paths) => {
                // A file command like File > Open: `dispatch` runs it through
                // `view_state::run_file_command`, so the project's comps are restored.
                if let Some(inv) = open_files_invocation(&paths) {
                    self.pending.push(inv);
                }
            }
            UserEvent::AccessKit(event) => {
                use egui_winit::accesskit_winit::WindowEvent as A;
                let Some(view) = self.view.as_mut() else {
                    return;
                };
                match event.window_event {
                    A::InitialTreeRequested => {
                        self.accesskit_active = true;
                        view.egui_ctx.enable_accesskit();
                    }
                    A::ActionRequested(request) => {
                        view.egui_state.on_accesskit_action_request(request);
                    }
                    A::AccessibilityDeactivated => {
                        self.accesskit_active = false;
                        view.egui_ctx.disable_accesskit();
                    }
                }
                view.window.request_redraw();
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.run_pending(event_loop);
        // Every project change goes through the dispatch above.
        self.sync_window_title();
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
        if let Some(at) = self.gate_deadline {
            deadline = Some(deadline.map_or(at, |d| d.min(at)));
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
