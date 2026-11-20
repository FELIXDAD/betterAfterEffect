//! The Illustrator document model every reader produces (.ai/PDF, the
//! Illustrator bridge JSON, clipboard data) and the importer consumes.
//!
//! It is deliberately close to what Illustrator shows: artboards, layers,
//! groups, (compound) paths with an appearance of fills and strokes, and point
//! text. Paints are bae-vector's types ([`Paint`], [`Brush`], [`Gradient`]),
//! so a path's appearance is exactly what the shape layer will draw, and
//! [`AiLayer::to_shape_group`] gives the bae-vector shape tree directly (for
//! previews and tests).
//!
//! **Coordinates** are artboard space: points (1 pt = 1 px, like AE's Ai
//! import), origin at the artboard's top-left corner, x right, y down. Readers
//! bake every transform (PDF `cm`, Ai group transforms) into the geometry.
//!
//! **Order** is the Layers panel order: index 0 is the topmost item, the same
//! convention as AE's timeline and bae-vector's shape tree.

use bae_model::BezierPath;
use bae_vector::{
    BlendMode, Composite, GroupTransform, Paint, PathItem, PathSource, ShapeGroup, ShapeItem,
};

/// Where a document came from (shown in the import report).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SourceFormat {
    /// An Illustrator file with PDF compatibility (its embedded PDF was read).
    AiPdf,
    /// A plain PDF file.
    Pdf,
    /// JSON written by the Illustrator bridge script (`bridges/ai/`).
    Bridge,
    /// Illustrator clipboard data (PDF or SVG flavor).
    Clipboard,
    /// Built in code.
    #[default]
    Code,
}

/// A whole Illustrator document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiDocument {
    /// Display name, usually the file name without extension.
    pub name: String,
    pub source: SourceFormat,
    /// In Illustrator's artboard order.
    pub artboards: Vec<Artboard>,
    /// Problems the reader found (unsupported features, approximations).
    pub warnings: Vec<String>,
}

/// One artboard and the artwork on it.
#[derive(Clone, Debug, PartialEq)]
pub struct Artboard {
    /// Illustrator's artboard name ("Artboard 1" / "画板 1" when unnamed).
    pub name: String,
    /// Top-left corner in document space (points, y down). Artboards are laid
    /// out side by side in Illustrator; this keeps that layout for the design
    /// workspace. Artwork coordinates are relative to this corner.
    pub origin: [f64; 2],
    /// Width and height in points.
    pub size: [f64; 2],
    /// Topmost layer first.
    pub layers: Vec<AiLayer>,
}

impl Artboard {
    /// An empty artboard at the document origin.
    pub fn new(name: impl Into<String>, size: [f64; 2]) -> Self {
        Self {
            name: name.into(),
            origin: [0.0; 2],
            size,
            layers: Vec::new(),
        }
    }
}

/// A top-level Illustrator layer.
#[derive(Clone, Debug, PartialEq)]
pub struct AiLayer {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    /// Topmost item first.
    pub items: Vec<AiItem>,
}

impl AiLayer {
    /// A visible, unlocked, empty layer.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            visible: true,
            locked: false,
            items: Vec::new(),
        }
    }

    /// The layer's vector artwork as a bae-vector shape tree, in artboard
    /// space: what the imported shape layer(s) draw, without text and clipping.
    pub fn to_shape_group(&self) -> ShapeGroup {
        let mut root = ShapeGroup::new();
        items_to_shapes(&self.items, &mut root.items);
        root
    }
}

/// One item of a layer or group.
#[derive(Clone, Debug, PartialEq)]
pub enum AiItem {
    Group(AiGroup),
    Path(AiPath),
    Text(AiText),
}

/// A group (or clip group).
#[derive(Clone, Debug, PartialEq)]
pub struct AiGroup {
    pub name: Option<String>,
    /// `[0, 1]`.
    pub opacity: f64,
    pub blend: BlendMode,
    pub hidden: bool,
    /// Clipping path of a clip group (Ai "Make Clipping Mask"); the sub-paths
    /// are combined with the non-zero rule.
    pub clip: Option<Vec<BezierPath>>,
    /// Topmost item first.
    pub items: Vec<AiItem>,
}

impl AiGroup {
    /// An empty, fully opaque, normal group.
    pub fn new(name: Option<String>) -> Self {
        Self {
            name,
            opacity: 1.0,
            blend: BlendMode::Normal,
            hidden: false,
            clip: None,
            items: Vec::new(),
        }
    }
}

/// A path or compound path with its appearance.
#[derive(Clone, Debug, PartialEq)]
pub struct AiPath {
    pub name: Option<String>,
    /// One entry for a simple path, several for a compound path.
    pub subpaths: Vec<BezierPath>,
    /// Fills and strokes, topmost first (Appearance panel order). Illustrator's
    /// default appearance is `[stroke, fill]`. Composite is always
    /// [`Composite::Below`], AE's default.
    pub paints: Vec<Paint>,
    /// `[0, 1]`.
    pub opacity: f64,
    pub blend: BlendMode,
    pub hidden: bool,
}

impl AiPath {
    /// A visible, unpainted path.
    pub fn new(subpaths: Vec<BezierPath>) -> Self {
        Self {
            name: None,
            subpaths,
            paints: Vec::new(),
            opacity: 1.0,
            blend: BlendMode::Normal,
            hidden: false,
        }
    }

    /// Adds a paint below the existing ones (builder style).
    #[must_use]
    pub fn with_paint(mut self, paint: Paint) -> Self {
        self.paints.push(paint);
        self
    }
}

/// Point text. Until text import lands (bae-text, workstream 05) it becomes a
/// text layer with a single-style text document.
#[derive(Clone, Debug, PartialEq)]
pub struct AiText {
    pub name: Option<String>,
    /// Lines separated by `\r` (AE's convention) or `\n`.
    pub text: String,
    /// PostScript font name, e.g. `MyriadPro-Regular`.
    pub font: String,
    /// Points.
    pub size: f64,
    /// Baseline origin of the first line, artboard space.
    pub position: [f64; 2],
    /// Degrees, clockwise.
    pub rotation: f64,
    /// Percent (horizontal / vertical scale of the text matrix).
    pub scale: [f64; 2],
    /// Straight RGBA; `None` = no fill.
    pub fill: Option<[f64; 4]>,
    /// Straight RGBA and width.
    pub stroke: Option<([f64; 4], f64)>,
    /// `left`, `center` or `right` (bae-model `TextDocument::justification`).
    pub justification: String,
    pub hidden: bool,
}

impl AiText {
    /// Black, left-aligned point text.
    pub fn new(text: impl Into<String>, font: impl Into<String>, size: f64) -> Self {
        Self {
            name: None,
            text: text.into(),
            font: font.into(),
            size,
            position: [0.0; 2],
            rotation: 0.0,
            scale: [100.0; 2],
            fill: Some([0.0, 0.0, 0.0, 1.0]),
            stroke: None,
            justification: "left".to_owned(),
            hidden: false,
        }
    }
}

fn items_to_shapes(items: &[AiItem], out: &mut Vec<ShapeItem>) {
    for item in items {
        match item {
            AiItem::Group(g) if !g.hidden => {
                let mut group = ShapeGroup {
                    transform: GroupTransform {
                        opacity: g.opacity * 100.0,
                        ..GroupTransform::default()
                    },
                    blend: g.blend,
                    ..ShapeGroup::default()
                };
                items_to_shapes(&g.items, &mut group.items);
                out.push(ShapeItem::Group(group));
            }
            AiItem::Path(p) if !p.hidden => out.push(ShapeItem::Group(path_group(p))),
            _ => {}
        }
    }
}

/// The group a path becomes: its sub-paths, then its paints (AE's layout for
/// "Create Shapes from Vector Layer").
fn path_group(p: &AiPath) -> ShapeGroup {
    let mut group = ShapeGroup {
        transform: GroupTransform {
            opacity: p.opacity * 100.0,
            ..GroupTransform::default()
        },
        blend: p.blend,
        ..ShapeGroup::default()
    };
    for sub in &p.subpaths {
        group.items.push(ShapeItem::Path(PathItem {
            source: PathSource::Bezier(sub.clone()),
            direction: bae_vector::Direction::Normal,
        }));
    }
    for paint in &p.paints {
        group.items.push(ShapeItem::Paint(Paint {
            composite: Composite::Below,
            ..paint.clone()
        }));
    }
    group
}
