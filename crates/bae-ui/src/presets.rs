//! Built-in workspaces and keymaps, registered by [`crate::UiModule`].
//!
//! * **Motion (动效)**: AE's Default workspace (sizes measured from AE 2026 at
//!   1728×1000, see the dock tests).
//! * **Review (审阅)**: large viewer, timeline, preview and history.
//! * **Agent**: viewer and timeline next to the agent activity log and proposals.
//!
//! The AE keymap holds the application-wide AE shortcuts the shell itself handles;
//! workspace crates add their panel shortcuts (`when: Some("timeline")` …) to the
//! same keymap through their own registrations or by registering a keymap of their
//! own (see `docs/spec/ui-shell.md`).

use serde_json::json;

use crate::action::ids;
use crate::dock::{Frame, Node};
use crate::keymap::{Binding, Keymap};
use crate::workspace::WorkspacePreset;

/// Id of the After Effects keymap.
pub const AE_KEYMAP: &str = "ae";

/// Motion: AE's Default workspace.
pub fn motion() -> WorkspacePreset {
    WorkspacePreset::new(
        "Motion",
        AE_KEYMAP,
        Node::row(vec![
            (
                1406.0,
                Node::column(vec![
                    (
                        640.0,
                        Node::row(vec![
                            (320.0, Node::tabs(&["project", "effect_controls"])),
                            (1082.0, Node::tabs(&["composition"])),
                        ]),
                    ),
                    (295.0, Node::tabs(&["timeline"])),
                ]),
            ),
            (
                316.0,
                Node::column(vec![
                    (52.0, Node::tabs(&["preview"])),
                    (
                        883.0,
                        Node::frame(
                            Frame::new(&["properties", "align", "audio", "effects"])
                                .stacked(Some(601.0)),
                        ),
                    ),
                ]),
            ),
        ]),
    )
    .order(0)
    .description(
        "Compositions, layers, keyframes, expressions and effects, laid out like After Effects.",
    )
}

/// Review: a large viewer with the timeline below; preview and history at the side.
pub fn review() -> WorkspacePreset {
    WorkspacePreset::new(
        "Review",
        AE_KEYMAP,
        Node::row(vec![
            (
                1406.0,
                Node::column(vec![
                    (760.0, Node::tabs(&["composition"])),
                    (175.0, Node::tabs(&["timeline"])),
                ]),
            ),
            (
                316.0,
                Node::column(vec![
                    (52.0, Node::tabs(&["preview"])),
                    (883.0, Node::tabs(&["history", "info"])),
                ]),
            ),
        ]),
    )
    .order(1)
    .description("Play back, annotate and compare versions.")
}

/// Agent: what agents are doing, their proposals, and the result in the viewer.
pub fn agent() -> WorkspacePreset {
    WorkspacePreset::new(
        "Agent",
        AE_KEYMAP,
        Node::row(vec![
            (
                1100.0,
                Node::column(vec![
                    (640.0, Node::tabs(&["composition"])),
                    (295.0, Node::tabs(&["timeline"])),
                ]),
            ),
            (
                622.0,
                Node::column(vec![
                    (520.0, Node::tabs(&["agent_activity"])),
                    (415.0, Node::tabs(&["agent_proposals", "history"])),
                ]),
            ),
        ]),
    )
    .order(2)
    .description("Watch what agents do and review their proposals.")
}

/// Application-wide After Effects shortcuts (Windows `Ctrl` = macOS `Cmd`).
/// Source: docs/spec/ae ui-preview-render-io (AE-UI-001, AE-UI-005) and the AE
/// 2026 keyboard shortcut editor.
pub fn ae_keymap() -> Keymap {
    let panel = |chord: &str, id: &str| {
        Binding::new(chord, ids::OPEN_PANEL).params(json!({ "id": id, "toggle": true }))
    };
    let workspace = |chord: &str, index: u32| {
        Binding::new(chord, ids::SWITCH_WORKSPACE).params(json!({ "index": index }))
    };
    Keymap::new(
        "After Effects",
        vec![
            // File
            Binding::new("Cmd+Alt+N", ids::NEW_PROJECT),
            Binding::new("Cmd+O", ids::OPEN_PROJECT),
            Binding::new("Cmd+S", ids::SAVE),
            Binding::new("Cmd+Shift+S", ids::SAVE_AS),
            Binding::new("Cmd+W", ids::CLOSE_PANEL),
            Binding::new("Cmd+Q", ids::QUIT),
            // Edit
            Binding::new("Cmd+Z", ids::UNDO),
            Binding::new("Cmd+Shift+Z", ids::REDO),
            Binding::new("Cmd+Alt+Quote", ids::KEYBOARD_SHORTCUTS),
            // Window: panels (AE-UI-001)
            panel("Cmd+0", "project"),
            panel("Cmd+Alt+0", "render_queue"),
            panel("Cmd+1", "tools"),
            panel("Cmd+2", "info"),
            panel("Cmd+3", "preview"),
            panel("Cmd+4", "audio"),
            panel("Cmd+5", "effects"),
            panel("Cmd+6", "character"),
            panel("Cmd+7", "paragraph"),
            panel("F3", "effect_controls"),
            // Window: workspaces (AE-UI-005). AE assigns Shift+F10 / F11 to the
            // Default and Review workspaces (Window ▸ Assign Shortcut to
            // Workspace), so the menu items show them; Shift+F12 (AE: Learn, which
            // BAE does not have) switches to the third workspace.
            Binding::new("Shift+F10", ids::SWITCH_WORKSPACE).params(json!({ "id": "motion" })),
            Binding::new("Shift+F11", ids::SWITCH_WORKSPACE).params(json!({ "id": "review" })),
            workspace("Shift+F12", 2),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ae_keymap_has_no_conflicts() {
        let km = ae_keymap();
        assert!(km.conflicts().is_empty(), "{:?}", km.conflicts());
        assert_eq!(
            km.chords_for(ids::UNDO).next().map(|c| c.to_string()),
            Some("Cmd+Z".to_owned())
        );
    }

    #[test]
    fn presets_use_known_panels() {
        for preset in [motion(), review(), agent()] {
            assert_eq!(preset.keymap, AE_KEYMAP);
            assert!(
                preset
                    .panel_ids()
                    .iter()
                    .any(|p| p.as_str() == "composition")
            );
        }
    }
}
