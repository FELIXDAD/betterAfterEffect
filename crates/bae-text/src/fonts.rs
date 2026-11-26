//! Fonts: system font enumeration (AE-TXT-081), matching a stored font name to
//! an installed face (AE-TXT-082), missing-font substitution (AE-TXT-083) and
//! the locale used for CJK fallback (AE-TXT-045/093).
//!
//! Fonts come from the operating system through fontique (no bundled fonts,
//! ARCHITECTURE §8). Project fonts and test fonts can be added with
//! [`FontSystem::register_font_data`].

use std::sync::Arc;

use fontique::{Blob, Collection, CollectionOptions, FontStyle, SourceCache};
use parley::{FontContext, LayoutContext};
use rustc_hash::FxHashMap;
use skrifa::raw::TableProvider;
use skrifa::{MetadataProvider, string::StringId};

/// One installed font face.
#[derive(Clone, Debug, PartialEq)]
pub struct FontFace {
    /// Family name as the OS reports it (e.g. `PingFang SC`).
    pub family: String,
    /// PostScript name (e.g. `PingFangSC-Regular`); what `TextDocument.font` stores.
    pub postscript_name: String,
    /// CSS weight (400 regular, 700 bold).
    pub weight: f32,
    pub italic: bool,
    /// Width ratio (1.0 normal).
    pub width: f32,
}

/// How a stored font name resolved.
#[derive(Clone, Debug, PartialEq)]
pub enum FontMatch {
    /// An installed face with that PostScript name (or family name).
    Found(FontFace),
    /// Not installed: the default font for the script is used instead and the
    /// stored name is kept (AE-TXT-083).
    Missing,
}

/// The default locale for CJK fallback: Simplified Chinese (BAE decision,
/// AE-TXT-093). Han characters in a Latin font fall back to PingFang SC on
/// macOS and Microsoft YaHei on Windows.
pub const DEFAULT_LOCALE: &str = "zh-Hans";

/// Font database plus the reusable layout scratch space.
///
/// Not thread-safe; keep one per thread or behind a lock. Creating one scans the
/// system fonts (fast; files are read lazily).
pub struct FontSystem {
    pub(crate) fcx: FontContext,
    pub(crate) lcx: LayoutContext<()>,
    locale: String,
    /// Stored name (lowercase) → resolution.
    resolved: FxHashMap<String, FontMatch>,
    /// Lowercase PostScript name → face, built on the first miss of the
    /// family-name heuristic.
    postscript_index: Option<FxHashMap<String, FontFace>>,
}

impl std::fmt::Debug for FontSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontSystem")
            .field("locale", &self.locale)
            .finish_non_exhaustive()
    }
}

impl Default for FontSystem {
    fn default() -> Self {
        Self::new()
    }
}

fn face_of(family: &str, info: &fontique::FontInfo, postscript_name: String) -> FontFace {
    FontFace {
        family: family.to_owned(),
        postscript_name,
        weight: info.weight().value(),
        italic: !matches!(info.style(), FontStyle::Normal),
        width: info.width().ratio(),
    }
}

/// The PostScript name of face `index` in `data` (name ID 6).
pub fn postscript_name(data: &[u8], index: u32) -> Option<String> {
    let font = skrifa::FontRef::from_index(data, index).ok()?;
    let name = font
        .localized_strings(StringId::POSTSCRIPT_NAME)
        .english_or_first()?
        .to_string();
    (!name.is_empty()).then_some(name)
}

/// `unitsPerEm` of face `index` in `data`.
pub fn units_per_em(data: &[u8], index: u32) -> Option<u16> {
    let font = skrifa::FontRef::from_index(data, index).ok()?;
    font.head().ok().map(|h| h.units_per_em())
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

impl FontSystem {
    /// System fonts, CJK fallback for [`DEFAULT_LOCALE`].
    pub fn new() -> Self {
        Self::with_collection(Collection::new(CollectionOptions {
            shared: false,
            system_fonts: true,
        }))
    }

    /// No system fonts: only fonts added with [`register_font_data`](Self::register_font_data)
    /// (deterministic tests, sandboxed renders).
    pub fn without_system_fonts() -> Self {
        Self::with_collection(Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }))
    }

    fn with_collection(collection: Collection) -> Self {
        Self {
            fcx: FontContext {
                collection,
                source_cache: SourceCache::default(),
            },
            lcx: LayoutContext::new(),
            locale: DEFAULT_LOCALE.to_owned(),
            resolved: FxHashMap::default(),
            postscript_index: None,
        }
    }

    /// The locale used to pick fallback fonts (BCP 47, e.g. `zh-Hans`, `zh-Hant`, `ja`).
    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// Changes the fallback locale.
    pub fn set_locale(&mut self, locale: &str) {
        self.locale = locale.to_owned();
    }

    /// Adds font files (TTF/OTF/TTC data); returns the faces they contain.
    pub fn register_font_data(&mut self, data: Vec<u8>) -> Vec<FontFace> {
        let blob = Blob::new(Arc::new(data));
        let registered = self.fcx.collection.register_fonts(blob, None);
        self.resolved.clear();
        self.postscript_index = None;
        let mut out = Vec::new();
        for (family_id, _) in registered {
            let Some(family) = self.fcx.collection.family(family_id) else {
                continue;
            };
            for info in family.fonts() {
                if let Some(data) = info.load(Some(&mut self.fcx.source_cache))
                    && let Some(ps) = postscript_name(data.as_ref(), info.index())
                {
                    let face = face_of(family.name(), info, ps);
                    if !out.contains(&face) {
                        out.push(face);
                    }
                }
            }
        }
        out
    }

    /// Every family name, sorted, without duplicates (Character panel list).
    pub fn families(&mut self) -> Vec<String> {
        let mut names: Vec<String> = self
            .fcx
            .collection
            .family_names()
            .map(str::to_owned)
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    }

    /// The faces of one family with their PostScript names (reads the font files).
    pub fn faces(&mut self, family: &str) -> Vec<FontFace> {
        let Some(info) = self.fcx.collection.family_by_name(family) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for font in info.fonts() {
            if let Some(data) = font.load(Some(&mut self.fcx.source_cache))
                && let Some(ps) = postscript_name(data.as_ref(), font.index())
            {
                out.push(face_of(info.name(), font, ps));
            }
        }
        out
    }

    /// Resolves a stored font name (PostScript name, as AE stores it, or a family
    /// name) to an installed face. Results are cached.
    pub fn resolve(&mut self, name: &str) -> FontMatch {
        let key = name.to_lowercase();
        if let Some(hit) = self.resolved.get(&key) {
            return hit.clone();
        }
        let result = self.resolve_uncached(name);
        self.resolved.insert(key, result.clone());
        result
    }

    fn resolve_uncached(&mut self, name: &str) -> FontMatch {
        if name.trim().is_empty() {
            return FontMatch::Missing;
        }
        // 1. A family name: its default (regular) face.
        if let Some(info) = self.fcx.collection.family_by_name(name)
            && let Some(font) = info.default_font()
        {
            let ps = font
                .load(Some(&mut self.fcx.source_cache))
                .and_then(|d| postscript_name(d.as_ref(), font.index()))
                .unwrap_or_else(|| name.to_owned());
            return FontMatch::Found(face_of(info.name(), font, ps));
        }
        // 2. PostScript names usually start with the family name without spaces
        //    (`PingFangSC-Regular`, `HelveticaNeue-Light`, `ArialMT`): check those
        //    families' faces first.
        let wanted = name.to_lowercase();
        let prefix = squash(name.split('-').next().unwrap_or(name));
        let stripped = prefix
            .strip_suffix("psmt")
            .or_else(|| prefix.strip_suffix("mt"))
            .or_else(|| prefix.strip_suffix("ps"))
            .unwrap_or(&prefix)
            .to_owned();
        let candidates: Vec<String> = self
            .fcx
            .collection
            .family_names()
            .filter(|f| {
                let s = squash(f);
                s == prefix || s == stripped
            })
            .map(str::to_owned)
            .collect();
        for family in candidates {
            if let Some(face) = self
                .faces(&family)
                .into_iter()
                .find(|f| f.postscript_name.to_lowercase() == wanted)
            {
                return FontMatch::Found(face);
            }
        }
        // 3. Every installed face (built once).
        if self.postscript_index.is_none() {
            self.postscript_index = Some(self.build_postscript_index());
        }
        match self.postscript_index.as_ref().and_then(|i| i.get(&wanted)) {
            Some(face) => FontMatch::Found(face.clone()),
            None => FontMatch::Missing,
        }
    }

    fn build_postscript_index(&mut self) -> FxHashMap<String, FontFace> {
        let mut index = FxHashMap::default();
        let families: Vec<String> = self
            .fcx
            .collection
            .family_names()
            .map(str::to_owned)
            .collect();
        for family in families {
            for face in self.faces(&family) {
                index
                    .entry(face.postscript_name.to_lowercase())
                    .or_insert(face);
            }
        }
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squash_ignores_case_spaces_and_dashes() {
        assert_eq!(squash("PingFang SC"), "pingfangsc");
        assert_eq!(squash("Helvetica-Neue"), "helveticaneue");
    }

    #[test]
    fn empty_collection_reports_missing() {
        let mut fs = FontSystem::without_system_fonts();
        assert!(fs.families().is_empty());
        assert_eq!(fs.resolve("ArialMT"), FontMatch::Missing);
        assert_eq!(fs.resolve(""), FontMatch::Missing);
        assert_eq!(fs.locale(), DEFAULT_LOCALE);
    }
}
