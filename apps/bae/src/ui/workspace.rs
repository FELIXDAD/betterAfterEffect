//! The window contents: `bae-ui`'s workbench (title bar, toolbar, workspace bar,
//! dock with the registered panels) plus the shell's status strip, About window and
//! the composition panel that shows the viewport texture until workstream 12
//! registers the real one.

use std::sync::Arc;

use bae_app::{App, Resources};
use bae_gpu::viewport::{ViewportTextureId, ViewportTextureRegistry};
use bae_ui::{
    Invocation, Panel, PanelContext, PanelKind, Workbench, WorkbenchContext, WorkbenchState,
};

use crate::document::Document;
use crate::i18n::tr;

/// What the status bar shows.
#[derive(Debug, Clone, Default)]
pub struct StatusInfo {
    /// GPU summary (`name (backend, type)`).
    pub gpu: String,
    /// Smoothed CPU time of a UI frame, in milliseconds.
    pub frame_ms: f32,
    /// Surface size in physical pixels.
    pub size_px: [u32; 2],
    /// Display scale.
    pub pixels_per_point: f32,
    /// Frames rendered so far.
    pub frames: u64,
    /// Why the last command failed, in the UI language (cleared by the next
    /// change).
    pub notice: String,
}

/// The About window's version line: [`bae_platform::version_line`] with the
/// build kind translated (the line itself stays English for logs).
fn about_version() -> String {
    let build = if cfg!(debug_assertions) {
        tr("debug")
    } else {
        tr("release")
    };
    format!(
        "{} {} ({}-{}, {build})",
        bae_platform::APP_NAME,
        bae_platform::VERSION,
        std::env::consts::ARCH,
        std::env::consts::OS
    )
}

/// The GPU in the status bar: `name (backend, kind)`, the kind translated
/// (backend and adapter names are proper nouns).
pub fn gpu_summary(gpu: &bae_gpu::GpuContext) -> String {
    let info = gpu.adapter_info();
    format!(
        "{} ({}, {})",
        info.name,
        bae_gpu::backend_name(info.backend),
        tr(gpu_kind(info.device_type))
    )
}

/// English name of a kind of GPU (translated with `tr`).
pub(crate) fn gpu_kind(kind: bae_gpu::wgpu::DeviceType) -> &'static str {
    use bae_gpu::wgpu::DeviceType;
    match kind {
        DeviceType::DiscreteGpu => "Discrete GPU",
        DeviceType::IntegratedGpu => "Integrated GPU",
        DeviceType::VirtualGpu => "Virtual GPU",
        DeviceType::Cpu => "Software Rendering",
        DeviceType::Other => "Other GPU",
    }
}

/// The status bar text for a failed command.
pub fn failure_notice(error: &bae_model::CommandError) -> String {
    match error {
        bae_model::CommandError::Model(bae_model::ModelError::LayerLocked { name, .. }) => {
            tr("Layer “{name}” is locked. Unlock it first.").replace("{name}", name)
        }
        other => tr("The last action failed: {reason}").replace("{reason}", &other.to_string()),
    }
}

/// The status bar template of a plugin notice (`{plugin}`, `{detail}`).
pub fn plugin_notice_template(kind: bae_plugin::NoticeKind) -> &'static str {
    use bae_plugin::NoticeKind as K;
    match kind {
        K::LoadFailed => "Plugin {plugin} could not be loaded and is disabled: {detail}",
        K::RenderFailed => "Plugin {plugin} failed to render a frame: {detail}",
        K::Crashed => "Plugin {plugin} crashed and will be restarted: {detail}",
        K::Disabled => {
            "Plugin {plugin} stopped working and was disabled for this session: {detail}"
        }
    }
}

/// The status bar text for a plugin notice (load failure, crash, disabled).
pub fn plugin_notice(notice: &bae_plugin::Notice) -> String {
    tr(plugin_notice_template(notice.kind))
        .replace("{plugin}", &notice.plugin)
        .replace("{detail}", &notice.detail)
}

/// The viewport texture the composition placeholder shows (an app resource).
#[derive(Debug, Clone, Copy)]
pub struct PlaceholderViewer(pub ViewportTextureId);

/// Registers the shell's composition viewer unless a workspace crate registered a
/// real one (it replaces `bae-ui`'s "not implemented" placeholder).
pub fn register_panels(app: &mut bae_app::AppBuilder) {
    let registry = app.registry::<PanelKind>();
    let placeholder = registry
        .snapshot()
        .entry("composition")
        .is_none_or(|e| e.owner() == &bae_app::Owner::module("bae-ui"));
    if placeholder {
        let kind = PanelKind::new("Composition", |_| Box::new(CompositionViewer));
        let _ = registry.replace(app.current_owner(), "composition", Arc::new(kind));
    }
}

/// A row of the Keyboard Shortcuts window (tests).
#[cfg(test)]
#[derive(Clone, Debug)]
pub struct ShortcutRow {
    pub id: String,
    pub params: serde_json::Value,
    /// English command title.
    pub title: String,
    /// English title of the panel / workspace argument (or a raw id when unknown).
    pub arg: Option<String>,
    /// The title in the current language, as the window shows it.
    pub shown: String,
}

/// Preferences > Startup & Repair > Reveal Preferences in Explorer / Finder.
pub const REVEAL_PREFERENCES: &str = "startup_repair.reveal_preferences_in_explorer";
pub const MEMORY_DETAILS: &str = "memory_cpu.show_memory_usage_details";
pub const GPU_INFORMATION: &str = "memory_cpu.gpu_information";

pub(crate) fn hardware_information(
    id: &str,
    resources: &Resources,
) -> Option<(&'static str, String)> {
    match id {
        MEMORY_DETAILS => {
            let show = |n: Option<u64>| {
                n.map(|n| format!("{:.1} MB", n as f64 / (1_u64 << 20) as f64))
                    .unwrap_or_else(|| "—".to_owned())
            };
            let body = format!(
                "{} {}\nBAE ({})   {} {}",
                tr("Installed RAM:"),
                show(bae_platform::memory::total_bytes()),
                std::process::id(),
                tr("Resident Memory:"),
                show(bae_platform::memory::resident_bytes())
            );
            Some((tr("Memory Usage Details"), body))
        }
        GPU_INFORMATION => {
            let body = resources
                .get::<Arc<bae_gpu::GpuContext>>()
                .map(|gpu| {
                    let info = gpu.adapter_info();
                    let mut lines = vec![gpu_summary(gpu)];
                    if !info.driver.is_empty() {
                        lines.push(format!("{} {}", tr("Driver:"), info.driver));
                    }
                    if !info.driver_info.is_empty() {
                        lines.push(info.driver_info.clone());
                    }
                    lines.push(format!(
                        "{} {} px",
                        tr("Maximum Texture Dimension:"),
                        gpu.limits().max_texture_dimension_2d
                    ));
                    lines.join("\n")
                })
                .unwrap_or_else(|| tr("No active GPU device.").to_owned());
            Some((tr("GPU Information"), body))
        }
        _ => None,
    }
}

/// The workbench plus shell-owned windows.
pub struct Workspace {
    workbench: Workbench,
    show_about: bool,
    home: bool,
    /// Preferences (app resource `Arc<PrefsStore>`; in memory when absent).
    prefs: Arc<bae_prefs::PrefsStore>,
    settings: bae_ui::settings::SettingsDialog,
    audio_page_visible: bool,
}

impl Workspace {
    pub fn new(app: &mut App, state: WorkbenchState) -> Self {
        let prefs = app
            .resource::<Arc<bae_prefs::PrefsStore>>()
            .cloned()
            .unwrap_or_default();
        bae_ui_motion::tools::configure_camera_keys(app, &prefs);
        if let Some(session) = app.resource::<bae_ui_motion::SharedSession>().cloned() {
            let apply = move |p: &bae_prefs::Preferences| {
                session.configure_output(
                    p.str("audio.audio_default_output"),
                    p.str("audio.audio_latency_ms")
                        .parse()
                        .expect("validated latency"),
                );
                session.lock().set_audio_mapping(
                    p.str("audio.audio_map_left") == "channel_2",
                    p.str("audio.audio_map_right") == "channel_1",
                );
            };
            apply(&prefs.current());
            prefs.subscribe(apply);
        }
        let mut workbench = Workbench::new(app, state);
        // Keyboard Shortcuts presets are files in the config folder (AE: aeks/).
        workbench.set_shortcut_presets_dir(
            app.resource::<bae_platform::AppDirs>()
                .map(|d| d.config.join(bae_ui::workbench::SHORTCUT_PRESETS_FOLDER)),
        );
        Self {
            workbench,
            show_about: false,
            home: false,
            audio_page_visible: false,
            prefs,
            settings: bae_ui::settings::SettingsDialog::default()
                .with_cache_root(
                    app.resource::<bae_platform::AppDirs>()
                        .map(|dirs| dirs.cache.clone()),
                )
                .with_installed_ram(bae_platform::memory::total_bytes())
                .with_host_buttons(&[
                    "appearance.reset_coachmarks",
                    REVEAL_PREFERENCES,
                    "startup_repair.start_in_safe_mode",
                    MEMORY_DETAILS,
                    GPU_INFORMATION,
                    "disk.empty_disk_cache",
                    "choose",
                ]),
        }
    }

    /// Opens Preferences (macOS: Settings) on `page` (`general`, …).
    pub fn show_settings(&mut self, page: Option<&str>) {
        let current = self.prefs.current();
        self.settings.open(page, &current);
    }

    pub fn settings_draft(&self) -> &bae_prefs::Preferences {
        self.settings.draft()
    }

    /// The Preferences window for application acceptance tests.
    #[cfg(test)]
    pub fn settings_dialog(&mut self) -> &mut bae_ui::settings::SettingsDialog {
        &mut self.settings
    }

    /// Buttons of the Preferences window the shell runs ([`REVEAL_PREFERENCES`],
    /// `choose:<key>`).
    pub fn take_settings_requests(&mut self) -> Vec<String> {
        let mut requests = self.settings.take_requests();
        requests.retain(|id| {
            if id == "appearance.reset_coachmarks" {
                self.workbench.reset_coachmarks();
                return false;
            }
            true
        });
        requests
    }

    pub fn cache_free_space(&mut self, dir: &std::path::Path, free: u64) {
        self.settings.set_cache_free_space(dir, free);
    }

    /// Answers a Choose Folder… button.
    pub fn set_setting(&mut self, key: &str, value: String) {
        self.settings.set(key, value);
    }

    /// Whether Home or another dialog currently owns keyboard input.
    pub fn modal_open(&self, resources: &Resources) -> bool {
        self.home || self.workbench.modal_open(resources) || self.settings.is_open()
    }

    /// Home gives way to native menu commands. Other dialogs retain their modal guard.
    pub fn accepts_native_menu(&mut self, resources: &Resources, inv: &Invocation) -> bool {
        self.dismiss_home();
        inv.id == bae_ui::action::ids::QUIT || !self.modal_open(resources)
    }

    /// Close the startup Home screen as soon as a command or project takes its place.
    pub fn dismiss_home(&mut self) {
        if self.home {
            self.home = false;
            // Native events can arrive before another frame refreshes the workbench.
            self.workbench.set_external_modal(self.settings.is_open());
        }
    }

    pub fn start_without_project(&mut self, first_run: bool) {
        if self
            .prefs
            .current()
            .bool("startup_repair.enable_home_screen")
        {
            if first_run {
                self.show_role_picker();
            } else {
                self.home = true;
            }
        }
    }

    pub fn show_role_picker(&mut self) {
        self.workbench.show_role_picker();
    }

    /// Model commands listed in the shortcut editor.
    pub fn set_extra_commands(&mut self, commands: Vec<(String, String)>) {
        self.workbench.set_extra_commands(commands);
    }

    /// Every row of the Keyboard Shortcuts window: command id, parameters, the
    /// English title parts (command, argument) and the title as shown now.
    #[cfg(test)]
    pub fn shortcut_rows(&self) -> Vec<ShortcutRow> {
        self.workbench
            .shortcut_commands()
            .into_iter()
            .map(|e| {
                let (title, arg) = self.workbench.command_title_parts(&e.id, &e.params);
                let shown = self.workbench.command_title(&e.id, &e.params);
                ShortcutRow {
                    id: e.id,
                    params: e.params,
                    title,
                    arg,
                    shown,
                }
            })
            .collect()
    }

    /// Where Keyboard Shortcuts keeps its presets (acceptance tests, macOS run:
    /// a temporary folder).
    #[cfg(all(test, target_os = "macos"))]
    pub fn set_shortcut_presets_dir(&mut self, dir: Option<std::path::PathBuf>) {
        self.workbench.set_shortcut_presets_dir(dir);
    }

    /// Whether the state should be saved now (OK in Keyboard Shortcuts).
    pub fn take_save_request(&mut self) -> bool {
        self.workbench.take_save_request()
    }

    /// The state to persist between sessions.
    pub fn state(&mut self) -> WorkbenchState {
        self.workbench.state()
    }

    /// Opens the About window.
    pub fn show_about(&mut self) {
        self.show_about = true;
    }

    /// Builds the UI for one frame; returns the invocations it produced.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        status: &StatusInfo,
        doc: &Document,
        resources: &Resources,
    ) -> Vec<Invocation> {
        // egui uses these timings for hover tips on both enabled and disabled controls.
        let show_tips = self.prefs.current().bool("appearance.show_tool_tips");
        let interaction = &ui.ctx().global_style().interaction;
        if show_tips == interaction.tooltip_delay.is_infinite() {
            let defaults = egui::style::Interaction::default();
            ui.ctx().global_style_mut(|style| {
                style.interaction.tooltip_delay = if show_tips {
                    defaults.tooltip_delay
                } else {
                    f32::INFINITY
                };
                style.interaction.tooltip_grace_time = if show_tips {
                    defaults.tooltip_grace_time
                } else {
                    0.0
                };
            });
        }
        if let Some(session) = resources.get::<bae_ui_motion::SharedSession>() {
            let audio_page = self.settings.is_open() && self.settings.page() == "audio";
            if audio_page && !self.audio_page_visible {
                session.refresh_output_devices();
            }
            self.audio_page_visible = audio_page;
            let mut s = session.lock();
            s.sync_playback(doc.stage());
            s.tick(std::time::Instant::now());
            if audio_page {
                let devices = s.output_devices();
                self.settings.set_audio_output(
                    s.audio_output_info(),
                    s.audio_buffer_ms(),
                    &devices.default_name,
                    devices.outputs.clone(),
                );
                if s.audio_device_pending() {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(50));
                }
            }
        }
        let title = doc.title();
        let cx = WorkbenchContext {
            stage: doc.stage(),
            author: doc.author(),
            resources,
            title: &title,
            dirty: doc.is_dirty(),
        };
        self.workbench
            .set_external_modal(self.settings.is_open() || self.home);
        let mut out = self.workbench.ui(ui, &cx, |ui| status_bar(ui, status));
        if self.home {
            let modal = egui::Modal::new(egui::Id::new("bae-home")).show(ui.ctx(), |ui| {
                ui.heading(tr("Welcome to BAE"));
                ui.horizontal(|ui| {
                    for (label, id) in [
                        ("New Project", bae_ui::action::ids::NEW_PROJECT),
                        ("Open Project", bae_ui::action::ids::OPEN_PROJECT),
                    ] {
                        if ui.button(tr(label)).clicked() {
                            out.push(Invocation::new(id));
                            self.dismiss_home();
                        }
                    }
                });
            });
            if modal.should_close() {
                self.dismiss_home();
            }
        }
        if self.settings.is_open() {
            let outcome = self.settings.show(ui.ctx());
            if outcome == bae_ui::settings::Outcome::None {
                if let Some(draft) = self.settings.take_display_preview() {
                    self.prefs.preview_display(Some(draft));
                    bae_ui::settings::apply(draft);
                    ui.ctx().request_repaint();
                }
            } else {
                self.prefs.preview_display(None);
                if let bae_ui::settings::Outcome::Ok(prefs) = outcome
                    && let Err(error) = self.prefs.set(prefs)
                {
                    tracing::warn!("could not save preferences: {error}");
                }
                bae_ui::settings::apply(&self.prefs.current());
                ui.ctx().request_repaint();
            }
        }
        if self.show_about {
            let mut open = true;
            egui::Window::new(tr("About BAE"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.heading(bae_platform::APP_NAME);
                    ui.label(about_version());
                    ui.label(tr("GPU-first motion graphics and compositing."));
                    ui.label(tr("License: {license}").replace("{license}", "Apache-2.0"));
                });
            self.show_about = open;
        }
        out
    }

    /// Lets the focused panel and the workbench handle `inv`; returns it back if the
    /// shell has to (undo/redo, application actions, model commands).
    pub fn handle(
        &mut self,
        inv: Invocation,
        doc: &Document,
        resources: &Resources,
    ) -> Option<Invocation> {
        let title = String::new();
        let cx = WorkbenchContext {
            stage: doc.stage(),
            author: doc.author(),
            resources,
            title: &title,
            dirty: false,
        };
        if self.workbench.handle(&inv, &cx) {
            return None;
        }
        let mut inv = inv;
        if matches!(inv.id.as_str(), "attr.set_at" | "attr.set_many")
            && resources
                .get::<bae_ui_motion::SharedSession>()
                .is_some_and(|s| s.lock().auto_key)
        {
            inv.params["auto_key"] = true.into();
        }
        Some(inv)
    }

    /// The menus modules contributed (`Registry<MenuItemSpec>`), for the active
    /// workspace.
    pub fn contributed_menus(&self, app: &App) -> bae_ui::MenuTree {
        match app.get_registry::<bae_ui::MenuItemSpec>() {
            // Entries AE sorts by name follow the display language.
            Some(registry) => bae_ui::menu::build_menus_localized(
                &registry.snapshot(),
                self.workbench.active_workspace(),
                &|t| bae_ui::tr(t).to_owned(),
            ),
            None => bae_ui::MenuTree::default(),
        }
    }

    /// The action ids the app's modules handle (`bae_ui::menu::handled_actions`):
    /// menu items of other actions stay greyed.
    pub fn handled_actions(&self, app: &App) -> std::collections::HashSet<String> {
        let handlers = app.get_registry::<dyn bae_ui::ActionHandler>();
        let infos = app.get_registry::<bae_ui::ActionInfo>();
        let panels = app.get_registry::<bae_ui::PanelKind>();
        match (handlers, infos, panels) {
            (Some(h), Some(i), Some(p)) => {
                bae_ui::menu::handled_actions(&h.snapshot(), &i.snapshot(), &p.snapshot())
            }
            _ => Default::default(),
        }
    }

    /// Gives the panel `kind` the keyboard focus (tests: the shortcut sweep).
    #[cfg(test)]
    pub fn focus_panel(&mut self, kind: &str) {
        if let Some(id) = self.open_panels().into_iter().find(|p| p.kind() == kind) {
            self.workbench.dock_mut().focus_panel(&id);
        }
    }

    /// The panel kind that has the keyboard focus.
    #[cfg(test)]
    pub fn focused_kind(&self) -> Option<String> {
        self.workbench.focused_panel().map(|p| p.kind().to_owned())
    }

    /// Panels open in the dock (Window menu check marks).
    pub fn open_panels(&self) -> Vec<bae_ui::PanelId> {
        self.workbench.dock().layout.panel_ids()
    }

    /// The effective keymap of the active workspace (menu shortcuts).
    #[cfg(target_os = "macos")]
    pub fn set_standard_system_shortcuts(&mut self, standard: bool) {
        self.workbench.set_standard_system_shortcuts(standard);
    }

    pub fn keymap(&self) -> &bae_ui::Keymap {
        self.workbench.keymap()
    }

    /// Remember the workspace independently of project edits and undo.
    pub fn remember_project_workspace(&self, doc: &mut Document) {
        doc.set_manifest_field("workspace", Some(self.active_workspace().into()));
    }

    pub fn restore_project_workspace(&mut self, doc: &Document) {
        if !self
            .prefs
            .current()
            .bool("project.preserve_current_workspace_on_open")
            && let Some(id) = doc.manifest_field("workspace").and_then(|v| v.as_str())
        {
            self.workbench.switch_workspace(id);
        }
    }

    /// Id of the active workspace.
    pub fn active_workspace(&self) -> &str {
        self.workbench.active_workspace()
    }

    /// English title of the active workspace.
    pub fn active_workspace_title(&self) -> &str {
        self.workspace_title(self.workbench.active_workspace())
            .unwrap_or("")
    }

    pub fn workspace_title(&self, workspace: &str) -> Option<&str> {
        self.workbench
            .workspaces()
            .find(|(id, _)| *id == workspace)
            .map(|(_, p)| p.title.as_str())
    }

    /// Workspace ids and English titles in bar order (Window menu).
    pub fn workspaces(&self) -> Vec<(String, String)> {
        self.workbench
            .workspaces()
            .map(|(id, p)| (id.to_owned(), p.title.clone()))
            .collect()
    }
}

fn status_bar(ui: &mut egui::Ui, status: &StatusInfo) {
    let small = |s: String| {
        egui::RichText::new(s)
            .size(10.5)
            .color(bae_ui::theme::color::TEXT_DIM)
    };
    if !status.notice.is_empty() {
        ui.label(
            egui::RichText::new(&status.notice)
                .size(10.5)
                .color(bae_ui::theme::color::TEXT_STRONG),
        );
        ui.separator();
    }
    if !status.gpu.is_empty() {
        let adapter = ui.label(small(status.gpu.clone()));
        bae_ui::a11y::value_source(&adapter, bae_ui::a11y::GPU_STATUS);
        ui.separator();
    }
    ui.label(small(
        tr("frame {ms} ms").replace("{ms}", &format!("{:.2}", status.frame_ms)),
    ));
    ui.separator();
    ui.label(small(format!(
        "{}×{} @{:.2}x",
        status.size_px[0], status.size_px[1], status.pixels_per_point
    )));
}

/// Shows the published viewport texture, fitted into the panel.
struct CompositionViewer;

impl Panel for CompositionViewer {
    fn tab(&self, _cx: &PanelContext<'_>) -> bae_ui::TabInfo {
        bae_ui::TabInfo::new(tr("Composition"))
            .label(bae_ui::theme::label_color(15))
            .viewer()
    }

    fn ui(&mut self, ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
        let image = cx
            .resources
            .get::<PlaceholderViewer>()
            .zip(cx.resources.get::<Arc<ViewportTextureRegistry>>())
            .and_then(|(viewer, registry)| {
                let handle = registry.display_handle(viewer.0)?;
                let texture = registry.get(viewer.0)?;
                Some((handle, texture.size))
            });
        match image {
            Some((handle, [w, h])) => {
                // Fit the comp into the panel, keeping its aspect ratio.
                let available = ui.available_size();
                let scale = (available.x / w as f32)
                    .min(available.y / h as f32)
                    .min(1.0);
                let size = egui::vec2(w as f32, h as f32) * scale.max(0.01);
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new((egui::TextureId::User(handle), size)));
                });
            }
            None => {
                ui.centered_and_justified(|ui| {
                    ui.weak(tr("Composition viewer: render output appears here."));
                });
            }
        }
    }
}
