//! The After Effects command catalog: every AE 26 main-menu item
//! (`docs/reference/ae/menus.json`) and every default shortcut command
//! (`docs/reference/ae/shortcuts.json`) mapped to a BAE action id, with the agent
//! that implements it and its status.
//!
//! This is the shared table of the AE-parity work: the menu bar, the AE keymap and
//! `docs/ui/ae-commands.md` are built from it. The document is generated from these
//! tables; `BAE_UPDATE_DOCS=1 cargo test -p bae-ui-motion catalog` rewrites it and
//! a test fails while it is stale.
//!
//! Action ids follow `docs/ui/ae-commands.md` ("动作 id 命名"): existing ids are
//! kept (`app.* file.* edit.* window.* workspace.* help.*`, `motion.*`,
//! `viewer.*` / `timeline.*` panel actions); new UI actions are
//! `motion.<area>.<snake_case AE title>` where area is an AE top-level menu
//! (`file edit comp layer effect anim view window help`) or `tool`, `timeline`,
//! `viewer`, `project`, `panel`. Variants of one action differ by parameters
//! (`motion.layer.blending_mode {"mode": "multiply"}`).

mod keys;
mod menu;
mod zh;

pub use keys::{KEYS, MAC_KEYS};
pub use menu::{EFFECT_CATEGORIES, MAC_APP_MENU, MENUS};
pub(crate) use zh::MENU_ZH;
pub use zh::SAME_IN_CHINESE;

/// Who implements the command behind an entry (the AE-parity agents, see
/// `docs/ui/ae-commands.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    /// Tools and the composition viewer (`ui/motion-tools`).
    A,
    /// Menus, shortcuts and the simple commands behind them (`ui/ae-menus`).
    B,
    /// Timeline, graph editor, keyframe dialogs, markers (`ui/motion-timeline`).
    C,
    /// Preferences (`ui/ae-settings`).
    D,
    /// Dialogs and panels (`ui/motion-dialogs`).
    E,
    /// Renderer features (`render/masks`).
    F,
}

/// Implementation state of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    /// Works like AE.
    Done,
    /// Works, with differences noted in `docs/ui/motion-parity.md`.
    Partial,
    /// Planned in this round; shown greyed until implemented.
    Todo,
    /// Not in this round; shown greyed.
    Later,
    /// An Adobe service or product integration (Bridge, Media Encoder, Dynamic
    /// Link …); shown greyed.
    Adobe,
    /// A placeholder for a list filled at run time (recent files, scripts,
    /// extensions); not a fixed item.
    Dynamic,
}

impl Status {
    /// Whether the command behind the entry runs.
    pub fn works(self) -> bool {
        matches!(self, Status::Done | Status::Partial)
    }
}

/// Platforms an item appears on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    All,
    /// Windows only (on macOS AE has it in the application menu: Exit, About,
    /// Preferences, Keyboard Shortcuts).
    Windows,
}

impl Platform {
    /// Whether the item is shown on this platform.
    pub fn current(self) -> bool {
        match self {
            Platform::All => true,
            Platform::Windows => !cfg!(target_os = "macos"),
        }
    }
}

/// One AE menu item.
#[derive(Clone, Copy, Debug)]
pub struct Item {
    /// AE's Win32 menu command id (`menus.json`); 0 for run-time list placeholders.
    pub ae: u32,
    /// English title (AE's), translated through `bae_ui::tr`.
    pub title: &'static str,
    /// BAE action id run by the item.
    pub action: &'static str,
    /// Parameters as JSON object text (`""` = none).
    pub params: &'static str,
    pub owner: Owner,
    pub status: Status,
    pub platform: Platform,
    /// AE sorts the item by its localized title among the marked items of its
    /// group (Keyframe Assistant, Export, the Window panel list, workspaces):
    /// alphabetically in English, by code point in Chinese.
    pub sorted: bool,
}

/// A submenu.
#[derive(Clone, Copy, Debug)]
pub struct Submenu {
    /// English title (AE's).
    pub title: &'static str,
    pub children: &'static [Node],
    pub platform: Platform,
}

/// A menu entry.
#[derive(Clone, Copy, Debug)]
pub enum Node {
    Item(Item),
    Separator,
    Submenu(Submenu),
    /// The effect categories (`EFFECT_CATEGORIES`), each listing its effects;
    /// filled from the effect registry at run time.
    Effects,
}

impl Node {
    /// The item, if this is one.
    pub fn item(&self) -> Option<&Item> {
        match self {
            Node::Item(it) => Some(it),
            _ => None,
        }
    }
}

/// One AE default-shortcut command (`shortcuts.json`).
#[derive(Clone, Copy, Debug)]
pub struct Key {
    /// AE shortcut context (`CSwitchboard`, `CCompCompCmd` …).
    pub context: &'static str,
    /// AE command identifier (`CompSettings`).
    pub command: &'static str,
    /// AE's default shortcuts in AE's notation, `|`-separated (`Ctrl+K`;
    /// `macControl` = the macOS Control key, those chords exist on macOS only).
    pub keys: &'static str,
    pub action: &'static str,
    /// Parameters as JSON object text (`""` = none).
    pub params: &'static str,
    pub owner: Owner,
    pub status: Status,
    /// [`NATIVE`], [`BOUND_ELSEWHERE`], [`SHADOWED`].
    pub flags: u8,
}

/// Text fields handle it themselves; not a keymap binding.
pub const NATIVE: u8 = 1;
/// Bound by the owning agent's own keymap extension (agent A's tool keys), not
/// by the menus' AE keymap table.
pub const BOUND_ELSEWHERE: u8 = 2;
/// AE binds the same chord in the same context to another command first; BAE
/// keeps the first one.
pub const SHADOWED: u8 = 4;
/// Only in AE's macOS shortcut file (26.2.1; the Windows table is 26.0).
pub const MAC_ONLY: u8 = 8;

impl Key {
    /// AE's default chords for this command on Windows (`mac = false`) or macOS,
    /// in BAE notation (see [`chords`]).
    pub fn chords(&self, mac: bool) -> Vec<String> {
        if mac
            && let Some((_, _, keys)) = MAC_KEYS
                .iter()
                .find(|(ctx, cmd, _)| *ctx == self.context && *cmd == self.command)
        {
            return mac_chords(keys);
        }
        if self.flags & MAC_ONLY != 0 && !mac {
            return Vec::new();
        }
        chords(self.keys, mac)
    }

    /// The BAE panel kind the AE context corresponds to (`None` = everywhere).
    pub fn scope(&self) -> Option<&'static str> {
        context_scope(self.context)
    }
}

/// BAE panel kind for an AE shortcut context; `None` for application-wide
/// contexts (AE's comp/timeline contexts are application-wide in BAE: the motion
/// actions work whichever motion panel has focus).
pub fn context_scope(context: &str) -> Option<&'static str> {
    Some(match context {
        "CCompMarkerCmd" | "COutline" | "CTopic" | "TLOutlinePano" => "timeline",
        "CDirTabPanel" | "CItem" | "CPanoProjItem" => "composition",
        "CPanoECOutline" => "effect_controls",
        "CDirItemTabPanelTime" | "CPanoProjFootage" => "footage",
        "CPanoProjLayer" | "CPanoProjLayerPano" | "CPanoProjLayerPanoMask" => "layer",
        "CPanoRender" | "RQOutlinePano" => "render_queue",
        "POutlinePano" => "project",
        "FloPano" => "flowchart",
        "Tracker" => "tracker",
        "CCompCloneCmd" => "clone_tool",
        "CCompPaintCmd" => "paint_tool",
        "CameraToolUI" => "camera_tool",
        "TextLayerUI" => "text_editing",
        "CSwitchboardModal" => "dialog",
        _ => return None,
    })
}

/// The English title AE shows on macOS for a catalog title (`menus.json` is the
/// Windows menu bar): Finder instead of Explorer.
pub fn mac_title(title: &str) -> &str {
    match title {
        "Reveal in Explorer" => "Reveal in Finder",
        other => other,
    }
}

/// The English title shown on the platform BAE runs on.
pub fn shown_title(title: &str) -> &str {
    if cfg!(target_os = "macos") {
        mac_title(title)
    } else {
        title
    }
}

/// Visit every item of a menu tree, depth first, with its English path.
pub fn walk(
    nodes: &'static [Node],
    path: &mut Vec<&'static str>,
    f: &mut dyn FnMut(&[&'static str], &'static Item),
) {
    for n in nodes {
        match n {
            Node::Item(it) => f(path, it),
            Node::Submenu(s) => {
                path.push(s.title);
                walk(s.children, path, f);
                path.pop();
            }
            Node::Separator | Node::Effects => {}
        }
    }
}

/// Every menu item with its English path (top-level menu first), the macOS
/// application menu (path `BAE`) included.
pub fn menu_items() -> Vec<(Vec<&'static str>, &'static Item)> {
    let mut out = Vec::new();
    let tops = MENUS.iter().copied().chain([("BAE", MAC_APP_MENU)]);
    for (top, nodes) in tops {
        let mut path = vec![top];
        walk(nodes, &mut path, &mut |p, it| out.push((p.to_vec(), it)));
    }
    out
}

/// AE shortcuts in AE's notation turned into BAE chord strings (`Cmd` = ⌘ on
/// macOS / Ctrl on Windows, `Ctrl` = the macOS Control key) for one platform.
/// Chords with `macControl` exist on macOS only. Keys BAE cannot bind yet (numpad
/// keys) are kept in AE's spelling (`Pad0`, `PadMultiply`).
pub fn chords(keys: &str, mac: bool) -> Vec<String> {
    keys.split('|')
        .filter(|k| !k.is_empty())
        .filter_map(|k| {
            let parts: Vec<&str> = k.split('+').collect();
            let (key, mods) = match parts.split_last() {
                // "Ctrl++" style is not used by AE; a trailing empty part means '+'.
                Some((key, mods)) => (*key, mods),
                None => return None,
            };
            let mut ctrl = false;
            let mut cmd = false;
            let mut alt = false;
            let mut shift = false;
            for m in mods {
                match *m {
                    "Ctrl" => cmd = true,
                    "Alt" => alt = true,
                    "Shift" => shift = true,
                    "macControl" => {
                        if !mac {
                            return None;
                        }
                        ctrl = true;
                    }
                    _ => return None,
                }
            }
            Some(chord_string(ctrl, cmd, alt, shift, key))
        })
        .collect()
}

/// AE's macOS notation (`Cmd+Option+macControl+H`) in BAE chord strings.
pub fn mac_chords(keys: &str) -> Vec<String> {
    keys.split('|')
        .filter(|k| !k.is_empty())
        .filter_map(|k| {
            let parts: Vec<&str> = k.split('+').collect();
            let (key, mods) = parts.split_last()?;
            let (mut ctrl, mut cmd, mut alt, mut shift) = (false, false, false, false);
            for m in mods {
                match *m {
                    "Cmd" => cmd = true,
                    "Option" => alt = true,
                    "Shift" => shift = true,
                    "macControl" => ctrl = true,
                    _ => return None,
                }
            }
            Some(chord_string(ctrl, cmd, alt, shift, key))
        })
        .collect()
}

fn chord_string(ctrl: bool, cmd: bool, alt: bool, shift: bool, key: &str) -> String {
    let mut s = String::new();
    if ctrl {
        s.push_str("Ctrl+");
    }
    if cmd {
        s.push_str("Cmd+");
    }
    if alt {
        s.push_str("Alt+");
    }
    if shift {
        s.push_str("Shift+");
    }
    s.push_str(ae_key_name(key));
    s
}

/// BAE key name (`egui::Key::name`) for an AE key name, or the AE name for keys
/// BAE does not distinguish yet (numpad).
pub fn ae_key_name(key: &str) -> &str {
    match key {
        "." => "Period",
        "Comma" => "Comma",
        "/" => "Slash",
        "Backslash" => "Backslash",
        ";" => "Semicolon",
        "SingleQuote" => "Quote",
        "`" => "Backtick",
        "=" => "Equals",
        "-" => "Minus",
        "[" => "OpenBracket",
        "]" => "CloseBracket",
        "HOME" => "Home",
        "END" => "End",
        "PageUP" => "PageUp",
        "PageDOWN" => "PageDown",
        "LeftArrow" => "ArrowLeft",
        "RightArrow" => "ArrowRight",
        "UpArrow" => "ArrowUp",
        "DownArrow" => "ArrowDown",
        // AE: "Delete" is the backspace key, "FwdDel" the forward delete key,
        // "Return" the main Enter key and "Enter" the numpad Enter key.
        "Delete" => "Backspace",
        "FwdDel" => "Delete",
        "Return" => "Enter",
        "Enter" => "PadEnter",
        "0" => "0",
        "1" => "1",
        "2" => "2",
        "3" => "3",
        "4" => "4",
        "5" => "5",
        "6" => "6",
        "7" => "7",
        "8" => "8",
        "9" => "9",
        other => other,
    }
}

/// Display form of a BAE chord string for docs: `Ctrl+Alt+F` (Windows) or
/// `Cmd+Opt+F` (macOS).
pub fn display_chord(chord: &str, mac: bool) -> String {
    chord
        .split('+')
        .map(|p| match (p, mac) {
            ("Cmd", false) => "Ctrl",
            ("Cmd", true) => "Cmd",
            ("Ctrl", true) => "Control",
            ("Alt", true) => "Opt",
            ("Period", _) => ".",
            ("Comma", _) => ",",
            ("Slash", _) => "/",
            ("Backslash", _) => "\\",
            ("Semicolon", _) => ";",
            ("Quote", _) => "'",
            ("Backtick", _) => "`",
            ("Equals", _) => "=",
            ("Minus", _) => "-",
            ("OpenBracket", _) => "[",
            ("CloseBracket", _) => "]",
            ("ArrowLeft", _) => "Left",
            ("ArrowRight", _) => "Right",
            ("ArrowUp", _) => "Up",
            ("ArrowDown", _) => "Down",
            (other, _) => other,
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests;
