//! BAE motion workspace UI (workstream 12): the After Effects-style panels of the
//! Motion workspace, registered with the shell (`bae-ui`) through its Panel API.
//!
//! | Panel id | Panel |
//! |---|---|
//! | `project` | Project: folders, comps, footage and solids (AE columns) |
//! | `composition` | Composition viewer: active comp at the current time, selection and moving, zoom, safe areas, grid |
//! | `timeline` | Timeline: layer outline with switches and Parent & Link, properties with stopwatches and values, ruler, work area, CTI, layer bars, keyframes |
//! | `properties` | Properties (AE 2024+): the selected layer's transform, text and shape fill |
//! | `effect_controls` | Effect Controls (F3) |
//! | `preview` | Preview: play/stop, frame stepping, loop |
//!
//! Panels share a [`SharedSession`] app resource (active comp, current time,
//! selection, playback); it holds view state only. All project changes are
//! commands (`bae-motion` and `bae-model`), run by the shell after the frame.
//! AE shortcuts are added to the shell's `ae` keymap ([`actions::bindings`]).
//!
//! The viewer renders through 04's GPU compositor when the app has a GPU context
//! (`gpu_preview`), and falls back to a simplified vector preview ([`render`])
//! without one (tests, headless hosts).

pub mod actions;
mod common;
pub mod demo;
mod gpu_preview;
mod i18n;
mod panels;
mod project;
mod props;
mod render;
pub mod session;
mod timeline;
mod viewer;

use bae_app::{AppBuilder, AppModule};
use bae_ui::{ActionInfo, KeymapBindings, PanelKind};

pub use gpu_preview::PreviewSources;
pub use panels::{EffectControlsPanel, PreviewPanel, PropertiesPanel};
pub use project::ProjectPanel;
pub use session::{Session, SharedSession};
pub use timeline::TimelinePanel;
pub use viewer::CompositionViewer;

/// App module: the motion commands ([`bae_motion::MotionModule`]) plus the panels,
/// shortcuts and session of the Motion workspace.
#[derive(Debug, Default)]
pub struct MotionUiModule;

impl AppModule for MotionUiModule {
    fn name(&self) -> &str {
        "bae-ui-motion"
    }

    fn build(&self, app: &mut AppBuilder) {
        app.add_module(bae_motion::MotionModule);
        bae_ui::i18n::register(i18n::ZH);
        if app.resource::<SharedSession>().is_none() {
            app.insert_resource(SharedSession::default());
        }
        let owner = app.current_owner();
        let kinds = app.registry::<PanelKind>();
        let panels: [(&str, PanelKind); 6] = [
            (
                "project",
                PanelKind::new("Project", |_| Box::new(ProjectPanel::default())),
            ),
            (
                "composition",
                PanelKind::new("Composition", |spawn| {
                    Box::new(CompositionViewer::new(spawn.state))
                }),
            ),
            (
                "timeline",
                PanelKind::new("Timeline", |_| Box::new(TimelinePanel::default())),
            ),
            (
                "properties",
                PanelKind::new("Properties", |_| Box::new(PropertiesPanel::default())),
            ),
            (
                "effect_controls",
                PanelKind::new("Effect Controls", |_| {
                    Box::new(EffectControlsPanel::default())
                }),
            ),
            (
                "preview",
                PanelKind::new("Preview", |_| Box::new(PreviewPanel)),
            ),
        ];
        kinds.batch(|tx| {
            for (id, kind) in panels {
                // Replace the shell's placeholders if they were registered first.
                let _ = tx.replace(owner.clone(), id, std::sync::Arc::new(kind));
            }
        });
        let actions = app.registry::<ActionInfo>();
        actions.batch(|tx| {
            for (id, title) in actions::ACTIONS {
                if !tx.contains(id) {
                    let _ = tx.register(owner.clone(), *id, ActionInfo::new(*title));
                }
            }
        });
        let _ = app.register::<KeymapBindings>("motion", actions::bindings());
    }

    fn finish(&self, app: &mut AppBuilder) {
        // Layer sources other modules registered (05's shape layers …) for the
        // viewer's GPU preview.
        let sources = app.registry::<dyn bae_render::SourceProvider>();
        app.insert_resource(PreviewSources(sources));
    }
}

#[cfg(test)]
mod tests;
