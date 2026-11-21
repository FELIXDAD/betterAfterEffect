//! Native menu bar (muda).
//!
//! Every menu item id is a command id (`bae_ui::action::ids`, `bae-model` command
//! ids); an id with parameters is written `command:argument` and becomes the
//! invocation `{"id": argument}` (`workspace.switch:motion`, `window.panel:project`).
//! Clicks go through the same dispatch as shortcuts and panel requests.
//!
//! The shell's own items deliberately carry no native key equivalents (except Quit
//! on macOS): an NSMenu key equivalent would swallow Cmd+Z / Cmd+C before egui sees
//! them, and the shortcuts switch with the workspace. `bae-ui`'s keymap handles
//! every shortcut. Items in `Registry<MenuItemSpec>` — the File items
//! ([`file_items`]) and the menus modules contribute — show the chord the active
//! keymap binds to their invocation ([`AppMenu::sync`]).

use std::collections::HashMap;
use std::sync::Arc;

use bae_ui::action::ids;
use bae_ui::menu::{MenuContext, MenuEntry, MenuItemSpec, MenuTree};
use bae_ui::{Chord, Invocation, Keymap};
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};

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

/// The shell's File menu items (AE order: New Project, Open Project… — Close
/// Project, Save, Save As…). Groups 0 and 10; other modules' File items (Import,
/// Export …) use other groups and are sorted in between or after.
pub fn file_items() -> Vec<(&'static str, MenuItemSpec)> {
    let item = |title: &str, action: &str, group: i32, order: i32| {
        MenuItemSpec::new(&["File"], title, action)
            .group(group)
            .order(order)
    };
    vec![
        (
            "shell.file.new_project",
            item("New Project", ids::NEW_PROJECT, 0, 0),
        ),
        (
            "shell.file.open_project",
            item("Open Project...", ids::OPEN_PROJECT, 0, 1),
        ),
        (
            "shell.file.close_project",
            item("Close Project", ids::CLOSE_PROJECT, 10, 0),
        ),
        ("shell.file.save", item("Save", ids::SAVE, 10, 1)),
        (
            "shell.file.save_as",
            item("Save As...", ids::SAVE_AS, 10, 2),
        ),
    ]
}

/// A contributed item and the state last pushed to the native menu.
struct Contributed {
    item: MenuItem,
    spec: Arc<MenuItemSpec>,
    enabled: bool,
    chord: Option<Chord>,
}

/// The application menu bar.
pub struct AppMenu {
    menu: Menu,
    undo: MenuItem,
    redo: MenuItem,
    undo_state: (Option<String>, Option<String>),
    /// Contributed items by registry id.
    contributed: HashMap<String, Contributed>,
}

impl AppMenu {
    /// Builds the menu bar; `workspaces` are `(id, English title)` in bar order,
    /// `contributed` the menus modules registered (placed in AE order between Edit
    /// and Window; items for File / Edit / Window / Help are appended to those).
    pub fn new(workspaces: &[(String, String)], contributed: &MenuTree) -> muda::Result<Self> {
        let menu = Menu::new();
        let mut items: HashMap<String, Contributed> = HashMap::new();
        let extra = |title: &str| {
            contributed
                .menus
                .iter()
                .find(|(t, _)| t == title)
                .map(|(_, e)| e)
        };

        #[cfg(target_os = "macos")]
        {
            let app = Submenu::new(bae_platform::APP_NAME, true);
            app.append_items(&[
                &PredefinedMenuItem::about(Some(tr("About BAE")), Some(about_metadata())),
                &PredefinedMenuItem::separator(),
                &MenuItem::with_id(
                    ids::KEYBOARD_SHORTCUTS,
                    tr("Keyboard Shortcuts"),
                    true,
                    None,
                ),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &MenuItem::with_id(
                    ids::QUIT,
                    tr("Quit BAE"),
                    true,
                    Some(muda::accelerator::Accelerator::new(
                        muda::accelerator::Modifiers::META,
                        muda::accelerator::Code::KeyQ,
                    )),
                ),
            ])?;
            menu.append(&app)?;
        }

        // File: the registered items (the shell's own come from `file_items`),
        // then Exit on Windows.
        let file = Submenu::new(tr("File"), true);
        let file_entries = extra("File").map(Vec::as_slice).unwrap_or_default();
        append_entries(&file, file_entries, &mut items)?;
        #[cfg(not(target_os = "macos"))]
        {
            if !file_entries.is_empty() {
                file.append(&PredefinedMenuItem::separator())?;
            }
            file.append(&MenuItem::with_id(ids::QUIT, tr("Exit"), true, None))?;
        }
        menu.append(&file)?;

        let edit = Submenu::new(tr("Edit"), true);
        let undo = MenuItem::with_id(ids::UNDO, tr("Undo"), false, None);
        let redo = MenuItem::with_id(ids::REDO, tr("Redo"), false, None);
        edit.append_items(&[&undo, &redo])?;
        #[cfg(not(target_os = "macos"))]
        edit.append_items(&[
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(
                ids::KEYBOARD_SHORTCUTS,
                tr("Keyboard Shortcuts"),
                true,
                None,
            ),
        ])?;
        if let Some(entries) = extra("Edit") {
            edit.append(&PredefinedMenuItem::separator())?;
            append_entries(&edit, entries, &mut items)?;
        }
        menu.append(&edit)?;

        // Menus contributed by modules (Composition, Layer, Animation …).
        for (title, entries) in &contributed.menus {
            if ["File", "Edit", "Window", "Help"].contains(&title.as_str()) {
                continue;
            }
            let sub = Submenu::new(tr(title), true);
            append_entries(&sub, entries, &mut items)?;
            menu.append(&sub)?;
        }

        let window = Submenu::new(tr("Window"), true);
        let workspace = Submenu::new(tr("Workspace"), true);
        for (id, title) in workspaces {
            workspace.append(&MenuItem::with_id(
                format!("{}:{id}", ids::SWITCH_WORKSPACE),
                tr(title),
                true,
                None,
            ))?;
        }
        workspace.append_items(&[
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(
                ids::SAVE_WORKSPACE,
                tr("Save Changes to this Workspace"),
                true,
                None,
            ),
            &MenuItem::with_id(
                ids::RESET_WORKSPACE,
                tr("Reset to Saved Layout"),
                true,
                None,
            ),
            &MenuItem::with_id(
                ids::NEW_WORKSPACE,
                tr("Save as New Workspace..."),
                true,
                None,
            ),
            &MenuItem::with_id(ids::EDIT_WORKSPACES, tr("Edit Workspaces..."), true, None),
        ])?;
        window.append(&workspace)?;
        window.append(&PredefinedMenuItem::separator())?;
        #[cfg(target_os = "macos")]
        window.append_items(&[
            &PredefinedMenuItem::minimize(Some(tr("Minimize"))),
            &PredefinedMenuItem::fullscreen(Some(tr("Full Screen"))),
            &PredefinedMenuItem::separator(),
        ])?;
        for (panel, title) in [
            ("project", "Project"),
            ("composition", "Composition"),
            ("timeline", "Timeline"),
            ("effect_controls", "Effect Controls"),
            ("effects", "Effects & Presets"),
            ("preview", "Preview"),
            ("properties", "Properties"),
            ("info", "Info"),
            ("history", "History"),
            ("agent_activity", "Agent Activity"),
            ("agent_proposals", "Proposals"),
        ] {
            window.append(&MenuItem::with_id(
                format!("{}:{panel}", ids::OPEN_PANEL),
                tr(title),
                true,
                None,
            ))?;
        }
        if let Some(entries) = extra("Window") {
            window.append(&PredefinedMenuItem::separator())?;
            append_entries(&window, entries, &mut items)?;
        }
        menu.append(&window)?;

        let help = Submenu::new(tr("Help"), true);
        help.append(&MenuItem::with_id(ids::HELP, tr("BAE Help"), true, None))?;
        #[cfg(not(target_os = "macos"))]
        help.append_items(&[
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(ids::ABOUT, tr("About BAE"), true, None),
        ])?;
        if let Some(entries) = extra("Help") {
            help.append(&PredefinedMenuItem::separator())?;
            append_entries(&help, entries, &mut items)?;
        }
        menu.append(&help)?;

        Ok(Self {
            menu,
            undo,
            redo,
            undo_state: (None, None),
            contributed: items,
        })
    }

    /// The invocation of a contributed item (`None` for the shell's own items,
    /// which [`invocation`] decodes).
    pub fn contributed_invocation(&self, menu_id: &str) -> Option<Invocation> {
        let id = menu_id.strip_prefix(CONTRIBUTED)?;
        self.contributed.get(id).map(|c| c.spec.invocation())
    }

    /// Updates contributed items: enabled state, and the shortcut the active
    /// keymap binds to the same invocation (display; on macOS the menu also
    /// handles the key, which then does not reach the UI, so it runs once).
    pub fn sync(&mut self, keymap: &Keymap, cx: &MenuContext<'_>) {
        for c in self.contributed.values_mut() {
            let enabled = c.spec.is_enabled(cx);
            if enabled != c.enabled {
                c.item.set_enabled(enabled);
                c.enabled = enabled;
            }
            let chord = keymap
                .chord_for_invocation(&c.spec.invocation())
                .filter(|ch| !ch.is_typing_key())
                .cloned();
            if chord != c.chord {
                let _ = c.item.set_accelerator(chord.as_ref().and_then(accelerator));
                c.chord = chord;
            }
        }
    }

    /// Updates the Undo / Redo items (`Some(label)` = enabled, AE: "Undo <step>").
    pub fn set_undo_state(&mut self, state: (Option<String>, Option<String>)) {
        if state == self.undo_state {
            return;
        }
        let text = |base: &str, label: &Option<String>| match label {
            Some(label) if !label.is_empty() => format!("{} {label}", tr(base)),
            _ => tr(base).to_owned(),
        };
        self.undo.set_text(text("Undo", &state.0));
        self.undo.set_enabled(state.0.is_some());
        self.redo.set_text(text("Redo", &state.1));
        self.redo.set_enabled(state.1.is_some());
        self.undo_state = state;
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

/// Appends resolved menu entries (items, separators, submenus) to `menu`.
fn append_entries(
    menu: &Submenu,
    entries: &[MenuEntry],
    items: &mut HashMap<String, Contributed>,
) -> muda::Result<()> {
    for e in entries {
        match e {
            MenuEntry::Separator => menu.append(&PredefinedMenuItem::separator())?,
            MenuEntry::Item { id, spec } => {
                let item =
                    MenuItem::with_id(format!("{CONTRIBUTED}{id}"), tr(&spec.title), true, None);
                menu.append(&item)?;
                items.insert(
                    id.clone(),
                    Contributed {
                        item,
                        spec: spec.clone(),
                        enabled: true,
                        chord: None,
                    },
                );
            }
            MenuEntry::Submenu { title, entries } => {
                let sub = Submenu::new(tr(title), true);
                append_entries(&sub, entries, items)?;
                menu.append(&sub)?;
            }
        }
    }
    Ok(())
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
    fn file_items_follow_ae_order_and_shortcuts() {
        let registry = bae_app::Registry::<MenuItemSpec>::new();
        for (id, item) in file_items() {
            registry.register(bae_app::Owner::Core, id, item).unwrap();
        }
        let tree = bae_ui::menu::build_menus(&registry.snapshot(), "motion");
        assert_eq!(tree.menus.len(), 1);
        let (title, entries) = &tree.menus[0];
        assert_eq!(title, "File");
        let shown: Vec<String> = entries
            .iter()
            .map(|e| match e {
                MenuEntry::Item { spec, .. } => spec.title.clone(),
                MenuEntry::Separator => "---".into(),
                MenuEntry::Submenu { title, .. } => title.clone(),
            })
            .collect();
        assert_eq!(
            shown,
            [
                "New Project",
                "Open Project...",
                "---",
                "Close Project",
                "Save",
                "Save As..."
            ]
        );
        // The AE keymap's chords are what the native menu shows (and on macOS
        // handles) for these items.
        let keymap = bae_ui::presets::ae_keymap();
        let chords: Vec<Option<String>> = tree
            .items()
            .iter()
            .map(|(_, spec)| {
                keymap
                    .chord_for_invocation(&spec.invocation())
                    .map(|c| c.to_string())
            })
            .collect();
        let expect = |s: &str| Some(s.to_owned());
        assert_eq!(
            chords,
            [
                expect("Cmd+Alt+N"),
                expect("Cmd+O"),
                None, // AE has no default shortcut for Close Project
                expect("Cmd+S"),
                expect("Cmd+Shift+S"),
            ]
        );
        for (_, spec) in tree.items() {
            assert!(
                crate::files::is_file_command(&spec.action),
                "{}",
                spec.action
            );
        }
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
