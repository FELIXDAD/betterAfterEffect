//! The tool strip of the toolbar: the area between the window's left edge and the
//! workspace bar, where AE shows its tools and the active tool's options (AE-UI-100).
//!
//! A workspace module registers a [`Toolbar`] in `bae_app::Registry<dyn Toolbar>`
//! under the id of the keymap its workspaces use (`"ae"` for the Motion workspace,
//! `"ai"` later for the design workspace). The workbench draws the toolbar of the
//! active workspace's keymap every frame; without one it shows nothing (the `ae`
//! keymap falls back to static tool icons).
//!
//! The toolbar owns its tool state (usually an app resource the module's panels
//! read, so the viewer knows the current tool). Its persistent part (current tool,
//! paint settings for new shapes …) is saved with the workbench state through
//! [`Toolbar::save_state`] / [`Toolbar::load_state`], like panel view state; it is
//! application state, not project data (AE-UI-300: "Tool state is per application").

use egui::Ui;

use crate::keymap::Keymap;
use crate::panel::PanelContext;

/// The tools and tool options of a workspace's toolbar.
pub trait Toolbar: Send + Sync {
    /// Draw the tools and options into `ui`, a left-to-right row clipped to the
    /// space left of the workspace bar. `keymap` is the effective keymap of the
    /// active workspace (shortcut labels in tooltips and flyouts; spring-loaded
    /// tools check whether their key is still held). Commands and actions go
    /// through `cx` like a panel's (`cx.panel` is `"toolbar"`).
    fn ui(&self, ui: &mut Ui, cx: &mut PanelContext<'_>, keymap: &Keymap);

    /// Persistent state (current tool, tool options), saved by the workbench.
    fn save_state(&self) -> Option<serde_json::Value> {
        None
    }

    /// Restore what [`Toolbar::save_state`] returned in an earlier session.
    fn load_state(&self, _state: &serde_json::Value) {}
}

/// Key of a toolbar's state in [`crate::WorkbenchState::panels`].
pub fn state_key(keymap: &str) -> String {
    format!("toolbar:{keymap}")
}
