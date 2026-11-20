use std::path::PathBuf;

use bae_model::{CommandError, ModelError, PathError};

/// Errors of reading, planning and applying an Illustrator import.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0}: not an Illustrator, PDF or BAE Ai bridge file")]
    UnknownFormat(String),
    #[error("{0} is not supported yet")]
    Unsupported(String),
    #[error("malformed file: {0}")]
    Malformed(String),
    #[error("the document has no artboards to import")]
    NoArtboards,
    #[error("artboard index {0} is out of range")]
    NoSuchArtboard(usize),
    #[error("step {step} ({command}) of the import failed: {source}")]
    Batch {
        step: usize,
        command: String,
        source: CommandError,
    },
    #[error(transparent)]
    Model(#[from] ModelError),
}

impl From<PathError> for Error {
    fn from(e: PathError) -> Self {
        Error::Model(e.into())
    }
}

impl From<Error> for CommandError {
    fn from(e: Error) -> Self {
        match e {
            Error::Model(m) => CommandError::Model(m),
            Error::Batch { source, .. } => source,
            other => CommandError::Failed(other.to_string()),
        }
    }
}
