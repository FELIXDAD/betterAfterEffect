//! Prim types and per-sublayer prim specs.

use std::borrow::Cow;
use std::fmt;

use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::attr::AttrSpec;
use crate::path::PrimPath;

/// The type of a prim. Built-in types are the associated constants; plugins register
/// their own types as `"<plugin-id>:<Type>"` (e.g. `"com.example.particles:Emitter"`).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PrimType(Cow<'static, str>);

macro_rules! prim_types {
    ($($(#[$m:meta])* $konst:ident = $name:literal;)*) => {
        impl PrimType {
            $( $(#[$m])* pub const $konst: PrimType = PrimType(Cow::Borrowed($name)); )*

            /// All built-in prim types.
            pub const BUILTIN: &'static [PrimType] = &[$(PrimType::$konst),*];
        }
    };
}

prim_types! {
    /// Project settings (one per project, at `/project`).
    PROJECT = "Project";
    /// Project panel folder.
    FOLDER = "Folder";
    /// Composition: size, frame rate, duration, background color, work area.
    COMP = "Comp";
    /// A layer inside a comp; its kind is the `kind` attribute ([`LayerKind`]).
    LAYER = "Layer";
    SHAPE_GROUP = "ShapeGroup";
    PATH = "Path";
    RECT = "Rect";
    ELLIPSE = "Ellipse";
    STAR = "Star";
    FILL = "Fill";
    STROKE = "Stroke";
    GRADIENT_FILL = "GradientFill";
    GRADIENT_STROKE = "GradientStroke";
    /// Shape operator (merge paths, trim paths, repeater, offset, round corners …);
    /// the operator is the `operator` attribute.
    SHAPE_OPERATOR = "ShapeOperator";
    MASK = "Mask";
    MOTION_TRACKER = "MotionTracker";
    TRACK_POINT = "TrackPoint";
    /// Comp or layer marker (`time` attribute; layer markers use layer time).
    MARKER = "Marker";
    /// Text animator group under a text layer.
    TEXT_ANIMATOR = "TextAnimator";
    /// Range / wiggly / expression selector under a text animator.
    TEXT_SELECTOR = "TextSelector";
    /// Effect instance; `match_name` attribute names the effect.
    EFFECT = "Effect";
    /// One brush, clone or eraser stroke under a Paint effect (`ADBE Paint`).
    PAINT_STROKE = "PaintStroke";
    ARTBOARD = "Artboard";
    BOARD = "Board";
    PANEL = "Panel";
    SHOT = "Shot";
    SEQUENCE = "Sequence";
    TRACK = "Track";
    CLIP = "Clip";
    ASSET = "Asset";
}

impl PrimType {
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        PrimType(name.into())
    }

    /// Parse a type name, reusing the built-in constant when there is one.
    pub fn parse(name: &str) -> Self {
        PrimType::BUILTIN
            .iter()
            .find(|t| t.as_str() == name)
            .cloned()
            .unwrap_or_else(|| PrimType(Cow::Owned(name.to_owned())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Built-in types are the constants on [`PrimType`]; everything else is a plugin type.
    pub fn is_builtin(&self) -> bool {
        Self::BUILTIN.contains(self)
    }

    /// Plugin ID for plugin-defined types (`"<plugin-id>:<Type>"`).
    pub fn plugin_id(&self) -> Option<&str> {
        self.0.rsplit_once(':').map(|(id, _)| id)
    }
}

impl fmt::Display for PrimType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for PrimType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PrimType({})", self.0)
    }
}

impl Serialize for PrimType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PrimType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = Cow::<str>::deserialize(d)?;
        Ok(PrimType::parse(&s))
    }
}

impl JsonSchema for PrimType {
    fn schema_name() -> Cow<'static, str> {
        "PrimType".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let builtin: Vec<&str> = PrimType::BUILTIN.iter().map(PrimType::as_str).collect();
        json_schema!({
            "type": "string",
            "description": format!(
                "Prim type. Built-in: {}. Plugin types are \"<plugin-id>:<Type>\".",
                builtin.join(", ")
            ),
        })
    }
}

/// Layer kinds (value of the `kind` attribute on `Layer` prims).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    Shape,
    Text,
    Solid,
    Footage,
    Null,
    Camera,
    Light,
    Adjustment,
    PreComp,
    Audio,
}

impl LayerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LayerKind::Shape => "shape",
            LayerKind::Text => "text",
            LayerKind::Solid => "solid",
            LayerKind::Footage => "footage",
            LayerKind::Null => "null",
            LayerKind::Camera => "camera",
            LayerKind::Light => "light",
            LayerKind::Adjustment => "adjustment",
            LayerKind::PreComp => "pre_comp",
            LayerKind::Audio => "audio",
        }
    }
}

/// Whether a spec defines a prim or only overrides one defined in a weaker sublayer
/// (USD `def` / `over`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Specifier {
    #[default]
    Def,
    Over,
}

impl Specifier {
    pub fn as_str(self) -> &'static str {
        match self {
            Specifier::Def => "def",
            Specifier::Over => "over",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "def" => Some(Specifier::Def),
            "over" => Some(Specifier::Over),
            _ => None,
        }
    }
}

/// A reference from one prim to another prim, possibly in another project
/// (pre-comps of linked projects, linked Ai artboards).
///
/// The referenced prim's subtree is composed under the referencing prim, weaker than
/// the referencing prim's own opinions.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Reference {
    /// Path of another project (`.bae` directory), relative to this project; `None`
    /// means the same project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub prim: PrimPath,
}

/// One sublayer's opinions about one prim (not including children).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PrimSpec {
    #[serde(default)]
    pub specifier: Specifier,
    /// Type opinion; usually only on `def` specs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prim_type: Option<PrimType>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attrs: BTreeMap<String, AttrSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<Reference>,
}

impl PrimSpec {
    pub fn def(prim_type: PrimType) -> Self {
        PrimSpec {
            specifier: Specifier::Def,
            prim_type: Some(prim_type),
            ..Default::default()
        }
    }

    pub fn over() -> Self {
        PrimSpec {
            specifier: Specifier::Over,
            ..Default::default()
        }
    }
}

/// A prim spec with its descendants, used to restore deleted subtrees (undo) and to
/// copy/paste or export subtrees.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PrimTree {
    pub name: String,
    #[serde(flatten)]
    pub spec: PrimSpec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PrimTree>,
}

impl PrimTree {
    /// Number of prims in this tree.
    pub fn len(&self) -> usize {
        1 + self.children.iter().map(PrimTree::len).sum::<usize>()
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prim_type_serde() {
        let t: PrimType = serde_json::from_str("\"Comp\"").unwrap();
        assert_eq!(t, PrimType::COMP);
        assert!(t.is_builtin());
        let p: PrimType = serde_json::from_str("\"com.example.fx:Emitter\"").unwrap();
        assert!(!p.is_builtin());
        assert_eq!(p.plugin_id(), Some("com.example.fx"));
    }
}
