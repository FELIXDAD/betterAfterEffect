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

/// The workbench plus shell-owned windows.
pub struct Workspace {
    workbench: Workbench,
    show_about: bool,
    /// Preferences (app resource `Arc<PrefsStore>`; in memory when absent).
    prefs: Arc<bae_prefs::PrefsStore>,
    settings: bae_ui::settings::SettingsDialog,
}

impl Workspace {
    pub fn new(app: &mut App, state: WorkbenchState) -> Self {
        let prefs = app
            .resource::<Arc<bae_prefs::PrefsStore>>()
            .cloned()
            .unwrap_or_default();
        Self {
            workbench: Workbench::new(app, state),
            show_about: false,
            prefs,
            settings: bae_ui::settings::SettingsDialog::default()
                .with_host_buttons(&[REVEAL_PREFERENCES, "choose"]),
        }
    }

    /// Opens Preferences (macOS: Settings) on `page` (`general`, …).
    pub fn show_settings(&mut self, page: Option<&str>) {
        let current = self.prefs.current();
        self.settings.open(page, &current);
    }

    /// The Preferences window (acceptance tests, macOS run).
    #[cfg(all(test, target_os = "macos"))]
    pub fn settings_dialog(&mut self) -> &mut bae_ui::settings::SettingsDialog {
        &mut self.settings
    }

    /// Buttons of the Preferences window the shell runs ([`REVEAL_PREFERENCES`],
    /// `choose:<key>`).
    pub fn take_settings_requests(&mut self) -> Vec<String> {
        self.settings.take_requests()
    }

    /// Answers a Choose Folder… button.
    pub fn set_setting(&mut self, key: &str, value: String) {
        self.settings.set(key, value);
    }

    /// Ask for the user's role (first launch).
    /// A modal dialog (the welcome / role picker, Preferences, a module dialog
    /// such as Composition Settings) is open: it owns the keyboard and the menu
    /// commands wait.
    pub fn modal_open(&self, resources: &Resources) -> bool {
        self.workbench.modal_open(resources) || self.settings.is_open()
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
        let title = doc.title();
        let cx = WorkbenchContext {
            stage: doc.stage(),
            author: doc.author(),
            resources,
            title: &title,
            dirty: doc.is_dirty(),
        };
        self.workbench.set_external_modal(self.settings.is_open());
        let out = self.workbench.ui(ui, &cx, |ui| status_bar(ui, status));
        if let bae_ui::settings::Outcome::Ok(prefs) = self.settings.show(ui.ctx())
            && let Err(error) = self.prefs.set(prefs)
        {
            tracing::warn!("could not save preferences: {error}");
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
        (!self.workbench.handle(&inv, &cx)).then_some(inv)
    }

    /// The menus modules contributed (`Registry<MenuItemSpec>`), for the active
    /// workspace.
    pub fn contributed_menus(&self, app: &App) -> bae_ui::MenuTree {
        match app.get_registry::<bae_ui::MenuItemSpec>() {
            Some(registry) => {
                bae_ui::menu::build_menus(&registry.snapshot(), self.workbench.active_workspace())
            }
            None => bae_ui::MenuTree::default(),
        }
    }

    /// The effective keymap of the active workspace (menu shortcuts).
    pub fn keymap(&self) -> &bae_ui::Keymap {
        self.workbench.keymap()
    }

    /// Id of the active workspace.
    pub fn active_workspace(&self) -> &str {
        self.workbench.active_workspace()
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
        ui.label(small(status.gpu.clone()));
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
