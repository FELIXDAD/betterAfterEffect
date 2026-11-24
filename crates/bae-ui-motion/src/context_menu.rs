//! Context menus as AE 26 shows them (`docs/reference/ae/mac/ui/context-menus.json`),
//! built from the command catalog: each entry is a catalog item or submenu by its
//! menu-bar path, so it has the menu bar's action id, Chinese title and greying.
//! Entries AE only has in context menus are listed here.

use bae_ui::{PanelContext, tr};
use egui::Ui;

use crate::catalog::{self, Node};
use crate::common;
use crate::timeline::{catalog_node, catalog_nodes};

/// One line of a context menu.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Entry {
    /// The catalog entry (item or submenu) at a menu-bar path, `"Layer/Mask"`.
    Menu(&'static str),
    /// The children of the catalog submenu at a path, inline.
    Items(&'static str),
    Sep,
    /// A submenu built here.
    Sub(&'static str, &'static [Entry]),
    /// The catalog item at a path under the title AE gives it in this menu.
    As(&'static str, &'static str),
    /// An item the panel runs itself: [`show`] returns its title when chosen.
    Own(&'static str),
    /// An item with no command yet: shown greyed.
    Off(&'static str),
}

use Entry::*;

/// The catalog node at a menu-bar path (`"Layer/Switches/Shy"`).
pub(crate) fn find(path: &str) -> Option<&'static Node> {
    let mut parts = path.split('/');
    let top = parts.next()?;
    let mut nodes = catalog::menus().iter().find(|(t, _)| *t == top)?.1;
    let mut found = None;
    for part in parts {
        let n = nodes.iter().find(|n| match n {
            Node::Item(it) => it.title == part,
            Node::Submenu(s) => s.title == part,
            _ => false,
        })?;
        if let Node::Submenu(s) = n {
            nodes = s.children;
        }
        found = Some(n);
    }
    found
}

/// The children of the top-level menu or submenu at `path`.
fn children(path: &str) -> &'static [Node] {
    if !path.contains('/') {
        return catalog::menus()
            .iter()
            .find(|(t, _)| *t == path)
            .map_or(&[], |(_, n)| *n);
    }
    match find(path) {
        Some(Node::Submenu(s)) => s.children,
        _ => &[],
    }
}

/// Draw `entries` in an open menu. Returns the title of the [`Own`] item chosen.
pub(crate) fn show(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    entries: &'static [Entry],
) -> Option<&'static str> {
    let mut chosen = None;
    for e in entries {
        match *e {
            Menu(path) => {
                if let Some(n) = find(path) {
                    catalog_node(ui, cx, n);
                }
            }
            Items(path) => catalog_nodes(ui, cx, children(path)),
            As(title, path) => {
                if let Some(Node::Item(it)) = find(path)
                    && ui
                        .add_enabled(it.status.works(), egui::Button::new(tr(title)))
                        .clicked()
                {
                    let params = serde_json::from_str(it.params).unwrap_or(serde_json::json!({}));
                    cx.invoke_with(it.action, params);
                    ui.close();
                }
            }
            Sep => {
                ui.separator();
            }
            Sub(title, list) => common::submenu(ui, tr(title), &mut |ui| {
                chosen = chosen.or(show(ui, cx, list));
            }),
            Own(title) => {
                if ui.button(tr(title)).clicked() {
                    chosen = Some(title);
                    ui.close();
                }
            }
            Off(title) => {
                ui.add_enabled(false, egui::Button::new(tr(title)));
            }
        }
    }
    chosen
}

// ---------------------------------------------------------------------------
// The menus (AE 26.2.1, macOS).

/// Composition panel, right-click where there is no layer.
pub(crate) const VIEWER_BLANK: &[Entry] = &[
    Sub(
        "New",
        &[
            Menu("View/Split with New Locked Viewer"),
            Sep,
            Items("Layer/New"),
        ],
    ),
    Sep,
    Menu("Composition/Composition Settings..."),
    Menu("Layer/Reveal/Reveal Composition in Project"),
    Sep,
    Menu("Composition/Preview"),
    Menu("View/Switch View Layout"),
    Menu("View/Switch 3D View"),
    Off("Rename"),
    Sep,
    Menu("Composition/Open in Essential Graphics"),
    Sep,
    Menu("Composition/Composition Flowchart"),
    Menu("Composition/Composition Mini-Flowchart"),
];

/// Composition panel, right-click on a layer (AE adds Select ▸, the layers under
/// the pointer, which the viewer appends).
pub(crate) const VIEWER_LAYER: &[Entry] = &[
    Menu("Layer/Mask"),
    Menu("Layer/Mask and Shape Path"),
    Menu("Layer/Quality"),
    Menu("Layer/Switches"),
    Menu("Layer/Transform"),
    Menu("Layer/Time"),
    Menu("Layer/Frame Blending"),
    Menu("Layer/3D Layer"),
    Menu("Layer/Guide Layer"),
    Menu("Layer/Environment Layer"),
    Menu("Layer/Markers"),
    Menu("Layer/Blending Mode"),
    Menu("Layer/Layer Styles"),
    Sep,
    Sub("Effect", &[Items("Effect")]),
    Menu("Animation/Keyframe Assistant"),
    Sub(
        "Track & Stabilize",
        &[
            Menu("Animation/Track Camera"),
            Menu("Animation/Warp Stabilizer VFX"),
            Menu("Animation/Track Motion"),
            Menu("Animation/Track Mask"),
        ],
    ),
    Sep,
    Sub(
        "Open",
        &[
            As("Open Effect Controls", "Effect/Effect Controls"),
            Menu("Layer/Open Layer"),
            Menu("Layer/Open Layer Source"),
            Menu("Edit/Edit Original..."),
        ],
    ),
    Menu("Layer/Reveal"),
    Menu("Layer/Create"),
    Sep,
    Menu("Layer/Camera"),
    Menu("Layer/Pre-compose..."),
    Sep,
    Own("Invert Selection"),
    Own("Select Children"),
    Off("Rename"),
    Off("Add to Essential Graphics"),
    Menu("Composition/Crop Comp to Selected Layer(s) Bounds"),
    Menu("Layer/Scene Edit Detection..."),
];

#[cfg(test)]
mod tests;
