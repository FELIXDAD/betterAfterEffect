//! The command tree of the Keyboard Shortcuts dialog, grouped like AE's
//! (docs/reference/ae/mac/ui/keyboard-shortcuts.json, `command_tree`):
//!
//! - **Application**: the tools first, then one group per menu in menu order
//!   (submenus are nested groups; no application or Help menu group), then the
//!   global commands that are in no menu.
//! - **Panels**: one group per panel scope (AE's names: "Timeline Panel" …),
//!   alphabetical, with the commands bound or registered for that panel.
//!
//! A row is one command in one scope: the same command id with other parameters
//! or in another panel is another row with its own shortcuts.

use std::collections::{HashMap, HashSet};

use crate::i18n::tr;
use crate::keymap::{Chord, Keymap};
use crate::menu::{APP_MENU, MenuEntry};
use crate::workbench::Workbench;

use super::CommandEntry;
use super::text;

/// Identifies a row: command id, parameters (JSON text) and scope.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RowKey {
    pub action: String,
    pub params: String,
    pub when: Option<String>,
}

impl RowKey {
    pub fn of(e: &CommandEntry) -> Self {
        RowKey {
            action: e.id.clone(),
            params: params_key(&e.params),
            when: e.panel.clone(),
        }
    }
}

/// `params` as a map key (`Null` and `{}` are the same: no parameters).
pub fn params_key(params: &serde_json::Value) -> String {
    match params {
        serde_json::Value::Object(m) if m.is_empty() => String::new(),
        serde_json::Value::Null => String::new(),
        p => p.to_string(),
    }
}

/// A panel scope of the Commands menu and of the Panels group: AE's name and
/// the BAE panel kinds it covers.
#[derive(Clone, Debug)]
pub struct Scope {
    /// English title ("Timeline Panel").
    pub title: String,
    pub kinds: Vec<String>,
}

/// AE's panel scopes (Mac 26.2.1: the Commands menu after Application) and
/// the BAE panel kinds in each. Kinds not listed get a scope of their own.
const AE_SCOPES: &[(&str, &[&str])] = &[
    (
        "Character and Paragraph Panels",
        &["text_editing", "character", "paragraph"],
    ),
    ("Composition Panel", &["composition", "camera_tool"]),
    ("Effect Controls Panel", &["effect_controls"]),
    ("Flowchart Panel", &["flowchart"]),
    // AE 26's Mac list has no Footage Panel: footage is the other item viewer.
    ("Layer Panel", &["layer", "footage"]),
    ("Paint Panel", &["paint_tool", "clone_tool", "paint"]),
    ("Project Panel", &["project"]),
    ("Render Queue Panel", &["render_queue"]),
    ("Timeline Panel", &["timeline"]),
    ("Tracker Panel", &["tracker"]),
    ("Viewer Panel", &["viewer"]),
];

/// Scopes of BAE panel kinds AE has no Commands entry for.
const EXTRA_SCOPES: &[(&str, &[&str])] = &[("Dialog Boxes", &["dialog"])];

/// One node of the tree, in pre-order.
#[derive(Clone, Debug)]
pub struct Node {
    pub depth: usize,
    pub parent: Option<usize>,
    /// Index after the node's last descendant.
    pub end: usize,
    /// Stable key of a group (expansion state): `app/File/New`, `panels/Timeline Panel`.
    pub key: String,
    /// Title as shown, in the UI language.
    pub title: String,
    /// Lowercase [`Node::title`] for search.
    pub lower: String,
    /// The command of a row; `None` for a group.
    pub entry: Option<CommandEntry>,
    /// Index into [`Tree::scopes`] of a panel group.
    pub scope: Option<usize>,
}

impl Node {
    pub fn is_group(&self) -> bool {
        self.entry.is_none()
    }
}

/// The whole tree, built when the dialog opens.
#[derive(Clone, Debug, Default)]
pub struct Tree {
    pub nodes: Vec<Node>,
    /// The panel scopes in Commands-menu order (localized alphabetical): AE's
    /// eleven, then BAE's own that have commands.
    pub scopes: Vec<Scope>,
}

/// Root group keys.
pub const APP_KEY: &str = "app";
pub const PANELS_KEY: &str = "panels";

struct Builder {
    nodes: Vec<Node>,
    stack: Vec<usize>,
    rows: HashSet<RowKey>,
}

impl Builder {
    fn open(&mut self, key: String, title: String, scope: Option<usize>) {
        let depth = self.stack.len();
        self.nodes.push(Node {
            depth,
            parent: self.stack.last().copied(),
            end: 0,
            key,
            lower: title.to_lowercase(),
            title,
            entry: None,
            scope,
        });
        self.stack.push(self.nodes.len() - 1);
    }

    fn close(&mut self) {
        let i = self.stack.pop().expect("open group");
        // Empty groups (a menu with only separators) are dropped.
        if self.nodes.len() == i + 1 {
            self.nodes.pop();
        } else {
            self.nodes[i].end = self.nodes.len();
        }
    }

    /// Adds a command row once (the first place it is listed wins).
    fn row(&mut self, entry: CommandEntry, title: String) -> bool {
        if !self.rows.insert(RowKey::of(&entry)) {
            return false;
        }
        self.nodes.push(Node {
            depth: self.stack.len(),
            parent: self.stack.last().copied(),
            end: self.nodes.len() + 1,
            key: String::new(),
            lower: title.to_lowercase(),
            title,
            entry: Some(entry),
            scope: None,
        });
        true
    }
}

fn is_tool(id: &str) -> bool {
    id.contains(".tool.") || id.starts_with("tool.")
}

impl Workbench {
    /// The command tree for `keymap` (the keymap being edited, so commands the
    /// user bound are listed too).
    pub(super) fn shortcut_tree(&self, keymap: &Keymap) -> Tree {
        let mut b = Builder {
            nodes: Vec::new(),
            stack: Vec::new(),
            rows: HashSet::new(),
        };
        let entries = self.shortcut_commands_for(keymap);
        let global = |e: &&CommandEntry| e.panel.is_none();

        // Application: tools, menus, the rest.
        b.open(APP_KEY.into(), tr("Application").into(), None);
        for e in entries.iter().filter(global).filter(|e| is_tool(&e.id)) {
            b.row(e.clone(), self.command_title(&e.id, &e.params));
        }
        let menus =
            crate::menu::build_menus_localized(&self.menus.snapshot(), &self.active, &|t| {
                tr(t).to_owned()
            });
        fn walk(b: &mut Builder, key: &str, entries: &[MenuEntry]) {
            for entry in entries {
                match entry {
                    MenuEntry::Item { spec, .. } => {
                        if spec.is_placeholder {
                            continue;
                        }
                        let e = CommandEntry {
                            id: spec.action.clone(),
                            params: spec.params.clone(),
                            title: spec.title.clone(),
                            panel: None,
                        };
                        b.row(e, spec.display_title().to_owned());
                    }
                    MenuEntry::Submenu { title, entries } => {
                        let key = format!("{key}/{title}");
                        b.open(key.clone(), tr(title).to_owned(), None);
                        walk(b, &key, entries);
                        b.close();
                    }
                    MenuEntry::Separator => {}
                }
            }
        }
        for (title, entries) in &menus.menus {
            if title == APP_MENU || title == "Help" {
                continue;
            }
            let key = format!("{APP_KEY}/{title}");
            b.open(key.clone(), tr(title).to_owned(), None);
            walk(&mut b, &key, entries);
            b.close();
        }
        for e in entries.iter().filter(global) {
            b.row(e.clone(), self.command_title(&e.id, &e.params));
        }
        b.close();

        // Panels: one group per scope.
        let scopes = self.shortcut_scopes(&entries);
        b.open(PANELS_KEY.into(), tr("Panels").into(), None);
        for (i, scope) in scopes.iter().enumerate() {
            b.open(
                format!("{PANELS_KEY}/{}", scope.title),
                tr(&scope.title).to_owned(),
                Some(i),
            );
            for e in entries.iter().filter(|e| {
                e.panel
                    .as_deref()
                    .is_some_and(|k| scope.kinds.iter().any(|s| s == k))
            }) {
                b.row(e.clone(), self.command_title(&e.id, &e.params));
            }
            b.close();
        }
        b.close();
        Tree {
            nodes: b.nodes,
            scopes,
        }
    }

    /// The panel scopes for the Commands menu: AE's eleven (always listed, like
    /// AE) and a scope for every other panel kind that has commands, sorted by
    /// their titles in the UI language.
    fn shortcut_scopes(&self, entries: &[CommandEntry]) -> Vec<Scope> {
        let mut scopes: Vec<Scope> = AE_SCOPES
            .iter()
            .map(|(t, k)| Scope {
                title: (*t).to_owned(),
                kinds: k.iter().map(|s| (*s).to_owned()).collect(),
            })
            .collect();
        for e in entries {
            let Some(kind) = e.panel.as_deref() else {
                continue;
            };
            if scopes.iter().any(|s| s.kinds.iter().any(|k| k == kind)) {
                continue;
            }
            let title = EXTRA_SCOPES
                .iter()
                .find(|(_, kinds)| kinds.contains(&kind))
                .map(|(t, _)| (*t).to_owned())
                .or_else(|| self.kinds.get(kind).map(|k| k.title.clone()))
                .unwrap_or_else(|| kind.to_owned());
            match scopes.iter_mut().find(|s| s.title == title) {
                Some(s) => s.kinds.push(kind.to_owned()),
                None => scopes.push(Scope {
                    title,
                    kinds: vec![kind.to_owned()],
                }),
            }
        }
        scopes.sort_by_cached_key(|s| tr(&s.title).to_lowercase());
        scopes
    }
}

/// Shortcuts of every row, in keymap order.
pub fn chords_by_row(keymap: &Keymap) -> HashMap<RowKey, Vec<Chord>> {
    let mut out: HashMap<RowKey, Vec<Chord>> = HashMap::new();
    for b in &keymap.bindings {
        let key = RowKey {
            action: b.action.clone(),
            params: params_key(&b.params),
            when: b.when.clone(),
        };
        let list = out.entry(key).or_default();
        if !list.contains(&b.chord) {
            list.push(b.chord);
        }
    }
    out
}

impl Tree {
    /// The rows to show: everything under expanded groups, or with a search
    /// query the commands whose name or shortcuts contain it, with their groups
    /// (all open).
    pub fn visible(
        &self,
        expanded: &HashSet<String>,
        query: &str,
        chords: &HashMap<RowKey, Vec<Chord>>,
    ) -> Vec<usize> {
        let mut out = Vec::new();
        if query.is_empty() {
            let mut i = 0;
            while i < self.nodes.len() {
                out.push(i);
                let n = &self.nodes[i];
                i = if n.is_group() && !expanded.contains(&n.key) {
                    n.end
                } else {
                    i + 1
                };
            }
            return out;
        }
        let query = query.to_lowercase();
        let mut keep = vec![false; self.nodes.len()];
        for (i, n) in self.nodes.iter().enumerate() {
            let Some(e) = &n.entry else { continue };
            let hit = n.lower.contains(&query)
                || chords.get(&RowKey::of(e)).is_some_and(|cs| {
                    cs.iter()
                        .any(|c| text::chord_text(c).to_lowercase().contains(&query))
                });
            if hit {
                let mut at = Some(i);
                while let Some(j) = at {
                    if keep[j] {
                        break;
                    }
                    keep[j] = true;
                    at = self.nodes[j].parent;
                }
            }
        }
        out.extend((0..self.nodes.len()).filter(|&i| keep[i]));
        out
    }

    /// The node of a row.
    pub fn find(&self, key: &RowKey) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.entry.as_ref().is_some_and(|e| RowKey::of(e) == *key))
    }

    /// The groups above node `i`, nearest first.
    pub fn ancestor_nodes(&self, i: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut at = self.nodes[i].parent;
        while let Some(j) = at {
            out.push(j);
            at = self.nodes[j].parent;
        }
        out
    }

    /// The node of every command row.
    pub fn row_index(&self) -> HashMap<RowKey, usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.entry.as_ref().map(|e| (RowKey::of(e), i)))
            .collect()
    }

    /// The group keys above node `i`.
    pub fn ancestors(&self, i: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut at = self.nodes[i].parent;
        while let Some(j) = at {
            out.push(self.nodes[j].key.clone());
            at = self.nodes[j].parent;
        }
        out
    }

    /// The group of panel scope `scope`.
    pub fn scope_group(&self, scope: usize) -> Option<usize> {
        self.nodes.iter().position(|n| n.scope == Some(scope))
    }

    /// The keys of the groups open when the dialog opens (AE: both roots).
    pub fn initially_expanded() -> HashSet<String> {
        [APP_KEY.to_owned(), PANELS_KEY.to_owned()].into()
    }
}
