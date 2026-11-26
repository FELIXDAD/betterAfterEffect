//! The After Effects command catalog: every AE 26 main-menu item
//! (`docs/reference/ae/menus.json`) and every default shortcut command
//! (`docs/reference/ae/shortcuts.json`) mapped to a BAE action id, with the agent
//! that implements it and its status.
//!
//! This is the shared table of the AE-parity work: the menu bar, the AE keymap and
//! `docs/ui/ae-commands.md` are built from it. The document is generated from these
//! tables; `BAE_UPDATE_DOCS=1 cargo test -p bae-ui-motion catalog` rewrites it and
//! a test fails while it is stale. The app reads the tables from `catalog.bin`,
//! which `build.rs` generates from the source tables (see `blob`).
//!
//! Action ids follow `docs/ui/ae-commands.md` ("动作 id 命名"): existing ids are
//! kept (`app.* file.* edit.* window.* workspace.* help.*`, `motion.*`,
//! `viewer.*` / `timeline.*` panel actions); new UI actions are
//! `motion.<area>.<snake_case AE title>` where area is an AE top-level menu
//! (`file edit comp layer effect anim view window help`) or `tool`, `timeline`,
//! `viewer`, `project`, `panel`. Variants of one action differ by parameters
//! (`motion.layer.blending_mode {"mode": "multiply"}`).

mod blob;
pub mod mnemonics;
// The source tables are compiled into tests only; the app reads them from the
// compressed `catalog.bin` that `build.rs` generates from them (see `blob`).
#[cfg(test)]
mod encode;
#[cfg(test)]
mod keys;
#[cfg(test)]
mod menu;
#[cfg(test)]
mod zh;

#[cfg(test)]
pub use keys::{KEYS, MAC_KEYS};
#[cfg(test)]
pub use menu::{EFFECT_CATEGORIES, MAC_APP_MENU, MENUS};
#[cfg(test)]
pub(crate) use zh::MENU_ZH;

mod types;
pub use types::*;

/// The top-level menus in AE's order: `(English title, entries)`.
pub fn menus() -> &'static [(&'static str, &'static [Node])] {
    blob::tables().menus
}

/// The macOS application menu (About, Settings ▸ the Preferences pages,
/// Services, Hide, Hide Others, Show All, Quit).
pub fn mac_app_menu() -> &'static [Node] {
    blob::tables().mac_app_menu
}

/// AE's effect categories (English, as in AE), in English alphabetical order;
/// the menu sorts them by their localized title like AE.
pub fn effect_categories() -> &'static [&'static str] {
    blob::tables().effect_categories
}

/// Chinese titles of the menu entries (`bae_ui::i18n` pairs).
pub fn menu_zh() -> &'static [(&'static str, &'static str)] {
    blob::tables().zh
}

/// AE 26's effects by category (299; the Effect menu lists them, greying the
/// ones no effect module implements).
pub fn ae_effects() -> &'static [AeEffect] {
    blob::tables().effects
}

/// Every AE default-shortcut command (`docs/reference/ae/shortcuts.json`) plus
/// the menu shortcuts AE's file does not list, in AE's order.
pub fn keys() -> &'static [Key] {
    blob::tables().keys
}

/// The commands whose macOS shortcuts are not the Windows ones with Ctrl → ⌘
/// and Alt → Option: `(context, command, keys in AE's macOS notation)`.
pub fn mac_keys() -> &'static [(&'static str, &'static str, &'static str)] {
    blob::tables().mac_keys
}

/// Menu titles that read the same in Chinese (AE keeps them in English).
pub const SAME_IN_CHINESE: &[&str] = &[
    // AE's bundled script panels (file names; Windows only).
    "Create Nulls From Paths.jsx",
    "VR Comp Editor.jsx",
    "Adobe Dynamic Link",
    "Maxon Cinema 4D Exporter...",
    "Shift+F10",
    "Shift+F11",
    "Shift+F12",
    "A",
    "B",
    "C",
    "RGB",
    "ProEXR...",
    "RotoBezier",
    "VR",
    "HDTV (Rec. 709)",
    "SDTV NTSC",
    "SDTV PAL",
    "Boris FX Mocha",
    "Cinema 4D",
    "Vanishing Point (.vpe)...",
    "Track in Boris FX Mocha",
    "Paste Mocha Mask",
];

impl Key {
    /// AE's default chords for this command on Windows (`mac = false`) or macOS,
    /// in BAE notation (see [`chords`]). On macOS they are the ones AE uses with
    /// "Use System Shortcut Keys" on (its default): the system keeps ⌘H, ⌥⌘H and
    /// ⌘M (`MacSysShortcutsStd`; the `MacSysShortcutsAlt` set is for the setting
    /// off) and AE's own commands on them get Control (see [`mac_system_key`]).
    pub fn chords(&self, mac: bool) -> Vec<String> {
        if !mac {
            if self.flags & MAC_ONLY != 0 {
                return Vec::new();
            }
            return chords(self.keys, false);
        }
        if self.context == "MacSysShortcutsAlt" {
            return Vec::new();
        }
        let file = match mac_keys()
            .iter()
            .find(|(ctx, cmd, _)| *ctx == self.context && *cmd == self.command)
        {
            Some((_, _, keys)) => mac_chords(keys),
            None => chords(self.keys, true),
        };
        if self.context == "MacSysShortcutsStd" {
            return file;
        }
        file.into_iter().map(mac_system_key).collect()
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
    let (menus, app) = (menus(), mac_app_menu());
    let tops = menus.iter().copied().chain([("BAE", app)]);
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

/// A macOS chord of AE's shortcut file as AE binds it: Hide (⌘H), Hide Others
/// (⌥⌘H) and Minimize (⌘M) belong to the system, so with "Use System Shortcut
/// Keys" on (AE's default) AE adds Control to its commands on them
/// (`docs/reference/ae/mac/menus.md` 5.4).
pub fn mac_system_key(chord: String) -> String {
    if ["Cmd+H", "Cmd+Alt+H", "Cmd+M"].contains(&chord.as_str()) {
        format!("Ctrl+{chord}")
    } else {
        chord
    }
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

/// Action ids some catalog entry runs already (status done or partial). Their
/// other variants (`motion.reveal {"what":"modified"}` next to the working
/// `{"what":"animated"}`) are not live just because the action has a handler:
/// they wait for the catalog. Panel items are told apart by their panel instead
/// (`bae_ui::menu::handler_key`).
pub fn working_actions() -> &'static std::collections::HashSet<&'static str> {
    static SET: std::sync::OnceLock<std::collections::HashSet<&'static str>> =
        std::sync::OnceLock::new();
    SET.get_or_init(|| {
        let mut set = std::collections::HashSet::new();
        for k in keys() {
            if k.status.works() {
                set.insert(k.action);
            }
        }
        let mut path = Vec::new();
        let tops = menus().iter().copied().chain([("BAE", mac_app_menu())]);
        for (top, nodes) in tops {
            path.push(top);
            walk(nodes, &mut path, &mut |_, it| {
                if it.status.works() {
                    set.insert(it.action);
                }
            });
            path.pop();
        }
        set.remove(bae_ui::action::ids::OPEN_PANEL);
        set
    })
}

/// Whether a catalog entry whose command is not done yet can light up because a
/// module handles it (`handled`: `bae_ui::menu::handled_actions`): its action is
/// new, not a further variant of a working one ([`working_actions`]).
pub fn live_by_handler(
    action: &str,
    params: &str,
    handled: &std::collections::HashSet<String>,
) -> bool {
    if working_actions().contains(action) {
        return false;
    }
    let params: serde_json::Value = if params.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(params).unwrap_or_default()
    };
    handled.contains(&bae_ui::menu::handler_key(action, &params))
}

/// Panel actions (`viewer.*`, `timeline.*`) only run in their own panel: AE
/// also lists them in other contexts (zoom in the Effect Controls, Layer and
/// Footage panels, `=` application-wide), but there the key would only be
/// swallowed. Their bindings stay in the panel they act on.
pub fn in_own_panel(k: &Key) -> bool {
    let own = match k.action.split('.').next() {
        Some("viewer") => "composition",
        Some("timeline") => "timeline",
        _ => return true,
    };
    k.scope() == Some(own)
}

/// AE's default shortcuts as keymap bindings for the platform BAE runs on: one
/// binding per chord of every command `live` accepts, scoped to the panel kind of
/// AE's context. Commands text fields handle ([`NATIVE`]), commands another
/// module binds ([`BOUND_ELSEWHERE`]), commands AE shadows ([`SHADOWED`]) and
/// chords BAE cannot tell apart yet (numpad keys) are left out.
pub fn ae_bindings(keys: &[Key], live: &dyn Fn(&Key) -> bool) -> Vec<bae_ui::Binding> {
    let mac = cfg!(target_os = "macos");
    let mut out: Vec<bae_ui::Binding> = Vec::new();
    for k in keys {
        if k.flags & (NATIVE | BOUND_ELSEWHERE | SHADOWED) != 0 || !live(k) || !in_own_panel(k) {
            continue;
        }
        let params: serde_json::Value = if k.params.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(k.params).expect("catalog params are JSON")
        };
        for chord in k.chords(mac) {
            if bae_ui::Chord::parse(&chord).is_err() {
                continue;
            }
            let mut b = bae_ui::Binding::new(&chord, k.action).params(params.clone());
            if let Some(scope) = k.scope() {
                b = b.when(scope);
            }
            if !out.iter().any(|o| {
                o.chord == b.chord
                    && o.when == b.when
                    && o.action == b.action
                    && o.params == b.params
            }) {
                out.push(b);
            }
        }
    }
    out
}

/// AE's Windows mnemonic of the item with AE command id `ae` (Alt+letter).
pub fn item_mnemonic(ae: u32) -> Option<char> {
    let i = mnemonics::ITEMS
        .binary_search_by_key(&ae, |(id, _)| *id)
        .ok()?;
    Some(mnemonics::ITEMS[i].1)
}

/// AE's Windows mnemonic of the menu or submenu titled `en` (looked up by its
/// Chinese title, whatever the UI language).
pub fn menu_mnemonic(en: &str) -> Option<char> {
    let zh = bae_ui::i18n::translation(en)
        .or_else(|| menu_zh().iter().find(|(e, _)| *e == en).map(|(_, z)| *z))
        .unwrap_or(en);
    let zh = zh.trim_end_matches("...").trim_end_matches('…');
    mnemonics::MENUS
        .iter()
        .find(|(t, _)| *t == zh)
        .map(|(_, c)| *c)
}
