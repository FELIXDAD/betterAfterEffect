use std::io;
use std::path::{Path, PathBuf};

use bae_model::{CommandError, ModelError, PrimPath, ProjectError};

/// Packaging errors.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("zip: {0}")]
    Zip(String),
    #[error("{0} already exists")]
    OutputExists(PathBuf),
    #[error("{0} is not an empty directory")]
    OutputNotEmpty(PathBuf),
    #[error("the output {0} would overwrite the project or one of its assets")]
    OutputIsSource(PathBuf),
    #[error("{} asset file(s) missing: {}", .0.len(), list(.0))]
    MissingAssets(Vec<PrimPath>),
    #[error("{0} is not an Asset prim")]
    NotAnAsset(PrimPath),
    #[error("no entry {0} in the package")]
    NoSuchEntry(String),
    #[error("{0}")]
    Unsupported(String),
    #[error(transparent)]
    Command(#[from] CommandError),
}

impl From<zip::result::ZipError> for PackageError {
    fn from(e: zip::result::ZipError) -> Self {
        PackageError::Zip(e.to_string())
    }
}

impl From<PackageError> for CommandError {
    fn from(e: PackageError) -> Self {
        match e {
            PackageError::Command(c) => c,
            PackageError::Model(m) => CommandError::Model(m),
            other => CommandError::Failed(other.to_string()),
        }
    }
}

pub(crate) fn io_err(path: &Path) -> impl FnOnce(io::Error) -> PackageError + '_ {
    move |source| PackageError::Io {
        path: path.to_owned(),
        source,
    }
}

fn list(paths: &[PrimPath]) -> String {
    let mut s: Vec<&str> = paths.iter().take(5).map(PrimPath::as_str).collect();
    if paths.len() > 5 {
        s.push("…");
    }
    s.join(", ")
}
