//! BAE UI shell: theme tokens, docking frames, title bar, icons, AE-style widgets,
//! panels, workspaces and keyboard shortcuts.
//!
//! The shell only renders and handles interaction; it keeps no project data
//! (see docs/ARCHITECTURE.md, principle 1). Feature crates plug in through
//! `bae_app` registries, from their `AppModule::build`:
//!
//! | Registry | Id | What |
//! |---|---|---|
//! | `Registry<PanelKind>` | panel kind (`"timeline"`) | a panel type and its factory |
//! | `Registry<WorkspacePreset>` | workspace (`"motion"`) | default layout + keymap id |
//! | `Registry<Keymap>` | keymap (`"ae"`) | shortcuts → command ids |
//! | `Registry<ActionInfo>` | command id (`"edit.undo"`) | title of a UI-level action |
//!
//! Add [`UiModule`] (the shell does) so the built-in actions are registered and
//! layouts never show an unknown panel. See `docs/spec/ui-shell.md`.

pub mod action;
pub mod dock;
pub mod i18n;
pub mod icons;
pub mod keymap;
pub mod panel;
pub mod theme;
pub mod titlebar;
pub mod widgets;
pub mod workspace;

use bae_app::{AppBuilder, AppModule};

pub use action::{ActionInfo, Invocation};
pub use dock::{Dock, DropZone, Frame, FrameId, Layout, Node, PanelHost, PanelId, TabInfo};
pub use i18n::{Lang, tr};
pub use icons::Icon;
pub use keymap::{Binding, Chord, Conflict, Keymap, KeymapOverrides};
pub use panel::{Panel, PanelContext, PanelKind, PanelRequests, PanelSpawn};
pub use workspace::WorkspacePreset;

/// Registers the shell's own actions and, after every module has built, a
/// placeholder for each panel a workspace layout uses but no module registered.
#[derive(Debug, Default)]
pub struct UiModule;

impl AppModule for UiModule {
    fn name(&self) -> &str {
        "bae-ui"
    }

    fn build(&self, app: &mut AppBuilder) {
        let owner = app.current_owner();
        let actions = app.registry::<ActionInfo>();
        actions.batch(|tx| {
            for (id, title) in action::SHELL_ACTIONS {
                if !tx.contains(id) {
                    let _ = tx.register(owner.clone(), *id, ActionInfo::new(*title));
                }
            }
        });
    }

    fn finish(&self, app: &mut AppBuilder) {
        let owner = app.current_owner();
        let presets = app.registry::<WorkspacePreset>().snapshot();
        let kinds = app.registry::<PanelKind>();
        kinds.batch(|tx| {
            for preset in presets.iter() {
                for id in preset.item().panel_ids() {
                    let kind = id.kind();
                    if !tx.contains(kind) {
                        let title = placeholder_title(kind);
                        let _ = tx.register(owner.clone(), kind, PanelKind::placeholder(title));
                    }
                }
            }
        });
    }
}

/// English titles of the AE panels the default workspaces use, for placeholders.
fn placeholder_title(kind: &str) -> String {
    let known = [
        ("project", "Project"),
        ("composition", "Composition"),
        ("timeline", "Timeline"),
        ("effect_controls", "Effect Controls"),
        ("effects", "Effects & Presets"),
        ("preview", "Preview"),
        ("properties", "Properties"),
        ("align", "Align"),
        ("audio", "Audio"),
        ("info", "Info"),
        ("character", "Character"),
        ("paragraph", "Paragraph"),
        ("render_queue", "Render Queue"),
        ("tools", "Tools"),
        ("history", "History"),
    ];
    known
        .iter()
        .find(|(k, _)| *k == kind)
        .map_or_else(|| kind.to_owned(), |(_, title)| (*title).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Motion;
    impl AppModule for Motion {
        fn build(&self, app: &mut AppBuilder) {
            app.register(
                "motion",
                WorkspacePreset::new(
                    "Motion",
                    "ae",
                    Node::row(vec![
                        (1.0, Node::tabs(&["project", "custom_thing"])),
                        (3.0, Node::tabs(&["timeline:main"])),
                    ]),
                ),
            )
            .unwrap();
            app.register("timeline", PanelKind::placeholder("Real Timeline"))
                .unwrap();
        }
    }

    #[test]
    fn module_fills_missing_panels_and_registers_actions() {
        let mut b = AppBuilder::new();
        b.add_module(UiModule).add_module(Motion);
        let mut app = b.build();
        let kinds = app.registry::<PanelKind>();
        assert_eq!(kinds.get("project").unwrap().title, "Project");
        assert_eq!(kinds.get("custom_thing").unwrap().title, "custom_thing");
        assert_eq!(kinds.get("timeline").unwrap().title, "Real Timeline");
        assert_eq!(kinds.len(), 3);
        let actions = app.registry::<ActionInfo>();
        assert_eq!(actions.get(action::ids::UNDO).unwrap().title, "Undo");
    }
}
