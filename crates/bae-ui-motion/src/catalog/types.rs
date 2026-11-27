//! The catalog's types. Shared with `build.rs`, which includes this file, the
//! source tables (`menu.rs`, `keys.rs`, `zh.rs`, `effects.tsv`) and `encode.rs`
//! to generate `catalog.bin`: no dependencies beyond `std`.

/// One AE effect of the Effect menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AeEffect {
    /// AE category (English, one of [`effect_categories`]).
    pub category: &'static str,
    /// AE matchName (the effect's document identity).
    pub match_name: &'static str,
    /// English name.
    pub name: &'static str,
    /// Name in AE's Chinese UI.
    pub name_zh: &'static str,
    /// GPU and channel depths published in the After Effects effect list.
    pub capabilities: u8,
}

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
    /// The effect categories ([`effect_categories`]), each listing its effects;
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
