//! Versioned JSON settings files.
//!
//! On disk a settings file is an envelope:
//!
//! ```json
//! { "kind": "app-settings", "version": 2, "data": { … } }
//! ```
//!
//! * Older versions are upgraded step by step with [`VersionedSettings::migrate`].
//! * A file written by a *newer* BAE is read leniently (unknown fields ignored,
//!   missing fields defaulted) and backed up once as `<file>.v<N>.bak` before we
//!   overwrite it, so downgrading never silently destroys newer settings.
//! * A corrupt file is moved aside to `<file>.corrupt` and defaults are used.
//! * Writes are atomic: temp file in the same directory, `fsync`, rename.

use std::io::{self, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// A settings type stored with [`SettingsFile`].
///
/// Use `#[serde(default)]` on the struct so new fields get defaults when reading
/// older or newer files.
pub trait VersionedSettings: Serialize + DeserializeOwned + Default {
    /// Envelope `kind`, e.g. `"app-settings"`; guards against loading the wrong file.
    const KIND: &'static str;
    /// Current schema version (start at 1, bump on incompatible changes).
    const VERSION: u32;

    /// Upgrades `data` from version `from` to `from + 1`. The default
    /// implementation keeps the data unchanged (fine for additive changes).
    fn migrate(from: u32, data: Value) -> Result<Value, String> {
        let _ = from;
        Ok(data)
    }
}

/// How [`SettingsFile::load`] obtained its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadStatus {
    /// The file existed and matched the current version.
    Loaded,
    /// The file did not exist; defaults were used.
    Missing,
    /// The file was migrated from an older version.
    Migrated {
        /// Version found on disk.
        from: u32,
    },
    /// The file came from a newer version and was read leniently.
    Newer {
        /// Version found on disk.
        found: u32,
    },
    /// The file was unreadable; it was moved to `backup` and defaults were used.
    Corrupt {
        /// Why it could not be read.
        reason: String,
        /// Where the bad file was moved.
        backup: Option<PathBuf>,
    },
}

/// A loaded value plus how it was obtained.
#[derive(Debug, Clone)]
pub struct Loaded<T> {
    /// The settings value.
    pub value: T,
    /// How it was obtained.
    pub status: LoadStatus,
}

/// A settings file at a fixed path.
#[derive(Debug, Clone)]
pub struct SettingsFile<T> {
    path: PathBuf,
    _marker: PhantomData<fn() -> T>,
}

#[derive(Serialize)]
struct EnvelopeOut<'a, T> {
    kind: &'a str,
    version: u32,
    data: &'a T,
}

impl<T: VersionedSettings> SettingsFile<T> {
    /// A settings file at `path` (not read until [`Self::load`]).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            _marker: PhantomData,
        }
    }

    /// File path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads the settings, falling back to defaults (never fails).
    pub fn load(&self) -> Loaded<T> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Loaded {
                    value: T::default(),
                    status: LoadStatus::Missing,
                };
            }
            Err(err) => return self.corrupt(format!("read failed: {err}")),
        };
        match self.decode(&text) {
            Ok(loaded) => loaded,
            Err(reason) => self.corrupt(reason),
        }
    }

    fn decode(&self, text: &str) -> Result<Loaded<T>, String> {
        let envelope: Value =
            serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))?;
        let kind = envelope.get("kind").and_then(Value::as_str);
        if kind != Some(T::KIND) {
            return Err(format!("expected kind {:?}, found {kind:?}", T::KIND));
        }
        let version = envelope
            .get("version")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or("missing or invalid version")?;
        let mut data = envelope.get("data").cloned().unwrap_or(Value::Null);
        let status = match version.cmp(&T::VERSION) {
            std::cmp::Ordering::Equal => LoadStatus::Loaded,
            std::cmp::Ordering::Less => {
                for from in version..T::VERSION {
                    data = T::migrate(from, data)
                        .map_err(|e| format!("migration from v{from} failed: {e}"))?;
                }
                LoadStatus::Migrated { from: version }
            }
            std::cmp::Ordering::Greater => {
                let backup = self.path.with_extension(format!("json.v{version}.bak"));
                if !backup.exists() {
                    let _ = std::fs::copy(&self.path, &backup);
                }
                LoadStatus::Newer { found: version }
            }
        };
        let value = serde_json::from_value(data).map_err(|e| format!("invalid data: {e}"))?;
        Ok(Loaded { value, status })
    }

    fn corrupt(&self, reason: String) -> Loaded<T> {
        let backup = self.path.with_extension("json.corrupt");
        let backup = std::fs::rename(&self.path, &backup).ok().map(|()| backup);
        Loaded {
            value: T::default(),
            status: LoadStatus::Corrupt { reason, backup },
        }
    }

    /// Saves `value` atomically, creating the parent directory if needed.
    pub fn save(&self, value: &T) -> io::Result<()> {
        let envelope = EnvelopeOut {
            kind: T::KIND,
            version: T::VERSION,
            data: value,
        };
        let mut bytes = serde_json::to_vec_pretty(&envelope).map_err(io::Error::other)?;
        bytes.push(b'\n');
        write_atomic(&self.path, &bytes)
    }
}

/// Writes `bytes` to `path` atomically (temp file + fsync + rename).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Prefs {
        language: String,
        recent: Vec<String>,
    }

    impl VersionedSettings for Prefs {
        const KIND: &'static str = "test-prefs";
        const VERSION: u32 = 2;

        fn migrate(from: u32, mut data: Value) -> Result<Value, String> {
            if from == 1 {
                // v1 called the field "lang".
                let obj = data.as_object_mut().ok_or("not an object")?;
                if let Some(lang) = obj.remove("lang") {
                    obj.insert("language".into(), lang);
                }
            }
            Ok(data)
        }
    }

    fn file(dir: &tempfile::TempDir) -> SettingsFile<Prefs> {
        SettingsFile::new(dir.path().join("prefs.json"))
    }

    #[test]
    fn missing_then_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let settings = file(&dir);
        assert_eq!(settings.load().status, LoadStatus::Missing);
        let prefs = Prefs {
            language: "zh-CN".into(),
            recent: vec!["a.bae".into()],
        };
        settings.save(&prefs).unwrap();
        let loaded = settings.load();
        assert_eq!(loaded.status, LoadStatus::Loaded);
        assert_eq!(loaded.value, prefs);
        let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "no temp files left behind");
    }

    #[test]
    fn migrates_old_versions() {
        let dir = tempfile::tempdir().unwrap();
        let settings = file(&dir);
        std::fs::write(
            settings.path(),
            r#"{"kind":"test-prefs","version":1,"data":{"lang":"en"}}"#,
        )
        .unwrap();
        let loaded = settings.load();
        assert_eq!(loaded.status, LoadStatus::Migrated { from: 1 });
        assert_eq!(loaded.value.language, "en");
    }

    #[test]
    fn newer_versions_are_read_leniently_and_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let settings = file(&dir);
        std::fs::write(
            settings.path(),
            r#"{"kind":"test-prefs","version":9,"data":{"language":"ja","future":true}}"#,
        )
        .unwrap();
        let loaded = settings.load();
        assert_eq!(loaded.status, LoadStatus::Newer { found: 9 });
        assert_eq!(loaded.value.language, "ja");
        assert!(dir.path().join("prefs.json.v9.bak").exists());
    }

    #[test]
    fn corrupt_files_are_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let settings = file(&dir);
        std::fs::write(settings.path(), "{ not json").unwrap();
        let loaded = settings.load();
        assert!(matches!(
            loaded.status,
            LoadStatus::Corrupt {
                backup: Some(_),
                ..
            }
        ));
        assert_eq!(loaded.value, Prefs::default());
        assert!(!settings.path().exists());

        std::fs::write(settings.path(), r#"{"kind":"other","version":2,"data":{}}"#).unwrap();
        assert!(matches!(settings.load().status, LoadStatus::Corrupt { .. }));
    }
}
