//! BAE preferences: AE's Edit > Preferences (macOS: After Effects > Settings).
//!
//! * [`schema`] describes every page and control: label, type, range, unit,
//!   default, the AE preferences key, enable conditions.
//! * [`Preferences`] holds the values, keyed by the dotted schema key. Reading is
//!   lenient: a missing, mistyped or out-of-range value falls back to the default
//!   (clamped for numbers), and unknown keys (a newer BAE) are kept and written
//!   back so downgrading loses nothing.
//! * [`PrefsStore`] is the app-wide current value (an app resource): cheap
//!   snapshots, a generation counter to poll and change subscribers.
//! * With the `file` feature, [`PrefsStore::open`] reads and writes the file with
//!   `bae_platform::SettingsFile` (atomic writes, corrupt files moved aside).
//!
//! The crate has no UI, GPU or platform dependency (without `file`), so domain
//! command crates can read preferences too.

pub mod i18n;
pub mod schema;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use serde_json::{Map, Value};

pub use schema::{DefaultValue, Item, Kind, LABEL_NAMES, Os, PAGES, Page, Pref, When};

/// File name in the config directory.
pub const FILE_NAME: &str = "preferences.json";

/// Preference values.
#[derive(Debug, Clone, PartialEq)]
pub struct Preferences {
    values: BTreeMap<&'static str, Value>,
    /// Keys this version does not know (kept for newer versions).
    extra: Map<String, Value>,
}

impl Default for Preferences {
    fn default() -> Self {
        let values = schema::all()
            .iter()
            .map(|p| (p.key, default_json(p)))
            .collect();
        Self {
            values,
            extra: Map::new(),
        }
    }
}

fn default_json(p: &Pref) -> Value {
    match p.default {
        DefaultValue::Bool(v) => Value::Bool(v),
        DefaultValue::Int(v) => Value::from(v),
        DefaultValue::Float(v) => Value::from(v),
        DefaultValue::Str(v) => Value::from(v),
    }
}

/// `v` as a valid value of `p`, or `None`.
fn validate(p: &Pref, v: &Value) -> Option<Value> {
    Some(match p.kind {
        Kind::Bool => Value::Bool(v.as_bool()?),
        Kind::Int { min, max } => {
            Value::from(v.as_f64()?.round().clamp(min as f64, max as f64) as i64)
        }
        Kind::Label => Value::from(v.as_i64()?.clamp(0, 16)),
        Kind::Float { min, max, .. } | Kind::Slider { min, max, .. } => {
            let f = v.as_f64().filter(|f| f.is_finite())?;
            Value::from(f.clamp(min, max))
        }
        Kind::Duration { max } => {
            let f = v.as_f64().filter(|f| f.is_finite())?;
            Value::from(f.clamp(0.0, max))
        }
        Kind::Choice { options, .. } => {
            let s = v.as_str()?;
            Value::from(*options.iter().find(|o| **o == s)?)
        }
        Kind::Color => Value::from(parse_color(v.as_str()?).map(format_color)?),
        Kind::Text | Kind::Path { .. } => Value::from(v.as_str()?),
    })
}

/// Parses `#rrggbb`.
pub fn parse_color(s: &str) -> Option<[u8; 3]> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([c(0)?, c(2)?, c(4)?])
}

/// Formats `#rrggbb`.
pub fn format_color(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

impl Preferences {
    /// Values from a JSON object (the file's `data`); anything invalid falls back
    /// to its default. Returns the keys that were replaced.
    pub fn from_json(data: &Map<String, Value>) -> (Self, Vec<String>) {
        let mut prefs = Self::default();
        let mut fixed = Vec::new();
        for (key, value) in data {
            match schema::find(key) {
                Some(p) => match validate(p, value) {
                    Some(v) => {
                        if &v != value
                            && !(v.is_number() && value.is_number() && v.as_f64() == value.as_f64())
                        {
                            fixed.push(key.clone());
                        }
                        prefs.values.insert(p.key, v);
                    }
                    None => fixed.push(key.clone()),
                },
                None => {
                    prefs.extra.insert(key.clone(), value.clone());
                }
            }
        }
        (prefs, fixed)
    }

    /// All values (and unknown keys) as a JSON object.
    pub fn to_json(&self) -> Map<String, Value> {
        let mut out = self.extra.clone();
        for (k, v) in &self.values {
            out.insert((*k).to_owned(), v.clone());
        }
        out
    }

    /// Raw value of `key` (`None` for an unknown key).
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// Sets `key`; invalid values are clamped or rejected. Returns whether the
    /// value changed.
    pub fn set(&mut self, key: &str, value: impl Into<Value>) -> bool {
        let Some(p) = schema::find(key) else {
            return false;
        };
        let Some(v) = validate(p, &value.into()) else {
            return false;
        };
        self.values.insert(p.key, v.clone()) != Some(v)
    }

    /// Resets `key` to its default.
    pub fn reset(&mut self, key: &str) {
        if let Some(p) = schema::find(key) {
            self.values.insert(p.key, default_json(p));
        }
    }

    pub fn bool(&self, key: &str) -> bool {
        self.get(key).and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn int(&self, key: &str) -> i64 {
        self.get(key).and_then(Value::as_i64).unwrap_or(0)
    }

    pub fn float(&self, key: &str) -> f64 {
        self.get(key).and_then(Value::as_f64).unwrap_or(0.0)
    }

    pub fn str(&self, key: &str) -> &str {
        self.get(key).and_then(Value::as_str).unwrap_or("")
    }

    /// A color preference as RGB.
    pub fn color(&self, key: &str) -> [u8; 3] {
        parse_color(self.str(key)).unwrap_or([0, 0, 0])
    }

    /// Whether the control for `p` is enabled (its [`When`] condition holds).
    pub fn enabled(&self, p: &Pref) -> bool {
        match p.enabled {
            None => true,
            Some(When::On(key)) => self.bool(key),
            Some(When::Is(key, option)) => self.str(key) == option,
        }
    }

    // ---- typed accessors for the settings features use ------------------------------

    /// General > Levels of Undo.
    pub fn levels_of_undo(&self) -> usize {
        self.int(keys::LEVELS_OF_UNDO) as usize
    }

    /// Label color `index` (1..=16; 0 is None).
    pub fn label_color(&self, index: usize) -> [u8; 3] {
        let key = format!("labels.color_{index}");
        match self.get(&key) {
            Some(_) => self.color(&key),
            None => parse_color(schema::LABEL_COLORS[0]).unwrap_or_default(),
        }
    }

    /// Label name `index` (the English default name until the user renames it).
    pub fn label_name(&self, index: usize) -> &str {
        self.str(&format!("labels.name_{index}"))
    }

    /// Label colors 0..=16 (index 0 is the fixed None color).
    pub fn label_colors(&self) -> [[u8; 3]; 17] {
        std::array::from_fn(|i| self.label_color(i))
    }

    /// Auto-save interval in minutes, or `None` when auto-save is off.
    pub fn auto_save_minutes(&self) -> Option<u64> {
        self.bool(keys::AUTO_SAVE)
            .then(|| self.int(keys::AUTO_SAVE_INTERVAL).max(1) as u64)
    }
}

/// Keys features read (the schema has them all).
pub mod keys {
    pub const LEVELS_OF_UNDO: &str = "general.levels_of_undo";
    pub const PATH_POINT_SIZE: &str = "general.path_point_size";
    pub const CREATE_LAYERS_AT_START: &str = "general.create_layers_at_start";
    pub const DEFAULT_SPATIAL_LINEAR: &str = "general.default_spatial_linear";
    pub const SPLIT_LAYERS_ABOVE: &str = "general.split_layers_above";
    pub const GRID_COLOR: &str = "grids.grid_color";
    pub const GRID_STYLE: &str = "grids.grid_style";
    pub const GRID_SPACING: &str = "grids.grid_spacing";
    pub const GRID_SUBDIVISIONS: &str = "grids.grid_subdivisions";
    pub const PROP_GRID_H: &str = "grids.prop_horizontal";
    pub const PROP_GRID_V: &str = "grids.prop_vertical";
    pub const GUIDE_COLOR: &str = "grids.guide_color";
    pub const GUIDE_STYLE: &str = "grids.guide_style";
    pub const ACTION_SAFE: &str = "grids.action_safe";
    pub const TITLE_SAFE: &str = "grids.title_safe";
    pub const CC_ACTION_SAFE: &str = "grids.cc_action_safe";
    pub const CC_TITLE_SAFE: &str = "grids.cc_title_safe";
    pub const BRIGHTNESS: &str = "appearance.brightness";
    pub const AFFECT_LABELS: &str = "appearance.affect_labels";
    pub const INTERACTIVE: &str = "appearance.interactive";
    pub const FOCUS: &str = "appearance.focus";
    pub const LABEL_HANDLES: &str = "appearance.label_handles";
    pub const AUTO_SAVE: &str = "auto_save.enabled";
    pub const AUTO_SAVE_INTERVAL: &str = "auto_save.interval";
    pub const AUTO_SAVE_MAX_VERSIONS: &str = "auto_save.max_versions";
    pub const AUTO_SAVE_LOCATION: &str = "auto_save.location";
    pub const AUTO_SAVE_FOLDER: &str = "auto_save.folder";
    pub const STILL_LENGTH: &str = "import.still_length";
    pub const STILL_DURATION: &str = "import.still_duration";
    pub const SEQUENCE_FPS: &str = "import.sequence_fps";
    pub const EXPR_FONT: &str = "scripting.font";
    pub const EXPR_FONT_SIZE: &str = "scripting.font_size";
}

type Subscriber = Box<dyn Fn(&Preferences) + Send + Sync>;

/// The app's current preferences (an app resource, `Arc<PrefsStore>`).
pub struct PrefsStore {
    current: RwLock<Arc<Preferences>>,
    generation: AtomicU64,
    subscribers: Mutex<Vec<Subscriber>>,
    #[cfg(feature = "file")]
    file: Option<bae_platform::SettingsFile<file::PrefsData>>,
}

impl std::fmt::Debug for PrefsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrefsStore")
            .field("generation", &self.generation())
            .finish_non_exhaustive()
    }
}

impl Default for PrefsStore {
    fn default() -> Self {
        Self::new(Preferences::default())
    }
}

impl PrefsStore {
    /// A store that is not backed by a file.
    pub fn new(prefs: Preferences) -> Self {
        Self {
            current: RwLock::new(Arc::new(prefs)),
            generation: AtomicU64::new(0),
            subscribers: Mutex::new(Vec::new()),
            #[cfg(feature = "file")]
            file: None,
        }
    }

    /// The current values (a cheap snapshot).
    pub fn current(&self) -> Arc<Preferences> {
        self.current
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Bumped on every change; poll it to refresh cached values.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Calls `f` with the new values after every change.
    pub fn subscribe(&self, f: impl Fn(&Preferences) + Send + Sync + 'static) {
        self.subscribers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Box::new(f));
    }

    /// Replaces the values (Preferences dialog OK), saves them when the store has a
    /// file, and notifies subscribers. Unchanged values do nothing.
    pub fn set(&self, prefs: Preferences) -> std::io::Result<()> {
        if *self.current() == prefs {
            return Ok(());
        }
        let prefs = Arc::new(prefs);
        *self
            .current
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = prefs.clone();
        self.generation.fetch_add(1, Ordering::AcqRel);
        for f in self
            .subscribers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
        {
            f(&prefs);
        }
        self.save()
    }

    /// Writes the current values to the file (no-op without one).
    pub fn save(&self) -> std::io::Result<()> {
        #[cfg(feature = "file")]
        if let Some(file) = &self.file {
            return file.save(&file::PrefsData(self.current().to_json()));
        }
        Ok(())
    }
}

#[cfg(feature = "file")]
pub mod file {
    //! Storage with `bae_platform::SettingsFile`.

    use super::*;
    use bae_platform::settings::LoadStatus;
    use bae_platform::{SettingsFile, VersionedSettings};

    /// The file's `data`: a flat object of dotted keys.
    #[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
    #[serde(transparent)]
    pub struct PrefsData(pub Map<String, Value>);

    impl VersionedSettings for PrefsData {
        const KIND: &'static str = "bae-preferences";
        const VERSION: u32 = 1;
    }

    impl PrefsStore {
        /// Loads `path` (defaults when missing or unreadable; a corrupt file is moved
        /// aside) and keeps it for [`PrefsStore::set`].
        pub fn open(path: impl Into<std::path::PathBuf>) -> (Self, LoadStatus) {
            let file = SettingsFile::<PrefsData>::new(path);
            let loaded = file.load();
            let (prefs, fixed) = Preferences::from_json(&loaded.value.0);
            if !fixed.is_empty() {
                tracing::warn!("preferences reset to defaults: {}", fixed.join(", "));
            }
            let mut store = Self::new(prefs);
            store.file = Some(file);
            (store, loaded.status)
        }
    }
}

#[cfg(test)]
mod tests;
