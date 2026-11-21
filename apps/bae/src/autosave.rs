//! Preferences > Auto-Save, like AE: every N minutes a project with unsaved
//! changes is copied to an "Auto-Save" folder next to it (or to the custom
//! folder), as `<name> auto-save <k>.bae`, keeping at most the configured number
//! of versions (the oldest is replaced). The open project's location and saved
//! state do not change. Untitled projects are copied only to a custom folder.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use bae_prefs::{Preferences, keys};

/// Folder created next to the project (AE: "Adobe After Effects Auto-Save").
pub const FOLDER: &str = "BAE Auto-Save";

/// Auto-save timer.
#[derive(Debug)]
pub struct AutoSave {
    since: Instant,
}

impl Default for AutoSave {
    fn default() -> Self {
        Self {
            since: Instant::now(),
        }
    }
}

impl AutoSave {
    /// When the next auto-save is due (`None`: auto-save is off).
    pub fn deadline(&self, prefs: &Preferences) -> Option<Instant> {
        let minutes = prefs.auto_save_minutes()?;
        Some(self.since + Duration::from_secs(minutes * 60))
    }

    /// Restarts the interval (after a manual save, opening a project).
    pub fn reset(&mut self) {
        self.since = Instant::now();
    }

    /// Saves a copy when due and the project has unsaved changes; returns where.
    pub fn tick(
        &mut self,
        now: Instant,
        prefs: &Preferences,
        doc: &crate::document::Document,
    ) -> Option<anyhow::Result<PathBuf>> {
        if self.deadline(prefs).is_none_or(|d| now < d) {
            return None;
        }
        self.since = now;
        if !doc.is_dirty() {
            return None;
        }
        let target = target(doc.location(), &doc.display_name(), prefs)?;
        Some(
            std::fs::create_dir_all(target.parent().unwrap_or(Path::new(".")))
                .map_err(anyhow::Error::from)
                .and_then(|()| {
                    if target.exists() {
                        std::fs::remove_dir_all(&target)?;
                    }
                    doc.save_copy(&target)
                })
                .map(|()| target),
        )
    }
}

/// The file the next auto-save of the project at `location` writes.
pub fn target(location: Option<&Path>, name: &str, prefs: &Preferences) -> Option<PathBuf> {
    let dir = if prefs.str(keys::AUTO_SAVE_LOCATION) == "Custom Location"
        && !prefs.str(keys::AUTO_SAVE_FOLDER).is_empty()
    {
        PathBuf::from(prefs.str(keys::AUTO_SAVE_FOLDER))
    } else {
        location?.parent()?.join(FOLDER)
    };
    let stem = location
        .and_then(Path::file_stem)
        .map_or_else(|| name.to_owned(), |s| s.to_string_lossy().into_owned());
    let max = prefs.int(keys::AUTO_SAVE_MAX_VERSIONS).max(1);
    let candidates: Vec<PathBuf> = (1..=max)
        .map(|k| dir.join(format!("{stem} auto-save {k}.bae")))
        .collect();
    // The first free slot, else the oldest.
    candidates
        .iter()
        .find(|p| !p.exists())
        .cloned()
        .or_else(|| {
            candidates
                .iter()
                .min_by_key(|p| {
                    std::fs::metadata(p)
                        .and_then(|m| m.modified())
                        .unwrap_or(SystemTime::UNIX_EPOCH)
                })
                .cloned()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_through_versions_next_to_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let mut prefs = Preferences::default();
        prefs.set(keys::AUTO_SAVE_MAX_VERSIONS, 2);
        let project = dir.path().join("Spot.bae");
        let mut doc = crate::document::Document::new(None);
        doc.save_as(&project).unwrap();
        let first = target(Some(&project), "Spot", &prefs).unwrap();
        assert_eq!(first, dir.path().join(FOLDER).join("Spot auto-save 1.bae"));
        doc.save_copy(&first).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        let second = target(Some(&project), "Spot", &prefs).unwrap();
        assert!(second.ends_with("Spot auto-save 2.bae"));
        doc.save_copy(&second).unwrap();
        // Both slots used: the oldest is replaced.
        assert_eq!(target(Some(&project), "Spot", &prefs).unwrap(), first);
        // The project itself is untouched.
        assert_eq!(doc.location(), Some(project.as_path()));
        // Untitled: only with a custom folder.
        assert_eq!(target(None, "Untitled", &prefs), None);
        prefs.set(keys::AUTO_SAVE_LOCATION, "Custom Location");
        prefs.set(
            keys::AUTO_SAVE_FOLDER,
            dir.path().to_string_lossy().as_ref(),
        );
        assert_eq!(
            target(None, "Untitled", &prefs).unwrap(),
            dir.path().join("Untitled auto-save 1.bae")
        );
    }

    #[test]
    fn tick_waits_for_the_interval_and_for_changes() {
        let prefs = Preferences::default();
        let doc = crate::document::Document::new(None);
        let mut auto = AutoSave::default();
        let start = auto.since;
        assert!(auto.tick(start, &prefs, &doc).is_none(), "not due yet");
        let due = auto.deadline(&prefs).unwrap();
        assert_eq!(due - start, Duration::from_secs(5 * 60));
        // Due, but nothing changed: no copy, timer restarts.
        assert!(auto.tick(due, &prefs, &doc).is_none());
        assert!(auto.deadline(&prefs).unwrap() > due);
        let mut off = prefs.clone();
        off.set(keys::AUTO_SAVE, false);
        assert_eq!(auto.deadline(&off), None);
    }
}
