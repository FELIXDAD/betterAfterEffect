//! Actions: everything a menu item, shortcut, toolbar button or panel can trigger
//! is identified by a **command id** string.
//!
//! An id resolves, in this order (see [`crate::workbench`]):
//!
//! 1. the focused panel ([`crate::Panel::on_action`]) — view-only actions such as
//!    timeline zoom, scoped to one panel kind;
//! 2. shell actions ([`ids`]): undo/redo, workspaces, windows, application;
//! 3. a `bae-model` command (`CommandRegistry::execute` on the open stage, one undo
//!    step attributed to the local author).
//!
//! Only (3) changes project data. UI-level actions are described by an
//! [`ActionInfo`] registered in `Registry<ActionInfo>` (id = command id) so the
//! keyboard-shortcut editor and the menus can show a localized title; model
//! commands are described by their `CommandInfo` and need no extra registration.

use serde::{Deserialize, Serialize};

/// Metadata of a UI-level action (registered in `bae_app::Registry<ActionInfo>`
/// under its command id).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionInfo {
    /// English title; displayed through [`crate::i18n::tr`].
    pub title: String,
    /// Panel kind the action belongs to (`Some("timeline")`); `None` for global
    /// actions. Panel actions are routed to the focused panel of that kind.
    pub panel: Option<String>,
}

/// The name of one variant of a command (a UI action or a model command): the
/// parameters that tell it apart and its English title, e.g. `motion.step` with
/// `{"frames": -10}` is "Back 10 Frames", `layer.create` with `{"kind": "solid"}` is
/// "New Solid". Registered in `bae_app::Registry<CommandVariant>`. The Keyboard
/// Shortcuts window lists variants by these names, and the shell labels Edit > Undo /
/// Redo and History with them (#140).
#[derive(Clone, Debug, PartialEq)]
pub struct CommandVariant {
    pub command: String,
    /// The variant matches when every key here has the same value in the
    /// invocation's parameters (other keys are ignored).
    pub params: serde_json::Value,
    /// English title, displayed through [`crate::i18n::tr`].
    pub title: String,
}

impl CommandVariant {
    pub fn new(
        command: impl Into<String>,
        params: serde_json::Value,
        title: impl Into<String>,
    ) -> Self {
        CommandVariant {
            command: command.into(),
            params,
            title: title.into(),
        }
    }

    /// Number of parameters matched, or `None` when the variant does not apply.
    fn matches(&self, command: &str, params: &serde_json::Value) -> Option<usize> {
        if self.command != command {
            return None;
        }
        let wanted = self.params.as_object()?;
        wanted
            .iter()
            .all(|(k, v)| params.get(k) == Some(v))
            .then_some(wanted.len())
    }
}

/// Title of the most specific variant of `command` matching `params`.
pub fn variant_title<'a>(
    variants: impl IntoIterator<Item = &'a CommandVariant>,
    command: &str,
    params: &serde_json::Value,
) -> Option<&'a str> {
    variants
        .into_iter()
        .filter_map(|v| v.matches(command, params).map(|n| (n, v)))
        .max_by_key(|(n, _)| *n)
        .map(|(_, v)| v.title.as_str())
}

impl ActionInfo {
    pub fn new(title: impl Into<String>) -> Self {
        ActionInfo {
            title: title.into(),
            panel: None,
        }
    }

    /// Scope the action to a panel kind.
    pub fn for_panel(mut self, kind: impl Into<String>) -> Self {
        self.panel = Some(kind.into());
        self
    }
}

/// A request to run a command id with JSON parameters (`Null` = no parameters;
/// model commands receive `{}` instead).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Invocation {
    pub id: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

impl Invocation {
    pub fn new(id: impl Into<String>) -> Self {
        Invocation {
            id: id.into(),
            params: serde_json::Value::Null,
        }
    }

    pub fn with_params(id: impl Into<String>, params: serde_json::Value) -> Self {
        Invocation {
            id: id.into(),
            params,
        }
    }

    /// The `"id"` string parameter (`workspace.switch`, `window.panel`).
    pub fn id_param(&self) -> Option<&str> {
        self.params.get("id").and_then(|v| v.as_str())
    }
}

/// Handles actions for a module whichever panel has focus (registered in
/// `bae_app::Registry<dyn ActionHandler>`, id = module name).
///
/// The workbench tries, in order: the focused panel's [`crate::Panel::on_action`],
/// then these handlers, then the shell's own actions ([`ids`]), then the shell
/// runs model commands. Among handlers, the ones whose [`ActionHandler::workspace`]
/// is the active workspace go first, then registration order; two handlers
/// claiming the same action id are a conflict the workbench logs (and reports in
/// `Workbench::handler_conflicts`).
pub trait ActionHandler: Send + Sync + 'static {
    /// Action ids this handler claims (e.g. `motion.play`).
    fn actions(&self) -> Vec<String>;

    /// Workspace of the module (it wins conflicts while that workspace is active).
    fn workspace(&self) -> Option<&str> {
        None
    }

    /// Handle `inv` (one of [`ActionHandler::actions`]); `cx` is the focused
    /// panel's context, or a workbench-level one when no panel has focus. Return
    /// `true` when handled.
    fn handle(&self, inv: &Invocation, cx: &mut crate::panel::PanelContext<'_>) -> bool;
}

/// Command ids handled by the shell itself.
pub mod ids {
    /// Undo the local author's last step (`Stage::undo`).
    pub const UNDO: &str = "edit.undo";
    /// Redo the local author's last undone step (`Stage::redo`).
    pub const REDO: &str = "edit.redo";
    /// Quit the application.
    pub const QUIT: &str = "app.quit";
    /// Show the About window.
    pub const ABOUT: &str = "app.about";
    /// Open the keyboard shortcut editor.
    pub const KEYBOARD_SHORTCUTS: &str = "app.keyboard_shortcuts";
    /// New / open / close / save project (handled by the shell's project owner).
    /// `file.open_project` takes an optional `{"path": "<.bae or .baez>"}` (no
    /// dialog); without it the shell shows the native open dialog.
    pub const NEW_PROJECT: &str = "file.new_project";
    pub const OPEN_PROJECT: &str = "file.open_project";
    pub const CLOSE_PROJECT: &str = "file.close_project";
    pub const SAVE: &str = "file.save";
    pub const SAVE_AS: &str = "file.save_as";
    /// Switch workspace; params `{"id": "<workspace id>"}`.
    pub const SWITCH_WORKSPACE: &str = "workspace.switch";
    /// Restore the active workspace's saved layout.
    pub const RESET_WORKSPACE: &str = "workspace.reset";
    /// Save the current layout as the active workspace's saved layout.
    pub const SAVE_WORKSPACE: &str = "workspace.save";
    /// Save the current layout as a new workspace; params `{"title": "..."}`
    /// (without params: open Edit Workspaces).
    pub const NEW_WORKSPACE: &str = "workspace.new";
    /// Open the Edit Workspaces dialog (reorder, hide, delete).
    pub const EDIT_WORKSPACES: &str = "workspace.edit";
    /// Open (or focus) a panel; params `{"id": "<panel id>"}`.
    pub const OPEN_PANEL: &str = "window.panel";
    /// Close the focused panel.
    pub const CLOSE_PANEL: &str = "window.close_panel";
    /// Maximize / restore the focused panel group (AE: `~`).
    pub const MAXIMIZE_FRAME: &str = "window.maximize_frame";
    /// Switch the UI language; params `{"id": "en" | "zh"}`.
    pub const SET_LANGUAGE: &str = "app.language";
    /// Open the help documentation.
    pub const HELP: &str = "help.docs";
}

/// Titles of the shell's own actions, registered by [`crate::UiModule`].
pub(crate) const SHELL_ACTIONS: &[(&str, &str)] = &[
    (ids::UNDO, "Undo"),
    (ids::REDO, "Redo"),
    (ids::QUIT, "Quit"),
    (ids::ABOUT, "About BAE"),
    (ids::KEYBOARD_SHORTCUTS, "Keyboard Shortcuts"),
    (ids::NEW_PROJECT, "New Project"),
    (ids::OPEN_PROJECT, "Open Project..."),
    (ids::CLOSE_PROJECT, "Close Project"),
    (ids::SAVE, "Save"),
    (ids::SAVE_AS, "Save As..."),
    (ids::SWITCH_WORKSPACE, "Switch Workspace"),
    (ids::RESET_WORKSPACE, "Reset to Saved Layout"),
    (ids::SAVE_WORKSPACE, "Save Changes to this Workspace"),
    (ids::NEW_WORKSPACE, "Save as New Workspace..."),
    (ids::EDIT_WORKSPACES, "Edit Workspaces..."),
    (ids::OPEN_PANEL, "Open Panel"),
    (ids::CLOSE_PANEL, "Close Panel"),
    (ids::MAXIMIZE_FRAME, "Maximize Panel Group"),
    (ids::SET_LANGUAGE, "Language"),
    (ids::HELP, "BAE Help"),
];

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_most_specific_matching_variant_names_the_command() {
        let variants = [
            CommandVariant::new(
                "layer.create",
                json!({ "kind": "shape" }),
                "New Shape Layer",
            ),
            CommandVariant::new(
                "layer.create",
                json!({ "kind": "shape", "shape": "rect" }),
                "New Rectangle",
            ),
            CommandVariant::new("layer.create", json!({ "kind": "solid" }), "New Solid"),
            CommandVariant::new("motion.step", json!({ "frames": -10 }), "Back 10 Frames"),
        ];
        let title = |id: &str, params: serde_json::Value| variant_title(&variants, id, &params);
        // Other parameters do not matter; the variant's keys must all match.
        assert_eq!(
            title(
                "layer.create",
                json!({ "kind": "solid", "name": "BG", "index": 0 })
            ),
            Some("New Solid")
        );
        assert_eq!(
            title("layer.create", json!({ "kind": "shape", "shape": "rect" })),
            Some("New Rectangle")
        );
        assert_eq!(
            title("layer.create", json!({ "kind": "shape", "shape": "star" })),
            Some("New Shape Layer")
        );
        assert_eq!(title("layer.create", json!({ "kind": "camera" })), None);
        assert_eq!(title("layer.create", serde_json::Value::Null), None);
        assert_eq!(
            title("motion.step", json!({ "frames": -10 })),
            Some("Back 10 Frames")
        );
        assert_eq!(title("motion.step", json!({ "frames": 10 })), None);
        assert_eq!(title("comp.create", json!({ "kind": "solid" })), None);
    }
}
