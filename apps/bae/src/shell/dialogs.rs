//! Native dialogs for the file commands (rfd): NSOpenPanel / NSSavePanel / NSAlert
//! sheets on macOS, IFileOpenDialog / IFileSaveDialog / MessageBoxW on Windows,
//! owned by the main window. Other platforms fall back to [`NoDialogs`]. Save As
//! on Windows is our own IFileSaveDialog ([`super::win_save`]), which treats a
//! `.bae` folder as the project it is instead of walking into it.
//!
//! A `.bae` project is a folder. macOS panels can pick a file or a folder, so the
//! open panel accepts `.bae` folders and `.baez` files directly; Windows dialogs
//! pick one or the other, so there the user picks a `.baez` file or the
//! `project.json` inside a `.bae` folder (`document::project_path` maps it back).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use winit::window::Window;

#[cfg(not(any(target_os = "macos", windows)))]
use crate::files::NoDialogs;
use crate::files::{FileDialogs, UnsavedChoice};
#[cfg(any(target_os = "macos", windows))]
use crate::i18n::tr;

/// The native dialogs, as sheets / owned windows of `parent`.
pub struct Native {
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    parent: Option<Arc<Window>>,
}

impl Native {
    pub fn new(parent: Option<Arc<Window>>) -> Self {
        Native { parent }
    }
}

#[cfg(any(target_os = "macos", windows))]
impl Native {
    fn files(&self) -> rfd::FileDialog {
        let dialog = rfd::FileDialog::new();
        match &self.parent {
            Some(window) => dialog.set_parent(&**window),
            None => dialog,
        }
    }

    fn message(&self, level: rfd::MessageLevel) -> rfd::MessageDialog {
        let dialog = rfd::MessageDialog::new().set_level(level);
        match &self.parent {
            Some(window) => dialog.set_parent(&**window),
            None => dialog,
        }
    }
}

#[cfg(any(target_os = "macos", windows))]
impl FileDialogs for Native {
    fn pick_project(&mut self) -> Option<PathBuf> {
        let dialog = self.files().set_title(tr("Open Project"));
        #[cfg(target_os = "macos")]
        return dialog
            .add_filter(tr("BAE Project"), &["bae", "baez"])
            .pick_file_or_folder();
        #[cfg(windows)]
        return dialog
            .add_filter(tr("BAE Project (.baez, project.json)"), &["baez", "json"])
            .pick_file();
    }

    fn pick_save_path(&mut self, name: &str, dir: Option<&Path>) -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            let mut dialog = self
                .files()
                .set_title(tr("Save As"))
                .set_file_name(name)
                .set_format_label(tr("Format:"))
                .add_filter(tr("BAE Project"), &["bae"])
                .add_filter(tr("BAE Package"), &["baez"]);
            if let Some(dir) = dir {
                dialog = dialog.set_directory(dir);
            }
            dialog.save_file()
        }
        // Our own IFileSaveDialog: rfd's walks into `.bae` folders (issue #126).
        #[cfg(windows)]
        {
            super::win_save::pick(&super::win_save::SaveRequest {
                title: tr("Save As"),
                name,
                dir,
                types: &[(tr("BAE Project"), "bae"), (tr("BAE Package"), "baez")],
                parent: self.parent.as_deref().and_then(hwnd),
            })
        }
    }

    fn ask_unsaved(&mut self, name: &str) -> UnsavedChoice {
        let question = tr("Save changes to “{}” before closing?").replace("{}", name);
        let dialog = self
            .message(rfd::MessageLevel::Warning)
            .set_title(question.as_str());
        // macOS: custom buttons in the HIG order (Save is the default, Don't Save
        // on the far side). Windows: a MessageBox's Yes / No / Cancel (custom
        // labels would need comctl32 v6, which the executable does not declare).
        #[cfg(target_os = "macos")]
        {
            let (save, dont_save) = (tr("Save").to_owned(), tr("Don't Save").to_owned());
            let result = dialog
                .set_description(tr("If you don't save, your changes will be lost."))
                .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                    save.clone(),
                    tr("Cancel").to_owned(),
                    dont_save.clone(),
                ))
                .show();
            match result {
                rfd::MessageDialogResult::Custom(c) if c == save => UnsavedChoice::Save,
                rfd::MessageDialogResult::Custom(c) if c == dont_save => UnsavedChoice::DontSave,
                _ => UnsavedChoice::Cancel,
            }
        }
        #[cfg(windows)]
        {
            let result = dialog
                .set_title(bae_platform::APP_NAME)
                .set_description(question.as_str())
                .set_buttons(rfd::MessageButtons::YesNoCancel)
                .show();
            match result {
                rfd::MessageDialogResult::Yes => UnsavedChoice::Save,
                rfd::MessageDialogResult::No => UnsavedChoice::DontSave,
                _ => UnsavedChoice::Cancel,
            }
        }
    }

    fn confirm_replace(&mut self, name: &str) -> bool {
        let question = tr("“{}” already exists. Do you want to replace it?").replace("{}", name);
        let detail = tr("The existing project is replaced. This cannot be undone.");
        let dialog = self.message(rfd::MessageLevel::Warning);
        #[cfg(target_os = "macos")]
        {
            let replace = tr("Replace").to_owned();
            let result = dialog
                .set_title(question.as_str())
                .set_description(detail)
                .set_buttons(rfd::MessageButtons::OkCancelCustom(
                    replace.clone(),
                    tr("Cancel").to_owned(),
                ))
                .show();
            matches!(result, rfd::MessageDialogResult::Custom(c) if c == replace)
        }
        #[cfg(windows)]
        {
            let result = dialog
                .set_title(bae_platform::APP_NAME)
                .set_description(format!("{question}\n\n{detail}"))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            result == rfd::MessageDialogResult::Yes
        }
    }

    fn save_dialog_confirms_replace(&self) -> bool {
        // In BAE.app, `.bae` is a package (Info.plist), so the save panel treats
        // it as a file and asks before replacing; a bare binary does not.
        cfg!(target_os = "macos")
            && std::env::current_exe()
                .is_ok_and(|exe| exe.to_string_lossy().contains(".app/Contents/MacOS/"))
    }

    fn error(&mut self, message: &str) {
        tracing::error!("{message}");
        let (title, detail) = message.split_once("\n\n").unwrap_or((message, ""));
        // NSAlert shows the title bold with the detail below; a MessageBox has
        // a caption and a body.
        let (title, detail) = if cfg!(windows) {
            (bae_platform::APP_NAME, message)
        } else {
            (title, detail)
        };
        self.message(rfd::MessageLevel::Error)
            .set_title(title)
            .set_description(detail)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

/// The window's Win32 handle, to own the save dialog.
#[cfg(windows)]
fn hwnd(window: &Window) -> Option<windows::Win32::Foundation::HWND> {
    use winit::raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(windows::Win32::Foundation::HWND(
            handle.hwnd.get() as *mut std::ffi::c_void
        )),
        _ => None,
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
impl FileDialogs for Native {
    fn pick_project(&mut self) -> Option<PathBuf> {
        NoDialogs.pick_project()
    }

    fn pick_save_path(&mut self, name: &str, dir: Option<&Path>) -> Option<PathBuf> {
        NoDialogs.pick_save_path(name, dir)
    }

    fn ask_unsaved(&mut self, name: &str) -> UnsavedChoice {
        NoDialogs.ask_unsaved(name)
    }

    fn confirm_replace(&mut self, name: &str) -> bool {
        NoDialogs.confirm_replace(name)
    }

    fn error(&mut self, message: &str) {
        NoDialogs.error(message);
    }
}

/// The native open panel for modules ([`bae_ui::files::FilePicker`]): File >
/// Import and other choosers, owned by the main window.
pub struct NativePicker {
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    parent: Option<Arc<Window>>,
}

impl NativePicker {
    pub fn new(parent: Option<Arc<Window>>) -> Self {
        NativePicker { parent }
    }
}

impl bae_ui::files::FilePicker for NativePicker {
    #[cfg(any(target_os = "macos", windows))]
    fn open(&self, request: &bae_ui::files::OpenRequest) -> Vec<PathBuf> {
        let mut dialog = Native::new(self.parent.clone())
            .files()
            .set_title(&request.title);
        for (label, extensions) in &request.filters {
            dialog = dialog.add_filter(label, extensions);
        }
        #[cfg(target_os = "macos")]
        if request.folders {
            return if request.multiple {
                dialog.pick_files_or_folders().unwrap_or_default()
            } else {
                dialog.pick_file_or_folder().into_iter().collect()
            };
        }
        if request.multiple {
            dialog.pick_files().unwrap_or_default()
        } else {
            dialog.pick_file().into_iter().collect()
        }
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    fn open(&self, _request: &bae_ui::files::OpenRequest) -> Vec<PathBuf> {
        Vec::new()
    }
}
