//! BAE UI shell: theme tokens, docking frames, title bar, icons and AE-style widgets.
//!
//! The shell only renders and handles interaction; it keeps no project data
//! (see docs/ARCHITECTURE.md, principle 1).

pub mod dock;
pub mod i18n;
pub mod icons;
pub mod theme;
pub mod titlebar;
pub mod widgets;

pub use dock::{Dock, DropZone, Frame, FrameId, Layout, Node, PanelHost, PanelId, TabInfo};
pub use i18n::{Lang, tr};
pub use icons::Icon;
