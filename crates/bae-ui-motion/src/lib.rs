//! BAE motion workspace UI (workstream 12): the After Effects-style panels of the
//! Motion workspace, registered with the shell (`bae-ui`) through its Panel API.
//!
//! | Panel id | Panel |
//! |---|---|
//! | `project` | Project: folders, comps, footage and solids (AE columns) |
//! | `composition` | Composition viewer: active comp at the current time, selection and moving, zoom, safe areas, grid |
//! | `timeline` | Timeline: layer outline with switches and Parent & Link, properties with stopwatches and values, ruler, work area, CTI, layer bars, keyframes; the Graph Editor (`graph`, Shift+F3) |
//! | `properties` | Properties (AE 2024+): the selected layer's transform, text and shape fill |
//! | `effect_controls` | Effect Controls (F3): the selected layer's effects, controls from the effect definitions |
//! | `effects` | Effects & Presets: AE's effect categories, search, double-click or drag onto a layer to apply |
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
mod assist;
pub mod catalog;
mod common;
mod context_menu;
pub mod demo;
pub mod dialogs;
pub mod effects;
mod expr;
mod expr_menu;
mod gpu_preview;
mod graph;
mod i18n;
pub mod import;
pub mod menus;
mod panels;
mod prefs;
mod project;
mod props;
mod render;
pub mod render_queue;
pub mod session;
mod timeline;
pub mod tools;
mod viewer;

use bae_app::{AppBuilder, AppModule};
use bae_ui::{ActionHandler, ActionInfo, KeymapBindings, MenuItemSpec, PanelKind};

pub use dialogs::{MotionDialogs, SharedDialogs};
pub use gpu_preview::{PreviewEffects, PreviewSources};
pub use panels::{EffectControlsPanel, PreviewPanel, PropertiesPanel};
pub use project::ProjectPanel;
pub use render_queue::{RenderQueuePanel, SharedRenderQueue};
pub use session::{Session, SharedSession};
pub use timeline::TimelinePanel;
pub use tools::{SharedTools, Tool};
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
        bae_ui::i18n::register(tools::ZH);
        bae_ui::i18n::register(catalog::menu_zh());
        effects::register_i18n();
        if app.resource::<SharedSession>().is_none() {
            app.insert_resource(SharedSession::default());
        }
        if app.resource::<SharedTools>().is_none() {
            app.insert_resource(SharedTools::default());
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
        let panels: [(&str, PanelKind); 9] = [
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
                "effects",
                PanelKind::new("Effects & Presets", |_| {
                    Box::new(effects::browser::EffectsPanel::default())
                }),
            ),
            (
                "render_queue",
                PanelKind::new("Render Queue", |_| Box::new(RenderQueuePanel::default())),
            ),
            (
                "mask_interpolation",
                PanelKind::new("Mask Interpolation", |_| {
                    Box::new(assist::mask_interpolation::MaskInterpolationPanel::default())
                }),
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
            for (id, title) in actions::ACTIONS.iter().chain(effects::ACTIONS) {
                if !tx.contains(id) {
                    let _ = tx.register(owner.clone(), *id, ActionInfo::new(*title));
                }
            }
        });
        let _ = app.register::<KeymapBindings>("motion", actions::bindings());
        // Tools: the toolbar of the `ae` keymap, tool shortcuts and their handler.
        let _ = app.register::<KeymapBindings>("motion.tools", tools::keymap_bindings());
        actions.batch(|tx| {
            for (id, title) in tools::actions() {
                if !tx.contains(id) {
                    let _ = tx.register(owner.clone(), id, ActionInfo::new(title));
                }
            }
        });
        let shared_tools = app.resource::<SharedTools>().cloned().unwrap_or_default();
        let toolbar: std::sync::Arc<dyn bae_ui::Toolbar> =
            std::sync::Arc::new(tools::toolbar::AeToolbar::new(shared_tools));
        let _ = app.register_arc::<dyn bae_ui::Toolbar>(bae_ui::presets::AE_KEYMAP, toolbar);
        let tool_actions: std::sync::Arc<dyn ActionHandler> =
            std::sync::Arc::new(tools::ToolActions);
        let _ = app.register_arc::<dyn ActionHandler>("bae-ui-motion.tools", tool_actions);
        let variants = app.registry::<bae_ui::CommandVariant>();
        variants.batch(|tx| {
            for (id, variant) in actions::variants().into_iter().chain(tools::variants()) {
                let _ = tx.register(owner.clone(), id, variant);
            }
        });
        // Menus and the global handler: AE shortcuts and menu items work whichever
        // panel has focus.
        let handler: std::sync::Arc<dyn ActionHandler> = std::sync::Arc::new(menus::MotionActions);
        let _ = app.register_arc::<dyn ActionHandler>("bae-ui-motion", handler);
        let effect_actions: std::sync::Arc<dyn ActionHandler> =
            std::sync::Arc::new(effects::EffectActions);
        let _ = app.register_arc::<dyn ActionHandler>("bae-ui-motion.effects", effect_actions);
        let menu = app.registry::<MenuItemSpec>();
        menu.batch(|tx| {
            for (id, item) in menus::items() {
                if !tx.contains(&id) {
                    let _ = tx.register(owner.clone(), id, item);
                }
            }
        });
    }

    fn finish(&self, app: &mut AppBuilder) {
        // AE's shortcuts for the AE commands other modules handle (registered in
        // their `build`): bound now that every handler is known.
        let handled = bae_ui::menu::handled_actions(
            &app.registry::<dyn ActionHandler>().snapshot(),
            &app.registry::<ActionInfo>().snapshot(),
            &app.registry::<PanelKind>().snapshot(),
        );
        let taken: Vec<bae_ui::Binding> = app
            .registry::<KeymapBindings>()
            .snapshot()
            .iter()
            .filter(|e| e.item().keymap == bae_ui::presets::AE_KEYMAP)
            .flat_map(|e| e.item().bindings.clone())
            .collect();
        let late = actions::late_bindings(&handled, &taken);
        app.insert_resource(context_menu::HandledActions(std::sync::Arc::new(
            handled.clone(),
        )));
        if !late.is_empty() {
            let _ = app.register::<KeymapBindings>(
                "motion.ae",
                KeymapBindings::new(bae_ui::presets::AE_KEYMAP, late)
                    .for_workspace(menus::WORKSPACE),
            );
        }
        // Layer sources other modules registered (05's shape layers …) for the
        // viewer's GPU preview.
        let sources = app.registry::<dyn bae_render::SourceProvider>();
        app.insert_resource(PreviewSources(sources));
        // Effects (07's built-ins, plugins) for the viewer and the render queue.
        let effect_impls = app.registry::<dyn bae_render::Effect>();
        app.insert_resource(PreviewEffects(effect_impls));
        // Effect definitions for the Effect menu and the effect panels.
        let registries = effects::EffectRegistries {
            defs: app.registry::<bae_model::EffectDef>(),
            infos: app.registry::<bae_effects::EffectInfo>(),
        };
        // Process-wide copy for code without resources (timeline labels). An
        // app without effect modules (tests) must not replace a real one.
        if !registries.defs.is_empty() {
            effects::set_global(registries.clone());
        }
        app.insert_resource(registries);
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod timeline_tests;
