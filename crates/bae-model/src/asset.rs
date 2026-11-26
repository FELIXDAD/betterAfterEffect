//! Asset file references (ARCHITECTURE §16).
//!
//! An `Asset` prim records where its file is in three ways: relative to the project,
//! the original absolute path, and a fingerprint (size, modification time, optional
//! content hash). [`AssetResolver`] looks for the file in that order, then applies
//! team path mappings, then searches by name and fingerprint; anything still not
//! found is *missing* and can be relinked. Relinking one asset yields a
//! [`PathMapping`] ([`infer_mapping`]) that usually finds the others too.
//!
//! All stored paths use `/` separators; Windows drive letters stay (`Z:/footage`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::compose::PrimView;
use crate::op::Op;
use crate::path::PrimPath;
use crate::value::Value;

/// Attribute names on `Asset` prims.
pub mod attrs {
    pub const RELATIVE: &str = "file:relative";
    pub const ORIGINAL: &str = "file:original";
    pub const SIZE: &str = "file:size";
    pub const MTIME: &str = "file:mtime";
    pub const HASH: &str = "file:hash";
}

/// Incremented by Replace Footage, including a reload of the same path.
pub const FILE_REVISION: &str = "file:revision";
pub fn file_revision(prim: &crate::PrimView<'_>) -> i64 {
    match prim.value(FILE_REVISION) {
        Some(crate::Value::Int(n)) => *n,
        _ => 0,
    }
}

/// Where an asset's file is, and how to recognize it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetFile {
    /// Path relative to the project's base directory (`/`-separated), if it is on
    /// the same volume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative: Option<String>,
    /// Absolute path when the asset was imported (`/`-separated).
    pub original: String,
    /// File size in bytes.
    pub size: u64,
    /// Modification time, milliseconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_ms: Option<i64>,
    /// BLAKE3 of the content, hex (optional; expensive for large media).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

impl AssetFile {
    /// Fingerprint a file on disk. `base_dir` is the project's base directory.
    pub fn from_path(
        file: &Path,
        base_dir: Option<&Path>,
        with_hash: bool,
    ) -> std::io::Result<Self> {
        let abs = fs::canonicalize(file)?;
        let meta = fs::metadata(&abs)?;
        let relative = base_dir
            .and_then(|b| fs::canonicalize(b).ok())
            .and_then(|b| relative_path(&b, &abs));
        Ok(AssetFile {
            relative,
            original: to_slash(&abs),
            size: meta.len(),
            mtime_ms: mtime_ms(&meta),
            hash: if with_hash {
                Some(hash_file(&abs)?)
            } else {
                None
            },
        })
    }

    /// Read from an `Asset` prim's attributes.
    pub fn from_prim(prim: &PrimView<'_>) -> Option<Self> {
        let s = |n| prim.value(n).and_then(Value::as_str).map(str::to_owned);
        let i = |n| match prim.value(n) {
            Some(Value::Int(v)) => Some(*v),
            _ => None,
        };
        Some(AssetFile {
            relative: s(attrs::RELATIVE),
            original: s(attrs::ORIGINAL)?,
            size: i(attrs::SIZE).unwrap_or(0).max(0) as u64,
            mtime_ms: i(attrs::MTIME),
            hash: s(attrs::HASH),
        })
    }

    /// Ops that write this reference onto the asset prim at `path`.
    pub fn to_ops(&self, path: &PrimPath) -> Vec<Op> {
        let set = |attr: &str, value: Option<Value>| Op::SetValue {
            path: path.clone(),
            attr: attr.to_owned(),
            value,
        };
        vec![
            set(attrs::RELATIVE, self.relative.clone().map(Value::String)),
            set(attrs::ORIGINAL, Some(Value::String(self.original.clone()))),
            set(attrs::SIZE, Some(Value::Int(self.size as i64))),
            set(attrs::MTIME, self.mtime_ms.map(Value::Int)),
            set(attrs::HASH, self.hash.clone().map(Value::String)),
        ]
    }

    pub fn file_name(&self) -> &str {
        self.original.rsplit('/').next().unwrap_or(&self.original)
    }
}

/// Replace a path prefix: `from` (as stored, `/`-separated) → `to`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PathMapping {
    pub from: String,
    pub to: String,
}

impl PathMapping {
    pub fn apply(&self, path: &str) -> Option<String> {
        let rest = strip_prefix_ci(path, &self.from)?;
        let mut out = self.to.trim_end_matches('/').to_owned();
        if !rest.is_empty() {
            out.push('/');
            out.push_str(rest.trim_start_matches('/'));
        }
        Some(out)
    }
}

/// How a file was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Relative(PathBuf),
    Original(PathBuf),
    Mapped(PathBuf),
    /// Found by name and fingerprint in a search directory.
    Searched(PathBuf),
    Missing,
}

impl Resolution {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Resolution::Relative(p)
            | Resolution::Original(p)
            | Resolution::Mapped(p)
            | Resolution::Searched(p) => Some(p),
            Resolution::Missing => None,
        }
    }

    pub fn is_missing(&self) -> bool {
        matches!(self, Resolution::Missing)
    }
}

/// Finds asset files (ARCHITECTURE §16 lookup order).
#[derive(Clone, Debug, Default)]
pub struct AssetResolver {
    /// Project base directory (relative paths resolve against it).
    pub base_dir: Option<PathBuf>,
    /// Project and team path mappings, tried in order.
    pub mappings: Vec<PathMapping>,
    /// Extra directories searched by name + fingerprint (recent folders, the
    /// project directory is always searched).
    pub search_dirs: Vec<PathBuf>,
    /// Directory depth for the name search.
    pub search_depth: usize,
}

impl AssetResolver {
    pub fn new(base_dir: Option<PathBuf>, mappings: Vec<PathMapping>) -> Self {
        AssetResolver {
            base_dir,
            mappings,
            search_dirs: Vec::new(),
            search_depth: 4,
        }
    }

    pub fn resolve(&self, file: &AssetFile) -> Resolution {
        if let (Some(base), Some(rel)) = (&self.base_dir, &file.relative) {
            let p = base.join(from_slash(rel));
            if p.is_file() {
                return Resolution::Relative(p);
            }
        }
        let original = PathBuf::from(from_slash(&file.original));
        if original.is_file() {
            return Resolution::Original(original);
        }
        for m in &self.mappings {
            if let Some(mapped) = m.apply(&file.original) {
                let p = PathBuf::from(from_slash(&mapped));
                if p.is_file() {
                    return Resolution::Mapped(p);
                }
            }
        }
        let dirs = self.base_dir.iter().chain(&self.search_dirs);
        let mut budget = 20_000usize;
        for dir in dirs {
            if let Some(p) = search(dir, file, self.search_depth, &mut budget) {
                return Resolution::Searched(p);
            }
        }
        Resolution::Missing
    }
}

/// The mapping implied by relinking `original` (as stored) to `found`: the longest
/// common tail of path segments is kept, the differing heads become the mapping.
/// `None` if only the file name matches nothing or the paths are identical.
pub fn infer_mapping(original: &str, found: &Path) -> Option<PathMapping> {
    let found = to_slash(found);
    let a: Vec<&str> = original.split('/').collect();
    let b: Vec<&str> = found.split('/').collect();
    let common = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x.eq_ignore_ascii_case(y))
        .count();
    if common == 0 || common == a.len().min(b.len()) && a.len() == b.len() {
        return None;
    }
    let from = a[..a.len() - common].join("/") + "/";
    let to = b[..b.len() - common].join("/") + "/";
    Some(PathMapping { from, to })
}

fn search(dir: &Path, file: &AssetFile, depth: usize, budget: &mut usize) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            subdirs.push(path);
        } else if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(file.file_name())
            && matches(&path, file)
        {
            return Some(path);
        }
    }
    if depth == 0 {
        return None;
    }
    subdirs.sort();
    subdirs
        .into_iter()
        .find_map(|d| search(&d, file, depth - 1, budget))
}

/// Fingerprint check: size must match; the hash too when recorded.
fn matches(path: &Path, file: &AssetFile) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if meta.len() != file.size {
        return false;
    }
    match &file.hash {
        Some(h) => hash_file(path).is_ok_and(|x| &x == h),
        None => true,
    }
}

fn mtime_ms(meta: &fs::Metadata) -> Option<i64> {
    let d = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(d.as_millis()).ok()
}

/// BLAKE3 of a file, hex.
pub fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut f = fs::File::open(path)?;
    std::io::copy(&mut f, &mut hasher)?;
    Ok(hasher.finalize().to_hex().to_string())
}

/// A path as stored: `/`-separated.
pub fn to_slash(path: &Path) -> String {
    let s = path.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.replace('\\', "/")
}

fn from_slash(s: &str) -> String {
    if cfg!(windows) {
        s.replace('/', "\\")
    } else {
        s.to_owned()
    }
}

fn relative_path(base: &Path, path: &Path) -> Option<String> {
    let base: Vec<_> = base.components().collect();
    let path_c: Vec<_> = path.components().collect();
    // Different roots (other drive / volume): no relative path.
    if base.first() != path_c.first() {
        return None;
    }
    let common = base.iter().zip(&path_c).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> =
        std::iter::repeat_n("..".to_owned(), base.len() - common).collect();
    parts.extend(
        path_c[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    Some(parts.join("/"))
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let prefix = prefix.trim_end_matches('/');
    if s.len() < prefix.len() || !s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        return None;
    }
    let rest = &s[prefix.len()..];
    (rest.is_empty() || rest.starts_with('/')).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_apply_and_infer() {
        let m = PathMapping {
            from: "Z:/".into(),
            to: "/Volumes/NAS/".into(),
        };
        assert_eq!(m.apply("Z:/proj/a.mov").unwrap(), "/Volumes/NAS/proj/a.mov");
        assert_eq!(m.apply("z:/proj").unwrap(), "/Volumes/NAS/proj");
        assert_eq!(m.apply("Y:/proj"), None);
        let inferred = infer_mapping(
            "Z:/proj/footage/a.mov",
            Path::new("/Volumes/NAS/proj/footage/a.mov"),
        )
        .unwrap();
        assert_eq!(inferred.from, "Z:/");
        assert_eq!(inferred.to, "/Volumes/NAS/");
        assert_eq!(
            inferred.apply("Z:/proj/footage/b.mov").unwrap(),
            "/Volumes/NAS/proj/footage/b.mov"
        );
        assert_eq!(infer_mapping("/a/b.mov", Path::new("/a/b.mov")), None);
    }

    #[test]
    fn relative_paths() {
        assert_eq!(
            relative_path(Path::new("/p/film.bae"), Path::new("/p/footage/a.mov")).unwrap(),
            "../footage/a.mov"
        );
        assert_eq!(
            relative_path(Path::new("/p"), Path::new("/p/a.mov")).unwrap(),
            "a.mov"
        );
    }
}
