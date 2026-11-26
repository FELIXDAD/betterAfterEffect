//! Menu contributions: modules add items to the application menu bar.
//!
//! A module registers [`MenuItemSpec`]s in `bae_app::Registry<MenuItemSpec>` (id =
//! a unique item id such as `"motion.layer.new.solid"`). Each item names its menu
//! path (`["Layer", "New"]`), an English title (displayed through
//! [`crate::i18n::tr`]), and the **command id + parameters it runs** — the same
//! invocation a shortcut binding carries, so a menu item and its shortcut are one
//! action, and the menu shows the shortcut the active keymap binds to it.
//!
//! [`build_menus`] turns the registry into a sorted tree: top-level menus in AE
//! order ([`MENU_ORDER`]), items by `(group, order, registration)`, a separator
//! between groups. An item can give its position at every level of its path
//! ([`MenuItemSpec::at`]): the AE menus (`bae-ui-motion`'s catalog) place each
//! submenu and separator exactly where AE has them that way. Entries can be
//! sorted by their localized titles ([`MenuPos::sorted`], AE's Window panel
//! list, workspaces, effects; [`build_menus_localized`]). A placeholder
//! ([`MenuItemSpec::placeholder`]) is not shown but makes its submenu exist (AE's
//! History, a list filled at run time); a submenu with nothing to show is
//! rendered greyed. Two items with the same menu path and title conflict: the
//! one registered for the active workspace wins, otherwise the first
//! registered; the loser is dropped and reported in [`MenuTree::conflicts`] (and
//! logged).
//!
//! Like AE, items whose command cannot run are shown greyed, not hidden: an
//! item's enabled check ([`MenuItemSpec::enabled_when`]) looks at the state, and
//! an item marked [`MenuItemSpec::needs_handler`] stays greyed until a module
//! handles its action ([`MenuContext::handles`], [`handled_actions`]).
//!
//! The shell renders the tree natively (muda) and runs a chosen item's
//! [`MenuItemSpec::invocation`] through the normal dispatch: focused panel →
//! module action handlers ([`crate::ActionHandler`]) → shell actions → model
//! commands.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bae_app::{RegistrySnapshot, Resources};
use bae_model::Stage;

use crate::action::{ActionHandler, ActionInfo, Invocation};
use crate::dock::PanelId;

/// Top-level menus in After Effects order. Contributed menus not listed here come
/// after them (before Window and Help), in first-registration order. `App` is the
/// macOS application menu (AE's "After Effects" menu: About, Settings, Quit …).
pub const MENU_ORDER: &[&str] = &[
    APP_MENU,
    "File",
    "Edit",
    "Composition",
    "Layer",
    "Effect",
    "Animation",
    "View",
    "Window",
    "Help",
];

/// Top-level title of the macOS application menu (the shell shows it under the
/// application's name and only on macOS).
pub const APP_MENU: &str = "App";

/// What an item's enabled and checked checks can look at.
#[non_exhaustive]
pub struct MenuContext<'a> {
    pub stage: &'a Stage,
    pub resources: &'a Resources,
    /// Id of the active workspace.
    pub workspace: &'a str,
    /// Panels open in the dock (AE checks the Window items of open panels).
    pub open_panels: &'a [PanelId],
    /// Changes with every menu update, so checks can share one snapshot of the
    /// state per update instead of each item recomputing it; 0 = no sharing.
    pub generation: u64,
    /// Action ids some module handles ([`handled_actions`]); `None` = unknown,
    /// so [`MenuItemSpec::needs_handler`] items stay greyed.
    pub actions: Option<&'a HashSet<String>>,
}

impl<'a> MenuContext<'a> {
    pub fn new(stage: &'a Stage, resources: &'a Resources, workspace: &'a str) -> Self {
        MenuContext {
            stage,
            resources,
            workspace,
            open_panels: &[],
            generation: 0,
            actions: None,
        }
    }

    /// The action ids modules handle ([`handled_actions`]).
    pub fn with_actions(mut self, actions: &'a HashSet<String>) -> Self {
        self.actions = Some(actions);
        self
    }

    /// Whether a module handles `action` (a [`MenuItemSpec::handler_key`]).
    pub fn handles(&self, action: &str) -> bool {
        self.actions.is_some_and(|a| a.contains(action))
    }

    /// The panels open in the dock.
    pub fn with_open_panels(mut self, open_panels: &'a [PanelId]) -> Self {
        self.open_panels = open_panels;
        self
    }

    /// The update this context belongs to (see [`MenuContext::generation`]).
    pub fn with_generation(mut self, generation: u64) -> Self {
        self.generation = generation;
        self
    }

    /// Whether a panel of this id is open.
    pub fn panel_open(&self, id: &str) -> bool {
        self.open_panels.iter().any(|p| p.0 == id)
    }
}

/// Decides whether an item is enabled (e.g. "Layer ▸ New" needs an active comp).
pub type MenuEnabled = dyn Fn(&MenuContext<'_>) -> bool + Send + Sync;

/// Decides whether a checkable item shows a check mark.
pub type MenuChecked = dyn Fn(&MenuContext<'_>) -> bool + Send + Sync;

/// Where an entry sits in one level of the menu: entries sort by `(group,
/// order)`; a separator goes between groups. `sorted`: the entry is placed by its
/// localized title among the sorted entries of its group (AE's panel list).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuPos {
    pub group: i32,
    pub order: i32,
    pub sorted: bool,
}

impl MenuPos {
    pub const fn new(group: i32, order: i32) -> Self {
        MenuPos {
            group,
            order,
            sorted: false,
        }
    }

    pub const fn sorted(mut self) -> Self {
        self.sorted = true;
        self
    }
}

/// One menu item contributed by a module.
#[derive(Clone)]
pub struct MenuItemSpec {
    /// English titles of the top-level menu and any submenus, e.g. `["Layer", "New"]`.
    pub path: Vec<String>,
    /// English title of the item.
    pub title: String,
    /// Optional package-supplied Simplified Chinese title.
    pub title_zh: Option<String>,
    /// Command id run when the item is chosen.
    pub action: String,
    /// Parameters of the command (`Null` = none).
    pub params: serde_json::Value,
    /// Items are sorted by group, then order; groups are separated by a line.
    pub group: i32,
    pub order: i32,
    /// Workspace of the contributing module: it wins conflicts while active.
    pub workspace: Option<String>,
    /// Position at each level of `path` (top-level menu first, the item's own
    /// level last); levels without one use `(group, order)`.
    pub levels: Vec<MenuPos>,
    /// Not shown; only makes its submenu exist ([`MenuItemSpec::placeholder`]).
    pub is_placeholder: bool,
    /// Greyed until a module handles the action ([`MenuItemSpec::needs_handler`]).
    pub handler_needed: bool,
    /// The letter Alt picks on Windows (AE's underlined letter), if any.
    pub mnemonic: Option<char>,
    enabled: Option<Arc<MenuEnabled>>,
    checked: Option<Arc<MenuChecked>>,
}

impl std::fmt::Debug for MenuItemSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuItemSpec")
            .field("path", &self.path)
            .field("title", &self.title)
            .field("action", &self.action)
            .field("params", &self.params)
            .field("group", &self.group)
            .field("order", &self.order)
            .field("levels", &self.levels)
            .field("checkable", &self.checked.is_some())
            .field("placeholder", &self.is_placeholder)
            .field("needs_handler", &self.handler_needed)
            .finish_non_exhaustive()
    }
}

impl MenuItemSpec {
    /// Menu label in the active language.
    pub fn display_title(&self) -> &str {
        if crate::i18n::lang() == crate::i18n::Lang::Zh
            && let Some(title) = &self.title_zh
        {
            return title;
        }
        crate::i18n::tr(&self.title)
    }

    /// An item in `path` (English menu titles) running `action`.
    pub fn new(path: &[&str], title: impl Into<String>, action: impl Into<String>) -> Self {
        MenuItemSpec {
            path: path.iter().map(|s| (*s).to_owned()).collect(),
            title: title.into(),
            title_zh: None,
            action: action.into(),
            params: serde_json::Value::Null,
            group: 0,
            order: 0,
            workspace: None,
            levels: Vec::new(),
            is_placeholder: false,
            handler_needed: false,
            mnemonic: None,
            enabled: None,
            checked: None,
        }
    }

    /// The Windows menu mnemonic (Alt+letter).
    pub fn mnemonic(mut self, letter: Option<char>) -> Self {
        self.mnemonic = letter;
        self
    }

    pub fn params(mut self, params: serde_json::Value) -> Self {
        self.params = params;
        self
    }

    pub fn group(mut self, group: i32) -> Self {
        self.group = group;
        self
    }

    pub fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    pub fn workspace(mut self, workspace: impl Into<String>) -> Self {
        self.workspace = Some(workspace.into());
        self
    }

    pub fn enabled_when(
        mut self,
        f: impl Fn(&MenuContext<'_>) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.enabled = Some(Arc::new(f));
        self
    }

    /// Position at every level of the path, top-level menu first; the last one
    /// is the item's own (also sets `group` / `order`).
    pub fn at(mut self, levels: Vec<MenuPos>) -> Self {
        if let Some(own) = levels.last() {
            self.group = own.group;
            self.order = own.order;
        }
        self.levels = levels;
        self
    }

    /// A checkable item (AE: Window panels, Layer ▸ Switches, View ▸ Show Grid):
    /// `f` says whether it shows a check mark.
    pub fn checked_when(
        mut self,
        f: impl Fn(&MenuContext<'_>) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.checked = Some(Arc::new(f));
        self
    }

    /// Holds a place without being shown: the submenu of its path exists even
    /// when nothing else is in it (AE's Edit ▸ History before any step, Open
    /// Recent with no recent project, run-time lists).
    pub fn placeholder(mut self) -> Self {
        self.is_placeholder = true;
        self
    }

    /// Greyed until a module handles the action ([`MenuContext::handles`]): the
    /// AE items whose command another module implements light up once it
    /// registers a handler for it.
    pub fn needs_handler(mut self) -> Self {
        self.handler_needed = true;
        self
    }

    /// What [`MenuItemSpec::needs_handler`] looks for in [`handled_actions`]: the
    /// action id, or `window.panel:<kind>` for a Window panel item (a real panel
    /// of that kind, not a placeholder).
    pub fn handler_key(&self) -> String {
        handler_key(&self.action, &self.params)
    }

    /// Whether the item is enabled now: its action can run
    /// ([`MenuItemSpec::needs_handler`]) and its check passes (always without one).
    pub fn is_enabled(&self, cx: &MenuContext<'_>) -> bool {
        !self.is_placeholder
            && (!self.handler_needed || cx.handles(&self.handler_key()))
            && self.enabled.as_ref().is_none_or(|f| f(cx))
    }

    /// Whether the item can show a check mark.
    pub fn is_checkable(&self) -> bool {
        self.checked.is_some()
    }

    /// Whether the item shows a check mark now.
    pub fn is_checked(&self, cx: &MenuContext<'_>) -> bool {
        self.checked.as_ref().is_some_and(|f| f(cx))
    }

    /// Position at level `depth` of the path (0 = the top-level menu's entries).
    fn pos(&self, depth: usize) -> MenuPos {
        self.levels
            .get(depth)
            .copied()
            .unwrap_or(MenuPos::new(self.group, self.order))
    }

    /// The invocation the item runs (also what a shortcut for it carries).
    pub fn invocation(&self) -> Invocation {
        Invocation::with_params(self.action.clone(), self.params.clone())
    }
}

/// An entry of a resolved menu.
#[derive(Clone, Debug)]
pub enum MenuEntry {
    /// A registered item (`id` = its registry id).
    Item {
        id: String,
        spec: Arc<MenuItemSpec>,
    },
    Separator,
    /// A submenu with its English title.
    Submenu {
        title: String,
        entries: Vec<MenuEntry>,
    },
}

impl MenuEntry {
    /// English title (empty for a separator).
    pub fn title(&self) -> &str {
        match self {
            MenuEntry::Item { spec, .. } => &spec.title,
            MenuEntry::Submenu { title, .. } => title,
            MenuEntry::Separator => "",
        }
    }
}

/// The contributed menus, ready to render.
#[derive(Clone, Debug, Default)]
pub struct MenuTree {
    /// `(English title, entries)` of each top-level menu, in display order.
    pub menus: Vec<(String, Vec<MenuEntry>)>,
    /// Items dropped because another item had the same path and title.
    pub conflicts: Vec<String>,
}

impl MenuTree {
    /// All items, depth first.
    pub fn items(&self) -> Vec<(&str, &Arc<MenuItemSpec>)> {
        fn walk<'a>(entries: &'a [MenuEntry], out: &mut Vec<(&'a str, &'a Arc<MenuItemSpec>)>) {
            for e in entries {
                match e {
                    MenuEntry::Item { id, spec } => out.push((id, spec)),
                    MenuEntry::Submenu { entries, .. } => walk(entries, out),
                    MenuEntry::Separator => {}
                }
            }
        }
        let mut out = Vec::new();
        for (_, entries) in &self.menus {
            walk(entries, &mut out);
        }
        out
    }
}

/// A node while building: items and submenus of one menu level.
#[derive(Default)]
struct Level {
    /// `(position, registration index, entry)`; submenus use their first item's.
    entries: Vec<(MenuPos, usize, Node)>,
}

enum Node {
    Item(String, Arc<MenuItemSpec>),
    Sub(String, Level),
}

impl Level {
    fn insert(&mut self, depth: usize, index: usize, id: &str, spec: &Arc<MenuItemSpec>) {
        let pos = spec.pos(depth);
        let key = (pos.group, pos.order, index);
        match spec.path[depth + 1..].split_first() {
            None => self
                .entries
                .push((pos, index, Node::Item(id.to_owned(), spec.clone()))),
            Some((sub, _)) => {
                let found = self
                    .entries
                    .iter()
                    .position(|(_, _, n)| matches!(n, Node::Sub(t, _) if t == sub));
                let i = match found {
                    Some(i) => {
                        // A submenu sorts by its earliest item.
                        let (p, at, _) = &self.entries[i];
                        if key < (p.group, p.order, *at) {
                            self.entries[i].0 = pos;
                            self.entries[i].1 = index;
                        }
                        i
                    }
                    None => {
                        self.entries
                            .push((pos, index, Node::Sub(sub.clone(), Level::default())));
                        self.entries.len() - 1
                    }
                };
                if let Node::Sub(_, level) = &mut self.entries[i].2 {
                    level.insert(depth + 1, index, id, spec);
                }
            }
        }
    }

    /// Entries by `(group, order, registration)`; within a group the entries
    /// marked [`MenuPos::sorted`] come first, by their title as `localize` shows
    /// it (AE sorts its panel list, workspaces and effects by the localized name:
    /// alphabetically in English, by code point in Chinese).
    fn finish(mut self, localize: &dyn Fn(&str) -> String) -> Vec<MenuEntry> {
        self.entries.sort_by_cached_key(|(pos, index, node)| {
            let title = match node {
                Node::Item(_, spec) if pos.sorted => localize(&spec.title),
                Node::Sub(title, _) if pos.sorted => localize(title),
                _ => String::new(),
            };
            let unsorted = (!pos.sorted).then_some((pos.order, *index));
            (pos.group, unsorted, title)
        });
        let mut out = Vec::new();
        let mut last_group = None;
        for (pos, _, node) in self.entries {
            if matches!(&node, Node::Item(_, spec) if spec.is_placeholder) {
                continue;
            }
            if last_group.is_some_and(|g| g != pos.group) {
                out.push(MenuEntry::Separator);
            }
            last_group = Some(pos.group);
            out.push(match node {
                Node::Item(id, spec) => MenuEntry::Item { id, spec },
                Node::Sub(title, level) => MenuEntry::Submenu {
                    title,
                    entries: level.finish(localize),
                },
            });
        }
        out
    }
}

/// What [`handled_actions`] lists for an invocation: the action id, or
/// `window.panel:<kind>` for opening a panel (a real panel of that kind).
pub fn handler_key(action: &str, params: &serde_json::Value) -> String {
    match params.get("id").and_then(|v| v.as_str()) {
        Some(kind) if action == crate::action::ids::OPEN_PANEL => format!("{action}:{kind}"),
        _ => action.to_owned(),
    }
}

/// Action ids the registered modules handle: their [`ActionHandler`]s' actions,
/// the UI actions described in `Registry<ActionInfo>` (panel actions), and
/// `window.panel:<kind>` for every real (not placeholder) panel kind. The shell
/// passes them to [`MenuContext::with_actions`].
pub fn handled_actions(
    handlers: &RegistrySnapshot<dyn ActionHandler>,
    infos: &RegistrySnapshot<ActionInfo>,
    panels: &RegistrySnapshot<crate::PanelKind>,
) -> HashSet<String> {
    let mut out: HashSet<String> = infos.iter().map(|e| e.id().to_string()).collect();
    for e in handlers.iter() {
        out.extend(e.item().actions());
    }
    for e in panels.iter() {
        if !e.item().is_placeholder {
            out.insert(format!("{}:{}", crate::action::ids::OPEN_PANEL, e.id()));
        }
    }
    out
}

/// Resolve the registered items into menus (see the module docs); sorted levels
/// follow the English titles.
pub fn build_menus(items: &RegistrySnapshot<MenuItemSpec>, active_workspace: &str) -> MenuTree {
    build_menus_localized(items, active_workspace, &|t| t.to_owned())
}

/// [`build_menus`] with the entries AE sorts by name ([`MenuPos::sorted`]) in the
/// order of their titles as `localize` shows them (the shell passes `tr`).
pub fn build_menus_localized(
    items: &RegistrySnapshot<MenuItemSpec>,
    active_workspace: &str,
    localize: &dyn Fn(&str) -> String,
) -> MenuTree {
    // Priority: items of the active workspace first, then registration order.
    let mut ranked: Vec<(usize, &str, Arc<MenuItemSpec>)> = items
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.item().path.is_empty())
        .map(|(i, e)| (i, e.id().as_ref(), e.item().clone()))
        .collect();
    ranked.sort_by_key(|(i, _, spec)| (spec.workspace.as_deref() != Some(active_workspace), *i));
    let mut tree = MenuTree::default();
    let mut seen: HashMap<(&[String], &str), &str> = HashMap::new();
    let mut kept: Vec<(usize, &str, Arc<MenuItemSpec>)> = Vec::new();
    for &(i, id, ref spec) in &ranked {
        if let Some(winner) = seen.get(&(spec.path.as_slice(), spec.title.as_str())) {
            let message = format!(
                "menu item `{id}` ({} > {}) conflicts with `{winner}`; `{winner}` is used",
                spec.path.join(" > "),
                spec.title
            );
            tracing::warn!("{message}");
            tree.conflicts.push(message);
            continue;
        }
        seen.insert((spec.path.as_slice(), spec.title.as_str()), id);
        kept.push((i, id, spec.clone()));
    }
    kept.sort_by_key(|(i, ..)| *i);
    let mut tops: Vec<(String, Level)> = Vec::new();
    for (i, id, spec) in kept {
        let top = &spec.path[0];
        let idx = match tops.iter().position(|(t, _)| t == top) {
            Some(idx) => idx,
            None => {
                tops.push((top.clone(), Level::default()));
                tops.len() - 1
            }
        };
        tops[idx].1.insert(0, i, id, &spec);
    }
    let rank = |t: &str| MENU_ORDER.iter().position(|m| *m == t);
    // Known menus in AE order; unknown ones before Window/Help, in first use order.
    let window = rank("Window").unwrap_or(usize::MAX);
    tops.sort_by_key(|(t, _)| rank(t).unwrap_or(window - 1));
    tree.menus = tops
        .into_iter()
        .map(|(title, level)| (title, level.finish(localize)))
        .collect();
    tree
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_app::{Owner, Registry};
    use serde_json::json;

    fn reg(items: Vec<(&str, MenuItemSpec)>) -> Registry<MenuItemSpec> {
        let r = Registry::new();
        for (id, item) in items {
            r.register(Owner::Core, id, item).unwrap();
        }
        r
    }

    fn titles(entries: &[MenuEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                MenuEntry::Item { spec, .. } => spec.title.clone(),
                MenuEntry::Separator => "---".into(),
                MenuEntry::Submenu { title, .. } => format!("{title} ▸"),
            })
            .collect()
    }

    #[test]
    fn menus_are_ordered_grouped_and_nested() {
        let r = reg(vec![
            (
                "anim.ease",
                MenuItemSpec::new(
                    &["Animation", "Keyframe Assistant"],
                    "Easy Ease",
                    "motion.ease",
                ),
            ),
            (
                "layer.solid",
                MenuItemSpec::new(&["Layer", "New"], "Solid", "motion.new_layer")
                    .params(json!({"kind": "solid"}))
                    .order(2),
            ),
            (
                "layer.text",
                MenuItemSpec::new(&["Layer", "New"], "Text", "motion.new_layer")
                    .params(json!({"kind": "text"}))
                    .order(1),
            ),
            (
                "layer.rect",
                MenuItemSpec::new(&["Layer", "New"], "Rectangle", "motion.new_layer").group(1),
            ),
            (
                "layer.arrange",
                MenuItemSpec::new(
                    &["Layer", "Arrange"],
                    "Bring Layer to Front",
                    "motion.order",
                )
                .group(2),
            ),
            (
                "comp.trim",
                MenuItemSpec::new(
                    &["Composition"],
                    "Trim Comp to Work Area",
                    "motion.trim_comp",
                )
                .group(1),
            ),
            (
                "comp.new",
                MenuItemSpec::new(&["Composition"], "New Composition", "motion.new_comp"),
            ),
            ("custom", MenuItemSpec::new(&["Plugins"], "Do It", "x.do")),
        ]);
        let tree = build_menus(&r.snapshot(), "motion");
        let tops: Vec<&str> = tree.menus.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(tops, ["Composition", "Layer", "Animation", "Plugins"]);
        assert_eq!(
            titles(&tree.menus[0].1),
            ["New Composition", "---", "Trim Comp to Work Area"]
        );
        assert_eq!(titles(&tree.menus[1].1), ["New ▸", "---", "Arrange ▸"]);
        let MenuEntry::Submenu { entries, .. } = &tree.menus[1].1[0] else {
            panic!("submenu");
        };
        assert_eq!(titles(entries), ["Text", "Solid", "---", "Rectangle"]);
        assert_eq!(tree.items().len(), 8);
        assert!(tree.conflicts.is_empty());
    }

    #[test]
    fn conflicts_prefer_the_active_workspace_then_registration() {
        let r = reg(vec![
            ("a", MenuItemSpec::new(&["Layer"], "New Thing", "a.run")),
            (
                "b",
                MenuItemSpec::new(&["Layer"], "New Thing", "b.run").workspace("edit"),
            ),
            ("c", MenuItemSpec::new(&["Layer"], "New Thing", "c.run")),
        ]);
        let motion = build_menus(&r.snapshot(), "motion");
        assert_eq!(motion.items()[0].1.action, "a.run");
        assert_eq!(motion.conflicts.len(), 2);
        let edit = build_menus(&r.snapshot(), "edit");
        assert_eq!(edit.items()[0].1.action, "b.run");
        assert_eq!(edit.items().len(), 1);
    }

    #[test]
    fn enabled_checks_and_invocations() {
        let item = MenuItemSpec::new(&["Layer"], "X", "motion.new_layer")
            .params(json!({"kind": "null"}))
            .enabled_when(|cx| cx.workspace == "motion");
        let stage = Stage::new();
        let resources = Resources::new();
        let cx = |w| MenuContext::new(&stage, &resources, w);
        assert!(item.is_enabled(&cx("motion")));
        assert!(!item.is_enabled(&cx("review")));
        let inv = item.invocation();
        assert_eq!(inv.id, "motion.new_layer");
        assert_eq!(inv.params, json!({"kind": "null"}));
        assert!(!item.is_checkable());

        let panel = MenuItemSpec::new(&["Window"], "Info", "window.panel")
            .checked_when(|cx| cx.panel_open("info"));
        let open = [PanelId::new("info")];
        assert!(panel.is_checkable());
        assert!(!panel.is_checked(&cx("motion")));
        assert!(
            panel.is_checked(
                &MenuContext::new(&stage, &resources, "motion").with_open_panels(&open)
            )
        );
    }

    /// Per-level positions put submenus and separators exactly where AE has
    /// them, even when a submenu has separators of its own.
    #[test]
    fn levels_place_submenus_and_separators() {
        let p = MenuPos::new;
        let r = reg(vec![
            (
                "mask.new",
                MenuItemSpec::new(&["Layer", "Mask"], "New Mask", "a").at(vec![p(0, 0), p(0, 0)]),
            ),
            (
                "mask.reset",
                MenuItemSpec::new(&["Layer", "Mask"], "Reset Mask", "b").at(vec![p(0, 0), p(1, 1)]),
            ),
            (
                "path.closed",
                MenuItemSpec::new(&["Layer", "Mask and Shape Path"], "Closed", "c")
                    .at(vec![p(0, 1), p(0, 0)]),
            ),
            (
                "layer.precompose",
                MenuItemSpec::new(&["Layer"], "Pre-compose...", "d").at(vec![p(1, 2)]),
            ),
        ]);
        let tree = build_menus(&r.snapshot(), "motion");
        let layer = &tree.menus[0].1;
        assert_eq!(
            titles(layer),
            ["Mask ▸", "Mask and Shape Path ▸", "---", "Pre-compose..."]
        );
        let MenuEntry::Submenu { entries, .. } = &layer[0] else {
            panic!("Mask");
        };
        assert_eq!(titles(entries), ["New Mask", "---", "Reset Mask"]);
    }

    /// Sorted levels follow the localized titles within each group.
    #[test]
    fn sorted_levels_follow_the_localized_titles() {
        let p = MenuPos::new;
        let item = |title: &str, order| {
            MenuItemSpec::new(&["Window"], title, "window.panel").at(vec![p(1, order).sorted()])
        };
        let r = reg(vec![
            (
                "ws",
                MenuItemSpec::new(&["Window"], "Workspace", "w").at(vec![p(0, 0)]),
            ),
            ("info", item("Info", 1)),
            ("align", item("Align", 2)),
            ("character", item("Character", 3)),
        ]);
        let tree = build_menus(&r.snapshot(), "motion");
        assert_eq!(
            titles(&tree.menus[0].1),
            ["Workspace", "---", "Align", "Character", "Info"]
        );
        let zh = |t: &str| {
            match t {
                "Info" => "信息",
                "Align" => "对齐",
                "Character" => "字符",
                other => other,
            }
            .to_owned()
        };
        let tree = build_menus_localized(&r.snapshot(), "motion", &zh);
        // AE's Chinese menu: 信息 (U+4FE1) < 字符 (U+5B57) < 对齐 (U+5BF9).
        assert_eq!(
            titles(&tree.menus[0].1),
            ["Workspace", "---", "Info", "Character", "Align"]
        );
    }

    /// Placeholders make their submenu exist without being shown (AE's empty
    /// Edit ▸ History) and add no separator.
    #[test]
    fn placeholders_keep_empty_submenus() {
        let p = MenuPos::new;
        let r = reg(vec![
            (
                "undo",
                MenuItemSpec::new(&["Edit"], "Undo", "edit.undo").at(vec![p(0, 0)]),
            ),
            (
                "history",
                MenuItemSpec::new(&["Edit", "History"], "(steps)", "x")
                    .at(vec![p(0, 1), p(0, 0)])
                    .placeholder(),
            ),
            (
                "recent",
                MenuItemSpec::new(&["Edit"], "(recent)", "y")
                    .at(vec![p(1, 2)])
                    .placeholder(),
            ),
            (
                "cut",
                MenuItemSpec::new(&["Edit"], "Cut", "edit.cut").at(vec![p(2, 3)]),
            ),
        ]);
        let tree = build_menus(&r.snapshot(), "motion");
        let edit = &tree.menus[0].1;
        assert_eq!(titles(edit), ["Undo", "History ▸", "---", "Cut"]);
        let MenuEntry::Submenu { entries, .. } = &edit[1] else {
            panic!("History");
        };
        assert!(entries.is_empty());
        assert_eq!(tree.items().len(), 2);
    }

    struct Empty;
    impl crate::Panel for Empty {
        fn ui(&mut self, _: &mut egui::Ui, _: &mut crate::panel::PanelContext<'_>) {}
    }

    /// `needs_handler` items stay greyed until a module handles the action.
    #[test]
    fn needs_handler_items_follow_the_handlers() {
        struct Handler;
        impl ActionHandler for Handler {
            fn actions(&self) -> Vec<String> {
                vec!["motion.view.show_grid".into()]
            }
            fn handle(&self, _: &Invocation, _: &mut crate::panel::PanelContext<'_>) -> bool {
                true
            }
        }
        let item =
            MenuItemSpec::new(&["View"], "Show Grid", "motion.view.show_grid").needs_handler();
        let other =
            MenuItemSpec::new(&["View"], "Show Rulers", "motion.view.show_rulers").needs_handler();
        let stage = Stage::new();
        let resources = Resources::new();
        let cx = MenuContext::new(&stage, &resources, "motion");
        assert!(!item.is_enabled(&cx), "no handler list");
        let handlers: Registry<dyn ActionHandler> = Registry::new();
        handlers
            .register_arc(
                Owner::Core,
                "grid",
                Arc::new(Handler) as Arc<dyn ActionHandler>,
            )
            .unwrap();
        let infos: Registry<ActionInfo> = Registry::new();
        infos
            .register(Owner::Core, "viewer.fit", ActionInfo::new("Fit"))
            .unwrap();
        let panels: Registry<crate::PanelKind> = Registry::new();
        panels
            .register(Owner::Core, "info", crate::PanelKind::placeholder("Info"))
            .unwrap();
        panels
            .register(
                Owner::Core,
                "project",
                crate::PanelKind::new("Project", |_| Box::new(Empty)),
            )
            .unwrap();
        let actions = handled_actions(&handlers.snapshot(), &infos.snapshot(), &panels.snapshot());
        assert!(actions.contains("viewer.fit"));
        let cx = cx.with_actions(&actions);
        assert!(item.is_enabled(&cx));
        assert!(!other.is_enabled(&cx));
        let panel = |id: &str| {
            MenuItemSpec::new(&["Window"], id, crate::action::ids::OPEN_PANEL)
                .params(serde_json::json!({ "id": id, "toggle": true }))
                .needs_handler()
        };
        assert!(panel("project").is_enabled(&cx));
        assert!(!panel("info").is_enabled(&cx), "placeholder panel");
    }
}
