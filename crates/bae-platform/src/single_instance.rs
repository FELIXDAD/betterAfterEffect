//! One GUI instance per user.
//!
//! Implemented with an OS advisory lock (`File::try_lock`: `flock` on Unix,
//! `LockFileEx` on Windows) on `<state dir>/<name>.lock`. The lock is released by
//! the OS when the process exits — including crashes — so stale locks cannot
//! happen. The owner's PID is written to a separate `<name>.pid` file because a
//! locked file is not readable by other processes on Windows.
//!
//! Forwarding the second launch's arguments (e.g. a project path) to the running
//! instance is planned on top of this (local socket); for now the second launch
//! reports the running PID and exits.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Errors acquiring the instance lock.
#[derive(Debug, thiserror::Error)]
pub enum SingleInstanceError {
    /// Another process holds the lock.
    #[error("another instance is already running{}", pid.map(|p| format!(" (pid {p})")).unwrap_or_default())]
    AlreadyRunning {
        /// PID of the running instance, if known.
        pid: Option<u32>,
    },
    /// The lock file could not be created or locked.
    #[error("cannot create instance lock {path}: {source}")]
    Io {
        /// Lock file path.
        path: PathBuf,
        /// Underlying error.
        source: io::Error,
    },
}

/// Holds the instance lock until dropped.
#[derive(Debug)]
pub struct SingleInstance {
    _lock: File,
    lock_path: PathBuf,
    pid_path: PathBuf,
}

impl SingleInstance {
    /// Acquires the lock `<dir>/<name>.lock`, creating `dir` if needed.
    pub fn acquire(dir: &Path, name: &str) -> Result<Self, SingleInstanceError> {
        let lock_path = dir.join(format!("{name}.lock"));
        let pid_path = dir.join(format!("{name}.pid"));
        let io_err = |source| SingleInstanceError::Io {
            path: lock_path.clone(),
            source,
        };
        std::fs::create_dir_all(dir).map_err(io_err)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(io_err)?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                let pid = std::fs::read_to_string(&pid_path)
                    .ok()
                    .and_then(|s| s.trim().parse().ok());
                return Err(SingleInstanceError::AlreadyRunning { pid });
            }
            Err(std::fs::TryLockError::Error(source)) => return Err(io_err(source)),
        }
        // Best effort: the PID file is informational only.
        if let Ok(mut file) = File::create(&pid_path) {
            let _ = write!(file, "{}", std::process::id());
        }
        Ok(Self {
            _lock: lock,
            lock_path,
            pid_path,
        })
    }

    /// Path of the lock file.
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.pid_path);
        // The lock itself is released when `_lock` is closed.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_fails_until_released() {
        let dir = tempfile::tempdir().unwrap();
        let first = SingleInstance::acquire(dir.path(), "bae-test").unwrap();
        assert!(first.lock_path().exists());
        match SingleInstance::acquire(dir.path(), "bae-test") {
            Err(SingleInstanceError::AlreadyRunning { pid }) => {
                assert_eq!(pid, Some(std::process::id()));
            }
            other => panic!("expected AlreadyRunning, got {other:?}"),
        }
        drop(first);
        let again = SingleInstance::acquire(dir.path(), "bae-test");
        assert!(again.is_ok(), "{again:?}");
    }
}
