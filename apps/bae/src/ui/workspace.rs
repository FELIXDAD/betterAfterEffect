//! Placeholder workspace: an egui_dock area with the AE motion-workspace panel
//! names, and a status bar. Workstream 11 (`bae-ui`) replaces the dock and panels;
//! the status bar contract (GPU name, frame time) stays.

use bae_gpu::viewport::{ViewportTextureId, ViewportTextureRegistry};
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};

use crate::i18n::tr;

/// Placeholder panels (AE "Standard" workspace names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    /// Project panel.
    Project,
    /// Composition viewer (shows the viewport texture).
    Composition,
    /// Timeline.
    Timeline,
    /// Effect Controls.
    EffectControls,
    /// Info.
    Info,
}

impl Panel {
    fn title(self) -> &'static str {
        match self {
            Self::Project => tr("Project"),
            Self::Composition => tr("Composition"),
            Self::Timeline => tr("Timeline"),
            Self::EffectControls => tr("Effect Controls"),
            Self::Info => tr("Info"),
        }
    }
}

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

/// The placeholder workspace state.
pub struct Workspace {
    dock: DockState<Panel>,
    viewer: Option<ViewportTextureId>,
    show_about: bool,
}

impl Workspace {
    /// The default layout: Project | Composition | Effect Controls+Info over Timeline.
    pub fn new(viewer: Option<ViewportTextureId>) -> Self {
        Self {
            dock: default_layout(),
            viewer,
            show_about: false,
        }
    }

    /// Sets the viewport texture shown in the Composition panel.
    pub fn set_viewer(&mut self, viewer: Option<ViewportTextureId>) {
        self.viewer = viewer;
    }

    /// Restores the default layout ("Reset Workspace").
    pub fn reset_layout(&mut self) {
        self.dock = default_layout();
    }

    /// Opens the About window.
    pub fn show_about(&mut self) {
        self.show_about = true;
    }

    /// Builds the UI for one frame.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        status: &StatusInfo,
        viewports: Option<&ViewportTextureRegistry>,
    ) {
        egui::Panel::bottom("bae.status")
            .exact_size(22.0)
            .show(ui, |ui| status_bar(ui, status));
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let mut viewer = PanelViewer {
                viewer: self.viewer,
                viewports,
            };
            DockArea::new(&mut self.dock)
                .style(Style::from_egui(ui.style().as_ref()))
                .show_close_buttons(false)
                .show_add_buttons(false)
                .show_inside(ui, &mut viewer);
        });
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
    }
}

fn default_layout() -> DockState<Panel> {
    let mut dock = DockState::new(vec![Panel::Composition]);
    let surface = dock.main_surface_mut();
    let [center, _timeline] = surface.split_below(NodeIndex::root(), 0.62, vec![Panel::Timeline]);
    let [center, _project] = surface.split_left(center, 0.22, vec![Panel::Project]);
    let [_center, _effects] =
        surface.split_right(center, 0.72, vec![Panel::EffectControls, Panel::Info]);
    dock
}

fn status_bar(ui: &mut egui::Ui, status: &StatusInfo) {
    ui.horizontal_centered(|ui| {
        ui.label(egui::RichText::new(&status.gpu).small());
        ui.separator();
        ui.label(
            egui::RichText::new(format!("{} {:.2} ms", tr("frame"), status.frame_ms))
                .small()
                .monospace(),
        );
        ui.separator();
        ui.label(
            egui::RichText::new(format!(
                "{}×{} @{:.2}x",
                status.size_px[0], status.size_px[1], status.pixels_per_point
            ))
            .small()
            .monospace(),
        );
    });
}

struct PanelViewer<'a> {
    viewer: Option<ViewportTextureId>,
    viewports: Option<&'a ViewportTextureRegistry>,
}

impl TabViewer for PanelViewer<'_> {
    type Tab = Panel;

    fn id(&mut self, tab: &mut Panel) -> egui::Id {
        egui::Id::new(("bae.panel", *tab))
    }

    fn title(&mut self, tab: &mut Panel) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Panel) {
        match tab {
            Panel::Composition => self.composition(ui),
            _ => {
                ui.centered_and_justified(|ui| {
                    ui.weak(tr("Not implemented yet."));
                });
            }
        }
    }

    fn is_closeable(&self, _tab: &Panel) -> bool {
        false
    }
}

impl PanelViewer<'_> {
    fn composition(&self, ui: &mut egui::Ui) {
        let image = self.viewer.zip(self.viewports).and_then(|(id, registry)| {
            let handle = registry.display_handle(id)?;
            let texture = registry.get(id)?;
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
