//! Native menu bar (muda).
//!
//! Menu item ids are command ids (`file.open`, `app.quit` …). Today the shell
//! handles the few it owns and reports the rest as not implemented; once the
//! command registry (bae-model, workstream 02) lands, menus are built from
//! registered commands and every click dispatches through it.
//!
//! Edit items deliberately carry no native key equivalents: on macOS an NSMenu
//! key equivalent would swallow Cmd+Z / Cmd+C before egui sees them. Shortcuts
//! are handled by the UI shortcut system (workstream 11).

use muda::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};

use crate::i18n::tr;

/// Command ids of the shell's own menu items.
pub mod ids {
    /// Quit the application.
    pub const QUIT: &str = "app.quit";
    /// Show the About window.
    pub const ABOUT: &str = "app.about";
    /// Restore the default panel layout.
    pub const RESET_WORKSPACE: &str = "window.reset_workspace";
    /// Open the documentation.
    pub const HELP: &str = "help.docs";
}

/// The application menu bar.
pub struct AppMenu {
    menu: Menu,
}

impl AppMenu {
    /// Builds the menu bar.
    pub fn new() -> muda::Result<Self> {
        let menu = Menu::new();

        #[cfg(target_os = "macos")]
        {
            let app = Submenu::new(bae_platform::APP_NAME, true);
            app.append_items(&[
                &PredefinedMenuItem::about(Some(tr("About BAE")), Some(about_metadata())),
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
            &MenuItem::with_id("file.new_project", tr("New Project"), false, None),
            &MenuItem::with_id("file.open_project", tr("Open Project…"), false, None),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id("file.save", tr("Save"), false, None),
        ])?;
        #[cfg(not(target_os = "macos"))]
        file.append_items(&[
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(ids::QUIT, tr("Exit"), true, None),
        ])?;
        menu.append(&file)?;

        let edit = Submenu::new(tr("Edit"), true);
        edit.append_items(&[
            &MenuItem::with_id("edit.undo", tr("Undo"), false, None),
            &MenuItem::with_id("edit.redo", tr("Redo"), false, None),
        ])?;
        menu.append(&edit)?;

        let window = Submenu::new(tr("Window"), true);
        #[cfg(target_os = "macos")]
        window.append_items(&[
            &PredefinedMenuItem::minimize(Some(tr("Minimize"))),
            &PredefinedMenuItem::fullscreen(Some(tr("Full Screen"))),
            &PredefinedMenuItem::separator(),
        ])?;
        window.append(&MenuItem::with_id(
            ids::RESET_WORKSPACE,
            tr("Reset Workspace"),
            true,
            None,
        ))?;
        menu.append(&window)?;

        let help = Submenu::new(tr("Help"), true);
        help.append(&MenuItem::with_id(ids::HELP, tr("BAE Help"), true, None))?;
        #[cfg(not(target_os = "macos"))]
        help.append_items(&[
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(ids::ABOUT, tr("About BAE"), true, None),
        ])?;
        menu.append(&help)?;

        Ok(Self { menu })
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
