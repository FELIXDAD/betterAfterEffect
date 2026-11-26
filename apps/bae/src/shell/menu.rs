//! Native menu bar (muda).
//!
//! The menu bar is the registered `Registry<MenuItemSpec>` tree: After Effects
//! 26's menus, which the motion workspace builds from its AE command catalog
//! (`bae_ui_motion::catalog`, with the macOS application menu), plus the shell's
//! BAE-only entries ([`extra_items`]: the History and Agent panels, the Agent
//! workspace). Like AE, items whose command cannot run now are greyed, not
//! hidden; checkable items (Window panels, workspaces) show a check mark.
//!
//! A click runs the item's invocation through the same dispatch as shortcuts
//! and panel requests. Every item shows the chord the active keymap binds to
//! its invocation ([`AppMenu::sync`]); the native key equivalent then handles
//! the key. Undo, Redo, Cut, Copy, Paste and Select All while a text field has
//! focus are handed back to the field (`Shell::user_event`), so the usual keys
//! still edit the text. The macOS application menu's About, Services, Hide,
//! Hide Others and Show All are the system's own items.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bae_ui::action::ids;
use bae_ui::menu::{APP_MENU, MenuContext, MenuEntry, MenuItemSpec, MenuPos, MenuTree};
use bae_ui::{Chord, Invocation, Keymap};
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

use crate::i18n::tr;

/// Turns a menu item id back into an invocation.
pub fn invocation(menu_id: &str) -> Invocation {
    match menu_id.split_once(':') {
        Some((id, arg)) => Invocation::with_params(id, serde_json::json!({ "id": arg })),
        None => Invocation::new(menu_id),
    }
}

/// Prefix of the ids of items modules contribute (`Registry<MenuItemSpec>`).
const CONTRIBUTED: &str = "bae.menu:";

/// Actions a focused text field takes over from the Edit menu (their native key
/// equivalents would otherwise never reach it).
pub const TEXT_EDIT_ACTIONS: &[&str] = &[
    ids::UNDO,
    ids::REDO,
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "motion.select_all",
];

/// The modifiers of the key press that fired a native key equivalent (`None`
/// when the menu was used with the mouse). AppKit fires a key equivalent on extra
/// modifiers too (the ⌃8 item on ⌃⌥8, issue #281): the shell then hands the press
/// to the UI, whose keymap runs the command bound to the chord pressed.
#[cfg(target_os = "macos")]
pub fn key_equivalent_modifiers() -> Option<egui::Modifiers> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    const KEY_DOWN: usize = 10; // NSEventTypeKeyDown
    // NSEventModifierFlagShift, Control, Option, Command.
    let (shift, ctrl, alt, cmd) = (1 << 17, 1 << 18, 1 << 19, 1 << 20);
    // SAFETY: muda calls the menu event handler on the main thread, from the
    // item's action; these are plain NSApplication / NSEvent getters.
    let flags: usize = unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let event: *mut AnyObject = msg_send![app, currentEvent];
        if event.is_null() {
            return None;
        }
        let kind: usize = msg_send![event, type];
        if kind != KEY_DOWN {
            return None;
        }
        msg_send![event, modifierFlags]
    };
    Some(egui::Modifiers {
        alt: flags & alt != 0,
        ctrl: flags & ctrl != 0,
        shift: flags & shift != 0,
        mac_cmd: flags & cmd != 0,
        command: flags & cmd != 0,
    })
}

/// Windows menus match accelerators exactly.
#[cfg(not(target_os = "macos"))]
pub fn key_equivalent_modifiers() -> Option<egui::Modifiers> {
    None
}

/// The key press to hand to the UI instead of running the item: its key
/// equivalent (`chord`) fired on a press with other modifiers (`keys`).
pub fn key_press_for_ui(
    chord: Option<Chord>,
    keys: Option<egui::Modifiers>,
) -> Option<egui::Event> {
    let (chord, mods) = (chord?, keys?);
    (!chord.matches_modifiers(mods)).then_some(egui::Event::Key {
        key: chord.key,
        physical_key: Some(chord.key),
        pressed: true,
        repeat: false,
        modifiers: mods,
    })
}

/// BAE's own entries, beyond AE's menus: the History and Agent panels (a group
/// after AE's panels in Window) and the Agent workspace (sorted among AE's
/// workspaces in Window ▸ Workspace).
pub fn extra_items() -> Vec<(&'static str, MenuItemSpec)> {
    let panel = |panel: &str, title: &str, order: i32| {
        let open = panel.to_owned();
        MenuItemSpec::new(&["Window"], title, ids::OPEN_PANEL)
            .params(serde_json::json!({ "id": panel, "toggle": true }))
            .group(10)
            .order(order)
            .checked_when(move |cx| cx.panel_open(&open))
    };
    vec![
        ("shell.window.history", panel("history", "History", 1)),
        (
            "shell.window.agent_activity",
            panel("agent_activity", "Agent Activity", 2),
        ),
        (
            "shell.window.agent_proposals",
            panel("agent_proposals", "Proposals", 3),
        ),
        (
            "shell.window.workspace.agent",
            MenuItemSpec::new(&["Window", "Workspace"], "Agent", ids::SWITCH_WORKSPACE)
                .params(serde_json::json!({ "id": "agent" }))
                .at(vec![MenuPos::new(0, 1), MenuPos::new(0, 0).sorted()])
                .checked_when(|cx| cx.workspace == "agent"),
        ),
    ]
}

/// A native item: plain or with a check mark.
enum Native {
    Plain(MenuItem),
    Check(CheckMenuItem),
}

impl Native {
    fn set_enabled(&self, enabled: bool) {
        match self {
            Native::Plain(i) => i.set_enabled(enabled),
            Native::Check(i) => i.set_enabled(enabled),
        }
    }

    fn set_accelerator(&self, accelerator: Option<Accelerator>) {
        let _ = match self {
            Native::Plain(i) => i.set_accelerator(accelerator),
            Native::Check(i) => i.set_accelerator(accelerator),
        };
    }

    fn set_text(&self, text: &str) {
        match self {
            Native::Plain(i) => i.set_text(text),
            Native::Check(i) => i.set_text(text),
        }
    }
}

/// A contributed item and the state last pushed to the native menu.
struct Contributed {
    item: Native,
    spec: Arc<MenuItemSpec>,
    enabled: bool,
    checked: bool,
    chord: Option<Chord>,
    /// The state-following title last shown ([`MenuItemSpec::label_when`]).
    label: Option<String>,
}

/// The application menu bar.
pub struct AppMenu {
    menu: Menu,
    undo_state: (Option<String>, Option<String>),
    /// Contributed items by registry id.
    contributed: HashMap<String, Contributed>,
    /// Registry ids of the Edit menu's Undo and Redo items.
    undo: Option<String>,
    redo: Option<String>,
    /// Window ▸ Workspace ▸ Reset "<workspace>" to Saved Layout: id and text.
    reset: Option<(String, String)>,
    assign: Option<Submenu>,
    assign_slots: Vec<(String, &'static str, Chord)>,
    /// Action ids modules handle (`bae_ui::menu::handled_actions`).
    actions: HashSet<String>,
    /// Counts [`AppMenu::sync`] calls (`MenuContext::generation`).
    generation: u64,
}

impl AppMenu {
    /// Builds the menu bar from the registered menus (`contributed`, AE order);
    /// `actions` are the action ids modules handle.
    pub fn new(contributed: &MenuTree, actions: HashSet<String>) -> muda::Result<Self> {
        let menu = Menu::new();
        let mut items: HashMap<String, Contributed> = HashMap::new();
        let mut assign = None;
        for (title, entries) in &contributed.menus {
            if title == APP_MENU {
                // The macOS application menu, under the application's name.
                if cfg!(target_os = "macos") {
                    let app = Submenu::new(bae_platform::APP_NAME, true);
                    append_entries(&app, entries, &mut items, &mut assign)?;
                    menu.append(&app)?;
                }
                continue;
            }
            let sub = Submenu::new(menu_text(tr(title), menu_mnemonic(title)), true);
            append_entries(&sub, entries, &mut items, &mut assign)?;
            #[cfg(target_os = "macos")]
            if title == "View" {
                sub.append(&PredefinedMenuItem::separator())?;
                sub.append(&PredefinedMenuItem::fullscreen(Some(tr("Full Screen"))))?;
            }
            menu.append(&sub)?;
        }
        let find = |action: &str| {
            items
                .iter()
                .find(|(_, c)| c.spec.action == action)
                .map(|(id, _)| id.clone())
        };
        let undo = find(ids::UNDO);
        let redo = find(ids::REDO);
        let reset = find(ids::RESET_WORKSPACE).map(|id| (id, String::new()));
        let assign_slots = items
            .iter()
            .filter(|(_, c)| c.spec.action == ids::ASSIGN_WORKSPACE_SHORTCUT)
            .filter_map(|(id, c)| {
                let slot = usize::try_from(c.spec.params["slot"].as_u64()?).ok()?;
                let key = *bae_ui::presets::WORKSPACE_KEYS.get(slot)?;
                Some((id.clone(), key, Chord::parse(key).ok()?))
            })
            .collect();
        let mut menu = Self {
            menu,
            undo_state: (Some(String::new()), None),
            contributed: items,
            undo,
            redo,
            reset,
            assign,
            assign_slots,
            actions,
            generation: 0,
        };
        menu.set_undo_state((None, None));
        Ok(menu)
    }

    /// The invocation of a contributed item (`None` for ids this menu did not
    /// make, which [`invocation`] decodes).
    pub fn contributed_invocation(&self, menu_id: &str) -> Option<Invocation> {
        let id = menu_id.strip_prefix(CONTRIBUTED)?;
        self.contributed.get(id).map(|c| c.spec.invocation())
    }

    /// The shortcut shown on a contributed item (its native key equivalent).
    pub fn contributed_chord(&self, menu_id: &str) -> Option<Chord> {
        let id = menu_id.strip_prefix(CONTRIBUTED)?;
        self.contributed.get(id).and_then(|c| c.chord)
    }

    /// Updates the items: enabled state, check marks, and the shortcut the
    /// active keymap binds to the same invocation (display; on macOS the menu
    /// also handles the key, which then does not reach the UI, so it runs once).
    /// `text_focus`: a text field has focus, so the Edit menu's text commands
    /// stay enabled for it ([`TEXT_EDIT_ACTIONS`]).
    pub fn sync(&mut self, keymap: &Keymap, cx: &MenuContext<'_>, text_focus: bool) {
        self.generation += 1;
        let cx = MenuContext::new(cx.stage, cx.resources, cx.workspace)
            .with_open_panels(cx.open_panels)
            .with_generation(self.generation)
            .with_actions(&self.actions);
        let undo = self.undo_state.0.is_some();
        let redo = self.undo_state.1.is_some();
        for (id, c) in &mut self.contributed {
            let enabled = if Some(id) == self.undo.as_ref() {
                undo || text_focus
            } else if Some(id) == self.redo.as_ref() {
                redo || text_focus
            } else {
                c.spec.is_enabled(&cx)
                    || (text_focus && TEXT_EDIT_ACTIONS.contains(&c.spec.action.as_str()))
            };
            if enabled != c.enabled {
                c.item.set_enabled(enabled);
                c.enabled = enabled;
            }
            if let Native::Check(item) = &c.item {
                let checked = c.spec.is_checked(&cx);
                // A click toggles the native mark itself: always re-assert it.
                if checked != c.checked || item.is_checked() != checked {
                    item.set_checked(checked);
                    c.checked = checked;
                }
            }
            let label = c.spec.label(&cx);
            if label.is_some() && label != c.label {
                c.item
                    .set_text(&native_text(label.as_deref().unwrap_or_default()));
                c.label = label;
            }
            let chord = shell_chord(keymap, &c.spec.invocation());
            if chord != c.chord {
                c.item.set_accelerator(chord.as_ref().and_then(accelerator));
                c.chord = chord;
            }
        }
    }

    /// Names the active workspace in Window ▸ Workspace ▸ Reset “<name>” to Saved
    /// Layout, as AE does (`title`: its English title).
    pub fn sync_workspace_titles(&mut self, workspace: &crate::ui::Workspace) {
        let Some((id, shown)) = &mut self.reset else {
            return;
        };
        let title = workspace.active_workspace_title();
        let text = tr("Reset “{name}” to Saved Layout").replace("{name}", tr(title));
        if text != *shown {
            if let Some(c) = self.contributed.get(id.as_str()) {
                c.item.set_text(&native_text(&text));
            }
            if let Some(sub) = &self.assign {
                sub.set_text(native_text(
                    &tr("Assign Shortcut to “{name}” Workspace").replace("{name}", tr(title)),
                ));
            }
            *shown = text;
        }
        for (id, key, chord) in &self.assign_slots {
            let Some(c) = self.contributed.get_mut(id) else {
                continue;
            };
            let target = workspace
                .keymap()
                .lookup(chord, None)
                .filter(|b| b.action == ids::SWITCH_WORKSPACE)
                .and_then(|b| b.params["id"].as_str())
                .and_then(|id| workspace.workspace_title(id));
            let label = match target {
                Some(name) => tr("{key} (replaces “{name}”)")
                    .replace("{key}", key)
                    .replace("{name}", tr(name)),
                None => (*key).to_owned(),
            };
            if c.label.as_ref() != Some(&label) {
                c.item.set_text(&native_text(&label));
                c.label = Some(label);
            }
        }
    }

    /// Updates the Undo / Redo items (`Some(label)` = enabled, AE: "Undo <step>",
    /// "Can’t Undo" when there is nothing to undo).
    pub fn set_undo_state(&mut self, state: (Option<String>, Option<String>)) {
        if state == self.undo_state {
            return;
        }
        for (id, base, label) in [
            (&self.undo, "Undo", state.0.as_deref()),
            (&self.redo, "Redo", state.1.as_deref()),
        ] {
            if let Some(c) = id.as_ref().and_then(|id| self.contributed.get(id)) {
                c.item.set_text(&native_text(&undo_redo_text(base, label)));
            }
        }
        self.undo_state = state;
    }

    /// Takes the menu bar off before another one replaces it (a language change).
    pub fn detach(&self, window: &winit::window::Window) {
        #[cfg(target_os = "macos")]
        {
            let _ = window;
            self.menu.remove_for_nsapp();
        }
        #[cfg(windows)]
        {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle()
                && let RawWindowHandle::Win32(win32) = handle.as_raw()
            {
                // SAFETY: `hwnd` is the live window this menu was attached to, on
                // the thread that owns it.
                if let Err(error) = unsafe { self.menu.remove_for_hwnd(win32.hwnd.get()) } {
                    tracing::warn!("cannot remove the native menu: {error}");
                }
            }
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = window;
        }
    }

    /// Attaches the menu bar: the global NSApp menu on macOS, the window's menu
    /// on Windows.
    pub fn attach(&self, window: &winit::window::Window) {
        #[cfg(target_os = "macos")]
        {
            let _ = window;
            self.menu.init_for_nsapp();
        }
        #[cfg(windows)]
        {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle()
                && let RawWindowHandle::Win32(win32) = handle.as_raw()
            {
                // SAFETY: `hwnd` is the live window's handle; the menu is attached
                // once, from the thread that owns the window.
                if let Err(error) = unsafe { self.menu.init_for_hwnd(win32.hwnd.get()) } {
                    tracing::warn!("native menu unavailable: {error}");
                }
            }
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = window;
            tracing::debug!("native menu bar not implemented on this platform");
        }
    }
}

/// A title as the native menu needs it: muda reads a single `&` as a mnemonic
/// marker and drops it (Windows and macOS), so "Effects & Presets" showed as
/// "Effects  Presets" (#226); `&&` shows one `&`.
/// AE's Windows mnemonic of a menu or submenu (Alt+letter).
fn menu_mnemonic(title: &str) -> Option<char> {
    bae_ui_motion::catalog::menu_mnemonic(title)
}

/// A title as the native menu shows it, with AE's mnemonic on Windows
/// (`文件(&F)`, `&File`); macOS menus have none.
fn menu_text(text: &str, mnemonic: Option<char>) -> String {
    let zh = bae_ui::i18n::lang() == bae_ui::Lang::Zh;
    mnemonic_text(text, mnemonic.filter(|_| cfg!(windows)), zh)
}

/// `text` (escaped for the native menu) with `mnemonic`: AE's Chinese menus
/// put `(&X)` after the title, before a trailing `...`; English ones mark the
/// letter in the word (`Effec&t`), else add ` (&X)`.
fn mnemonic_text(text: &str, mnemonic: Option<char>, zh: bool) -> String {
    let escaped = native_text(text);
    let Some(c) = mnemonic else {
        return escaped;
    };
    if zh {
        let (base, tail) = match escaped.strip_suffix("...") {
            Some(base) => (base, "..."),
            None => (escaped.as_str(), ""),
        };
        return format!("{base}(&{c}){tail}");
    }
    match escaped
        .char_indices()
        .find(|(_, ch)| ch.eq_ignore_ascii_case(&c))
    {
        Some((i, _)) => format!("{}&{}", &escaped[..i], &escaped[i..]),
        None => format!("{escaped} (&{c})"),
    }
}

fn native_text(text: &str) -> String {
    text.replace('&', "&&")
}

/// The system's own item for a macOS application-menu action, if it has one.
fn predefined(spec: &MenuItemSpec) -> Option<PredefinedMenuItem> {
    if !cfg!(target_os = "macos") || spec.path.first().map(String::as_str) != Some(APP_MENU) {
        return None;
    }
    let owned = native_text(spec.display_title());
    let text = Some(owned.as_str());
    Some(match spec.action.as_str() {
        ids::ABOUT => PredefinedMenuItem::about(text, Some(about_metadata())),
        "app.services" => PredefinedMenuItem::services(text),
        "app.hide" => PredefinedMenuItem::hide(text),
        "app.hide_others" => PredefinedMenuItem::hide_others(text),
        "app.show_all" => PredefinedMenuItem::show_all(text),
        _ => return None,
    })
}

/// Appends resolved menu entries (items, separators, submenus) to `menu`.
fn append_entries(
    menu: &Submenu,
    entries: &[MenuEntry],
    items: &mut HashMap<String, Contributed>,
    assign: &mut Option<Submenu>,
) -> muda::Result<()> {
    for e in entries {
        match e {
            MenuEntry::Separator => menu.append(&PredefinedMenuItem::separator())?,
            MenuEntry::Item { id, spec } => {
                if let Some(item) = predefined(spec) {
                    menu.append(&item)?;
                    continue;
                }
                let menu_id = format!("{CONTRIBUTED}{id}");
                let item = if spec.is_checkable() {
                    let item = CheckMenuItem::with_id(
                        menu_id,
                        menu_text(spec.display_title(), spec.mnemonic),
                        true,
                        false,
                        None,
                    );
                    menu.append(&item)?;
                    Native::Check(item)
                } else {
                    let item = MenuItem::with_id(
                        menu_id,
                        menu_text(spec.display_title(), spec.mnemonic),
                        true,
                        None,
                    );
                    menu.append(&item)?;
                    Native::Plain(item)
                };
                items.insert(
                    id.clone(),
                    Contributed {
                        item,
                        spec: spec.clone(),
                        enabled: true,
                        checked: false,
                        chord: None,
                        label: None,
                    },
                );
            }
            MenuEntry::Submenu { title, entries } => {
                // AE shows a submenu with nothing in it (History) greyed.
                let sub = Submenu::new(
                    menu_text(tr(title), menu_mnemonic(title)),
                    !entries.is_empty(),
                );
                if title == "Assign Shortcut to Workspace" {
                    *assign = Some(sub.clone());
                }
                append_entries(&sub, entries, items, assign)?;
                menu.append(&sub)?;
            }
        }
    }
    Ok(())
}

/// The chord an item shows: what the keymap binds to its command (plain typing
/// keys never: the menu would take them from text fields).
fn shell_chord(keymap: &Keymap, invocation: &Invocation) -> Option<Chord> {
    keymap
        .chord_for_invocation(invocation)
        .filter(|ch| !ch.is_typing_key())
        .cloned()
}

/// The native accelerator for a chord (`None` for keys the menu cannot show).
fn accelerator(chord: &Chord) -> Option<Accelerator> {
    use egui::Key as K;
    let code = match chord.key {
        K::A => Code::KeyA,
        K::B => Code::KeyB,
        K::C => Code::KeyC,
        K::D => Code::KeyD,
        K::E => Code::KeyE,
        K::F => Code::KeyF,
        K::G => Code::KeyG,
        K::H => Code::KeyH,
        K::I => Code::KeyI,
        K::J => Code::KeyJ,
        K::K => Code::KeyK,
        K::L => Code::KeyL,
        K::M => Code::KeyM,
        K::N => Code::KeyN,
        K::O => Code::KeyO,
        K::P => Code::KeyP,
        K::Q => Code::KeyQ,
        K::R => Code::KeyR,
        K::S => Code::KeyS,
        K::T => Code::KeyT,
        K::U => Code::KeyU,
        K::V => Code::KeyV,
        K::W => Code::KeyW,
        K::X => Code::KeyX,
        K::Y => Code::KeyY,
        K::Z => Code::KeyZ,
        K::Num0 => Code::Digit0,
        K::Num1 => Code::Digit1,
        K::Num2 => Code::Digit2,
        K::Num3 => Code::Digit3,
        K::Num4 => Code::Digit4,
        K::Num5 => Code::Digit5,
        K::Num6 => Code::Digit6,
        K::Num7 => Code::Digit7,
        K::Num8 => Code::Digit8,
        K::Num9 => Code::Digit9,
        K::F1 => Code::F1,
        K::F2 => Code::F2,
        K::F3 => Code::F3,
        K::F4 => Code::F4,
        K::F5 => Code::F5,
        K::F6 => Code::F6,
        K::F7 => Code::F7,
        K::F8 => Code::F8,
        K::F9 => Code::F9,
        K::F10 => Code::F10,
        K::F11 => Code::F11,
        K::F12 => Code::F12,
        K::OpenBracket => Code::BracketLeft,
        K::CloseBracket => Code::BracketRight,
        K::Comma => Code::Comma,
        K::Period => Code::Period,
        K::Slash => Code::Slash,
        K::Minus => Code::Minus,
        K::Equals => Code::Equal,
        K::Quote => Code::Quote,
        K::Semicolon => Code::Semicolon,
        K::Backslash => Code::Backslash,
        K::Backtick => Code::Backquote,
        K::Space => Code::Space,
        K::Delete => Code::Delete,
        K::Backspace => Code::Backspace,
        K::Home => Code::Home,
        K::End => Code::End,
        K::PageUp => Code::PageUp,
        K::PageDown => Code::PageDown,
        K::ArrowLeft => Code::ArrowLeft,
        K::ArrowRight => Code::ArrowRight,
        K::ArrowUp => Code::ArrowUp,
        K::ArrowDown => Code::ArrowDown,
        K::Enter => Code::Enter,
        K::Escape => Code::Escape,
        K::Tab => Code::Tab,
        _ => return None,
    };
    let mut mods = Modifiers::empty();
    if chord.command {
        mods |= if cfg!(target_os = "macos") {
            Modifiers::META
        } else {
            Modifiers::CONTROL
        };
    }
    if chord.mac_ctrl {
        mods |= Modifiers::CONTROL;
    }
    if chord.alt {
        mods |= Modifiers::ALT;
    }
    if chord.shift {
        mods |= Modifiers::SHIFT;
    }
    Some(Accelerator::new(mods, code))
}

#[cfg(target_os = "macos")]
fn about_metadata() -> AboutMetadata {
    AboutMetadata {
        name: Some(bae_platform::APP_NAME.to_owned()),
        version: Some(bae_platform::VERSION.to_owned()),
        license: Some("Apache-2.0".to_owned()),
        comments: Some(tr("GPU-first motion graphics and compositing.").to_owned()),
        ..Default::default()
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn about_metadata() -> AboutMetadata {
    AboutMetadata::default()
}

/// "Undo New Layer" / "撤消 新建图层": the Edit menu's undo and redo items. `label` is
/// the command's English title (what the stage records); both parts are translated
/// (#136). `None`: nothing to undo ("Can’t Undo").
pub(crate) fn undo_redo_text(base: &str, label: Option<&str>) -> String {
    match label {
        Some(label) if !label.is_empty() => format!("{} {}", tr(base), tr(label)),
        Some(_) => tr(base).to_owned(),
        // AE: "Can't Undo" / "Can't Redo" while there is nothing to undo.
        None => tr(&format!("Can’t {base}")).to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_map_to_invocations() {
        assert_eq!(invocation("edit.undo"), Invocation::new("edit.undo"));
        let inv = invocation("workspace.switch:review");
        assert_eq!(inv.id, "workspace.switch");
        assert_eq!(inv.id_param(), Some("review"));
    }

    /// The menu bar the app's modules register, as `AppMenu::new` gets it.
    fn app_menu_tree() -> MenuTree {
        let mut app = crate::app_builder().build();
        let tree = bae_ui::menu::build_menus(&app.registry::<MenuItemSpec>().snapshot(), "motion");
        assert!(tree.conflicts.is_empty(), "{:?}", tree.conflicts);
        tree
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

    /// Issue #117: the menu shows the keymap's shortcut for each item (Undo
    /// Cmd+Z, Redo Cmd+Shift+Z, Keyboard Shortcuts, Quit / Exit, the Window panel
    /// bindings).
    #[test]
    fn items_show_keymap_shortcuts() {
        let keymap = bae_ui::presets::ae_keymap();
        let tree = app_menu_tree();
        let shown = |action: &str, params: serde_json::Value| {
            let (_, spec) = tree
                .items()
                .into_iter()
                .find(|(_, s)| s.action == action && s.params == params)
                .unwrap_or_else(|| panic!("no menu item for {action} {params}"));
            shell_chord(&keymap, &spec.invocation()).map(|c| c.to_string())
        };
        let parse = |s: &str| Some(Chord::parse(s).unwrap().to_string());
        let none = serde_json::Value::Null;
        assert_eq!(shown(ids::UNDO, none.clone()), parse("Cmd+Z"));
        assert_eq!(shown(ids::REDO, none.clone()), parse("Cmd+Shift+Z"));
        assert_eq!(
            shown(ids::KEYBOARD_SHORTCUTS, none.clone()),
            parse("Cmd+Alt+Quote")
        );
        assert_eq!(shown(ids::QUIT, none), parse("Cmd+Q"));
        let panel = |id: &str| serde_json::json!({ "id": id, "toggle": true });
        assert_eq!(shown(ids::OPEN_PANEL, panel("project")), parse("Cmd+0"));
        assert_eq!(
            shown(ids::OPEN_PANEL, panel("effect_controls")),
            parse("F3")
        );
        assert_eq!(shown(ids::OPEN_PANEL, panel("timeline")), None);
    }

    /// The File menu is AE's (New ▸, Open Project…, Open Recent ▸ …) with AE's
    /// shortcuts; BAE's own panels follow AE's in Window, the Agent workspace is
    /// sorted among AE's workspaces.
    #[test]
    fn menu_bar_is_ae_with_bae_extras() {
        let tree = app_menu_tree();
        let menu = |title: &str| {
            &tree
                .menus
                .iter()
                .find(|(t, _)| t == title)
                .unwrap_or_else(|| panic!("no {title} menu"))
                .1
        };
        assert_eq!(
            titles(menu("File"))[..9],
            [
                "New ▸",
                "Open Project...",
                "Open Recent ▸",
                "Browse in Bridge...",
                "---",
                "Close",
                "Close Project",
                "Save",
                "Save As ▸",
            ]
        );
        let keymap = bae_ui::presets::ae_keymap();
        let chord = |id: &str| {
            let (_, spec) = tree.items().into_iter().find(|(i, _)| *i == id).unwrap();
            keymap
                .chord_for_invocation(&spec.invocation())
                .map(|c| c.to_string())
        };
        assert_eq!(
            chord("shell.file.new_project").as_deref(),
            Some("Cmd+Alt+N")
        );
        assert_eq!(chord("shell.file.open_project").as_deref(), Some("Cmd+O"));
        assert_eq!(chord("shell.file.close_project"), None);
        assert_eq!(chord("shell.file.save").as_deref(), Some("Cmd+S"));
        assert_eq!(chord("shell.file.save_as").as_deref(), Some("Cmd+Shift+S"));
        let window = titles(menu("Window"));
        assert_eq!(
            window[window.len() - 4..],
            ["---", "History", "Agent Activity", "Proposals"]
        );
        let Some(MenuEntry::Submenu { entries, .. }) = menu("Window").first() else {
            panic!("Window ▸ Workspace");
        };
        assert_eq!(titles(entries)[..3], ["Agent", "All Panels", "Animation"]);
    }

    /// AE: "Undo <step>", and "Can’t Undo" / "Can’t Redo" (无法撤消) when there
    /// is nothing to undo.
    #[test]
    fn undo_items_say_what_they_undo() {
        crate::i18n::register_tables();
        assert_eq!(
            undo_redo_text("Undo", Some("New Solid")),
            format!("{} {}", tr("Undo"), tr("New Solid"))
        );
        for base in ["Undo", "Redo"] {
            let cant = format!("Can’t {base}");
            assert!(bae_ui::i18n::has_translation(&cant), "{cant}");
            assert_eq!(undo_redo_text(base, None), tr(&cant));
        }
    }

    /// #226: titles with `&` (Effects & Presets, Blur & Sharpen, Startup & Repair…)
    /// keep it in the native menu.
    #[test]
    fn windows_menus_get_ae_mnemonics() {
        assert_eq!(mnemonic_text("文件", Some('F'), true), "文件(&F)");
        assert_eq!(
            mnemonic_text("打开项目...", Some('O'), true),
            "打开项目(&O)..."
        );
        assert_eq!(mnemonic_text("Effect", Some('T'), false), "Effec&t");
        assert_eq!(mnemonic_text("File", Some('F'), false), "&File");
        assert_eq!(
            mnemonic_text("Effects & Presets", None, false),
            "Effects && Presets"
        );
        // Every top-level AE menu has its letter (the Chinese Window menu has
        // none in AE 26).
        let _app = crate::acceptance::build_app();
        crate::i18n::register_tables();
        assert_eq!(menu_mnemonic("Window"), None);
        for (title, letter) in [
            ("File", 'F'),
            ("Edit", 'E'),
            ("Composition", 'C'),
            ("Layer", 'L'),
            ("Effect", 'T'),
            ("Animation", 'A'),
            ("View", 'V'),
            ("Help", 'H'),
        ] {
            assert_eq!(menu_mnemonic(title), Some(letter), "{title}");
        }
    }

    #[test]
    fn ampersands_survive_the_native_menu() {
        assert_eq!(native_text("Effects & Presets"), "Effects && Presets");
        assert_eq!(native_text("效果和预设"), "效果和预设");
        assert_eq!(native_text("A && B"), "A &&&& B");
        // Every registered title with an `&` is escaped where the menu is built.
        let tree = app_menu_tree();
        let with_amp: Vec<&str> = tree
            .items()
            .iter()
            .map(|(_, s)| s.title.as_str())
            .filter(|t| t.contains('&'))
            .collect();
        assert!(with_amp.contains(&"Effects & Presets"), "{with_amp:?}");
        for t in with_amp {
            assert_eq!(
                native_text(t).matches("&&").count(),
                t.matches('&').count(),
                "{t}"
            );
        }
    }

    /// ⌃⌥8 fires the ⌃8 item (Add Marker) on macOS: the press goes to the UI,
    /// whose keymap runs Add Marker with Dialog. The exact chord, a mouse click
    /// and items without a shortcut run the item.
    #[test]
    fn a_key_equivalent_with_other_modifiers_goes_to_the_ui() {
        let ctrl_8 = Chord::parse("Ctrl+8").ok();
        let mods = |ctrl, alt| egui::Modifiers {
            ctrl,
            alt,
            ..Default::default()
        };
        let press = key_press_for_ui(ctrl_8, Some(mods(true, true)));
        assert!(matches!(
            press,
            Some(egui::Event::Key { key: egui::Key::Num8, pressed: true, modifiers, .. })
                if modifiers == mods(true, true)
        ));
        if cfg!(target_os = "macos") {
            assert!(key_press_for_ui(ctrl_8, Some(mods(true, false))).is_none());
        }
        assert!(key_press_for_ui(ctrl_8, None).is_none());
        assert!(key_press_for_ui(None, Some(mods(true, true))).is_none());
    }

    #[test]
    fn chords_become_native_accelerators() {
        let cmd = if cfg!(target_os = "macos") {
            Modifiers::META
        } else {
            Modifiers::CONTROL
        };
        let a = |s: &str| accelerator(&Chord::parse(s).unwrap());
        assert_eq!(a("Cmd+N"), Some(Accelerator::new(cmd, Code::KeyN)));
        assert_eq!(
            a("Cmd+Alt+Shift+Y"),
            Some(Accelerator::new(
                cmd | Modifiers::ALT | Modifiers::SHIFT,
                Code::KeyY
            ))
        );
        assert_eq!(
            a("F9"),
            Some(Accelerator::new(Modifiers::empty(), Code::F9))
        );
        assert_eq!(
            a("Cmd+CloseBracket"),
            Some(Accelerator::new(cmd, Code::BracketRight))
        );
    }

    #[test]
    fn contributed_ids_are_namespaced() {
        // Shell items keep `command:argument` ids; contributed items use their own
        // prefix and are looked up by `AppMenu::contributed_invocation`.
        assert!(format!("{CONTRIBUTED}motion.layer.new.solid").starts_with("bae.menu:"));
        assert_ne!(
            invocation("bae.menu:motion.layer.new.solid").id,
            "motion.new_layer"
        );
    }
}
