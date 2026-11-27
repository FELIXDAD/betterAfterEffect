//! Shortcut presets of the Keyboard Shortcuts dialog (AE: the `.txt` files in
//! `aeks/`): the built-in "Default.txt" and the user's presets, one file each in
//! BAE's config folder (`<config>/Keyboard Shortcuts/<name>.txt`, JSON: the
//! keymap id and the changes against it). The dialog stages Save As and Delete;
//! OK writes them ([`PresetStore::apply`]). Also AE's Copy To Clipboard text.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::keymap::KeymapOverrides;

use super::tree::Tree;
use super::{CommandEntry, text};

/// Name of the built-in preset (AE: "After Effects Default.txt").
pub const DEFAULT: &str = "Default.txt";
/// The preset a change to the built-in one becomes.
pub const CUSTOM: &str = "Custom.txt";
/// A new preset's name in Save As when the built-in one is selected (AE:
/// "Untitled Custom Preset", $$$/dvaui/ksv/DefaultFileName).
pub const UNTITLED: &str = "Untitled Custom Preset.txt";
/// Folder of the preset files in the config folder.
pub const FOLDER: &str = "Keyboard Shortcuts";

/// AE's Keyboard Layout menu, in the order of the platform's AE (macOS 26.2.1,
/// Windows 26.0). BAE draws the US layout ("en"); the others are listed greyed.
pub const KEYBOARD_LAYOUTS: &[&str] = if cfg!(target_os = "macos") {
    &[
        "en", "sv", "ru", "pt", "nb", "ko", "de", "it", "es", "fr", "ja",
    ]
} else {
    &[
        "en", "pt", "ja", "ko", "sv", "ru", "fr", "es", "nb", "it", "de",
    ]
};

/// A preset file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PresetFile {
    /// Format marker and version.
    pub kind: String,
    pub version: u32,
    /// Id of the keymap the changes apply to (`"ae"`).
    pub keymap: String,
    #[serde(flatten)]
    pub changes: KeymapOverrides,
}

const KIND: &str = "bae-keyboard-shortcuts";

impl PresetFile {
    pub fn new(keymap: &str, changes: KeymapOverrides) -> Self {
        PresetFile {
            kind: KIND.into(),
            version: 1,
            keymap: keymap.into(),
            changes,
        }
    }
}

/// A preset name as typed in Save As, made a file name (`My Keys` → `My Keys.txt`);
/// `None` when it cannot be one (empty, a path, the built-in preset's name).
pub fn file_name(typed: &str) -> Option<String> {
    let t = typed.trim();
    let base = t.strip_suffix(".txt").unwrap_or(t).trim();
    let bad = |c: char| {
        matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()
    };
    if base.is_empty() || base.starts_with('.') || base.chars().any(bad) || base.len() > 120 {
        return None;
    }
    let name = format!("{base}.txt");
    (!name.eq_ignore_ascii_case(DEFAULT)).then_some(name)
}

/// The presets of one keymap: what is on disk and what the dialog changed.
#[derive(Clone, Debug, Default)]
pub struct PresetStore {
    dir: Option<PathBuf>,
    keymap: String,
    /// Presets by file name (the built-in one is not listed).
    pub presets: BTreeMap<String, KeymapOverrides>,
    /// Staged: written / deleted on OK.
    saves: BTreeMap<String, KeymapOverrides>,
    deletes: Vec<String>,
}

impl PresetStore {
    /// The presets of `keymap` in `dir` (none without a folder: tests and
    /// smoke runs keep them in memory).
    pub fn load(dir: Option<&Path>, keymap: &str) -> Self {
        let mut presets = BTreeMap::new();
        if let Some(dir) = dir
            && let Ok(entries) = std::fs::read_dir(dir)
        {
            for e in entries.flatten() {
                let path = e.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !name.ends_with(".txt") || name.eq_ignore_ascii_case(DEFAULT) {
                    continue;
                }
                match std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|t| serde_json::from_str::<PresetFile>(&t).ok())
                {
                    Some(f) if f.kind == KIND && f.keymap == keymap => {
                        presets.insert(name.to_owned(), f.changes);
                    }
                    Some(_) => {}
                    None => {
                        tracing::warn!("ignoring unreadable shortcut preset {}", path.display())
                    }
                }
            }
        }
        PresetStore {
            dir: dir.map(Path::to_path_buf),
            keymap: keymap.to_owned(),
            presets,
            saves: BTreeMap::new(),
            deletes: Vec::new(),
        }
    }

    /// The names for the Preset menu: the built-in one, then the user's,
    /// alphabetical (AE).
    pub fn names(&self) -> Vec<String> {
        let mut v = vec![DEFAULT.to_owned()];
        let mut user: Vec<&String> = self.presets.keys().collect();
        user.sort_by_key(|n| n.to_lowercase());
        v.extend(user.into_iter().cloned());
        v
    }

    pub fn get(&self, name: &str) -> Option<&KeymapOverrides> {
        self.presets.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.presets.contains_key(name)
    }

    /// Stage `name` with these changes (Save As, or a changed preset at OK).
    pub fn put(&mut self, name: &str, changes: KeymapOverrides) {
        self.deletes.retain(|d| d != name);
        self.presets.insert(name.to_owned(), changes.clone());
        self.saves.insert(name.to_owned(), changes);
    }

    /// The preset selected when the dialog opens, with the shortcuts in use
    /// (not written unless changed).
    pub fn adopt(&mut self, name: &str, changes: KeymapOverrides) {
        self.presets.insert(name.to_owned(), changes);
    }

    /// Stage deleting `name`.
    pub fn remove(&mut self, name: &str) {
        self.presets.remove(name);
        self.saves.remove(name);
        if !self.deletes.iter().any(|d| d == name) {
            self.deletes.push(name.to_owned());
        }
    }

    /// OK: write the staged files.
    pub fn apply(&mut self) -> std::io::Result<()> {
        let Some(dir) = &self.dir else {
            self.saves.clear();
            self.deletes.clear();
            return Ok(());
        };
        for name in std::mem::take(&mut self.deletes) {
            match std::fs::remove_file(dir.join(&name)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        if !self.saves.is_empty() {
            std::fs::create_dir_all(dir)?;
        }
        for (name, changes) in std::mem::take(&mut self.saves) {
            let file = PresetFile::new(&self.keymap, changes);
            let json = serde_json::to_string_pretty(&file).map_err(std::io::Error::other)?;
            let path = dir.join(&name);
            let tmp = path.with_extension("txt.tmp");
            std::fs::write(&tmp, json + "\n")?;
            std::fs::rename(&tmp, &path)?;
        }
        Ok(())
    }
}

/// AE's Copy To Clipboard text: the whole tree, one line per group and command
/// (`<tabs><name>\t<shortcuts>\t`), CR line ends, an empty line at the end.
pub fn clipboard_text(
    tree: &Tree,
    chords: &std::collections::HashMap<super::tree::RowKey, Vec<crate::keymap::Chord>>,
) -> String {
    let mut out = String::new();
    for n in &tree.nodes {
        for _ in 0..n.depth {
            out.push('\t');
        }
        out.push_str(&n.title);
        out.push('\t');
        if let Some(e) = &n.entry {
            let key = super::tree::RowKey::of(e as &CommandEntry);
            if let Some(cs) = chords.get(&key) {
                out.push_str(&text::chords_text(cs));
            }
        }
        out.push_str("\t\r");
    }
    out.push('\r');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Binding;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bae-presets-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn names_become_file_names() {
        assert_eq!(file_name("My Keys").as_deref(), Some("My Keys.txt"));
        assert_eq!(file_name(" Mine.txt ").as_deref(), Some("Mine.txt"));
        assert_eq!(
            file_name("无标题自定义预设.txt").as_deref(),
            Some("无标题自定义预设.txt")
        );
        for bad in [
            "",
            "  ",
            ".txt",
            "a/b",
            "a\\b",
            "x:y",
            ".hidden",
            "Default.txt",
            "default",
        ] {
            assert_eq!(file_name(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn staged_until_applied_then_on_disk() {
        let dir = temp_dir("apply");
        let mut changes = KeymapOverrides::default();
        changes.bind(Binding::new("Cmd+J", "comp.new"));
        let mut store = PresetStore::load(Some(&dir), "ae");
        assert_eq!(store.names(), [DEFAULT]);
        store.put("Mine.txt", changes.clone());
        store.put(CUSTOM, KeymapOverrides::default());
        assert_eq!(store.names(), [DEFAULT, CUSTOM, "Mine.txt"]);
        assert!(!dir.exists(), "nothing written before OK");
        store.apply().unwrap();
        let again = PresetStore::load(Some(&dir), "ae");
        assert_eq!(again.get("Mine.txt"), Some(&changes));
        assert!(
            PresetStore::load(Some(&dir), "ai").presets.is_empty(),
            "other keymap"
        );
        // Deleting: staged, then gone.
        let mut store = again;
        store.remove("Mine.txt");
        assert!(dir.join("Mine.txt").exists());
        store.apply().unwrap();
        assert!(!dir.join("Mine.txt").exists());
        // Unreadable files are skipped.
        std::fs::write(dir.join("junk.txt"), "not json").unwrap();
        assert_eq!(
            PresetStore::load(Some(&dir), "ae").names(),
            [DEFAULT, CUSTOM]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
