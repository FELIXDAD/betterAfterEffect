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
//! | `Registry<KeymapBindings>` | any (`"motion.timeline"`) | shortcuts a module adds to a keymap |
//! | `Registry<ActionInfo>` | command id (`"edit.undo"`) | title of a UI-level action |
//!
//! Add [`UiModule`] (the shell does) so the built-in actions are registered and
//! layouts never show an unknown panel. See `docs/spec/ui-shell.md`.

pub mod a11y;
pub mod action;
pub mod agent;
pub mod dock;
pub mod history;
pub mod i18n;
pub mod icons;
pub mod keymap;
pub mod menu;
pub mod panel;
pub mod presets;
pub mod settings;
pub mod theme;
pub mod titlebar;
pub mod toolbar;
pub mod widgets;
pub mod workbench;
pub mod workspace;

use bae_app::{AppBuilder, AppModule};

pub use a11y::Accessible;
pub use action::{ActionHandler, ActionInfo, CommandVariant, Invocation};
pub use dock::{Dock, DropZone, Frame, FrameId, Layout, Node, PanelHost, PanelId, TabInfo};
pub use i18n::{Lang, tr};
pub use icons::Icon;
pub use keymap::{Binding, Chord, Conflict, Keymap, KeymapBindings, KeymapOverrides};
pub use menu::{MenuContext, MenuEntry, MenuItemSpec, MenuTree};
pub use panel::{
    Panel, PanelContext, PanelKind, PanelRequests, PanelSpawn, modal_open, panel_key_pressed,
};
pub use toolbar::Toolbar;
pub use workbench::{Workbench, WorkbenchContext, WorkbenchState};
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
        for (id, preset) in [
            ("motion", presets::motion()),
            ("review", presets::review()),
            ("agent", presets::agent()),
        ] {
            if let Err(error) = app.register(id, preset) {
                debug_assert!(false, "workspace {id}: {error}");
            }
        }
        if let Err(error) = app.register(presets::AE_KEYMAP, presets::ae_keymap()) {
            debug_assert!(false, "keymap: {error}");
        }
        settings::register(app);
        // The history log the shell feeds from `Stage::subscribe`.
        if app
            .resource::<std::sync::Arc<history::HistoryLog>>()
            .is_none()
        {
            app.insert_resource(std::sync::Arc::new(history::HistoryLog::new()));
        }
        let history = PanelKind::new("History", |spawn| {
            Box::new(history::HistoryPanel::from_state(spawn.state))
        });
        if let Err(error) = app.register("history", history) {
            debug_assert!(false, "history panel: {error}");
        }
        let agent_panels = [
            (
                "agent_activity",
                PanelKind::new("Agent Activity", |_| {
                    Box::new(agent::ActivityPanel::default())
                }),
            ),
            (
                "agent_proposals",
                PanelKind::new("Proposals", |_| Box::new(agent::ProposalsPanel::default())),
            ),
        ];
        for (id, kind) in agent_panels {
            if let Err(error) = app.register(id, kind) {
                debug_assert!(false, "{id} panel: {error}");
            }
        }
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

/// English title of a placeholder panel: the AE name when known, else the id.
fn placeholder_title(kind: &str) -> String {
    known_panel_title(kind).map_or_else(|| kind.to_owned(), str::to_owned)
}

/// English titles of the AE panels the default workspaces and keymaps use. The
/// shortcut editor also uses them for panels that are not registered (a binding
/// such as Cmd+6 = Character), so it never shows a raw id (#136).
pub(crate) fn known_panel_title(kind: &str) -> Option<&'static str> {
    const KNOWN: [(&str, &str); 17] = [
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
        ("agent_activity", "Agent Activity"),
        ("agent_proposals", "Proposals"),
    ];
    KNOWN
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, title)| *title)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Motion;
    impl AppModule for Motion {
        fn build(&self, app: &mut AppBuilder) {
            app.register(
                "custom",
                WorkspacePreset::new(
                    "Custom",
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
        assert!(
            kinds.contains("agent_activity"),
            "built-in presets are filled too"
        );
        let actions = app.registry::<ActionInfo>();
        assert_eq!(actions.get(action::ids::UNDO).unwrap().title, "Undo");
    }
}
