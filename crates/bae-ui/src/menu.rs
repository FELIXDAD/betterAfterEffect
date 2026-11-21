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
//! between groups. Two items with the same menu path and title conflict: the one
//! registered for the active workspace wins, otherwise the first registered; the
//! loser is dropped and reported in [`MenuTree::conflicts`] (and logged).
//!
//! The shell renders the tree natively (muda) and runs a chosen item's
//! [`MenuItemSpec::invocation`] through the normal dispatch: focused panel →
//! module action handlers ([`crate::ActionHandler`]) → shell actions → model
//! commands.

use std::sync::Arc;

use bae_app::{RegistrySnapshot, Resources};
use bae_model::Stage;

use crate::action::Invocation;

/// Top-level menus in After Effects order. Contributed menus not listed here come
/// after them (before Window and Help), in first-registration order.
pub const MENU_ORDER: &[&str] = &[
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

/// What an item's enabled check can look at.
pub struct MenuContext<'a> {
    pub stage: &'a Stage,
    pub resources: &'a Resources,
    /// Id of the active workspace.
    pub workspace: &'a str,
}

/// Decides whether an item is enabled (e.g. "Layer ▸ New" needs an active comp).
pub type MenuEnabled = dyn Fn(&MenuContext<'_>) -> bool + Send + Sync;

/// One menu item contributed by a module.
#[derive(Clone)]
pub struct MenuItemSpec {
    /// English titles of the top-level menu and any submenus, e.g. `["Layer", "New"]`.
    pub path: Vec<String>,
    /// English title of the item.
    pub title: String,
    /// Command id run when the item is chosen.
    pub action: String,
    /// Parameters of the command (`Null` = none).
    pub params: serde_json::Value,
    /// Items are sorted by group, then order; groups are separated by a line.
    pub group: i32,
    pub order: i32,
    /// Workspace of the contributing module: it wins conflicts while active.
    pub workspace: Option<String>,
    enabled: Option<Arc<MenuEnabled>>,
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
            .finish_non_exhaustive()
    }
}

impl MenuItemSpec {
    /// An item in `path` (English menu titles) running `action`.
    pub fn new(path: &[&str], title: impl Into<String>, action: impl Into<String>) -> Self {
        MenuItemSpec {
            path: path.iter().map(|s| (*s).to_owned()).collect(),
            title: title.into(),
            action: action.into(),
            params: serde_json::Value::Null,
            group: 0,
            order: 0,
            workspace: None,
            enabled: None,
        }
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

    /// Whether the item is enabled now (always, without a check).
    pub fn is_enabled(&self, cx: &MenuContext<'_>) -> bool {
        self.enabled.as_ref().is_none_or(|f| f(cx))
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
    /// `(group, order, registration index, entry)`; submenus use their first item's key.
    entries: Vec<((i32, i32, usize), Node)>,
}

enum Node {
    Item(String, Arc<MenuItemSpec>),
    Sub(String, Level),
}

impl Level {
    fn insert(
        &mut self,
        rest: &[String],
        key: (i32, i32, usize),
        id: &str,
        spec: &Arc<MenuItemSpec>,
    ) {
        match rest.split_first() {
            None => self
                .entries
                .push((key, Node::Item(id.to_owned(), spec.clone()))),
            Some((sub, rest)) => {
                let pos = self
                    .entries
                    .iter()
                    .position(|(_, n)| matches!(n, Node::Sub(t, _) if t == sub));
                let i = match pos {
                    Some(i) => {
                        // A submenu sorts by its earliest item.
                        if key < self.entries[i].0 {
                            self.entries[i].0 = key;
                        }
                        i
                    }
                    None => {
                        self.entries
                            .push((key, Node::Sub(sub.clone(), Level::default())));
                        self.entries.len() - 1
                    }
                };
                if let Node::Sub(_, level) = &mut self.entries[i].1 {
                    level.insert(rest, key, id, spec);
                }
            }
        }
    }

    fn finish(mut self) -> Vec<MenuEntry> {
        self.entries.sort_by_key(|(k, _)| *k);
        let mut out = Vec::new();
        let mut last_group = None;
        for ((group, _, _), node) in self.entries {
            if last_group.is_some_and(|g| g != group) {
                out.push(MenuEntry::Separator);
            }
            last_group = Some(group);
            out.push(match node {
                Node::Item(id, spec) => MenuEntry::Item { id, spec },
                Node::Sub(title, level) => MenuEntry::Submenu {
                    title,
                    entries: level.finish(),
                },
            });
        }
        out
    }
}

/// Resolve the registered items into menus (see the module docs).
pub fn build_menus(items: &RegistrySnapshot<MenuItemSpec>, active_workspace: &str) -> MenuTree {
    // Priority: items of the active workspace first, then registration order.
    let mut ranked: Vec<(usize, &str, Arc<MenuItemSpec>)> = items
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.item().path.is_empty())
        .map(|(i, e)| (i, e.id().as_ref(), e.item().clone()))
        .collect();
    ranked.sort_by_key(|(i, _, spec)| (spec.workspace.as_deref() != Some(active_workspace), *i));
    let mut tree = MenuTree::default();
    let mut seen: Vec<(Vec<String>, String, String)> = Vec::new();
    let mut kept: Vec<(usize, &str, Arc<MenuItemSpec>)> = Vec::new();
    for (i, id, spec) in ranked {
        if let Some((_, _, winner)) = seen
            .iter()
            .find(|(path, title, _)| *path == spec.path && *title == spec.title)
        {
            let message = format!(
                "menu item `{id}` ({} > {}) conflicts with `{winner}`; `{winner}` is used",
                spec.path.join(" > "),
                spec.title
            );
            tracing::warn!("{message}");
            tree.conflicts.push(message);
            continue;
        }
        seen.push((spec.path.clone(), spec.title.clone(), id.to_owned()));
        kept.push((i, id, spec));
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
        let key = (spec.group, spec.order, i);
        tops[idx].1.insert(&spec.path[1..], key, id, &spec);
    }
    let rank = |t: &str| MENU_ORDER.iter().position(|m| *m == t);
    // Known menus in AE order; unknown ones before Window/Help, in first use order.
    let window = rank("Window").unwrap_or(usize::MAX);
    tops.sort_by_key(|(t, _)| rank(t).unwrap_or(window - 1));
    tree.menus = tops
        .into_iter()
        .map(|(title, level)| (title, level.finish()))
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
        let cx = |w| MenuContext {
            stage: &stage,
            resources: &resources,
            workspace: w,
        };
        assert!(item.is_enabled(&cx("motion")));
        assert!(!item.is_enabled(&cx("review")));
        let inv = item.invocation();
        assert_eq!(inv.id, "motion.new_layer");
        assert_eq!(inv.params, json!({"kind": "null"}));
    }
}
