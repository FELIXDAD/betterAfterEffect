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

/// The workbench plus shell-owned windows.
pub struct Workspace {
    workbench: Workbench,
    show_about: bool,
}

impl Workspace {
    pub fn new(app: &mut App, state: WorkbenchState) -> Self {
        Self {
            workbench: Workbench::new(app, state),
            show_about: false,
        }
    }

    /// Model commands listed in the shortcut editor.
    pub fn set_extra_commands(&mut self, commands: Vec<(String, String)>) {
        self.workbench.set_extra_commands(commands);
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
        let out = self.workbench.ui(ui, &cx, |ui| status_bar(ui, status));
        if self.show_about {
            let mut open = true;
            egui::Window::new(tr("About BAE"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.heading(bae_platform::APP_NAME);
                    ui.label(bae_platform::version_line());
                    ui.label(tr("GPU-first motion graphics and compositing."));
                    ui.label("Apache-2.0");
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
    ui.label(small(status.gpu.clone()));
    ui.separator();
    ui.label(small(format!("{} {:.2} ms", tr("frame"), status.frame_ms)));
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
