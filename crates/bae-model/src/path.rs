//! Prim paths, e.g. `/comps/main/layers/title`.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Error for a malformed path or prim name.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("prim path must be absolute (start with '/'): {0:?}")]
    NotAbsolute(String),
    #[error("invalid prim name {0:?}: names are non-empty and use letters, digits, '_' or '-'")]
    InvalidName(String),
}

/// An absolute prim path. `/` is the pseudo-root that every top-level prim hangs from.
///
/// Prim names are identifiers (letters, digits, `_`, `-`; any Unicode letter is allowed)
/// so that paths stay unambiguous. The user-visible name of a layer or comp, which
/// may contain spaces, slashes or duplicates, is an attribute, not the prim name.
///
/// Cloning is cheap (shared string).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PrimPath(Arc<str>);

impl PrimPath {
    /// The pseudo-root `/`.
    pub fn root() -> Self {
        thread_local! {
            static ROOT: PrimPath = PrimPath(Arc::from("/"));
        }
        ROOT.with(Clone::clone)
    }

    /// Parse and validate an absolute path. A trailing `/` is not allowed (except for the root).
    pub fn new(s: &str) -> Result<Self, PathError> {
        if !s.starts_with('/') {
            return Err(PathError::NotAbsolute(s.to_owned()));
        }
        if s == "/" {
            return Ok(Self::root());
        }
        for seg in s[1..].split('/') {
            validate_name(seg)?;
        }
        Ok(PrimPath(Arc::from(s)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        &*self.0 == "/"
    }

    /// Last segment; empty for the root.
    pub fn name(&self) -> &str {
        match self.0.rfind('/') {
            Some(i) => &self.0[i + 1..],
            None => "",
        }
    }

    /// Parent path; `None` for the root.
    pub fn parent(&self) -> Option<PrimPath> {
        if self.is_root() {
            return None;
        }
        let i = self.0.rfind('/').expect("absolute path");
        if i == 0 {
            Some(Self::root())
        } else {
            Some(PrimPath(Arc::from(&self.0[..i])))
        }
    }

    /// Child path `self/name`.
    pub fn child(&self, name: &str) -> Result<PrimPath, PathError> {
        validate_name(name)?;
        Ok(self.child_unchecked(name))
    }

    pub(crate) fn child_unchecked(&self, name: &str) -> PrimPath {
        let mut s = String::with_capacity(self.0.len() + 1 + name.len());
        s.push_str(&self.0);
        if !self.is_root() {
            s.push('/');
        }
        s.push_str(name);
        PrimPath(Arc::from(s))
    }

    /// Number of segments (root = 0).
    pub fn depth(&self) -> usize {
        if self.is_root() {
            0
        } else {
            self.0.matches('/').count()
        }
    }

    /// Path segments, root first.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/').filter(|s| !s.is_empty())
    }

    /// True if `self` is `other` or one of its ancestors.
    pub fn is_prefix_of(&self, other: &PrimPath) -> bool {
        if self.is_root() {
            return true;
        }
        other.0.starts_with(&*self.0)
            && (other.0.len() == self.0.len() || other.0.as_bytes()[self.0.len()] == b'/')
    }

    /// True if `self` is a strict ancestor of `other`.
    pub fn is_ancestor_of(&self, other: &PrimPath) -> bool {
        self != other && self.is_prefix_of(other)
    }

    /// Ancestors from the parent up to (and including) the root.
    pub fn ancestors(&self) -> impl Iterator<Item = PrimPath> {
        std::iter::successors(self.parent(), PrimPath::parent)
    }

    /// Replace the prefix `from` with `to`. `None` if `from` is not a prefix of `self`.
    pub fn reparent(&self, from: &PrimPath, to: &PrimPath) -> Option<PrimPath> {
        if !from.is_prefix_of(self) {
            return None;
        }
        let rest = &self.0[if from.is_root() { 0 } else { from.0.len() }..];
        if rest.is_empty() {
            return Some(to.clone());
        }
        let rest = rest.trim_start_matches('/');
        Some(to.child_unchecked(rest))
    }
}

/// Validate a single prim name segment.
pub fn validate_name(name: &str) -> Result<(), PathError> {
    let ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        && !name.starts_with('-');
    if ok {
        Ok(())
    } else {
        Err(PathError::InvalidName(name.to_owned()))
    }
}

impl fmt::Display for PrimPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for PrimPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PrimPath({})", self.0)
    }
}

impl FromStr for PrimPath {
    type Err = PathError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        PrimPath::new(s)
    }
}

impl TryFrom<&str> for PrimPath {
    type Error = PathError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        PrimPath::new(s)
    }
}

impl AsRef<str> for PrimPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Serialize for PrimPath {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PrimPath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = Cow::<str>::deserialize(d)?;
        PrimPath::new(&s).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for PrimPath {
    fn schema_name() -> Cow<'static, str> {
        "PrimPath".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^/([\\p{L}\\p{N}_][\\p{L}\\p{N}_-]*(/[\\p{L}\\p{N}_][\\p{L}\\p{N}_-]*)*)?$",
            "description": "Absolute prim path, e.g. /comps/main/layers/title",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PrimPath {
        PrimPath::new(s).unwrap()
    }

    #[test]
    fn parse_and_navigate() {
        let path = p("/comps/main/layers/标题_1");
        assert_eq!(path.name(), "标题_1");
        assert_eq!(path.depth(), 4);
        assert_eq!(path.parent().unwrap(), p("/comps/main/layers"));
        assert_eq!(p("/comps").parent().unwrap(), PrimPath::root());
        assert_eq!(PrimPath::root().parent(), None);
        assert_eq!(PrimPath::root().child("a").unwrap(), p("/a"));
        assert_eq!(path.ancestors().count(), 4);
        assert_eq!(
            path.segments().collect::<Vec<_>>(),
            ["comps", "main", "layers", "标题_1"]
        );
    }

    #[test]
    fn rejects_bad_paths() {
        for bad in ["", "a", "/a/", "//a", "/a b", "/a.b", "/-a", "/a/../b"] {
            assert!(PrimPath::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn prefixes() {
        assert!(p("/a").is_prefix_of(&p("/a/b")));
        assert!(!p("/a").is_prefix_of(&p("/ab")));
        assert!(PrimPath::root().is_ancestor_of(&p("/a")));
        assert!(!p("/a").is_ancestor_of(&p("/a")));
        assert_eq!(
            p("/a/b/c").reparent(&p("/a"), &p("/x/y")).unwrap(),
            p("/x/y/b/c")
        );
        assert_eq!(p("/a").reparent(&p("/a"), &p("/z")).unwrap(), p("/z"));
        assert_eq!(
            p("/a").reparent(&PrimPath::root(), &p("/z")).unwrap(),
            p("/z/a")
        );
        assert_eq!(p("/ab").reparent(&p("/a"), &p("/z")), None);
    }

    #[test]
    fn serde_roundtrip() {
        let path = p("/a/b");
        let json = serde_json::to_string(&path).unwrap();
        assert_eq!(json, "\"/a/b\"");
        assert_eq!(serde_json::from_str::<PrimPath>(&json).unwrap(), path);
        assert!(serde_json::from_str::<PrimPath>("\"a/b\"").is_err());
    }
}
