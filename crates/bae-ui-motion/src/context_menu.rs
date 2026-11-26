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
    /// An entry that applies to some kinds of Project items only ([`COMP`],
    /// [`FOOTAGE`], [`FOLDER`], [`SOLID`]); greyed on the others (AE 26).
    Only(u8, &'static Entry),
    /// Project ▸ Reveal Project in Finder / Explorer: shows the saved project
    /// file; greyed while the project has not been saved.
    RevealProject,
}

/// Project item kinds for [`Entry::Only`] and [`show_item`].
pub(crate) const COMP: u8 = 1;
pub(crate) const FOOTAGE: u8 = 2;
pub(crate) const FOLDER: u8 = 4;
pub(crate) const SOLID: u8 = 8;
const ANY: u8 = u8::MAX;

/// AE's title of Reveal Project on this platform.
pub(crate) fn reveal_project_title() -> &'static str {
    if cfg!(target_os = "macos") {
        "Reveal Project in Finder"
    } else {
        "Reveal Project in Explorer"
    }
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

/// Whether a catalog item runs: its command is done, or a module handles its
/// action (as the menu bar decides, `catalog::live_by_handler`).
pub(crate) fn runs(cx: &PanelContext<'_>, it: &catalog::Item) -> bool {
    it.status.works()
        || cx
            .resources
            .get::<HandledActions>()
            .is_some_and(|h| catalog::live_by_handler(it.action, it.params, &h.0))
}

/// The actions the app's modules handle (`bae_ui::menu::handled_actions`), known
/// once every module is built.
#[derive(Clone, Debug, Default)]
pub struct HandledActions(pub std::sync::Arc<std::collections::HashSet<String>>);

/// Draw `entries` in an open menu. Returns the title of the [`Own`] item chosen.
pub(crate) fn show(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    entries: &'static [Entry],
) -> Option<&'static str> {
    show_with(ui, cx, entries, &[])
}

/// [`show`], with the [`Own`] items titled in `off` greyed (where the panel
/// cannot do them: Rename in the Composition panel).
pub(crate) fn show_with(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    entries: &'static [Entry],
    off: &[&str],
) -> Option<&'static str> {
    show_item(ui, cx, entries, off, ANY)
}

/// [`show_with`] for a Project item of `kind` ([`COMP`] …): [`Entry::Only`]
/// entries for other kinds are greyed.
pub(crate) fn show_item(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    entries: &'static [Entry],
    off: &[&str],
    kind: u8,
) -> Option<&'static str> {
    let mut chosen = None;
    for e in entries {
        match *e {
            Only(kinds, entry) => {
                let one = std::slice::from_ref(entry);
                let r =
                    ui.add_enabled_ui(kinds & kind != 0, |ui| show_item(ui, cx, one, off, kind));
                chosen = chosen.or(r.inner);
            }
            RevealProject => {
                let saved = cx
                    .resources
                    .get::<bae_ui::files::ProjectFile>()
                    .is_some_and(|f| f.get().is_some());
                if ui
                    .add_enabled(saved, egui::Button::new(tr(reveal_project_title())))
                    .clicked()
                {
                    cx.invoke(bae_ui::action::ids::REVEAL_PROJECT);
                    ui.close();
                }
            }
            Menu(path) => {
                if let Some(n) = find(path) {
                    catalog_node(ui, cx, n);
                }
            }
            Items(path) => catalog_nodes(ui, cx, children(path)),
            As(title, path) => {
                if let Some(Node::Item(it)) = find(path)
                    && ui
                        .add_enabled(runs(cx, it), egui::Button::new(tr(title)))
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
            // Filled from the project in AE (Reveal in Composition): greyed.
            Sub(title, []) => {
                ui.add_enabled(false, egui::Button::new(tr(title)));
            }
            Sub(title, list) => common::submenu(ui, tr(title), &mut |ui| {
                chosen = chosen.or(show_item(ui, cx, list, off, kind));
            }),
            Own(title) => {
                if ui
                    .add_enabled(!off.contains(&title), egui::Button::new(tr(title)))
                    .clicked()
                {
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

/// Right-click on a layer, in the Composition panel and the Timeline (AE 26 on
/// the Mac shows the same menu in both). The Composition panel greys Rename and
/// adds Select ▸, the layers under the pointer.
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
    Own("Rename"),
    Off("Add to Essential Graphics"),
    Menu("Composition/Crop Comp to Selected Layer(s) Bounds"),
    Menu("Layer/Scene Edit Detection..."),
];

/// Timeline, right-click on empty space: the Composition panel's comp menu with
/// Rename enabled (AE 26: it renames the comp, in the Project panel).
pub(crate) const TIMELINE_BLANK: &[Entry] = &[
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
    Own("Rename"),
    Sep,
    Menu("Composition/Open in Essential Graphics"),
    Sep,
    Menu("Composition/Composition Flowchart"),
    Menu("Composition/Composition Mini-Flowchart"),
];

/// Project panel, right-click on empty space.
pub(crate) const PROJECT_BLANK: &[Entry] = &[
    Menu("Composition/New Composition..."),
    Menu("File/New/New Folder"),
    As(
        "New Adobe Photoshop File...",
        "File/New/Adobe Photoshop File...",
    ),
    As(
        "New Maxon Cinema 4D File...",
        "File/New/Maxon Cinema 4D File...",
    ),
    Menu("File/Import"),
    Menu("File/Import Recent Footage"),
    RevealProject,
];

/// Project panel, right-click on an item (comp, footage, folder: the same menu,
/// AE greys what does not apply to the item: `project.item.comp` / `.footage` /
/// `.folder`, #600).
pub(crate) const PROJECT_ITEM: &[Entry] = &[
    Only(COMP, &Menu("Composition/Composition Settings...")),
    Only(COMP, &Menu("Composition/Open in Essential Graphics")),
    As("New Comp from Selection", "File/New Comp from Selection..."),
    Off("Replace with Precomp"),
    Only(COMP, &Menu("File/Create Proxy")),
    Only(COMP, &Menu("File/Set Proxy")),
    // AE greys Import with a comp selected.
    Only(FOOTAGE | FOLDER | SOLID, &Menu("File/Import")),
    Only(
        FOOTAGE | FOLDER | SOLID,
        &Menu("File/Import Recent Footage"),
    ),
    Only(FOOTAGE, &Menu("File/Interpret Footage")),
    Only(FOOTAGE, &Menu("File/Replace Footage")),
    Only(FOOTAGE, &Menu("File/Reload Footage")),
    Menu("File/License..."),
    Only(FOOTAGE, &Menu("File/Reveal in Explorer")),
    Menu("File/Reveal in Bridge"),
    Sub("Reveal in Composition", &[]),
    Off("Solids Folder"),
    Own("Rename"),
];

/// Chinese strings of this module (keys are the English text, see
/// `bae_ui::i18n`).
pub(crate) const ZH: &[(&str, &str)] = &[
    // Context menus (AE 26).
    ("Layer Styles", "图层样式"),
    ("Open Effect Controls", "打开效果控件"),
    // Layer context menu items that are not in the menu bar.
    ("Invert Selection", "反向选择"),
];

/// Registers this module's Chinese strings (`MotionUiModule::build`).
pub(crate) fn register(_app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
}

#[cfg(test)]
mod tests;
