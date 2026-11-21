//! Accessibility labels for the widgets the shell paints itself (accesskit:
//! VoiceOver on macOS, Narrator and UI Automation on Windows; automated tests
//! find widgets the same way). egui's own widgets label themselves; custom ones
//! built from `ui.interact` / `allocate_exact_size` call these.
//!
//! Callers pass display text: fixed strings through [`crate::i18n::tr`], names
//! (layers, panels, workspaces) as they are shown.

use egui::{Response, WidgetInfo, WidgetType};

/// Accessibility labels for a [`Response`].
pub trait Accessible: Sized {
    /// Name the widget.
    fn labeled(self, kind: WidgetType, label: &str) -> Self;
    /// Name a toggle (switch, tab, tool) and say whether it is on / selected.
    fn labeled_toggle(self, kind: WidgetType, label: &str, on: bool) -> Self;
    /// Name a widget that shows a value (hot numbers, dropdowns, timecodes).
    fn labeled_value(self, kind: WidgetType, label: &str, value: &str) -> Self;
}

/// The fixed labels bae-ui gives its widgets (translated through `tr`).
pub const LABELS: &[&str] = &[
    "Home",
    "Selection Tool",
    "Hand Tool",
    "Zoom Tool",
    "Orbit Around Cursor Tool",
    "Pan Under Cursor Tool",
    "Dolly Towards Cursor Tool",
    "Rotation Tool",
    "Pan Behind (Anchor Point) Tool",
    "Rectangle Tool",
    "Cube Tool",
    "Pen Tool",
    "Horizontal Type Tool",
    "Brush Tool",
    "Clone Stamp Tool",
    "Eraser Tool",
    "Roto Brush Tool",
    "Puppet Position Pin Tool",
    "Close",
    "Minimize",
    "Zoom",
    "Resize Panels",
    "Close Panel",
    "Panel Menu",
    "More Panels",
    "Lock",
    "Workspace Menu",
    "Time-Vary Stopwatch",
    "Go to Previous Keyframe",
    "Add or Remove Keyframe at Current Time",
    "Go to Next Keyframe",
    "Expand",
    "Collapse",
    "Color",
    "Search",
];

impl Accessible for Response {
    fn labeled(self, kind: WidgetType, label: &str) -> Self {
        let enabled = self.enabled();
        self.widget_info(|| WidgetInfo::labeled(kind, enabled, label));
        self
    }

    fn labeled_toggle(self, kind: WidgetType, label: &str, on: bool) -> Self {
        let enabled = self.enabled();
        self.widget_info(|| WidgetInfo::selected(kind, enabled, on, label));
        self
    }

    fn labeled_value(self, kind: WidgetType, label: &str, value: &str) -> Self {
        let enabled = self.enabled();
        self.widget_info(|| {
            let mut info = WidgetInfo::labeled(kind, enabled, label);
            info.current_text_value = Some(value.to_owned());
            info
        });
        self
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_label_has_chinese() {
        let missing: Vec<_> = super::LABELS
            .iter()
            .filter(|l| !crate::i18n::has_translation(l))
            .collect();
        assert!(missing.is_empty(), "no Chinese for {missing:?}");
    }
}
