//! The open project and the command path into it.
//!
//! Everything that changes project data arrives here as a command id: undo/redo
//! go to `Stage::undo/redo` for the local author, every other id is run through
//! the `bae-model` command registry as one undo step (ARCHITECTURE §2 rule 1).
//!
//! The document also owns the project's place on disk: new, open (`.bae` folder or
//! `.baez` file, read through `bae-package` so embedded assets are found in place),
//! save and save as. Saving only serializes the stage; it never changes project
//! data. "Unsaved changes" is not a flag kept next to the model: it compares the
//! version of every sublayer (Loro's version vector, advanced by every committed
//! change, undo, redo and remote import) and the layer stack with the ones recorded
//! when the project was last opened or saved.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context as _;
use bae_model::loro::VersionVector;
use bae_model::{AuthorId, BaezAsset, CommandError, CommandRegistry, Project, Stage, SublayerId};
use bae_package::{AssetLocator, OpenedProject};
use bae_ui::Invocation;
use bae_ui::action::ids;
use bae_ui::history::HistoryLog;

use crate::i18n::tr;

/// File name inside a `.bae` folder that identifies it (open dialogs that cannot
/// pick folders let the user choose this file instead).
const MANIFEST: &str = "project.json";
/// Extension of the single-file package.
const BAEZ: &str = bae_model::project::BAEZ_EXTENSION;

/// The open project (an untitled one until a file is opened or saved).
pub struct Document {
    project: Project,
    /// Where the project's asset files are: its base directory and, for a `.baez`,
    /// the embedded files (`bae-package`).
    assets: AssetLocator,
    commands: CommandRegistry,
    author: AuthorId,
    /// Sublayer versions when the project was last opened or saved.
    saved: SavePoint,
    history: Option<Arc<HistoryLog>>,
}

/// What [`Document::run`] did with an invocation.
#[derive(Debug)]
pub enum Outcome {
    /// The project changed.
    Changed,
    /// Recognized but nothing to do (empty undo stack, …).
    Unchanged,
    /// No such command.
    Unknown,
    /// The command failed.
    Failed(CommandError),
}

/// The layer stack with every sublayer's version vector, strongest first.
#[derive(Debug, PartialEq)]
struct SavePoint(Vec<(SublayerId, VersionVector)>);

impl SavePoint {
    fn of(stage: &Stage) -> Self {
        SavePoint(
            stage
                .layer_stack()
                .iter()
                .filter_map(|id| stage.sublayer(id).map(|l| (id.clone(), l.version())))
                .collect(),
        )
    }
}

impl Document {
    /// A new untitled project; changes are attributed to the local user. Every
    /// stage event is recorded in `history` (the History panel's data).
    pub fn new(history: Option<Arc<HistoryLog>>) -> Self {
        let project = Project::new();
        let mut doc = Document {
            saved: SavePoint::of(&project.stage),
            project,
            assets: AssetLocator::default(),
            commands: {
                // Model commands plus the agent proposal commands (accept / reject).
                let mut commands = CommandRegistry::with_builtins();
                bae_agent::register_commands(&mut commands);
                commands
            },
            author: local_author(),
            history,
        };
        doc.watch_history();
        doc
    }

    /// Add the commands app modules published in `Registry<dyn DynCommand>` (domain
    /// command crates such as `bae-motion`). Built-in ids are kept.
    pub fn add_module_commands(&mut self, app: &bae_app::App) {
        let Some(registry) = app.get_registry::<dyn bae_model::DynCommand>() else {
            return;
        };
        for entry in registry.snapshot().iter() {
            if self.commands.get(entry.id()).is_none() {
                let _ = self.commands.register_dyn(entry.item().clone());
            }
        }
    }

    /// `(id, English title)` of every model command (for the shortcut editor).
    pub fn command_titles(&self) -> Vec<(String, String)> {
        self.commands
            .infos()
            .map(|i| (i.id.clone(), i.title.clone()))
            .collect()
    }

    pub fn stage(&self) -> &Stage {
        &self.project.stage
    }

    pub fn author(&self) -> &AuthorId {
        &self.author
    }

    /// The `.bae` folder or `.baez` file of the project (`None` until saved).
    pub fn location(&self) -> Option<&Path> {
        self.project.location()
    }

    /// A top-level field of `project.json` that is not project data (e.g. the
    /// saved view, `view_state`).
    pub fn manifest_field(&self, key: &str) -> Option<&serde_json::Value> {
        self.project.manifest.extra.get(key)
    }

    /// Set (`None`: remove) such a field; it is written with the next save and
    /// does not count as an unsaved change.
    pub fn set_manifest_field(&mut self, key: &str, value: Option<serde_json::Value>) {
        match value {
            Some(v) => {
                self.project.manifest.extra.insert(key.to_owned(), v);
            }
            None => {
                self.project.manifest.extra.remove(key);
            }
        }
    }

    /// Whether the project changed since it was opened or last saved.
    pub fn is_dirty(&self) -> bool {
        SavePoint::of(&self.project.stage) != self.saved
    }

    /// Project name: the file name, or "Untitled Project".
    pub fn display_name(&self) -> String {
        self.location()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| tr("Untitled Project").to_owned())
    }

    /// Window title: project name, with AE's `*` after it while there are unsaved
    /// changes ("BAE - MyFilm.bae *").
    pub fn title(&self) -> String {
        let mark = if self.is_dirty() { " *" } else { "" };
        format!("{} - {}{mark}", bae_platform::APP_NAME, self.display_name())
    }

    /// Labels for the Edit menu: `(can_undo, undo label, can_redo, redo label)`.
    pub fn undo_state(&self) -> (Option<String>, Option<String>) {
        let stage = &self.project.stage;
        let undo = stage.can_undo(&self.author).then(|| {
            stage
                .undo_label(&self.author)
                .unwrap_or_default()
                .to_owned()
        });
        let redo = stage.can_redo(&self.author).then(|| {
            stage
                .redo_label(&self.author)
                .unwrap_or_default()
                .to_owned()
        });
        (undo, redo)
    }

    /// Run a command id: undo/redo or a registered model command.
    pub fn run(&mut self, inv: &Invocation) -> Outcome {
        let stage = &mut self.project.stage;
        let label = |text: Option<&str>| {
            if let (Some(log), Some(text)) = (&self.history, text) {
                log.label_next(text);
            }
        };
        let changed = match inv.id.as_str() {
            ids::UNDO => {
                label(stage.undo_label(&self.author));
                stage.undo(&self.author).is_some()
            }
            ids::REDO => {
                label(stage.redo_label(&self.author));
                stage.redo(&self.author).is_some()
            }
            id => {
                let Some(cmd) = self.commands.get(id) else {
                    return Outcome::Unknown;
                };
                label(Some(&cmd.info().title));
                let params = if inv.params.is_null() {
                    serde_json::json!({})
                } else {
                    inv.params.clone()
                };
                match self.commands.execute(stage, &self.author, id, params) {
                    Ok(out) => !out.event.is_empty(),
                    Err(error) => return Outcome::Failed(error),
                }
            }
        };
        if changed {
            Outcome::Changed
        } else {
            Outcome::Unchanged
        }
    }

    // ---- files ----

    /// Read a project without replacing the open one (so a broken file is reported
    /// before anything is asked or discarded). `path` is a `.bae` folder, a `.baez`
    /// file, or the `project.json` inside a `.bae` folder.
    pub fn load(path: &Path) -> anyhow::Result<OpenedProject> {
        let path = project_path(path);
        bae_package::open_project(&path).map_err(|error| {
            // Name the file unless the error already does.
            let shown = path.display().to_string();
            if error.to_string().contains(&shown) {
                anyhow::Error::new(error)
            } else {
                anyhow::Error::new(error).context(shown)
            }
        })
    }

    /// Replace the open project with a loaded one (unsaved changes are dropped:
    /// ask first).
    pub fn replace(&mut self, opened: OpenedProject) {
        self.project = opened.project;
        self.assets = opened.assets;
        self.saved = SavePoint::of(&self.project.stage);
        if let Some(log) = &self.history {
            log.clear();
        }
        self.watch_history();
    }

    /// Open a project in place of the current one ([`Document::load`] +
    /// [`Document::replace`]).
    pub fn open(&mut self, path: &Path) -> anyhow::Result<()> {
        let opened = Self::load(path)?;
        self.replace(opened);
        Ok(())
    }

    /// Replace the open project with a new untitled one.
    pub fn new_project(&mut self) {
        self.replace(OpenedProject {
            project: Project::new(),
            assets: AssetLocator::default(),
        });
    }

    /// Save to the current location (`Err` for an untitled project: use
    /// [`Document::save_as`]).
    pub fn save(&mut self) -> anyhow::Result<()> {
        let location = self
            .location()
            .map(Path::to_owned)
            .context("the project has not been saved yet")?;
        self.write(&location)
    }

    /// Save to `path` and make it the project's location: a `.baez` file when it
    /// has that extension, otherwise a `.bae` folder.
    pub fn save_as(&mut self, path: &Path) -> anyhow::Result<()> {
        self.write(path)
    }

    /// Save As over another existing `.bae` project folder (issue #126): the
    /// project is written to a temporary folder next to it first, then swapped
    /// in; if anything fails, the existing project is left as it was. Files
    /// embedded in a `.baez` the project came from are extracted with it.
    pub fn save_as_replacing(&mut self, target: &Path) -> anyhow::Result<()> {
        let context = || target.display().to_string();
        let tmp = sibling(target, "saving");
        let backup = sibling(target, "replaced");
        let written = (|| -> anyhow::Result<()> {
            let mut copy = self.detached_copy()?;
            copy.save_as(&tmp)?;
            if let Some(pkg) = self.assets.package() {
                for entry in pkg.entries() {
                    pkg.extract_entry(&entry.name, &tmp.join(native(&entry.name)))?;
                }
            }
            Ok(())
        })();
        if let Err(error) = written {
            let _ = fs::remove_dir_all(&tmp);
            return Err(error.context(context()));
        }
        if let Err(error) = fs::rename(target, &backup) {
            let _ = fs::remove_dir_all(&tmp);
            return Err(anyhow::Error::new(error).context(context()));
        }
        if let Err(error) = fs::rename(&tmp, target) {
            let _ = fs::rename(&backup, target);
            let _ = fs::remove_dir_all(&tmp);
            return Err(anyhow::Error::new(error).context(context()));
        }
        let _ = fs::remove_dir_all(&backup);
        // The folder holds the project now; make it the location (rewrites the
        // same project files in place, atomically per file).
        self.project.save_as(target).with_context(context)?;
        self.saved = SavePoint::of(&self.project.stage);
        self.assets = AssetLocator::for_project(&self.project).with_context(context)?;
        Ok(())
    }

    /// The project as an independent copy (same sublayers and manifest, no
    /// location), for writing somewhere without touching the open one.
    fn detached_copy(&self) -> anyhow::Result<Project> {
        let stage = &self.project.stage;
        let mut layers = Vec::with_capacity(stage.layer_stack().len());
        for id in stage.layer_stack() {
            let layer = stage
                .sublayer(id)
                .with_context(|| format!("sublayer {id} is missing"))?;
            layers.push(bae_model::Sublayer::from_snapshot(
                id.clone(),
                &layer.export_snapshot()?,
            )?);
        }
        let mut copy = Project::from_stage(Stage::from_sublayers(layers)?);
        copy.manifest = self.project.manifest.clone();
        Ok(copy)
    }

    /// Serialize the project to `target`. Files embedded in the `.baez` the project
    /// came from stay reachable under the same `assets/…` names: they are copied
    /// into a `.baez` target and extracted into a `.bae` target (whose folder is
    /// the base of relative paths).
    fn write(&mut self, target: &Path) -> anyhow::Result<()> {
        let context = || target.display().to_string();
        let package = self.assets.package().cloned();
        let embedded: Vec<String> = package
            .iter()
            .flat_map(|p| p.entries().map(|e| e.name.clone()))
            .collect();
        let same_place = self.location().is_some_and(|l| same_path(l, target));
        if is_baez(target) {
            // Copy the embedded files out first: `target` may be the very package
            // they are read from.
            let mut assets = Vec::with_capacity(embedded.len());
            let staging = match &package {
                Some(pkg) if !embedded.is_empty() => {
                    let staging = Staging::next_to(target).with_context(context)?;
                    for name in &embedded {
                        let source = staging.path().join(native(name));
                        pkg.extract_entry(name, &source).with_context(context)?;
                        assets.push(BaezAsset {
                            name: name.clone(),
                            source,
                        });
                    }
                    Some(staging)
                }
                _ => None,
            };
            if !same_place {
                // Sets the location; writes the project data once more below
                // when files must go in with it.
                self.project.save_as(target).with_context(context)?;
            }
            if same_place || !assets.is_empty() {
                // One atomic write (temporary file + rename).
                self.project
                    .write_baez(target, &assets)
                    .with_context(context)?;
            }
            drop(staging);
        } else {
            self.project.save_as(target).with_context(context)?;
            if let Some(pkg) = &package {
                for name in &embedded {
                    pkg.extract_entry(name, &target.join(native(name)))
                        .with_context(context)?;
                }
            }
        }
        self.saved = SavePoint::of(&self.project.stage);
        // New base directory, or new entry offsets in a rewritten `.baez`.
        self.assets = AssetLocator::for_project(&self.project).with_context(context)?;
        Ok(())
    }

    fn watch_history(&mut self) {
        if let Some(log) = self.history.clone() {
            self.project.stage.subscribe(move |event| log.record(event));
        }
    }
}

/// The project a path names: the folder of a `project.json` picked inside a `.bae`
/// folder, otherwise the path itself.
pub fn project_path(path: &Path) -> PathBuf {
    match path.parent() {
        Some(dir) if path.file_name().is_some_and(|n| n == MANIFEST) && path.is_file() => {
            dir.to_owned()
        }
        _ => path.to_owned(),
    }
}

/// Whether `path` names a single-file package (the model's rule: exactly the
/// `.baez` extension; `files` lower-cases chosen extensions first).
pub(crate) fn is_baez(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == BAEZ)
}

/// Two paths name the same file or folder (symlinks and `..` resolved).
pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (fs::canonicalize(a), fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// A hidden sibling of `target` for temporary work (`.<name>.<what>-<pid>`).
fn sibling(target: &Path, what: &str) -> PathBuf {
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    target.with_file_name(format!(".{name}.{what}-{}", std::process::id()))
}

/// A `/`-separated entry name as a relative native path.
fn native(name: &str) -> PathBuf {
    name.split('/').collect()
}

/// A scratch folder next to a save target (same volume), removed when dropped.
struct Staging(PathBuf);

impl Staging {
    fn next_to(target: &Path) -> std::io::Result<Self> {
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = target.with_file_name(format!(".{name}.saving-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        Ok(Staging(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `user:<login>` from the environment, `user:local` if unknown.
fn local_author() -> AuthorId {
    let login = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    AuthorId::new(&format!("user:{login}"))
        .ok()
        .filter(|_| !login.is_empty())
        .unwrap_or_else(|| AuthorId::new("user:local").unwrap_or_else(|_| AuthorId::system()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create(doc: &mut Document, path: &str) {
        let inv = Invocation::with_params(
            "prim.create",
            serde_json::json!({ "path": path, "prim_type": "Comp" }),
        );
        match doc.run(&inv) {
            Outcome::Changed => {}
            other => panic!("prim.create {path}: {other:?}"),
        }
    }

    fn has(doc: &Document, path: &str) -> bool {
        doc.stage().prim(&path.parse().unwrap()).is_some()
    }

    #[test]
    fn commands_and_undo_go_through_the_model() {
        let log = Arc::new(HistoryLog::new());
        let mut doc = Document::new(Some(log.clone()));
        assert!(matches!(
            doc.run(&Invocation::new("no.such.command")),
            Outcome::Unknown
        ));
        assert!(matches!(
            doc.run(&Invocation::new(ids::UNDO)),
            Outcome::Unchanged
        ));
        create(&mut doc, "/Comp1");
        assert!(doc.is_dirty());
        let (undo, redo) = doc.undo_state();
        assert!(undo.is_some() && redo.is_none());
        assert!(matches!(
            doc.run(&Invocation::new(ids::UNDO)),
            Outcome::Changed
        ));
        let (undo, redo) = doc.undo_state();
        assert!(undo.is_none() && redo.is_some());
        let labels: Vec<_> = log.entries().into_iter().map(|e| e.label).collect();
        assert_eq!(labels, ["Create Prim", "Create Prim"]);
        assert_eq!(log.entries()[1].origin, bae_model::EventOrigin::Undo);
    }

    #[test]
    fn dirty_follows_the_model_versions() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::new(None);
        assert!(!doc.is_dirty(), "a new project has no changes");
        // Reading does not count as a change; a failed command neither.
        let _ = doc.stage().prim(&"/Nothing".parse().unwrap());
        assert!(matches!(
            doc.run(&Invocation::with_params(
                "prim.create",
                serde_json::json!({ "path": "bad path", "prim_type": "Comp" })
            )),
            Outcome::Failed(_)
        ));
        assert!(!doc.is_dirty());
        assert!(!doc.title().ends_with('*'));
        create(&mut doc, "/Comp1");
        assert!(doc.is_dirty());
        assert!(doc.title().ends_with(" *"), "{}", doc.title());
        doc.save_as(&dir.path().join("a.bae")).unwrap();
        assert!(!doc.is_dirty());
        assert_eq!(doc.display_name(), "a.bae");
        // Undo after a save is a change again (the saved file has the comp).
        doc.run(&Invocation::new(ids::UNDO));
        assert!(doc.is_dirty());
        doc.save().unwrap();
        assert!(!doc.is_dirty());
        // A layer stack change (no prim edit) is a change too.
        doc.run(&Invocation::with_params(
            "sublayer.create",
            serde_json::json!({ "id": "anim/shot010", "index": 0 }),
        ));
        assert!(doc.is_dirty());
    }

    #[test]
    fn bae_roundtrip_and_errors_keep_the_open_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Film.bae");
        let mut doc = Document::new(None);
        assert!(doc.save().is_err(), "untitled: Save needs a location");
        create(&mut doc, "/Comp1");
        doc.save_as(&path).unwrap();
        assert_eq!(doc.location(), Some(path.as_path()));

        let mut other = Document::new(None);
        other.open(&path).unwrap();
        assert!(has(&other, "/Comp1"));
        assert!(!other.is_dirty());
        // The project.json inside the folder opens the folder.
        other.open(&path.join(MANIFEST)).unwrap();
        assert_eq!(other.location(), Some(path.as_path()));

        // Opening something that is not a project leaves the open one alone.
        create(&mut other, "/Comp2");
        let junk = dir.path().join("notes.txt");
        fs::write(&junk, "hello").unwrap();
        assert!(other.open(&junk).is_err());
        assert!(other.open(&dir.path().join("missing.bae")).is_err());
        assert!(has(&other, "/Comp2") && other.is_dirty());

        other.new_project();
        assert!(other.location().is_none() && !other.is_dirty());
        assert!(!has(&other, "/Comp1"));
    }

    #[test]
    fn baez_open_save_and_save_as_keep_embedded_assets() {
        let dir = tempfile::tempdir().unwrap();
        // A package with one embedded file, written by the model.
        let media = dir.path().join("clip.mov");
        fs::write(&media, b"not really a movie").unwrap();
        let package = dir.path().join("Handoff.baez");
        let mut project = Project::new();
        CommandRegistry::with_builtins()
            .execute(
                &mut project.stage,
                &AuthorId::system(),
                "prim.create",
                serde_json::json!({ "path": "/Comp1", "prim_type": "Comp" }),
            )
            .unwrap();
        project
            .write_baez(
                &package,
                &[BaezAsset {
                    name: "assets/footage/clip.mov".into(),
                    source: media.clone(),
                }],
            )
            .unwrap();

        let mut doc = Document::new(None);
        doc.open(&package).unwrap();
        assert!(has(&doc, "/Comp1") && !doc.is_dirty());
        let entry = |doc: &Document| {
            doc.assets
                .package()
                .and_then(|p| p.read_entry("assets/footage/clip.mov").ok())
        };
        assert_eq!(entry(&doc).as_deref(), Some(&b"not really a movie"[..]));

        // Save in place: project data rewritten, embedded file kept.
        create(&mut doc, "/Comp2");
        doc.save().unwrap();
        assert!(!doc.is_dirty());
        assert_eq!(entry(&doc).as_deref(), Some(&b"not really a movie"[..]));
        let mut again = Document::new(None);
        again.open(&package).unwrap();
        assert!(has(&again, "/Comp2"));
        assert_eq!(entry(&again).as_deref(), Some(&b"not really a movie"[..]));
        // No staging folder is left behind.
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");

        // Save As another package and as a folder.
        let copy = dir.path().join("Copy.baez");
        again.save_as(&copy).unwrap();
        assert_eq!(entry(&again).as_deref(), Some(&b"not really a movie"[..]));
        let folder = dir.path().join("Work.bae");
        again.save_as(&folder).unwrap();
        assert_eq!(
            fs::read(folder.join("assets").join("footage").join("clip.mov")).unwrap(),
            b"not really a movie"
        );
        let mut reopened = Document::new(None);
        reopened.open(&folder).unwrap();
        assert!(has(&reopened, "/Comp2"));
        reopened.open(&copy).unwrap();
        assert_eq!(
            entry(&reopened).as_deref(),
            Some(&b"not really a movie"[..])
        );
    }
}
