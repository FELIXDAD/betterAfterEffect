//! Asset collection: every `Asset` prim of a project, which of them the timeline
//! uses (directly or through nested comps), and the fonts text layers need.
//!
//! "Used" follows AE's *Collect Files*: with no roots, every comp, layer, clip or
//! panel of the project counts, so an asset is used when any prim references it.
//! With roots (selected comps), only those prims and whatever they reference —
//! pre-comps recursively — count. A reference is an `asset_ref` / `prim_ref` value,
//! static or keyframed, in the winning opinion of an attribute.

use std::collections::{BTreeMap, HashSet};

use bae_model::{AssetFile, AttrSpec, PrimPath, PrimType, PrimView, Stage, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Attribute names read from `Asset` prims besides the file reference
/// (`bae_model::asset::attrs`).
pub mod attrs {
    pub const NAME: &str = "name";
    pub const KIND: &str = "kind";
    pub const SEQUENCE_PATTERN: &str = "sequence:pattern";
    pub const SEQUENCE_FIRST_FRAME: &str = "sequence:first_frame";
}

/// The `kind` of an `Asset` prim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    /// One file: a movie, a still, an audio file.
    Footage,
    /// An image sequence: one file per frame.
    Sequence,
    /// A solid color (no file).
    Solid,
    /// A placeholder for footage that does not exist yet (no file).
    Placeholder,
    /// A kind this build does not know (plugin assets); packed like footage.
    Other,
}

impl AssetKind {
    fn parse(token: Option<&str>) -> Self {
        match token {
            None | Some("footage") => AssetKind::Footage,
            Some("sequence") => AssetKind::Sequence,
            Some("solid") => AssetKind::Solid,
            Some("placeholder") => AssetKind::Placeholder,
            Some(_) => AssetKind::Other,
        }
    }
}

/// How an image sequence names its frames.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SequencePattern {
    /// File name with one `#` per padded digit, e.g. `shot_####.png`.
    pub pattern: Option<String>,
    /// First frame number.
    pub first_frame: i64,
}

/// One `Asset` prim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetEntry {
    pub path: PrimPath,
    /// Display name (`name` attribute).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub kind: AssetKind,
    /// The file reference; `None` for solids, placeholders and assets that never
    /// had a file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<AssetFile>,
    /// For image sequences.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<SequencePattern>,
    /// Prims (in scope) that reference this asset: layers, clips, panels …
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<PrimPath>,
}

impl AssetEntry {
    /// Read an asset prim.
    pub fn from_prim(prim: &PrimView<'_>) -> Self {
        let s = |n| prim.value(n).and_then(Value::as_str).map(str::to_owned);
        let kind = AssetKind::parse(prim.value(attrs::KIND).and_then(Value::as_str));
        let file = AssetFile::from_prim(prim);
        let sequence = (kind == AssetKind::Sequence).then(|| SequencePattern {
            pattern: s(attrs::SEQUENCE_PATTERN).filter(|p| !p.is_empty()),
            first_frame: match prim.value(attrs::SEQUENCE_FIRST_FRAME) {
                Some(Value::Int(v)) => *v,
                _ => 0,
            },
        });
        AssetEntry {
            path: prim.path().clone(),
            name: s(attrs::NAME),
            kind,
            file,
            sequence,
            users: Vec::new(),
        }
    }

    /// Whether anything in scope references the asset.
    pub fn is_used(&self) -> bool {
        !self.users.is_empty()
    }

    /// Whether the asset is backed by files (footage, sequences, plugin kinds).
    pub fn has_file(&self) -> bool {
        self.file.is_some()
    }
}

/// A font used by text layers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FontUse {
    /// PostScript name, as stored in the text document.
    pub name: String,
    /// Text layers (in scope) that use it.
    pub users: Vec<PrimPath>,
}

/// What a project needs from outside its project data.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Inventory {
    /// Every `Asset` prim of the project, in project order, used or not.
    pub assets: Vec<AssetEntry>,
    /// Fonts used in scope, sorted by name.
    pub fonts: Vec<FontUse>,
    /// Other projects referenced in scope (`Reference::project`). Packaging does
    /// not embed them.
    pub external_projects: Vec<String>,
}

impl Inventory {
    pub fn asset(&self, path: &PrimPath) -> Option<&AssetEntry> {
        self.assets.iter().find(|a| &a.path == path)
    }

    pub fn used(&self) -> impl Iterator<Item = &AssetEntry> {
        self.assets.iter().filter(|a| a.is_used())
    }

    pub fn unused(&self) -> impl Iterator<Item = &AssetEntry> {
        self.assets.iter().filter(|a| !a.is_used())
    }
}

/// Scope of a collection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CollectOptions {
    /// Collect for these prims (usually comps) and everything they reference,
    /// recursively. Empty: the whole project.
    #[serde(default)]
    pub roots: Vec<PrimPath>,
}

/// Find the project's assets and fonts and who uses them.
pub fn collect(stage: &Stage, options: &CollectOptions) -> Inventory {
    let all = stage.traverse();
    let mut assets: Vec<AssetEntry> = Vec::new();
    let mut asset_index: BTreeMap<PrimPath, usize> = BTreeMap::new();
    for path in &all {
        if let Some(view) = stage.prim(path)
            && view.is_a(&PrimType::ASSET)
        {
            asset_index.insert(path.clone(), assets.len());
            assets.push(AssetEntry::from_prim(&view));
        }
    }

    let mut fonts: BTreeMap<String, Vec<PrimPath>> = BTreeMap::new();
    let mut external: Vec<String> = Vec::new();
    let mut visit = |view: &PrimView<'_>, follow: &mut Vec<PrimPath>| {
        for r in view.references() {
            if let Some(key) = &r.project
                && !external.contains(key)
            {
                external.push(key.clone());
            }
        }
        for name in view.attr_names() {
            let Some(spec) = view.attr(name) else {
                continue;
            };
            for value in values(spec) {
                match value {
                    Value::AssetRef(target) | Value::PrimRef(target) => {
                        if let Some(&i) = asset_index.get(target) {
                            let users = &mut assets[i].users;
                            if !users.contains(view.path()) {
                                users.push(view.path().clone());
                            }
                        } else {
                            follow.push(target.clone());
                        }
                    }
                    Value::TextDocument(doc) if !doc.font.is_empty() => {
                        let users = fonts.entry(doc.font.clone()).or_default();
                        if !users.contains(view.path()) {
                            users.push(view.path().clone());
                        }
                    }
                    _ => {}
                }
            }
        }
    };

    if options.roots.is_empty() {
        let mut ignored = Vec::new();
        for path in &all {
            if asset_index.contains_key(path) {
                continue;
            }
            if let Some(view) = stage.prim(path) {
                visit(&view, &mut ignored);
            }
            ignored.clear();
        }
    } else {
        // Breadth-first over subtrees: roots first, then whatever they reference
        // (pre-comps, nested pre-comps …). Each subtree is scanned once.
        let mut queue: Vec<PrimPath> = options.roots.clone();
        let mut scanned: HashSet<PrimPath> = HashSet::new();
        while let Some(root) = queue.pop() {
            if asset_index.contains_key(&root)
                || scanned.iter().any(|s| s.is_prefix_of(&root))
                || !stage.has_prim(&root)
            {
                continue;
            }
            scanned.insert(root.clone());
            let mut stack = vec![root];
            let mut follow = Vec::new();
            while let Some(path) = stack.pop() {
                if let Some(view) = stage.prim(&path) {
                    visit(&view, &mut follow);
                }
                stack.extend(stage.children(&path));
            }
            queue.extend(follow);
        }
    }

    external.sort();
    Inventory {
        assets,
        fonts: fonts
            .into_iter()
            .map(|(name, users)| FontUse { name, users })
            .collect(),
        external_projects: external,
    }
}

/// The static value and every keyframe value of an opinion.
fn values(spec: &AttrSpec) -> impl Iterator<Item = &Value> {
    spec.value
        .iter()
        .chain(spec.keyframes.values().map(|k| &k.value))
}
