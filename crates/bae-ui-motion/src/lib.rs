//! BAE motion workspace UI (workstream 12): the After Effects-style panels of the
//! Motion workspace, registered with the shell (`bae-ui`) through its Panel API.
//!
//! | Panel id | Panel |
//! |---|---|
//! | `project` | Project: folders, comps, footage and solids (AE columns) |
//! | `composition` | Composition viewer: active comp at the current time, selection and moving, zoom, safe areas, grid |
//! | `timeline` | Timeline: layer outline with switches and Parent & Link, properties with stopwatches and values, ruler, work area, CTI, layer bars, keyframes; the Graph Editor (`graph`, Shift+F3) |
//! | `properties` | Properties (AE 2024+): the selected layer's transform, text and shape fill |
//! | `effect_controls` | Effect Controls (F3) |
//! | `preview` | Preview: play/stop, frame stepping, loop |
//! | `render_queue` | Render Queue: queued comps, output format / path / frames, render on a background thread through 04's export pipeline, progress, stop, show in Finder |
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
pub mod catalog;
mod common;
pub mod demo;
pub mod dialogs;
mod gpu_preview;
mod graph;
mod i18n;
pub mod menus;
mod panels;
mod project;
mod props;
mod render;
pub mod render_queue;
pub mod session;
mod timeline;
mod viewer;

use bae_app::{AppBuilder, AppModule};
use bae_ui::{ActionHandler, ActionInfo, KeymapBindings, MenuItemSpec, PanelKind};

pub use dialogs::{MotionDialogs, SharedDialogs};
pub use gpu_preview::PreviewSources;
pub use panels::{EffectControlsPanel, PreviewPanel, PropertiesPanel};
pub use project::ProjectPanel;
pub use render_queue::{RenderQueuePanel, SharedRenderQueue};
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
        if app.resource::<SharedRenderQueue>().is_none() {
            app.insert_resource(SharedRenderQueue::default());
        }
        if app.resource::<SharedDialogs>().is_none() {
            app.insert_resource(SharedDialogs::default());
        }
        let dialogs: std::sync::Arc<dyn bae_ui::DialogHost> = std::sync::Arc::new(MotionDialogs);
        let _ = app.register_arc::<dyn bae_ui::DialogHost>("bae-ui-motion", dialogs);
        let owner = app.current_owner();
        let kinds = app.registry::<PanelKind>();
        let panels: [(&str, PanelKind); 7] = [
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
            (
                "render_queue",
                PanelKind::new("Render Queue", |_| Box::new(RenderQueuePanel::default())),
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
        let variants = app.registry::<bae_ui::CommandVariant>();
        variants.batch(|tx| {
            for (id, variant) in actions::variants() {
                let _ = tx.register(owner.clone(), id, variant);
            }
        });
        // Menus and the global handler: AE shortcuts and menu items work whichever
        // panel has focus.
        let handler: std::sync::Arc<dyn ActionHandler> = std::sync::Arc::new(menus::MotionActions);
        let _ = app.register_arc::<dyn ActionHandler>("bae-ui-motion", handler);
        let menu = app.registry::<MenuItemSpec>();
        menu.batch(|tx| {
            for (id, item) in menus::items() {
                if !tx.contains(id) {
                    let _ = tx.register(owner.clone(), id, item);
                }
            }
        });
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
#[cfg(test)]
mod timeline_tests;
