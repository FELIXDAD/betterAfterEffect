//! Native dialogs for the file commands (rfd): NSOpenPanel / NSSavePanel / NSAlert
//! sheets on macOS, IFileOpenDialog / IFileSaveDialog / MessageBoxW on Windows,
//! owned by the main window. Other platforms fall back to [`NoDialogs`].
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

    fn error(&mut self, message: &str) {
        NoDialogs.error(message);
    }
}
