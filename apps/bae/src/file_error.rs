//! Messages for projects that cannot be opened or saved: which file, and what
//! is wrong in words a user understands ("does not exist", "no permission",
//! "is damaged", "was saved by a newer version"), in the UI language — never the
//! system's English text. The technical cause goes to the log only, at debug
//! level (`--log debug` / `BAE_LOG=debug`).
//!
//! Opening: File ▸ Open, `bae <project>`, `bae --headless <project>`,
//! `bae export-json` ([`describe_open`]). Saving: Save / Save As
//! ([`describe_save`]).

use std::io::ErrorKind;
use std::path::Path;

use bae_model::{ModelError, ProjectError};
use bae_package::PackageError;

use crate::i18n::tr;

/// What is wrong with a project that did not open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Nothing at that path.
    Missing,
    /// A folder without `project.json`.
    FolderWithoutManifest,
    /// Some other kind of file.
    NotAProject,
    /// A `.baez` that is not a readable package (empty, truncated, not a zip).
    DamagedPackage,
    /// `project.json` is not valid.
    DamagedManifest,
    /// Saved by a newer BAE.
    NewerFormat { found: u32, supported: u32 },
    /// A sublayer listed in `project.json` has no data.
    Incomplete(String),
    /// Sublayer data that cannot be decoded.
    DamagedLayers,
    /// The user may not read it.
    NoPermission,
    /// Another file system error (the cause is in the details).
    CannotRead,
    /// Anything else.
    Other,
}

/// Classify why `path` did not open.
pub fn classify(path: &Path, error: &anyhow::Error) -> Problem {
    if !path.exists() {
        return Problem::Missing;
    }
    // A folder that cannot be listed would otherwise look like one without
    // project.json.
    let readable = if path.is_dir() {
        std::fs::read_dir(path).map(drop)
    } else {
        std::fs::File::open(path).map(drop)
    };
    if readable.is_err_and(|e| e.kind() == ErrorKind::PermissionDenied) {
        return Problem::NoPermission;
    }
    for cause in error.chain() {
        let project = cause.downcast_ref::<ProjectError>().or_else(|| {
            match cause.downcast_ref::<PackageError>() {
                Some(PackageError::Project(p)) => Some(p),
                _ => None,
            }
        });
        if let Some(p) = project {
            return from_project_error(path, p);
        }
        match cause.downcast_ref::<PackageError>() {
            Some(PackageError::Zip(_)) => return Problem::DamagedPackage,
            Some(PackageError::Io { source, .. }) => return from_io(source),
            Some(PackageError::Model(_)) => return Problem::DamagedLayers,
            _ => {}
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            return from_io(io);
        }
    }
    Problem::Other
}

fn from_project_error(path: &Path, error: &ProjectError) -> Problem {
    match error {
        ProjectError::NotAProject(_) if path.is_dir() => Problem::FolderWithoutManifest,
        ProjectError::NotAProject(_) if crate::document::is_baez(path) => Problem::DamagedPackage,
        ProjectError::NotAProject(_) => Problem::NotAProject,
        ProjectError::Zip(_) => Problem::DamagedPackage,
        ProjectError::Manifest(_) | ProjectError::Migration { .. } => Problem::DamagedManifest,
        ProjectError::NewerFormat { found, supported } => Problem::NewerFormat {
            found: *found,
            supported: *supported,
        },
        ProjectError::MissingSublayer(id) => Problem::Incomplete(id.to_string()),
        ProjectError::Io { source, .. } => from_io(source),
        ProjectError::Model(ModelError::Storage(_)) | ProjectError::Model(_) => {
            Problem::DamagedLayers
        }
        ProjectError::NoLocation => Problem::Other,
    }
}

fn from_io(error: &std::io::Error) -> Problem {
    match error.kind() {
        ErrorKind::NotFound => Problem::Missing,
        ErrorKind::PermissionDenied => Problem::NoPermission,
        _ => Problem::CannotRead,
    }
}

/// The sentence for `problem` about `path`, in the UI language.
pub fn sentence(path: &Path, problem: &Problem) -> String {
    let file = path.display().to_string();
    let text = match problem {
        Problem::Missing => tr("“{file}” does not exist.").to_owned(),
        Problem::FolderWithoutManifest => {
            tr("“{file}” is a folder but not a BAE project: it has no project.json.").to_owned()
        }
        Problem::NotAProject => {
            tr("“{file}” is not a BAE project. Open a .bae project folder or a .baez file.")
                .to_owned()
        }
        Problem::DamagedPackage => {
            tr("“{file}” is damaged or is not a .baez project package.").to_owned()
        }
        Problem::DamagedManifest => {
            tr("The project description (project.json) in “{file}” is damaged.").to_owned()
        }
        Problem::NewerFormat { found, supported } => tr(
            "“{file}” was saved by a newer version of BAE (format {found}; this version reads up to {supported}). Update BAE to open it.",
        )
        .replace("{found}", &found.to_string())
        .replace("{supported}", &supported.to_string()),
        Problem::Incomplete(id) => {
            tr("“{file}” is incomplete: the data of sublayer “{id}” is missing.").replace("{id}", id)
        }
        Problem::DamagedLayers => tr("The sublayer data in “{file}” is damaged.").to_owned(),
        Problem::NoPermission => tr("You don't have permission to open “{file}”.").to_owned(),
        Problem::CannotRead => tr("“{file}” cannot be read.").to_owned(),
        Problem::Other => tr("“{file}” could not be opened.").to_owned(),
    };
    text.replace("{file}", &file)
}

/// The full message: the sentence, then the technical cause (left out for a
/// missing file, where it would only say the same less clearly).
pub fn describe_open(path: &Path, error: &anyhow::Error) -> String {
    let problem = classify(path, error);
    tracing::debug!("cannot open {}: {error:#}", path.display());
    sentence(path, &problem)
}

/// Why a save failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveProblem {
    /// No write permission in the folder (or the file is read-only).
    NoPermission,
    /// The folder to save in is gone.
    FolderMissing,
    /// The disk is full.
    DiskFull,
    /// Anything else (the cause is in the details).
    Other,
}

/// Classify why saving to `path` failed.
pub fn classify_save(error: &anyhow::Error) -> SaveProblem {
    classify_save_chain(error.chain())
}

/// [`classify_save`] for an error and its `source()` chain.
fn classify_save_chain<'a>(
    causes: impl Iterator<Item = &'a (dyn std::error::Error + 'static)>,
) -> SaveProblem {
    let io = causes.into_iter().find_map(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .or_else(|| match cause.downcast_ref::<ProjectError>() {
                Some(ProjectError::Io { source, .. }) => Some(source),
                _ => None,
            })
            .or_else(|| match cause.downcast_ref::<PackageError>() {
                Some(PackageError::Io { source, .. }) => Some(source),
                Some(PackageError::Project(ProjectError::Io { source, .. })) => Some(source),
                _ => None,
            })
    });
    match io.map(std::io::Error::kind) {
        Some(ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem) => {
            SaveProblem::NoPermission
        }
        Some(ErrorKind::NotFound) => SaveProblem::FolderMissing,
        Some(ErrorKind::StorageFull | ErrorKind::QuotaExceeded) => SaveProblem::DiskFull,
        _ => SaveProblem::Other,
    }
}

/// The message for a failed save to `path` (the cause goes to the log).
pub fn describe_save(path: &Path, error: &anyhow::Error) -> String {
    tracing::debug!("cannot save {}: {error:#}", path.display());
    save_sentence(path, classify_save(error))
}

/// [`describe_save`] for any error type (the MCP server's autosave).
pub fn describe_save_error(path: &Path, error: &(dyn std::error::Error + 'static)) -> String {
    tracing::debug!("cannot save {}: {error}", path.display());
    let chain = std::iter::successors(Some(error), |e| e.source());
    save_sentence(path, classify_save_chain(chain))
}

fn save_sentence(path: &Path, problem: SaveProblem) -> String {
    let file = path.display().to_string();
    let folder = path
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| file.clone());
    match problem {
        // An existing project that became read-only, or a folder that is.
        SaveProblem::NoPermission if path.exists() => {
            tr("You don't have permission to change “{file}”.").replace("{file}", &file)
        }
        SaveProblem::NoPermission => {
            tr("You don't have permission to save in “{folder}”.").replace("{folder}", &folder)
        }
        SaveProblem::FolderMissing => {
            tr("The folder “{folder}” does not exist.").replace("{folder}", &folder)
        }
        SaveProblem::DiskFull => {
            tr("There is not enough space to save “{file}”.").replace("{file}", &file)
        }
        SaveProblem::Other => tr("“{file}” could not be saved.").replace("{file}", &file),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::document::Document;

    fn problem_of(path: &Path) -> Problem {
        let error = Document::load(path).expect_err("should not open");
        classify(path, &error)
    }

    #[test]
    fn broken_projects_are_named_and_explained() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        assert_eq!(problem_of(&d.join("不存在.bae")), Problem::Missing);
        fs::create_dir(d.join("空目录.bae")).unwrap();
        assert_eq!(
            problem_of(&d.join("空目录.bae")),
            Problem::FolderWithoutManifest
        );
        fs::write(d.join("notes.txt"), "hi").unwrap();
        assert_eq!(problem_of(&d.join("notes.txt")), Problem::NotAProject);
        for (name, bytes) in [
            ("empty.baez", &b""[..]),
            ("garbage.baez", &b"PK\x03\x04 not really a zip"[..]),
            ("random.baez", &[7u8; 300][..]),
        ] {
            fs::write(d.join(name), bytes).unwrap();
            assert_eq!(problem_of(&d.join(name)), Problem::DamagedPackage, "{name}");
        }
        for (name, json) in [
            ("bad-json.bae", &b"{\"format\": \"bae.project\", \"vers"[..]),
            ("wrong-schema.bae", &b"{\"hello\": [1, 2, 3]}"[..]),
            ("binary.bae", &[0xffu8, 0xfe, 0x00, 0x01][..]),
        ] {
            fs::create_dir(d.join(name)).unwrap();
            fs::write(d.join(name).join("project.json"), json).unwrap();
            assert_eq!(
                problem_of(&d.join(name)),
                Problem::DamagedManifest,
                "{name}"
            );
        }
        let newer = d.join("newer.bae");
        fs::create_dir(&newer).unwrap();
        fs::write(
            newer.join("project.json"),
            br#"{"format": "bae.project", "version": 99, "layer_stack": ["root"]}"#,
        )
        .unwrap();
        assert!(matches!(
            problem_of(&newer),
            Problem::NewerFormat { found: 99, .. }
        ));
        let incomplete = d.join("incomplete.bae");
        fs::create_dir(&incomplete).unwrap();
        fs::write(
            incomplete.join("project.json"),
            br#"{"format": "bae.project", "version": 1, "layer_stack": ["root"]}"#,
        )
        .unwrap();
        assert_eq!(problem_of(&incomplete), Problem::Incomplete("root".into()));
        let layers = d.join("layers.bae");
        fs::create_dir_all(layers.join("sublayers")).unwrap();
        fs::write(
            layers.join("project.json"),
            br#"{"format": "bae.project", "version": 1, "layer_stack": ["root"]}"#,
        )
        .unwrap();
        fs::write(layers.join("sublayers").join("root.loro"), b"not loro").unwrap();
        assert_eq!(problem_of(&layers), Problem::DamagedLayers);
    }

    #[test]
    fn messages_name_the_file_without_the_raw_cause() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("坏的 文件.baez");
        fs::write(&path, b"PK\x03\x04 not really a zip").unwrap();
        let error = Document::load(&path).err().unwrap();
        let message = describe_open(&path, &error);
        assert!(!message.contains('\n'), "one sentence: {message}");
        assert!(message.contains(&path.display().to_string()), "{message}");
        assert!(message.contains("damaged"), "{message}");
        for raw in ["EOCD", "zip:", "Details", "is not a BAE project"] {
            assert!(!message.contains(raw), "raw cause {raw:?} shown: {message}");
        }
        // Any error type (the MCP autosave): our wording, no "os error".
        let io = std::io::Error::from(ErrorKind::PermissionDenied);
        let m = describe_save_error(Path::new("/x/a.bae"), &io);
        assert_eq!(m, "You don't have permission to save in “/x”.");
    }

    #[test]
    fn unreadable_projects_say_permission() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("locked.baez");
        fs::write(&file, b"PK").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
        let folder = dir.path().join("locked.bae");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("project.json"), b"{}").unwrap();
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o000)).unwrap();
        let file_problem = problem_of(&file);
        let folder_problem = problem_of(&folder);
        let message = describe_open(&file, &Document::load(&file).unwrap_err());
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        if nix_root() {
            return; // root reads anything
        }
        assert_eq!(file_problem, Problem::NoPermission);
        assert_eq!(folder_problem, Problem::NoPermission);
        let first = message.lines().next().unwrap();
        assert!(
            first.starts_with("You don't have permission to open"),
            "{message}"
        );
        assert!(!first.contains("os error"), "{message}");
    }

    #[cfg(unix)]
    fn nix_root() -> bool {
        std::env::var("USER").is_ok_and(|u| u == "root")
    }

    #[test]
    fn save_failures_are_explained() {
        let io = |kind: ErrorKind| {
            anyhow::Error::new(ProjectError::Io {
                path: "/x/a.bae".into(),
                source: std::io::Error::from(kind),
            })
            .context("/x/a.bae")
        };
        assert_eq!(
            classify_save(&io(ErrorKind::PermissionDenied)),
            SaveProblem::NoPermission
        );
        assert_eq!(
            classify_save(&io(ErrorKind::ReadOnlyFilesystem)),
            SaveProblem::NoPermission
        );
        assert_eq!(
            classify_save(&io(ErrorKind::NotFound)),
            SaveProblem::FolderMissing
        );
        assert_eq!(
            classify_save(&io(ErrorKind::StorageFull)),
            SaveProblem::DiskFull
        );
        assert_eq!(classify_save(&io(ErrorKind::Other)), SaveProblem::Other);
        let message = describe_save(Path::new("/x/a.bae"), &io(ErrorKind::PermissionDenied));
        let first = message.lines().next().unwrap();
        assert_eq!(first, "You don't have permission to save in “/x”.");
    }

    #[test]
    fn every_sentence_has_chinese() {
        crate::i18n::register_tables();
        let keys = [
            "“{file}” does not exist.",
            "“{file}” is a folder but not a BAE project: it has no project.json.",
            "“{file}” is not a BAE project. Open a .bae project folder or a .baez file.",
            "“{file}” is damaged or is not a .baez project package.",
            "The project description (project.json) in “{file}” is damaged.",
            "“{file}” was saved by a newer version of BAE (format {found}; this version reads up to {supported}). Update BAE to open it.",
            "“{file}” is incomplete: the data of sublayer “{id}” is missing.",
            "The sublayer data in “{file}” is damaged.",
            "You don't have permission to open “{file}”.",
            "“{file}” cannot be read.",
            "You don't have permission to save in “{folder}”.",
            "You don't have permission to change “{file}”.",
            "The folder “{folder}” does not exist.",
            "There is not enough space to save “{file}”.",
            "“{file}” could not be saved.",
            "“{file}” could not be opened.",
        ];
        let missing: Vec<_> = keys
            .iter()
            .filter(|k| !bae_ui::i18n::has_translation(k))
            .collect();
        assert!(missing.is_empty(), "no Chinese for {missing:?}");
    }
}
