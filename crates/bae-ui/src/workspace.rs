//! Workspace presets: a dock layout plus the keymap that goes with it.
//!
//! Presets are registered in `bae_app::Registry<WorkspacePreset>` (id e.g.
//! `"motion"`, `"review"`, `"agent"`); the workspace bar lists them by
//! [`WorkspacePreset::order`]. The user's own modifications (AE's "Save Changes to
//! this Workspace") are stored separately by the shell; a preset is what "Reset"
//! goes back to when nothing was saved.

use crate::dock::{Layout, Node};

/// A registered workspace.
#[derive(Clone, Debug)]
pub struct WorkspacePreset {
    /// English title; displayed through [`crate::i18n::tr`].
    pub title: String,
    /// Id of the keymap (`Registry<Keymap>`) active in this workspace.
    pub keymap: String,
    /// Default layout. Panel ids that no module registered show a placeholder.
    pub layout: Layout,
    /// Position in the workspace bar (ascending).
    pub order: i32,
}

impl WorkspacePreset {
    pub fn new(title: impl Into<String>, keymap: impl Into<String>, root: Node) -> Self {
        WorkspacePreset {
            title: title.into(),
            keymap: keymap.into(),
            layout: Layout {
                root,
                floating: Vec::new(),
            },
            order: 0,
        }
    }

    pub fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    /// Every panel id in the layout (docked and floating).
    pub fn panel_ids(&self) -> Vec<crate::dock::PanelId> {
        self.layout.panel_ids()
    }
}
