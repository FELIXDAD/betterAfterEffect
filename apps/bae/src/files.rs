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
    /// Ask whether to replace the existing project `name` (Save As onto it).
    /// `true` = replace.
    fn confirm_replace(&mut self, name: &str) -> bool;
    /// Whether the save dialog itself already asked before returning an existing
    /// project's path (macOS app bundle: `.bae` is a package there, a file to the
    /// save panel). Then [`confirm_replace`](Self::confirm_replace) is not asked
    /// again.
    fn save_dialog_confirms_replace(&self) -> bool {
        false
    }
    /// Ask whether to drop the changes and reload the last saved version
    /// (File > Revert). `true` = revert.
    fn confirm_revert(&mut self) -> bool {
        false
    }
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

    fn confirm_replace(&mut self, name: &str) -> bool {
        tracing::warn!("{name} exists and no dialog can ask to replace it; not saved");
        false
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
        ids::NEW_PROJECT
            | ids::OPEN_PROJECT
            | ids::CLOSE_PROJECT
            | ids::SAVE
            | ids::SAVE_AS
            | ids::SAVE_A_COPY
            | ids::INCREMENT_AND_SAVE
            | ids::REVERT
    )
}

/// Commands that must see every edit made before them: the file commands (a
/// save must contain the value just typed) and Quit (the unsaved-changes question
/// must know about it).
pub fn waits_for_edits(id: &str) -> bool {
    is_file_command(id) || id == ids::QUIT
}

/// Holds file commands and Quit back until the edits before them are in the
/// project.
///
/// Two orderings would otherwise lose the last edit (the timeline shows it, the
/// saved file does not):
///
/// * In one UI frame, shortcuts are read before the panels draw, so typing a
///   value, Enter and Cmd/Ctrl+S in quick succession yields `[file.save,
///   attr.set_at]`. The gate runs the batch's other commands first. (macOS menu
///   clicks and key equivalents are queued behind the last frame's edits too.)
/// * Cmd/Ctrl+S while a value field is still being edited: the field commits
///   only when it loses focus (Enter or a click elsewhere, AE's rule). The shell
///   then ends the edit ([`Admitted::stop_editing`]), draws one more frame — in
///   which the field commits — and only then runs the held commands.
#[derive(Debug, Default)]
pub struct CommitGate {
    held: Vec<Invocation>,
    /// UI pass counter value when the shell was asked to end the edit; the held
    /// commands run after a later pass.
    after_pass: Option<u64>,
}

/// What [`CommitGate::admit`] lets through.
#[derive(Debug, Default, PartialEq)]
pub struct Admitted {
    /// Run these now, in order.
    pub run: Vec<Invocation>,
    /// End the text edit in progress and draw another frame.
    pub stop_editing: bool,
}

impl CommitGate {
    /// Take a batch of invocations collected since the last call. `editing`: a
    /// widget has keyboard focus (a value is being typed); `pass`: how many UI
    /// frames have run so far.
    pub fn admit(&mut self, batch: Vec<Invocation>, editing: bool, pass: u64) -> Admitted {
        let (held, mut run): (Vec<_>, Vec<_>) =
            batch.into_iter().partition(|inv| waits_for_edits(&inv.id));
        self.held.extend(held);
        let mut stop_editing = false;
        if !self.held.is_empty() {
            match self.after_pass {
                // The frame that commits the edit has not run yet.
                Some(asked) if pass <= asked => {}
                None if editing => {
                    self.after_pass = Some(pass);
                    stop_editing = true;
                }
                _ => {
                    self.after_pass = None;
                    run.append(&mut self.held);
                }
            }
        }
        Admitted { run, stop_editing }
    }

    /// Commands are held for a frame.
    pub fn is_waiting(&self) -> bool {
        !self.held.is_empty()
    }

    /// Let the held commands go without waiting for the frame (the window did
    /// not redraw in time, e.g. while minimized).
    pub fn release(&mut self) -> Vec<Invocation> {
        self.after_pass = None;
        std::mem::take(&mut self.held)
    }
}

/// Run a file command (see [`is_file_command`]); `None` for other ids.
/// `file.open_project` opens `{"path": …}` without a dialog when given.
pub fn run(
    doc: &mut Document,
    dialogs: &mut dyn FileDialogs,
    inv: &Invocation,
    new_template: Option<&Path>,
) -> Option<FileOutcome> {
    Some(match inv.id.as_str() {
        ids::NEW_PROJECT | ids::CLOSE_PROJECT => {
            if inv.id == ids::NEW_PROJECT
                && let Some(path) = new_template
            {
                return Some(replace_from_disk(
                    doc,
                    dialogs,
                    path,
                    Document::load_template,
                ));
            }
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
        ids::SAVE_A_COPY => save_a_copy(doc, dialogs),
        ids::INCREMENT_AND_SAVE => increment_and_save(doc, dialogs),
        ids::REVERT => revert(doc, dialogs),
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
    replace_from_disk(doc, dialogs, &path, Document::load)
}

pub fn template_path(prefs: &bae_prefs::Preferences) -> Option<&Path> {
    let path = prefs.str("project.project_template");
    (prefs.bool("project.new_project_loads_template") && !path.is_empty()).then(|| Path::new(path))
}

fn replace_from_disk(
    doc: &mut Document,
    dialogs: &mut dyn FileDialogs,
    path: &Path,
    load: fn(&Path) -> anyhow::Result<bae_package::OpenedProject>,
) -> FileOutcome {
    let opened = match load(path) {
        Ok(opened) => opened,
        Err(error) => {
            dialogs.error(&format!(
                "{}\n\n{}",
                tr("Could not open the project."),
                crate::file_error::describe_open(path, &error)
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
            let location = doc.location().map(Path::to_owned).unwrap_or_default();
            report_save_error(dialogs, &location, &error);
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
    let Some(picked) = dialogs.pick_save_path(&name, dir) else {
        return FileOutcome::Cancelled;
    };
    let picked = with_project_extension(picked);
    // Issue #126: a path inside an existing `.bae` project folder (Windows save
    // dialogs walk into such folders) means that project — never a project
    // nested in another.
    let path = save_target(&picked);
    let is_open_project = location
        .as_deref()
        .is_some_and(|l| crate::document::same_path(l, &path));
    let result = if is_bae_project_dir(&path) && !is_open_project {
        let native_asked = path == picked && dialogs.save_dialog_confirms_replace();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !native_asked && !dialogs.confirm_replace(&name) {
            return FileOutcome::Cancelled;
        }
        doc.save_as_replacing(&path)
    } else if is_foreign_folder(&path) {
        dialogs.error(&format!(
            "{}\n\n{}",
            tr("Could not save the project."),
            tr("“{file}” is a folder but not a BAE project. Choose another name.")
                .replace("{file}", &path.display().to_string())
        ));
        return FileOutcome::Failed;
    } else {
        doc.save_as(&path)
    };
    match result {
        Ok(()) => {
            tracing::info!("saved {}", path.display());
            FileOutcome::Saved
        }
        Err(error) => {
            report_save_error(dialogs, &path, &error);
            FileOutcome::Failed
        }
    }
}

/// Save a Copy: ask for a location and write the project there; the open
/// project keeps its location and its unsaved state (AE).
pub fn save_a_copy(doc: &Document, dialogs: &mut dyn FileDialogs) -> FileOutcome {
    let location = doc.location().map(Path::to_owned);
    let stem = location
        .as_deref()
        .and_then(Path::file_stem)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| tr("Untitled Project").to_owned());
    let name = format!("{stem} {}.bae", tr("copy"));
    let dir = location.as_deref().and_then(Path::parent);
    let Some(picked) = dialogs.pick_save_path(&name, dir) else {
        return FileOutcome::Cancelled;
    };
    let path = save_target(&with_project_extension(picked));
    if location
        .as_deref()
        .is_some_and(|l| crate::document::same_path(l, &path))
        || is_bae_project_dir(&path)
        || is_foreign_folder(&path)
    {
        // A copy never replaces a project (the open one or another).
        dialogs.error(&format!(
            "{}\n\n{}",
            tr("Could not save the project."),
            tr("“{file}” already exists. Choose another name.")
                .replace("{file}", &path.display().to_string())
        ));
        return FileOutcome::Failed;
    }
    match doc.save_a_copy(&path) {
        Ok(()) => {
            tracing::info!("saved a copy at {}", path.display());
            FileOutcome::Saved
        }
        Err(error) => {
            report_save_error(dialogs, &path, &error);
            FileOutcome::Failed
        }
    }
}

/// Increment and Save (AE): save next to the project under its name with the
/// next number (“Project 2.bae”, then “Project 3.bae”) and go on working in
/// that file. An untitled project asks where first (Save As).
pub fn increment_and_save(doc: &mut Document, dialogs: &mut dyn FileDialogs) -> FileOutcome {
    let Some(location) = doc.location().map(Path::to_owned) else {
        return save_as(doc, dialogs);
    };
    let path = incremented(&location);
    match doc.save_as(&path) {
        Ok(()) => {
            tracing::info!("saved {}", path.display());
            FileOutcome::Saved
        }
        Err(error) => {
            report_save_error(dialogs, &path, &error);
            FileOutcome::Failed
        }
    }
}

/// The first free name after `path` for Increment and Save: a trailing number
/// goes up by one (keeping its zero padding), otherwise “ 2” is appended.
pub fn incremented(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    let base = stem.trim_end_matches(|c: char| c.is_ascii_digit());
    let digits = &stem[base.len()..];
    let (base, width, mut n) = match digits.parse::<u64>() {
        Ok(n) => (base.to_owned(), digits.len(), n + 1),
        Err(_) => (format!("{stem} "), 1, 2),
    };
    loop {
        let name = match &ext {
            Some(ext) => format!("{base}{n:0width$}.{ext}"),
            None => format!("{base}{n:0width$}"),
        };
        let candidate = path.with_file_name(name);
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Revert (AE): after asking, reload the saved project and drop the changes
/// made since. Nothing to do for an untitled or unchanged project.
pub fn revert(doc: &mut Document, dialogs: &mut dyn FileDialogs) -> FileOutcome {
    let Some(path) = doc.location().map(Path::to_owned) else {
        return FileOutcome::Cancelled;
    };
    if !doc.is_dirty() || !dialogs.confirm_revert() {
        return FileOutcome::Cancelled;
    }
    match Document::load(&path) {
        Ok(opened) => {
            doc.replace(opened);
            tracing::info!("reverted {}", path.display());
            FileOutcome::Replaced
        }
        Err(error) => {
            dialogs.error(&format!(
                "{}\n\n{}",
                tr("Could not open the project."),
                crate::file_error::describe_open(&path, &error)
            ));
            FileOutcome::Failed
        }
    }
}

/// Where Save As writes `picked`: the outermost `.bae` project folder it lies
/// in, or `picked` itself.
pub fn save_target(picked: &Path) -> PathBuf {
    let mut chain: Vec<&Path> = picked.ancestors().collect();
    chain.reverse(); // root first
    chain
        .into_iter()
        .find(|p| is_bae_project_dir(p))
        .unwrap_or(picked)
        .to_owned()
}

/// A `.bae` folder holding a project.
pub fn is_bae_project_dir(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bae"))
        && path.join("project.json").is_file()
}

/// A non-empty folder named like a project that is not one (writing there
/// would mix the project into someone else's files).
fn is_foreign_folder(path: &Path) -> bool {
    path.is_dir()
        && !is_bae_project_dir(path)
        && std::fs::read_dir(path).is_ok_and(|mut d| d.next().is_some())
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

fn report_save_error(dialogs: &mut dyn FileDialogs, path: &Path, error: &anyhow::Error) {
    dialogs.error(&format!(
        "{}\n\n{}",
        tr("Could not save the project."),
        crate::file_error::describe_save(path, error)
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
        /// Answers to "replace the existing project?" and who was asked.
        replace: VecDeque<bool>,
        asked_replace: Vec<String>,
        /// Answers to "revert to the last saved version?".
        revert: VecDeque<bool>,
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
        fn confirm_replace(&mut self, name: &str) -> bool {
            self.asked_replace.push(name.to_owned());
            self.replace
                .pop_front()
                .expect("unexpected replace question")
        }
        fn confirm_revert(&mut self) -> bool {
            self.revert.pop_front().expect("unexpected revert question")
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
        run(doc, d, &Invocation::new(id), None).expect("file command")
    }

    /// Increment and Save goes on in “Film 2.bae”, then “Film 3.bae”; Revert
    /// asks, then drops the changes since the last save (AE).
    #[test]
    fn increment_and_save_and_revert() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::new(None);
        let mut d = Script::default();
        edit(&mut doc, "/Comp1");
        d.save.push_back(Some(dir.path().join("Film")));
        assert_eq!(cmd(&mut doc, &mut d, ids::SAVE), FileOutcome::Saved);
        assert_eq!(
            cmd(&mut doc, &mut d, ids::INCREMENT_AND_SAVE),
            FileOutcome::Saved
        );
        assert_eq!(
            doc.location(),
            Some(dir.path().join("Film 2.bae").as_path())
        );
        assert_eq!(
            cmd(&mut doc, &mut d, ids::INCREMENT_AND_SAVE),
            FileOutcome::Saved
        );
        assert_eq!(
            doc.location(),
            Some(dir.path().join("Film 3.bae").as_path())
        );
        assert!(is_bae_project_dir(&dir.path().join("Film.bae")));
        assert_eq!(
            incremented(Path::new("/x/Shot_v09.baez")),
            Path::new("/x/Shot_v10.baez")
        );
        // Revert: nothing to do without changes; Cancel keeps them.
        assert_eq!(cmd(&mut doc, &mut d, ids::REVERT), FileOutcome::Cancelled);
        edit(&mut doc, "/Comp2");
        d.revert.push_back(false);
        assert_eq!(cmd(&mut doc, &mut d, ids::REVERT), FileOutcome::Cancelled);
        assert!(has(&doc, "/Comp2"));
        d.revert.push_back(true);
        assert_eq!(cmd(&mut doc, &mut d, ids::REVERT), FileOutcome::Replaced);
        assert!(has(&doc, "/Comp1") && !has(&doc, "/Comp2") && !doc.is_dirty());
        assert_eq!(
            doc.location(),
            Some(dir.path().join("Film 3.bae").as_path())
        );
    }

    /// Save a Copy writes the project elsewhere; the open one keeps its
    /// location and stays unsaved (AE).
    #[test]
    fn save_a_copy_keeps_the_open_project() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::new(None);
        let mut d = Script::default();
        edit(&mut doc, "/Comp1");
        d.save.push_back(Some(dir.path().join("Film")));
        assert_eq!(cmd(&mut doc, &mut d, ids::SAVE), FileOutcome::Saved);
        edit(&mut doc, "/Comp2");
        d.save.push_back(Some(dir.path().join("Backup")));
        assert_eq!(cmd(&mut doc, &mut d, ids::SAVE_A_COPY), FileOutcome::Saved);
        assert_eq!(
            d.suggested.last().map(String::as_str),
            Some("Film copy.bae")
        );
        assert_eq!(doc.location(), Some(dir.path().join("Film.bae").as_path()));
        assert!(doc.is_dirty(), "the copy does not save the open project");
        let mut copy = Document::new(None);
        d.open.push_back(Some(dir.path().join("Backup.bae")));
        assert_eq!(
            cmd(&mut copy, &mut d, ids::OPEN_PROJECT),
            FileOutcome::Replaced
        );
        assert!(has(&copy, "/Comp1") && has(&copy, "/Comp2"));
        // A copy never replaces a project.
        d.save.push_back(Some(dir.path().join("Film.bae")));
        assert_eq!(cmd(&mut doc, &mut d, ids::SAVE_A_COPY), FileOutcome::Failed);
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
        assert_eq!(
            run(&mut other, &mut d, &inv, None),
            Some(FileOutcome::Replaced)
        );
        assert!(has(&other, "/Comp2"));
        assert!(d.errors.is_empty(), "{:?}", d.errors);
        assert_eq!(
            run(&mut other, &mut d, &Invocation::new(ids::UNDO), None),
            None
        );
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

    /// Issue #126: paths inside an existing `.bae` project mean that project.
    #[test]
    fn save_targets_never_nest_projects() {
        let dir = tempfile::tempdir().unwrap();
        let outer = dir.path().join("outer.bae");
        fs::create_dir_all(outer.join("sublayers")).unwrap();
        fs::write(outer.join("project.json"), "{}").unwrap();
        // The project folder itself, a project picked inside it, any path inside.
        assert_eq!(save_target(&outer), outer);
        assert_eq!(save_target(&outer.join("outer.bae")), outer);
        assert_eq!(save_target(&outer.join("sublayers/x.bae")), outer);
        // Elsewhere: unchanged. A `.bae` folder without project.json is not a project.
        let plain = dir.path().join("new.bae");
        assert_eq!(save_target(&plain), plain);
        let fake = dir.path().join("fake.bae");
        fs::create_dir(&fake).unwrap();
        assert_eq!(save_target(&fake.join("a.bae")), fake.join("a.bae"));
        assert!(is_bae_project_dir(&outer) && !is_bae_project_dir(&fake));
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
