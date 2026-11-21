//! Messages for projects that cannot be opened: which file, and what is wrong
//! with it in words a user understands ("does not exist", "is damaged", "was
//! saved by a newer version"), in the UI language. The technical cause follows
//! on its own line for support.
//!
//! Used by File ▸ Open, `bae <project>`, `bae --headless <project>` and
//! `bae export-json`.

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
    /// The file system refused (permissions, …).
    CannotRead(String),
    /// Anything else.
    Other,
}

/// Classify why `path` did not open.
pub fn classify(path: &Path, error: &anyhow::Error) -> Problem {
    if !path.exists() {
        return Problem::Missing;
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
        std::io::ErrorKind::NotFound => Problem::Missing,
        _ => Problem::CannotRead(error.to_string()),
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
        Problem::CannotRead(reason) => {
            tr("“{file}” cannot be read: {reason}").replace("{reason}", reason)
        }
        Problem::Other => tr("“{file}” could not be opened.").to_owned(),
    };
    text.replace("{file}", &file)
}

/// The full message: the sentence, then the technical cause (left out for a
/// missing file, where it would only say the same less clearly).
pub fn describe(path: &Path, error: &anyhow::Error) -> String {
    let problem = classify(path, error);
    let sentence = sentence(path, &problem);
    if problem == Problem::Missing {
        return sentence;
    }
    format!("{sentence}\n{} {error:#}", tr("Details:"))
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
    fn messages_name_the_file_and_keep_the_cause() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("坏的 文件.baez");
        fs::write(&path, b"PK\x03\x04 not really a zip").unwrap();
        let error = Document::load(&path).err().unwrap();
        let message = describe(&path, &error);
        let (first, cause) = message.split_once('\n').unwrap();
        assert!(first.contains(&path.display().to_string()), "{message}");
        assert!(first.contains("damaged"), "{message}");
        assert!(cause.starts_with("Details:"), "{message}");
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
            "“{file}” cannot be read: {reason}",
            "“{file}” could not be opened.",
            "Details:",
        ];
        let missing: Vec<_> = keys
            .iter()
            .filter(|k| !bae_ui::i18n::has_translation(k))
            .collect();
        assert!(missing.is_empty(), "no Chinese for {missing:?}");
    }
}
