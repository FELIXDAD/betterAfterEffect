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

pub(crate) mod builtin_scripts;
mod dialogs;
mod menu;
mod numpad;
#[cfg(target_os = "macos")]
mod open_documents;
#[cfg(any(windows, test))]
mod save_guard;
mod script_bindings;
#[cfg(any(windows, test))]
mod win_keys;
#[cfg(windows)]
mod win_save;
mod window_state;

// Only the macOS acceptance suite calls it (Windows test builds would warn).
#[cfg(all(test, target_os = "macos"))]
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
        script_bindings::install(app);
        // Agent panels: tool-call log and proposals (bae-agent, workstream 17).
        let audit = Arc::new(bae_agent::AuditLog::new());
        let feed: Arc<dyn bae_ui::agent::AgentFeed> =
            Arc::new(crate::agent_feed::BaeAgentFeed::new(audit.clone()));
        app.insert_resource(audit);
        app.insert_resource(feed);
    }

    fn finish(&self, app: &mut AppBuilder) {
        builtin_scripts::register(app);
        ui::register_panels(app);
        // BAE's own menu entries (History and Agent panels, Agent workspace),
        // registered next to the AE menus the motion workspace contributes.
        let owner = app.current_owner();
        let menu = app.registry::<bae_ui::MenuItemSpec>();
        menu.batch(|tx| {
            for (id, item) in menu::extra_items() {
                if !tx.contains(id) {
                    let _ = tx.register(owner.clone(), id, item);
                }
            }
        });
    }
}

/// English titles of the shell's menu items and their menu path (i18n test).
#[cfg(test)]
pub fn file_menu_titles() -> Vec<String> {
    menu::extra_items()
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
    /// A native menu item was chosen; with the modifiers of the key press when
    /// its key equivalent fired ([`menu::key_equivalent_modifiers`]).
    Menu(muda::MenuEvent, Option<egui::Modifiers>),
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
    // The UI's system fonts load while the window and GPU are being set up.
    bae_ui::theme::prefetch_system_ui_fonts();
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

    let mut builder = EventLoop::<UserEvent>::with_user_event();
    #[cfg(windows)]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows as _;
        builder.with_msg_hook(win_keys::hook);
    }
    let event_loop = match builder.build() {
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
            let keys = menu::key_equivalent_modifiers();
            let _ = proxy.send_event(UserEvent::Menu(event, keys));
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
                #[cfg(target_os = "macos")]
                if global.smoke_test.is_none()
                    && let Some(path) = document.location()
                {
                    open_documents::note_recent(path);
                }
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
    // Preferences: read from the config folder; smoke tests use the defaults and
    // never write them.
    let prefs = Arc::new(match &dirs {
        Some(dirs) if global.smoke_test.is_none() => {
            bae_prefs::PrefsStore::open(dirs.config_file(bae_prefs::FILE_NAME)).0
        }
        _ => bae_prefs::PrefsStore::default(),
    });
    app.insert_resource(prefs.clone());
    let project_file = bae_ui::files::ProjectFile::default();
    project_file.set(document.location().map(std::path::Path::to_owned));
    app.insert_resource(project_file);
    crate::i18n::apply_setting(prefs.current().str(bae_prefs::keys::LANGUAGE));
    bae_ui::settings::apply(&prefs.current());
    document.set_undo_limit(prefs.current().levels_of_undo());
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
        last_present: None,
        last_resize: None,
        recoveries: 0,
        gate: CommitGate::default(),
        gate_deadline: None,
        ui_passes: 0,
        accesskit_active: false,
        exit_code: ExitCode::SUCCESS,
        prefs_generation: prefs.generation(),
        prefs,
        menu_lang: bae_ui::i18n::lang(),
        menu_registry_generation: 0,
        autosave: crate::autosave::AutoSave::default(),
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
    /// Preferences and the generation last applied.
    prefs: Arc<bae_prefs::PrefsStore>,
    prefs_generation: u64,
    autosave: crate::autosave::AutoSave,
    app: App,
    proxy: EventLoopProxy<UserEvent>,
    smoke: Option<SmokeTest>,
    window_file: Option<SettingsFile<WindowState>>,
    window_state: WindowState,
    menu: Option<menu::AppMenu>,
    /// The UI language the menu bar was built in.
    menu_lang: bae_ui::Lang,
    menu_registry_generation: u64,
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
    /// When the last frame reached the surface (paces idle repaints).
    last_present: Option<Instant>,
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
    /// Smoke-test fallback target when the window cannot present; also the
    /// target of screenshots (`ViewportCommand::Screenshot`).
    offscreen: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Screenshot replies for the next frame's input (the eyedropper).
    screenshots: Vec<egui::Event>,
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
                PresentModePreference::LowLatency
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
        // The UI thread never waits for the warm-up (see `warm_up`).
        bae_render::set_interactive_thread(true);
        {
            let tasks = self
                .app
                .get_registry::<dyn bae_gpu::WarmUp>()
                .map(|r| r.snapshot().iter().map(|e| e.item().clone()).collect())
                .unwrap_or_default();
            // Frames drawn without something still warming up are incomplete
            // (not remembered by the viewers): repaint when work finishes.
            let proxy = std::sync::Mutex::new(self.proxy.clone());
            bae_gpu::warm_up::set_redraw(move || {
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(UserEvent::Repaint {
                        delay: Duration::ZERO,
                    });
                }
            });
            bae_gpu::warm_up::spawn(gpu.clone(), tasks);
        }

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

        // Native open panels for modules (File > Import …), owned by the window;
        // none in smoke tests (they must never block).
        if self.smoke.is_none() {
            self.app
                .insert_resource(bae_ui::files::SharedFilePicker::new(
                    dialogs::NativePicker::new(Some(window.clone())),
                ));
        }

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
            gpu: ui::gpu_summary(&gpu),
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
            screenshots: Vec::new(),
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
        let delay = delay.max(self.idle_frame_wait());
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
            ids::REVEAL_PROJECT => {
                if let Some(path) = self.document.location() {
                    bae_platform::reveal(path);
                }
            }
            bae_ui::settings::ACTION => self
                .workspace
                .show_settings(inv.params.get("page").and_then(|p| p.as_str())),
            id if files::is_file_command(id) => {
                // Saves record the open comps; another project restores its own.
                let mut dialogs = self.dialogs();
                let outcome = crate::view_state::run_file_command(
                    &mut self.app,
                    &mut self.document,
                    &mut *dialogs,
                    &inv,
                );
                if cfg!(target_os = "macos")
                    && self.smoke.is_none()
                    && inv.id != ids::SAVE_A_COPY
                    && matches!(
                        outcome,
                        Some(files::FileOutcome::Saved | files::FileOutcome::Replaced)
                    )
                    && let Some(_path) = self.document.location()
                {
                    #[cfg(target_os = "macos")]
                    open_documents::note_recent(_path);
                }
                // A save or another project restarts the auto-save interval.
                self.autosave.reset();
                if let Some(file) = self.app.resource::<bae_ui::files::ProjectFile>() {
                    file.set(self.document.location().map(std::path::Path::to_owned));
                }
            }
            ids::MINIMIZE => {
                if let Some(view) = &self.view {
                    view.window.set_minimized(true);
                }
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

    /// Preferences changed (OK in the dialog): apply what the shell owns.
    fn sync_prefs(&mut self) {
        let generation = self.prefs.generation();
        if generation == self.prefs_generation {
            return;
        }
        self.prefs_generation = generation;
        let prefs = self.prefs.current();
        crate::i18n::apply_setting(prefs.str(bae_prefs::keys::LANGUAGE));
        bae_ui::settings::apply(&prefs);
        self.document.set_undo_limit(prefs.levels_of_undo());
        if let Some(menu) = &mut self.menu {
            menu.set_undo_state(self.document.undo_state());
        }
        self.request_redraw();
    }

    /// The UI language changed (Preferences > Appearance > Language, also while
    /// the window is open): what is not drawn from `tr` every frame follows, the
    /// menu bar is rebuilt in it and new items are named in it.
    fn sync_language(&mut self) {
        let lang = bae_ui::i18n::lang();
        let generation = self
            .app
            .get_registry::<bae_ui::MenuItemSpec>()
            .map_or(0, |r| r.generation());
        if lang == self.menu_lang && generation == self.menu_registry_generation {
            return;
        }
        self.menu_lang = lang;
        bae_motion::names::set_chinese(lang == bae_ui::Lang::Zh);
        if self.menu.is_some() {
            self.build_menu();
        }
        self.request_redraw();
    }

    /// Builds the menu bar from the registered menus in the current language and
    /// attaches it in place of the previous one.
    fn build_menu(&mut self) {
        let Some(view) = self.view.as_ref() else {
            return;
        };
        if let Some(old) = self.menu.take() {
            old.detach(&view.window);
        }
        self.menu_registry_generation = self
            .app
            .get_registry::<bae_ui::MenuItemSpec>()
            .map_or(0, |r| r.generation());
        let contributed = self.workspace.contributed_menus(&self.app);
        let actions = self.workspace.handled_actions(&self.app);
        match menu::AppMenu::new(&contributed, actions) {
            Ok(mut menu) => {
                menu.set_undo_state(self.document.undo_state());
                menu.attach(&view.window);
                self.menu = Some(menu);
            }
            Err(error) => tracing::warn!("native menu unavailable: {error}"),
        }
    }

    /// Buttons of the Preferences window that need the system.
    fn run_settings_requests(&mut self) {
        for request in self.workspace.take_settings_requests() {
            if let Some((title, body)) =
                crate::ui::hardware_information(&request, self.app.resources())
            {
                let mut dialog = rfd::MessageDialog::new()
                    .set_title(title)
                    .set_description(body);
                if let Some(view) = &self.view {
                    dialog = dialog.set_parent(&*view.window);
                }
                dialog.show();
            } else if request == crate::ui::REVEAL_PREFERENCES {
                if let Some(dirs) = self.app.resource::<AppDirs>() {
                    reveal(&dirs.config_file(bae_prefs::FILE_NAME));
                }
            } else if let Some(key) = request.strip_prefix("choose:") {
                let mut dialog = rfd::FileDialog::new();
                if let Some(view) = &self.view {
                    dialog = dialog.set_parent(&*view.window);
                }
                let picked = if key == "project.project_template" {
                    dialog.add_filter("BAE", &["bae", "baez"]).pick_file()
                } else {
                    dialog.pick_folder()
                };
                if let Some(path) = picked {
                    self.workspace
                        .set_setting(key, path.to_string_lossy().into_owned());
                    self.request_redraw();
                }
            }
        }
    }

    /// Auto-save (Preferences > Auto-Save); returns when it is due next.
    fn run_autosave(&mut self) -> Option<Instant> {
        if self.smoke.is_some() {
            return None;
        }
        let prefs = self.prefs.current();
        match self.autosave.tick(Instant::now(), &prefs, &self.document) {
            Some(Ok(path)) => tracing::info!("auto-saved {}", path.display()),
            Some(Err(error)) => tracing::warn!("auto-save failed: {error:#}"),
            None => {}
        }
        self.autosave.deadline(&prefs)
    }

    /// Plugin problems (a plugin failed to load, crashed or was disabled) in
    /// the status bar; the latest wins.
    fn show_plugin_notices(&mut self) {
        let notice = self
            .app
            .resource::<bae_plugin::PluginRuntime>()
            .and_then(|r| r.take_notices().pop());
        if let Some(notice) = notice {
            self.set_notice(ui::plugin_notice(&notice));
        }
    }

    /// Show `text` in the status bar (empty clears it).
    fn set_notice(&mut self, text: String) {
        if let Some(view) = &mut self.view {
            view.status.notice = text;
        }
    }

    /// Mailbox presentation does not wait for vblank, so continuous egui
    /// repaints outside playback (which paces itself by frame deadlines) wait
    /// for the display's next refresh after the last presented frame.
    fn idle_frame_wait(&self) -> Duration {
        let (Some(view), Some(last)) = (&self.view, self.last_present) else {
            return Duration::ZERO;
        };
        let playing = self
            .app
            .resource::<bae_ui_motion::SharedSession>()
            .is_some_and(|s| s.lock().is_playing());
        if playing || self.smoke.is_some() {
            return Duration::ZERO;
        }
        let millihertz = view
            .window
            .current_monitor()
            .and_then(|m| m.refresh_rate_millihertz())
            .unwrap_or(60_000)
            .max(1);
        let interval = Duration::from_secs_f64(1000.0 / f64::from(millihertz));
        (last + interval).saturating_duration_since(Instant::now())
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
        self.show_plugin_notices();
        let Some(view) = self.view.as_mut() else {
            return;
        };
        #[cfg(target_os = "macos")]
        {
            // A backing-scale notification can precede the physical resize event.
            let size = view.window.inner_size();
            if view.surface.size() != (size.width, size.height) {
                view.surface.resize(size.width, size.height);
            }
        }
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
        input.events.append(&mut view.screenshots);
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
            let open = workspace.open_panels();
            let cx = bae_ui::menu::MenuContext::new(
                document.stage(),
                resources,
                workspace.active_workspace(),
            )
            .with_open_panels(&open);
            menu.sync(workspace.keymap(), &cx, view.egui_ctx.text_edit_focused());
            menu.sync_workspace_titles(workspace);
        }
        view.egui_state.handle_platform_output_with_event_loop(
            &view.window,
            event_loop,
            output.platform_output,
        );

        // Root-viewport commands from the UI (custom title bar: drag, minimize,
        // maximize, close …).
        let mut close_requested = false;
        // Screenshots the UI asked for (the color picker's eyedropper): the
        // frame is drawn a second time offscreen and read back below.
        let screenshots: Vec<egui::UserData> = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|root| {
                root.commands
                    .iter()
                    .filter_map(|c| match c {
                        egui::ViewportCommand::Screenshot(u) => Some(u.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let screenshot_shapes = (!screenshots.is_empty()).then(|| output.shapes.clone());
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
        let session = self.app.resource::<bae_ui_motion::SharedSession>();
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
                if let Some(session) = session {
                    session.lock().frame_presented(Instant::now());
                }
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
        if let Some(shapes) = screenshot_shapes
            && presented
            && !device_lost
        {
            let ppp = output.pixels_per_point;
            if let Some(image) = screenshot(view, shapes, [width, height], ppp, background) {
                let image = Arc::new(image);
                for user_data in screenshots {
                    view.screenshots.push(egui::Event::Screenshot {
                        viewport_id: egui::ViewportId::ROOT,
                        user_data,
                        image: image.clone(),
                    });
                }
                view.window.request_redraw();
            }
        }

        if presented {
            self.last_present = Some(Instant::now());
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
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

/// Shows `path` in Finder / Explorer (Reveal Preferences).
fn reveal(path: &std::path::Path) {
    bae_platform::reveal(path);
}

/// Draws `shapes` (this frame's UI, textures already uploaded) offscreen and
/// reads the pixels back, for `ViewportCommand::Screenshot`. Rare (one click of
/// the eyedropper), so the readback waits for the GPU.
fn screenshot(
    view: &mut WindowView,
    shapes: Vec<egui::epaint::ClippedShape>,
    size: [u32; 2],
    pixels_per_point: f32,
    background: egui::Color32,
) -> Option<egui::ColorImage> {
    let target = offscreen_target(view, size);
    view.renderer.render(
        &view.gpu,
        &view.egui_ctx,
        UiOutput {
            shapes,
            textures_delta: Default::default(),
            pixels_per_point,
        },
        &target,
        size,
        background,
    );
    let texture = view.offscreen.as_ref()?.0.clone();
    let data = bae_gpu::transfer::read_texture(&view.gpu, &texture)
        .map_err(|e| tracing::warn!("screenshot readback failed: {e}"))
        .ok()?;
    let bgra = matches!(
        data.format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    let mut pixels = Vec::with_capacity((data.width * data.height) as usize);
    for y in 0..data.height {
        for x in 0..data.width {
            let p = data.pixel(x, y);
            let (r, g, b) = if bgra {
                (p[2], p[1], p[0])
            } else {
                (p[0], p[1], p[2])
            };
            pixels.push(egui::Color32::from_rgb(r, g, b));
        }
    }
    Some(egui::ColorImage::new(
        [data.width as usize, data.height as usize],
        pixels,
    ))
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
                view.window.request_redraw();
                self.view = Some(view);
                if self.menu.is_none() {
                    self.menu_lang = bae_ui::i18n::lang();
                    self.build_menu();
                }
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
        // egui reports keypad Enter as Enter; AE tells them apart.
        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state.is_pressed()
            && let winit::keyboard::PhysicalKey::Code(
                code @ (winit::keyboard::KeyCode::Enter | winit::keyboard::KeyCode::NumpadEnter),
            ) = key.physical_key
        {
            bae_ui::set_keypad_enter(code == winit::keyboard::KeyCode::NumpadEnter);
        }
        #[cfg(target_os = "macos")]
        if matches!(
            event,
            WindowEvent::DroppedFile(_) | WindowEvent::DoubleTapGesture { .. }
        ) && let Some(pos) = open_documents::pointer_in_view(&view.window)
        {
            let pos = pos / view.egui_ctx.zoom_factor();
            view.egui_state
                .egui_input_mut()
                .events
                .push(egui::Event::PointerMoved(pos));
        }
        let before = view.egui_state.egui_input().events.len();
        let response = view.egui_state.on_window_event(&view.window, &event);
        // Numpad keys are their own keys in AE's shortcuts (#261).
        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && let winit::keyboard::PhysicalKey::Code(code) = key.physical_key
            && let Some(pad) = numpad::key(code, &key.logical_key)
        {
            // egui-winit sends no event for numpad `*`: the modifiers held now.
            let modifiers = numpad::current_modifiers(
                &view.egui_state.egui_input().events,
                view.egui_ctx.input(|i| i.modifiers),
            );
            numpad::rewrite(
                &mut view.egui_state.egui_input_mut().events,
                before,
                pad,
                key.state.is_pressed(),
                key.repeat,
                modifiers,
            );
        }
        tracing::trace!(
            event = egui_winit::short_window_event_description(&event),
            repaint = response.repaint,
            "window event"
        );
        if response.repaint {
            view.window.request_redraw();
        }
        match event {
            #[cfg(target_os = "macos")]
            WindowEvent::DoubleTapGesture { .. } => {
                bae_ui_motion::request_smart_zoom(&view.egui_ctx);
                view.window.request_redraw();
            }
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
            UserEvent::Menu(event, keys) => {
                let id = event.id.as_ref();
                // The key equivalent fired on a chord with more modifiers than
                // its own (⌃⌥8 on the ⌃8 item): the UI's keymap gets that press.
                let chord = self.menu.as_ref().and_then(|m| m.contributed_chord(id));
                if let Some(press) = menu::key_press_for_ui(chord, keys)
                    && let Some(view) = self.view.as_mut()
                {
                    view.egui_state.egui_input_mut().events.push(press);
                    view.window.request_redraw();
                    return;
                }
                let inv = self
                    .menu
                    .as_ref()
                    .and_then(|m| m.contributed_invocation(id))
                    .unwrap_or_else(|| menu::invocation(id));
                // A modal dialog is open: menu commands (and their key
                // equivalents) do not reach the project behind it; Quit still works.
                if self.workspace.modal_open(self.app.resources()) && inv.id != ids::QUIT {
                    return;
                }
                // Undo / Redo, Cut / Copy / Paste and Select All (their native
                // key equivalents too) while a text field is focused edit the
                // text, as in any Mac / Windows app.
                if menu::TEXT_EDIT_ACTIONS.contains(&inv.id.as_str())
                    && let Some(view) = self.view.as_mut()
                    && view.egui_ctx.text_edit_focused()
                {
                    let key = |key: egui::Key, shift: bool| egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers {
                            shift,
                            ..egui::Modifiers::COMMAND
                        },
                    };
                    let event = match inv.id.as_str() {
                        ids::UNDO => key(egui::Key::Z, false),
                        ids::REDO => key(egui::Key::Z, true),
                        "edit.cut" => egui::Event::Cut,
                        "edit.copy" => egui::Event::Copy,
                        "edit.paste" => {
                            egui::Event::Paste(view.egui_state.clipboard_text().unwrap_or_default())
                        }
                        _ => key(egui::Key::A, false),
                    };
                    view.egui_state.egui_input_mut().events.push(event);
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
        self.sync_prefs();
        self.sync_language();
        self.run_settings_requests();
        // OK in Keyboard Shortcuts: save the shortcuts now, not only at exit.
        if self.workspace.take_save_request() {
            self.save_window_state();
        }
        let autosave_at = self.run_autosave();
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
        if let Some(at) = autosave_at {
            deadline = Some(deadline.map_or(at, |d| d.min(at)));
        }
        if let Some(at) = self
            .app
            .resource::<bae_ui_motion::SharedSession>()
            .and_then(|s| s.lock().playback_deadline())
        {
            if at <= now {
                self.request_redraw();
            } else {
                deadline = Some(deadline.map_or(at, |d| d.min(at)));
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
