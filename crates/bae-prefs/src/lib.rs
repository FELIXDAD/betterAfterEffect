//! BAE preferences: AE 26's Edit > Preferences (macOS: After Effects > Settings…),
//! 16 pages.
//!
//! * [`schema`] describes every page and control: label, type, range, unit,
//!   default, the AE preferences key, enable conditions. The pages are generated
//!   from R1's AE 26 data ([`pages`], `gen/pages.py`).
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

mod expr_themes;
pub mod i18n;
pub mod pages;
pub mod schema;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use serde_json::{Map, Value};

pub use schema::{DefaultValue, Item, Kind, LABEL_NAMES, Opt, Os, PAGES, Page, Pref, Style, When};

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
        Kind::Choice { options, .. } => {
            let s = v.as_str()?;
            Value::from(options.iter().find(|o| o.value == s)?.value)
        }
        Kind::Color => Value::from(parse_color(v.as_str()?).map(format_color)?),
        Kind::Timecode => Value::from(parse_timecode(v.as_str()?).map(|_| v.as_str())??),
        Kind::Text | Kind::Path => Value::from(v.as_str()?),
    })
}

/// Parses a timecode `h:mm:ss:ff` into its four numbers.
pub fn parse_timecode(s: &str) -> Option<[u32; 4]> {
    let mut parts = s.split(':').map(|p| p.trim().parse::<u32>().ok());
    let tc = [
        parts.next()??,
        parts.next()??,
        parts.next()??,
        parts.next()??,
    ];
    (parts.next().is_none() && tc[1] < 60 && tc[2] < 60).then_some(tc)
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
        if let Some(theme) = data.get(keys::EXPR_THEME) {
            prefs.set(keys::EXPR_THEME, theme.clone());
        }
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
        if self.values.get(p.key) == Some(&v) {
            if key == keys::EXPR_THEME && self.selected_expression_theme().is_some() {
                self.update_expression_palette(key);
                return true;
            }
            return false;
        }
        self.values.insert(p.key, v);
        self.update_expression_palette(key);
        true
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
        p.enabled.is_none_or(|w| self.holds(&w))
    }

    /// Whether an enable condition holds.
    pub fn holds(&self, w: &When) -> bool {
        match w {
            When::On(key) => self.bool(key),
            When::Off(key) => !self.bool(key),
            When::Is(key, value) => self.str(key) == *value,
            When::All(all) => all.iter().all(|w| self.holds(w)),
        }
    }

    // ---- typed accessors for the settings features use ------------------------------

    /// Levels of Undo (not in AE 26's dialog; stored only).
    pub fn levels_of_undo(&self) -> usize {
        self.int(keys::LEVELS_OF_UNDO) as usize
    }

    /// Label color `index` (1..=16) as stored (AE's values, e.g. Red `#b53838`);
    /// index 0 (None) is not a setting and returns black.
    pub fn label_color(&self, index: usize) -> [u8; 3] {
        self.get(&format!("labels.label_color_{index}"))
            .and_then(Value::as_str)
            .and_then(parse_color)
            .unwrap_or_default()
    }

    /// Whether label color `index` is still AE's default.
    pub fn label_color_is_default(&self, index: usize) -> bool {
        let key = format!("labels.label_color_{index}");
        match schema::find(&key) {
            Some(Pref {
                default: DefaultValue::Str(d),
                ..
            }) => self.str(&key) == *d,
            _ => true,
        }
    }

    /// Label name `index` (the English default name until the user renames it).
    pub fn label_name(&self, index: usize) -> &str {
        self.str(&format!("labels.label_name_{index}"))
    }

    /// Auto-save interval in minutes, or `None` when auto-save is off.
    pub fn auto_save_minutes(&self) -> Option<u64> {
        self.bool(keys::AUTO_SAVE)
            .then(|| self.int(keys::AUTO_SAVE_INTERVAL).max(1) as u64)
    }
}

/// Keys features read (the schema has them all).
pub mod keys {
    pub const PREVIEW_CACHE_MIB: &str = "memory_cpu.preview_cache_mib";
    pub const RAM_RESERVED_GB: &str = "memory_cpu.ram_reserved_for_other_apps_gb";
    pub const LEVELS_OF_UNDO: &str = "general.levels_of_undo";
    // Appearance
    /// BAE: the UI language (`system`, `zh-CN`, `en`).
    pub const LANGUAGE: &str = "appearance.language";
    pub const PATH_POINT_SIZE: &str = "appearance.path_point_and_handle_size";
    pub const COLOR_THEME: &str = "appearance.color_theme";
    pub const LABEL_HANDLES: &str = "appearance.use_label_color_for_handles_and_paths";
    pub const MOTION_PATH_MODE: &str = "appearance.motion_path_mode";
    // Composition
    pub const CREATE_LAYERS_AT_START: &str = "composition.create_layers_at_comp_start";
    pub const DEFAULT_SPATIAL_LINEAR: &str = "composition.default_spatial_interpolation_linear";
    pub const SPLIT_LAYERS_ABOVE: &str = "composition.create_split_layers_above_original";
    pub const CENTER_SHAPE_ANCHOR: &str = "composition.center_anchor_point_new_shape";
    // Grids & Guides
    pub const GRID_COLOR: &str = "grids_guides.grid_color";
    pub const GRID_STYLE: &str = "grids_guides.grid_style";
    pub const GRID_SPACING: &str = "grids_guides.gridline_every";
    pub const GRID_SUBDIVISIONS: &str = "grids_guides.grid_subdivisions";
    pub const PROP_GRID_H: &str = "grids_guides.proportional_grid_horizontal";
    pub const PROP_GRID_V: &str = "grids_guides.proportional_grid_vertical";
    pub const GUIDE_COLOR: &str = "grids_guides.guides_color";
    pub const GUIDE_STYLE: &str = "grids_guides.guides_style";
    pub const ACTION_SAFE: &str = "grids_guides.action_safe";
    pub const TITLE_SAFE: &str = "grids_guides.title_safe";
    pub const CC_ACTION_SAFE: &str = "grids_guides.center_cut_action_safe";
    pub const CC_TITLE_SAFE: &str = "grids_guides.center_cut_title_safe";
    // Project > Auto-Save
    pub const AUTO_SAVE: &str = "project.auto_save_every_enabled";
    pub const AUTO_SAVE_INTERVAL: &str = "project.auto_save_interval_minutes";
    pub const AUTO_SAVE_MAX_VERSIONS: &str = "project.max_project_versions";
    pub const AUTO_SAVE_LOCATION: &str = "project.auto_save_location_mode";
    pub const AUTO_SAVE_FOLDER: &str = "project.auto_save_folder";
    /// [`AUTO_SAVE_LOCATION`] value for a custom folder.
    pub const CUSTOM_LOCATION: &str = "custom_location";
    // Import
    pub const STILL_LENGTH: &str = "import.still_footage_duration_mode";
    pub const STILL_DURATION: &str = "import.still_footage_duration_timecode";
    pub const SEQUENCE_FPS: &str = "import.sequence_footage_fps";
    // Scripting & Expressions
    pub const EXPR_FONT_SIZE: &str = "scripting_expressions.expr_font_size";
    pub const EXPR_THEME: &str = "scripting_expressions.expr_theme";
}

type Subscriber = Box<dyn Fn(&Preferences) + Send + Sync>;

/// The app's current preferences (an app resource, `Arc<PrefsStore>`).
pub struct PrefsStore {
    current: RwLock<Arc<Preferences>>,
    display_preview: RwLock<Option<Arc<Preferences>>>,
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
            display_preview: RwLock::default(),
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

    /// Visual settings for painting the next frame while the dialog is open.
    pub fn display(&self) -> Arc<Preferences> {
        self.display_preview
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| self.current())
    }

    pub fn preview_display(&self, preview: Option<&Preferences>) {
        *self
            .display_preview
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = preview.cloned().map(Arc::new);
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
