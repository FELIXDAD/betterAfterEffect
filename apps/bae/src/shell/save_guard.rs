//! What the Windows save dialog's callbacks decide about existing `.bae` projects
//! (issue #126), kept free of Win32 so it is tested on every platform.
//!
//! A `.bae` project is a folder. In an IFileSaveDialog, selecting `proj.bae` (or
//! typing its name) and pressing Save walks into the folder, and typing `proj`
//! (the default extension makes it `proj.bae`) fails with "You don't have
//! permission to save in this location". Both mean "save over that project", so
//! the dialog closes with the project's path and Save As asks the usual "Replace
//! the existing project?" question ([`crate::files::save_as`]).

use std::path::{Path, PathBuf};

use crate::files::is_bae_project_dir;

/// State shared by one save dialog's event callbacks.
#[derive(Debug, Default)]
pub struct SaveGuard {
    /// A folder change has completed: the dialog is up and later navigations are
    /// the user's (its own first navigation to the start folder is not).
    shown: bool,
    /// The existing project the user chose.
    project: Option<PathBuf>,
}

impl SaveGuard {
    /// `OnFolderChanging`: the dialog is about to show `target`; `current` is the
    /// folder it shows now. `true` = an existing project was chosen: block the
    /// navigation and close the dialog.
    ///
    /// The dialog's own first navigation (to the start folder, which may be a
    /// remembered one inside a project) is let through: it is neither after a
    /// completed folder change nor from the project's parent.
    pub fn folder_changing(&mut self, target: &Path, current: Option<&Path>) -> bool {
        if !is_bae_project_dir(target) {
            return false;
        }
        let from_parent = match (target.parent(), current) {
            (Some(parent), Some(current)) => crate::document::same_path(parent, current),
            _ => false,
        };
        if !(self.shown || from_parent) {
            return false;
        }
        self.project = Some(target.to_owned());
        true
    }

    /// `OnFolderChange`: a navigation completed.
    pub fn folder_changed(&mut self) {
        self.shown = true;
    }

    /// `OnFileOk`: the paths the dialog may mean (its result, and the typed name
    /// in the current folder with the default extension). An existing project
    /// among them is the choice; the dialog then closes normally.
    pub fn file_ok(&mut self, candidates: &[PathBuf]) {
        if let Some(project) = candidates.iter().find(|p| is_bae_project_dir(p)) {
            self.project = Some(project.clone());
        }
    }

    /// `OnOverwrite`: `true` = accept without the system's prompt, because Save As
    /// asks its own question about replacing a project.
    pub fn overwrite_is_ours(&self, item: &Path) -> bool {
        is_bae_project_dir(item)
    }

    /// The dialog's answer: the chosen project, else what the dialog returned
    /// (`None` = cancelled).
    pub fn picked(&mut self, result: Option<PathBuf>) -> Option<PathBuf> {
        self.project.take().or(result)
    }
}

/// The path the user typed: `typed` in `folder`, with `default_ext` added when it
/// has no extension (what the dialog does), e.g. `proj` → `proj.bae`.
pub fn typed_path(folder: &Path, typed: &str, default_ext: &str) -> Option<PathBuf> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    let mut path = folder.join(typed);
    if path.extension().is_none() && !default_ext.is_empty() {
        let mut name = path.file_name()?.to_owned();
        name.push(".");
        name.push(default_ext);
        path.set_file_name(name);
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn project(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("project.json"), b"{}").unwrap();
        path
    }

    #[test]
    fn walking_into_a_project_from_its_folder_picks_it() {
        let dir = tempfile::tempdir().unwrap();
        let proj = project(dir.path(), "proj.bae");
        let mut guard = SaveGuard::default();
        // Selected in the list + Save, or double-clicked: before any completed
        // folder change, from the project's parent.
        assert!(guard.folder_changing(&proj, Some(dir.path())));
        assert_eq!(guard.picked(None), Some(proj));
    }

    #[test]
    fn after_the_dialog_is_up_any_way_into_a_project_picks_it() {
        let dir = tempfile::tempdir().unwrap();
        let proj = project(&dir.path().join("deep"), "Film.BAE");
        let mut guard = SaveGuard::default();
        guard.folder_changed();
        // Typed as a full path elsewhere, upper-case extension.
        assert!(guard.folder_changing(&proj, Some(dir.path())));
        assert_eq!(guard.picked(None), Some(proj));
    }

    #[test]
    fn the_first_navigation_and_plain_folders_are_let_through() {
        let dir = tempfile::tempdir().unwrap();
        let proj = project(dir.path(), "proj.bae");
        let plain = dir.path().join("renders.bae");
        fs::create_dir(&plain).unwrap(); // named like one, no project.json
        let other = dir.path().join("Videos");
        fs::create_dir(&other).unwrap();
        let mut guard = SaveGuard::default();
        // The start folder is a remembered project: the dialog opens there.
        assert!(!guard.folder_changing(&proj, None));
        assert!(!guard.folder_changing(&proj, Some(&proj)));
        guard.folder_changed();
        assert!(!guard.folder_changing(&plain, Some(dir.path())));
        assert!(!guard.folder_changing(&other, Some(dir.path())));
        assert_eq!(guard.picked(None), None, "cancelled");
        let file = dir.path().join("x.baez");
        assert_eq!(guard.picked(Some(file.clone())), Some(file));
    }

    #[test]
    fn a_typed_name_that_becomes_a_project_picks_it() {
        let dir = tempfile::tempdir().unwrap();
        let proj = project(dir.path(), "proj.bae");
        let typed = typed_path(dir.path(), "proj", "bae").unwrap();
        assert_eq!(typed, proj);
        let mut guard = SaveGuard::default();
        guard.file_ok(&[dir.path().join("proj"), typed]);
        assert!(guard.overwrite_is_ours(&proj));
        assert_eq!(guard.picked(Some(dir.path().join("proj"))), Some(proj));
    }

    #[test]
    fn new_names_and_files_stay_the_dialogs_answer() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("old.baez");
        fs::write(&file, b"PK").unwrap();
        let mut guard = SaveGuard::default();
        let new = typed_path(dir.path(), " new ", "bae").unwrap();
        assert_eq!(new, dir.path().join("new.bae"));
        guard.file_ok(&[new.clone(), file.clone()]);
        assert!(!guard.overwrite_is_ours(&file), "the system asks for files");
        assert_eq!(guard.picked(Some(new.clone())), Some(new));
        assert_eq!(
            typed_path(dir.path(), "pkg.baez", "bae").unwrap(),
            dir.path().join("pkg.baez")
        );
        assert_eq!(typed_path(dir.path(), "  ", "bae"), None);
    }
}
