//! File ▸ New Project / Open Project / Close Project / Save / Save As, and the
//! unsaved-changes question before a project goes away (close, new, open, quit).
//!
//! The flows only talk to the user through [`FileDialogs`]: the shell passes the
//! native dialogs (`shell::dialogs`), tests pass scripted answers and paths, and
//! smoke tests pass [`NoDialogs`] so nothing ever blocks. Shortcuts follow AE:
//! New Project Cmd/Ctrl+Alt+N, Open Cmd/Ctrl+O, Save Cmd/Ctrl+S, Save As
//! Cmd/Ctrl+Shift+S (`bae_ui::presets::ae_keymap`).

use std::path::{Path, PathBuf};

use bae_ui::Invocation;
use bae_ui::action::ids;

use crate::document::Document;
use crate::i18n::tr;

/// The answer to "Save changes before closing?".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsavedChoice {
    Save,
    DontSave,
    Cancel,
}

/// The dialogs the file commands need.
pub trait FileDialogs {
    /// Choose a project to open: a `.bae` folder or a `.baez` file (or the
    /// `project.json` inside a `.bae` folder). `None` = cancelled.
    fn pick_project(&mut self) -> Option<PathBuf>;
    /// Choose where to save; `name` is the suggested file name, `dir` the folder
    /// to start in. `None` = cancelled.
    fn pick_save_path(&mut self, name: &str, dir: Option<&Path>) -> Option<PathBuf>;
    /// Ask whether to save the changes to project `name` before it is closed.
    fn ask_unsaved(&mut self, name: &str) -> UnsavedChoice;
    /// Tell the user something failed.
    fn error(&mut self, message: &str);
}

/// Dialogs that never block: pickers are cancelled, unsaved changes are kept
/// (the close is cancelled), errors are logged. Smoke tests and platforms without
/// native dialogs.
#[derive(Debug, Default)]
pub struct NoDialogs;

impl FileDialogs for NoDialogs {
    fn pick_project(&mut self) -> Option<PathBuf> {
        tracing::warn!("no file dialog available; open a project with `bae <path>`");
        None
    }

    fn pick_save_path(&mut self, _name: &str, _dir: Option<&Path>) -> Option<PathBuf> {
        tracing::warn!("no file dialog available; the project was not saved");
        None
    }

    fn ask_unsaved(&mut self, name: &str) -> UnsavedChoice {
        tracing::warn!("{name} has unsaved changes and no dialog can ask about them");
        UnsavedChoice::Cancel
    }

    fn error(&mut self, message: &str) {
        tracing::error!("{message}");
    }
}

/// What a file command did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOutcome {
    /// Another project is open now (new, open, close): the shell resets the
    /// session state that belonged to the old one.
    Replaced,
    /// The project was written.
    Saved,
    /// The user cancelled a dialog or the unsaved-changes question.
    Cancelled,
    /// It failed; the error was reported through [`FileDialogs::error`].
    Failed,
}

/// Whether `id` is one of the file commands [`run`] handles.
pub fn is_file_command(id: &str) -> bool {
    matches!(
        id,
        ids::NEW_PROJECT | ids::OPEN_PROJECT | ids::CLOSE_PROJECT | ids::SAVE | ids::SAVE_AS
    )
}

/// Run a file command (see [`is_file_command`]); `None` for other ids.
/// `file.open_project` opens `{"path": …}` without a dialog when given.
pub fn run(
    doc: &mut Document,
    dialogs: &mut dyn FileDialogs,
    inv: &Invocation,
) -> Option<FileOutcome> {
    Some(match inv.id.as_str() {
        ids::NEW_PROJECT | ids::CLOSE_PROJECT => {
            // AE: Close Project leaves an empty untitled project.
            if !confirm_discard(doc, dialogs) {
                return Some(FileOutcome::Cancelled);
            }
            doc.new_project();
            FileOutcome::Replaced
        }
        ids::OPEN_PROJECT => {
            let path = inv
                .params
                .get("path")
                .and_then(|p| p.as_str())
                .map(PathBuf::from);
            open(doc, dialogs, path)
        }
        ids::SAVE => save(doc, dialogs),
        ids::SAVE_AS => save_as(doc, dialogs),
        _ => return None,
    })
}

/// Before the open project goes away (close, new, open, quit): when it has
/// unsaved changes, ask Save / Don't Save / Cancel. `true` = go ahead (saved or
/// discarded), `false` = keep the project open.
pub fn confirm_discard(doc: &mut Document, dialogs: &mut dyn FileDialogs) -> bool {
    if !doc.is_dirty() {
        return true;
    }
    match dialogs.ask_unsaved(&doc.display_name()) {
        UnsavedChoice::Save => save(doc, dialogs) == FileOutcome::Saved,
        UnsavedChoice::DontSave => true,
        UnsavedChoice::Cancel => false,
    }
}

/// Open a project (asking for it when `path` is `None`). The file is read first,
/// so a broken one is reported without touching the open project; then the
/// unsaved-changes question; then the switch.
pub fn open(
    doc: &mut Document,
    dialogs: &mut dyn FileDialogs,
    path: Option<PathBuf>,
) -> FileOutcome {
    let Some(path) = path.or_else(|| dialogs.pick_project()) else {
        return FileOutcome::Cancelled;
    };
    let opened = match Document::load(&path) {
        Ok(opened) => opened,
        Err(error) => {
            dialogs.error(&format!(
                "{}\n\n{error:#}",
                tr("Could not open the project.")
            ));
            return FileOutcome::Failed;
        }
    };
    if !confirm_discard(doc, dialogs) {
        return FileOutcome::Cancelled;
    }
    doc.replace(opened);
    tracing::info!("opened {}", path.display());
    FileOutcome::Replaced
}

/// Save to the current location; an untitled project asks where (Save As).
pub fn save(doc: &mut Document, dialogs: &mut dyn FileDialogs) -> FileOutcome {
    if doc.location().is_none() {
        return save_as(doc, dialogs);
    }
    match doc.save() {
        Ok(()) => FileOutcome::Saved,
        Err(error) => {
            report_save_error(dialogs, &error);
            FileOutcome::Failed
        }
    }
}

/// Ask for a location and save there (`.bae` folder, or `.baez` when chosen).
pub fn save_as(doc: &mut Document, dialogs: &mut dyn FileDialogs) -> FileOutcome {
    let location = doc.location().map(Path::to_owned);
    let name = match &location {
        Some(path) => path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        None => format!("{}.bae", tr("Untitled Project")),
    };
    let dir = location.as_deref().and_then(Path::parent);
    let Some(path) = dialogs.pick_save_path(&name, dir) else {
        return FileOutcome::Cancelled;
    };
    let path = with_project_extension(path);
    match doc.save_as(&path) {
        Ok(()) => {
            tracing::info!("saved {}", path.display());
            FileOutcome::Saved
        }
        Err(error) => {
            report_save_error(dialogs, &error);
            FileOutcome::Failed
        }
    }
}

/// `MyFilm` → `MyFilm.bae`; `.bae` and `.baez` names are kept (the extension
/// lower-cased: the model recognizes exactly `.baez`).
fn with_project_extension(path: PathBuf) -> PathBuf {
    let known = path.extension().and_then(|e| e.to_str()).and_then(|e| {
        ["bae", "baez"]
            .into_iter()
            .find(|k| e.eq_ignore_ascii_case(k))
    });
    match known {
        Some(ext) => path.with_extension(ext),
        None => {
            let mut name = path.into_os_string();
            name.push(".bae");
            PathBuf::from(name)
        }
    }
}

fn report_save_error(dialogs: &mut dyn FileDialogs, error: &anyhow::Error) {
    dialogs.error(&format!(
        "{}\n\n{error:#}",
        tr("Could not save the project.")
    ));
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;

    use super::*;
    use crate::document::Outcome;

    /// Scripted dialogs: queued paths and answers; records what was asked.
    #[derive(Default)]
    struct Script {
        open: VecDeque<Option<PathBuf>>,
        save: VecDeque<Option<PathBuf>>,
        answers: VecDeque<UnsavedChoice>,
        asked: Vec<String>,
        suggested: Vec<String>,
        errors: Vec<String>,
    }

    impl FileDialogs for Script {
        fn pick_project(&mut self) -> Option<PathBuf> {
            self.open.pop_front().expect("unexpected open dialog")
        }
        fn pick_save_path(&mut self, name: &str, _dir: Option<&Path>) -> Option<PathBuf> {
            self.suggested.push(name.to_owned());
            self.save.pop_front().expect("unexpected save dialog")
        }
        fn ask_unsaved(&mut self, name: &str) -> UnsavedChoice {
            self.asked.push(name.to_owned());
            self.answers
                .pop_front()
                .expect("unexpected unsaved question")
        }
        fn error(&mut self, message: &str) {
            self.errors.push(message.to_owned());
        }
    }

    fn edit(doc: &mut Document, path: &str) {
        let inv = Invocation::with_params(
            "prim.create",
            serde_json::json!({ "path": path, "prim_type": "Comp" }),
        );
        assert!(matches!(doc.run(&inv), Outcome::Changed));
    }

    fn has(doc: &Document, path: &str) -> bool {
        doc.stage().prim(&path.parse().unwrap()).is_some()
    }

    fn cmd(doc: &mut Document, d: &mut Script, id: &str) -> FileOutcome {
        run(doc, d, &Invocation::new(id)).expect("file command")
    }

    #[test]
    fn save_as_then_open_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::new(None);
        let mut d = Script::default();
        edit(&mut doc, "/Comp1");

        // Save on an untitled project asks where; the extension is added.
        d.save.push_back(Some(dir.path().join("Film")));
        assert_eq!(cmd(&mut doc, &mut d, ids::SAVE), FileOutcome::Saved);
        let path = dir.path().join("Film.bae");
        assert_eq!(doc.location(), Some(path.as_path()));
        assert_eq!(d.suggested, ["Untitled Project.bae"]);
        assert!(!doc.is_dirty());

        // Save again writes in place without a dialog.
        edit(&mut doc, "/Comp2");
        assert_eq!(cmd(&mut doc, &mut d, ids::SAVE), FileOutcome::Saved);

        // New project (nothing unsaved: no question), then open the saved one.
        assert_eq!(
            cmd(&mut doc, &mut d, ids::NEW_PROJECT),
            FileOutcome::Replaced
        );
        assert!(doc.location().is_none() && !has(&doc, "/Comp1"));
        d.open.push_back(Some(path.clone()));
        assert_eq!(
            cmd(&mut doc, &mut d, ids::OPEN_PROJECT),
            FileOutcome::Replaced
        );
        assert!(has(&doc, "/Comp1") && has(&doc, "/Comp2"));
        assert!(d.asked.is_empty(), "no unsaved changes, no question");

        // Save As to a package, then open it through the command's path param.
        d.save.push_back(Some(dir.path().join("Film.baez")));
        assert_eq!(cmd(&mut doc, &mut d, ids::SAVE_AS), FileOutcome::Saved);
        assert_eq!(d.suggested.last().map(String::as_str), Some("Film.bae"));
        let mut other = Document::new(None);
        let inv = Invocation::with_params(
            ids::OPEN_PROJECT,
            serde_json::json!({ "path": dir.path().join("Film.baez") }),
        );
        assert_eq!(run(&mut other, &mut d, &inv), Some(FileOutcome::Replaced));
        assert!(has(&other, "/Comp2"));
        assert!(d.errors.is_empty(), "{:?}", d.errors);
        assert_eq!(run(&mut other, &mut d, &Invocation::new(ids::UNDO)), None);
    }

    #[test]
    fn unsaved_changes_ask_before_the_project_goes_away() {
        let dir = tempfile::tempdir().unwrap();
        let saved = dir.path().join("Saved.bae");
        let mut doc = Document::new(None);
        let mut d = Script::default();
        edit(&mut doc, "/Keep");

        // Cancel keeps the project and its changes.
        d.answers.push_back(UnsavedChoice::Cancel);
        assert_eq!(
            cmd(&mut doc, &mut d, ids::CLOSE_PROJECT),
            FileOutcome::Cancelled
        );
        assert!(has(&doc, "/Keep") && doc.is_dirty());
        assert_eq!(d.asked, ["Untitled Project"]);

        // Save, but cancel the Save As dialog: still nothing closes.
        d.answers.push_back(UnsavedChoice::Save);
        d.save.push_back(None);
        assert_eq!(
            cmd(&mut doc, &mut d, ids::NEW_PROJECT),
            FileOutcome::Cancelled
        );
        assert!(has(&doc, "/Keep"));

        // Save for real, then the new project replaces it.
        d.answers.push_back(UnsavedChoice::Save);
        d.save.push_back(Some(saved.clone()));
        assert_eq!(
            cmd(&mut doc, &mut d, ids::NEW_PROJECT),
            FileOutcome::Replaced
        );
        assert!(!has(&doc, "/Keep"));
        let mut check = Document::new(None);
        check.open(&saved).unwrap();
        assert!(has(&check, "/Keep"));

        // Don't Save discards; opening asks only after the file was read.
        edit(&mut doc, "/Scratch");
        d.open.push_back(Some(dir.path().join("missing.bae")));
        assert_eq!(
            cmd(&mut doc, &mut d, ids::OPEN_PROJECT),
            FileOutcome::Failed
        );
        assert_eq!(d.errors.len(), 1);
        assert_eq!(d.asked.len(), 3, "a broken file is reported before asking");
        assert!(has(&doc, "/Scratch"));
        d.open.push_back(Some(saved.clone()));
        d.answers.push_back(UnsavedChoice::DontSave);
        assert_eq!(
            cmd(&mut doc, &mut d, ids::OPEN_PROJECT),
            FileOutcome::Replaced
        );
        assert!(has(&doc, "/Keep") && !has(&doc, "/Scratch"));

        // Cancelling the open dialog does nothing (and asks nothing).
        edit(&mut doc, "/More");
        d.open.push_back(None);
        assert_eq!(
            cmd(&mut doc, &mut d, ids::OPEN_PROJECT),
            FileOutcome::Cancelled
        );
        assert_eq!(d.asked.len(), 4);

        // Quit: the same question; Save writes in place (the project has a location).
        d.answers.push_back(UnsavedChoice::Save);
        assert!(confirm_discard(&mut doc, &mut d));
        assert!(!doc.is_dirty());
        assert!(confirm_discard(&mut doc, &mut d), "clean: no question");
        assert_eq!(
            d.asked,
            [
                "Untitled Project",
                "Untitled Project",
                "Untitled Project",
                "Untitled Project",
                "Saved.bae"
            ]
        );
    }

    #[test]
    fn failures_are_reported_and_keep_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::new(None);
        let mut d = Script::default();
        edit(&mut doc, "/Comp1");
        // A regular file where the .bae folder should go.
        let blocked = dir.path().join("Blocked.bae");
        fs::write(&blocked, "x").unwrap();
        d.save.push_back(Some(blocked));
        assert_eq!(cmd(&mut doc, &mut d, ids::SAVE_AS), FileOutcome::Failed);
        assert_eq!(d.errors.len(), 1);
        assert!(doc.is_dirty() && doc.location().is_none());
        // Quitting after a failed save keeps the app open.
        d.answers.push_back(UnsavedChoice::Save);
        d.save.push_back(Some(dir.path().join("Blocked.bae")));
        assert!(!confirm_discard(&mut doc, &mut d));
        // The non-interactive dialogs never discard changes.
        assert!(!confirm_discard(&mut doc, &mut NoDialogs));
        assert_eq!(open(&mut doc, &mut NoDialogs, None), FileOutcome::Cancelled);
        assert!(has(&doc, "/Comp1"));
    }

    #[test]
    fn project_extensions() {
        assert_eq!(with_project_extension("a".into()), PathBuf::from("a.bae"));
        assert_eq!(
            with_project_extension("a.BAE".into()),
            PathBuf::from("a.bae")
        );
        assert_eq!(
            with_project_extension("a.Baez".into()),
            PathBuf::from("a.baez")
        );
        assert_eq!(
            with_project_extension("a.baez".into()),
            PathBuf::from("a.baez")
        );
        assert_eq!(
            with_project_extension("a.v2".into()),
            PathBuf::from("a.v2.bae")
        );
    }
}
