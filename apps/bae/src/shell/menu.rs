//! Native menu bar (muda).
//!
//! Every menu item id is a command id (`bae_ui::action::ids`, `bae-model` command
//! ids); an id with parameters is written `command:argument` and becomes the
//! invocation `{"id": argument}` (`workspace.switch:motion`, `window.panel:project`).
//! Clicks go through the same dispatch as shortcuts and panel requests.
//!
//! Items deliberately carry no native key equivalents (except Quit on macOS): an
//! NSMenu key equivalent would swallow Cmd+Z / Cmd+C before egui sees them, and the
//! shortcuts switch with the workspace. `bae-ui`'s keymap handles every shortcut.

use bae_ui::Invocation;
use bae_ui::action::ids;
use muda::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};

use crate::i18n::tr;

/// Turns a menu item id back into an invocation.
pub fn invocation(menu_id: &str) -> Invocation {
    match menu_id.split_once(':') {
        Some((id, arg)) => Invocation::with_params(id, serde_json::json!({ "id": arg })),
        None => Invocation::new(menu_id),
    }
}

/// The application menu bar.
pub struct AppMenu {
    menu: Menu,
    undo: MenuItem,
    redo: MenuItem,
    undo_state: (Option<String>, Option<String>),
}

impl AppMenu {
    /// Builds the menu bar; `workspaces` are `(id, English title)` in bar order.
    pub fn new(workspaces: &[(String, String)]) -> muda::Result<Self> {
        let menu = Menu::new();

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

        let file = Submenu::new(tr("File"), true);
        file.append_items(&[
            &MenuItem::with_id(ids::NEW_PROJECT, tr("New Project"), true, None),
            &MenuItem::with_id(ids::OPEN_PROJECT, tr("Open Project…"), true, None),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(ids::SAVE, tr("Save"), true, None),
            &MenuItem::with_id(ids::SAVE_AS, tr("Save As..."), true, None),
        ])?;
        #[cfg(not(target_os = "macos"))]
        file.append_items(&[
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(ids::QUIT, tr("Exit"), true, None),
        ])?;
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
        menu.append(&edit)?;

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
        menu.append(&window)?;

        let help = Submenu::new(tr("Help"), true);
        help.append(&MenuItem::with_id(ids::HELP, tr("BAE Help"), true, None))?;
        #[cfg(not(target_os = "macos"))]
        help.append_items(&[
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(ids::ABOUT, tr("About BAE"), true, None),
        ])?;
        menu.append(&help)?;

        Ok(Self {
            menu,
            undo,
            redo,
            undo_state: (None, None),
        })
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
}
