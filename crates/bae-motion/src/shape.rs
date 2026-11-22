//! Shape and mask creation: `shape.add` (what AE's shape tools and Pen tool do in a
//! shape layer, AE-SHP-008 / AE-SHP-051) and `mask.create` (the same tools on a
//! layer that gets a mask, AE-CMP-036).
//!
//! Both take one [`Geometry`]: a parametric rectangle (rounded with `roundness`),
//! ellipse, polygon or star, or a free Bezier path, in the target layer's space.
//! Parametric paths use bae-vector's construction, so a mask or a "Bezier Path"
//! shape has exactly the vertices the renderer draws for the parametric shape
//! (AE's start vertex and direction).

use bae_model::{BezierPath, Command, CommandError, LayerKind, PrimPath, PrimType, Value};
use bae_vector::{Direction, EllipseShape, RectShape, StarShape, StarType};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::layer::{CreateLayer, CreateLayerParams};
use crate::query;
use crate::util::{child, create, ensure_unlocked, fail, set};

/// What to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    /// Rectangle (Rectangle and Rounded Rectangle tools; `roundness` > 0 rounds it).
    Rect,
    Ellipse,
    /// Polystar of type polygon.
    Polygon,
    /// Polystar of type star.
    Star,
    /// A free Bezier path (`path`), e.g. drawn with the Pen tool.
    Path,
}

/// The shape, in the target layer's space (pixels; for a new shape layer the
/// origin is the comp center, where AE puts the new layer's origin).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Geometry {
    pub kind: ShapeKind,
    /// Center of the shape. Shapes: the new group's Position (the primitive sits
    /// at the group origin). Masks: where the path is centered. Ignored for `path`.
    #[serde(default)]
    pub position: [f64; 2],
    /// Rectangle / ellipse: width and height.
    #[serde(default)]
    pub size: Option<[f64; 2]>,
    /// Rectangle corner radius in pixels.
    #[serde(default)]
    pub roundness: f64,
    /// Polygon / star: number of points (default 5).
    #[serde(default)]
    pub points: Option<f64>,
    /// Polygon / star: outer radius (default 100).
    #[serde(default)]
    pub outer_radius: Option<f64>,
    /// Star: inner radius (default half the outer radius).
    #[serde(default)]
    pub inner_radius: Option<f64>,
    /// Polygon / star rotation in degrees (0 = a point straight up).
    #[serde(default)]
    pub rotation: f64,
    /// Polygon / star outer roundness, percent.
    #[serde(default)]
    pub outer_roundness: f64,
    /// Star inner roundness, percent.
    #[serde(default)]
    pub inner_roundness: f64,
    /// `path`: the vertices (layer space; shapes keep the group transform at
    /// identity, as AE does for Pen paths).
    #[serde(default)]
    pub path: Option<BezierPath>,
}

impl Geometry {
    fn points(&self) -> f64 {
        self.points.unwrap_or(5.0).max(3.0)
    }

    fn outer(&self) -> f64 {
        self.outer_radius.unwrap_or(100.0).max(0.0)
    }

    fn inner(&self) -> f64 {
        self.inner_radius.unwrap_or(self.outer() / 2.0).max(0.0)
    }

    fn size(&self) -> [f64; 2] {
        self.size.unwrap_or([100.0, 100.0])
    }

    /// The outline as a Bezier path centered at `center` (shapes: the group
    /// origin; masks: `position`).
    pub fn to_path(&self, center: [f64; 2]) -> Result<BezierPath, CommandError> {
        Ok(match self.kind {
            ShapeKind::Rect => RectShape {
                size: self.size(),
                position: center,
                roundness: self.roundness.max(0.0),
            }
            .to_path(Direction::Normal),
            ShapeKind::Ellipse => EllipseShape {
                size: self.size(),
                position: center,
            }
            .to_path(Direction::Normal),
            ShapeKind::Polygon | ShapeKind::Star => StarShape {
                star_type: if self.kind == ShapeKind::Star {
                    StarType::Star
                } else {
                    StarType::Polygon
                },
                points: self.points(),
                position: center,
                rotation: self.rotation,
                inner_radius: self.inner(),
                outer_radius: self.outer(),
                inner_roundness: self.inner_roundness,
                outer_roundness: self.outer_roundness,
            }
            .to_path(Direction::Normal),
            ShapeKind::Path => {
                let p = self
                    .path
                    .clone()
                    .ok_or_else(|| fail("kind `path` needs `path`"))?;
                if !p.is_consistent() || p.vertices.is_empty() {
                    return Err(fail(
                        "path needs vertices with matching in/out tangent arrays",
                    ));
                }
                p
            }
        })
    }
}

/// A fill for a new shape.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct FillParams {
    /// Straight RGBA 0–1.
    pub color: [f64; 4],
    /// Percent (default 100).
    #[serde(default)]
    pub opacity: Option<f64>,
}

/// A stroke for a new shape.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct StrokeParams {
    /// Straight RGBA 0–1.
    pub color: [f64; 4],
    /// Pixels (default 2).
    #[serde(default)]
    pub width: Option<f64>,
    /// Percent (default 100).
    #[serde(default)]
    pub opacity: Option<f64>,
}

/// Base names of the items a new shape group contains, numbered by the command
/// ("Rectangle 1", "Rectangle Path 1", "Stroke 1", "Fill 1"). Omitted ones are
/// the defaults in the naming language ([`crate::names`]: "矩形", "矩形路径" …).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct ShapeNames {
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub fill: Option<String>,
    #[serde(default)]
    pub stroke: Option<String>,
}

fn english_names(kind: ShapeKind, bezier: bool) -> (&'static str, &'static str) {
    let path = |p: &'static str| if bezier { "Path" } else { p };
    match kind {
        ShapeKind::Rect => ("Rectangle", path("Rectangle Path")),
        ShapeKind::Ellipse => ("Ellipse", path("Ellipse Path")),
        ShapeKind::Polygon | ShapeKind::Star => ("Polystar", path("Polystar Path")),
        ShapeKind::Path => ("Shape", "Path"),
    }
}

// ---------------------------------------------------------------------------

pub struct AddShape;

#[derive(Deserialize, JsonSchema)]
pub struct AddShapeParams {
    /// Shape layer to add the shape to (as a new group at the top of its
    /// contents). Omit to create a new shape layer in `comp`.
    #[serde(default)]
    pub layer: Option<PrimPath>,
    /// Comp of the new shape layer (when `layer` is omitted).
    #[serde(default)]
    pub comp: Option<PrimPath>,
    /// New layer: display name (default "Shape Layer N").
    #[serde(default)]
    pub name: Option<String>,
    /// New layer: position in the layer stack, 0 = top (default 0).
    #[serde(default)]
    pub index: Option<usize>,
    /// New layer with a parametric shape: put the layer's Anchor Point at the
    /// shape's center and Position there in the comp, so the shape stays put (AE
    /// preference "Center Anchor Point in New Shape Layers"; default false: the
    /// anchor stays at the comp center, as AE's tutorials show).
    #[serde(default)]
    pub center_anchor: Option<bool>,
    #[serde(flatten)]
    pub geometry: Geometry,
    /// Create a Bezier Path with the primitive's geometry instead of a
    /// parametric shape (Tools panel "Bezier Path").
    #[serde(default)]
    pub bezier: bool,
    /// Fill of the new group; omitted = no Fill (Fill type None).
    #[serde(default)]
    pub fill: Option<FillParams>,
    /// Stroke of the new group; omitted = no Stroke.
    #[serde(default)]
    pub stroke: Option<StrokeParams>,
    /// Translated base names of the group's items.
    #[serde(default)]
    pub names: Option<ShapeNames>,
}

/// Output of `shape.add`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddShapeOutput {
    /// The shape layer (new or given).
    pub layer: PrimPath,
    /// The new shape group.
    pub group: PrimPath,
}

impl Command for AddShape {
    type Params = AddShapeParams;
    type Output = AddShapeOutput;
    const ID: &'static str = "shape.add";
    const TITLE: &'static str = "Add Shape";
    const DESCRIPTION: &'static str = "Draw a rectangle, rounded rectangle, ellipse, polygon, star or Bezier path like AE's shape and Pen tools: into an existing shape layer (new group on top) or into a new shape layer (layer origin at the comp center), with Fill and Stroke.";

    fn run(
        &self,
        tx: &mut bae_model::Transaction<'_>,
        p: Self::Params,
    ) -> Result<AddShapeOutput, CommandError> {
        // Check the geometry before creating anything.
        let outline = p.geometry.to_path([0.0, 0.0])?;
        let parametric = p.geometry.kind != ShapeKind::Path && !p.bezier;
        let layer = match &p.layer {
            Some(layer) => {
                if query::layer_kind(tx.stage(), layer).as_deref() != Some("shape") {
                    return Err(fail(format!("{layer} is not a shape layer")));
                }
                ensure_unlocked(tx.stage(), [layer])?;
                layer.clone()
            }
            None => {
                let comp = p
                    .comp
                    .clone()
                    .ok_or_else(|| fail("give `layer` or `comp`"))?;
                let info = query::comp_info(tx.stage(), &comp)
                    .ok_or_else(|| fail(format!("{comp} is not a comp")))?;
                let out = CreateLayer.run(
                    tx,
                    CreateLayerParams {
                        comp,
                        kind: LayerKind::Shape,
                        name: p.name.clone(),
                        source: None,
                        index: p.index,
                        color: None,
                        size: None,
                        text: None,
                        font_size: None,
                        shape: None,
                        position: None,
                        start: None,
                    },
                )?;
                let center = [f64::from(info.width) / 2.0, f64::from(info.height) / 2.0];
                if p.center_anchor.unwrap_or(false) && p.geometry.kind != ShapeKind::Path {
                    let [x, y] = p.geometry.position;
                    set(
                        tx,
                        &out.path,
                        "transform:anchor_point",
                        Value::Vec3([x, y, 0.0]),
                    )?;
                    set(
                        tx,
                        &out.path,
                        "transform:position",
                        Value::Vec3([center[0] + x, center[1] + y, 0.0]),
                    )?;
                }
                out.path
            }
        };

        let contents = child(&layer, query::CONTENTS)?;
        if !tx.stage().has_prim(&contents) {
            create(tx, &contents, PrimType::FOLDER, None)?;
        }
        let names = p.names.clone().unwrap_or_default();
        let (group_en, path_en) = english_names(p.geometry.kind, p.bezier);
        let local = crate::names::localized;
        let group_base = names.group.as_deref().unwrap_or(local(group_en));
        let group_name = query::next_numbered_name(tx.stage(), &contents, group_base);
        let group = child(
            &contents,
            &query::unique_child_name(tx.stage(), &contents, &group_name),
        )?;
        // New shapes go on top (AE adds them above the existing contents).
        create(tx, &group, PrimType::SHAPE_GROUP, Some(0))?;
        set(tx, &group, "name", Value::String(group_name))?;
        if p.geometry.kind != ShapeKind::Path {
            let [x, y] = p.geometry.position;
            set(tx, &group, "transform:position", Value::Vec2([x, y]))?;
        }
        let inner = child(&group, query::CONTENTS)?;
        create(tx, &inner, PrimType::FOLDER, None)?;

        // Path first, then Stroke above Fill (AE's order).
        let path_name = format!("{} 1", names.path.as_deref().unwrap_or(local(path_en)));
        let shape = child(&inner, &query::sanitize_name(&path_name))?;
        let g = &p.geometry;
        let ty = if parametric {
            match g.kind {
                ShapeKind::Rect => PrimType::RECT,
                ShapeKind::Ellipse => PrimType::ELLIPSE,
                _ => PrimType::STAR,
            }
        } else {
            PrimType::PATH
        };
        create(tx, &shape, ty, None)?;
        set(tx, &shape, "name", Value::String(path_name))?;
        if parametric {
            match g.kind {
                ShapeKind::Rect => {
                    set(tx, &shape, "size", Value::Vec2(g.size()))?;
                    if g.roundness > 0.0 {
                        set(tx, &shape, "roundness", Value::Float(g.roundness))?;
                    }
                }
                ShapeKind::Ellipse => set(tx, &shape, "size", Value::Vec2(g.size()))?,
                _ => {
                    let star = g.kind == ShapeKind::Star;
                    let ty = if star { "star" } else { "polygon" };
                    set(tx, &shape, "star_type", Value::Enum(ty.into()))?;
                    set(tx, &shape, "points", Value::Float(g.points()))?;
                    set(tx, &shape, "outer_radius", Value::Float(g.outer()))?;
                    set(tx, &shape, "rotation", Value::Float(g.rotation))?;
                    set(
                        tx,
                        &shape,
                        "outer_roundness",
                        Value::Float(g.outer_roundness),
                    )?;
                    if star {
                        set(tx, &shape, "inner_radius", Value::Float(g.inner()))?;
                        set(
                            tx,
                            &shape,
                            "inner_roundness",
                            Value::Float(g.inner_roundness),
                        )?;
                    }
                }
            }
        } else {
            set(tx, &shape, "path", Value::Bezier(outline))?;
        }
        if let Some(stroke) = &p.stroke {
            let name = format!("{} 1", names.stroke.as_deref().unwrap_or(local("Stroke")));
            let s = child(&inner, &query::sanitize_name(&name))?;
            create(tx, &s, PrimType::STROKE, None)?;
            set(tx, &s, "name", Value::String(name))?;
            set(tx, &s, "color", Value::Color(stroke.color))?;
            set(tx, &s, "width", Value::Float(stroke.width.unwrap_or(2.0)))?;
            if let Some(o) = stroke.opacity {
                set(tx, &s, "opacity", Value::Float(o))?;
            }
        }
        if let Some(fill) = &p.fill {
            let name = format!("{} 1", names.fill.as_deref().unwrap_or(local("Fill")));
            let f = child(&inner, &query::sanitize_name(&name))?;
            create(tx, &f, PrimType::FILL, None)?;
            set(tx, &f, "name", Value::String(name))?;
            set(tx, &f, "color", Value::Color(fill.color))?;
            if let Some(o) = fill.opacity {
                set(tx, &f, "opacity", Value::Float(o))?;
            }
        }
        Ok(AddShapeOutput { layer, group })
    }
}

// ---------------------------------------------------------------------------

/// AE 26's default label colors in label order (1 Red … 16 Dark Green).
const LABEL_RGB: [[u8; 3]; 16] = [
    [0xb5, 0x38, 0x38],
    [0xe4, 0xd8, 0x4c],
    [0xa9, 0xcb, 0xc7],
    [0xe5, 0xbc, 0xc9],
    [0xa9, 0xa9, 0xca],
    [0xe7, 0xc1, 0x9e],
    [0xb3, 0xc7, 0xb3],
    [0x67, 0x7d, 0xe0],
    [0x4a, 0xa4, 0x4c],
    [0x8e, 0x2c, 0x9a],
    [0xe8, 0x92, 0x0d],
    [0x7f, 0x45, 0x2a],
    [0xf4, 0x6d, 0xd6],
    [0x3d, 0xa2, 0xa5],
    [0xa8, 0x96, 0x77],
    [0x1e, 0x40, 0x1e],
];

/// Label indices (0-based) new masks cycle through ("Cycle Mask Colors" on):
/// the first six as measured in AE 26 (Sea Foam, Orange, Fuchsia, Red, Yellow,
/// Peach); the rest in label order (待核).
const MASK_CYCLE: [usize; 16] = [6, 10, 12, 0, 1, 5, 2, 3, 4, 7, 8, 9, 11, 13, 14, 15];

/// Color of the `n`-th mask (0-based) of a layer.
pub fn mask_color(n: usize, cycle: bool) -> [f64; 4] {
    // Cycling off: every mask gets the first color of the cycle (待核).
    let i = MASK_CYCLE[if cycle { n % MASK_CYCLE.len() } else { 0 }];
    let [r, g, b] = LABEL_RGB[i];
    [
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
        1.0,
    ]
}

/// Layer kinds that can have masks (AV layers: not cameras, lights, nulls or
/// audio-only layers).
pub fn takes_masks(kind: &str) -> bool {
    matches!(
        kind,
        "shape" | "text" | "solid" | "footage" | "adjustment" | "pre_comp"
    )
}

pub struct CreateMask;

#[derive(Deserialize, JsonSchema)]
pub struct CreateMaskParams {
    /// The layer to mask.
    pub layer: PrimPath,
    /// The mask path, in layer space (pixels from the layer's top-left corner).
    #[serde(flatten)]
    pub geometry: Geometry,
    /// Display name (default "Mask N" in the naming language, N = number of masks
    /// after this one is added).
    #[serde(default)]
    pub name: Option<String>,
    /// none, add, subtract, intersect, lighten, darken or difference (default add).
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub inverted: bool,
    /// Outline color in the viewer, RGBA 0–1 (default: the next label color).
    #[serde(default)]
    pub color: Option<[f64; 4]>,
    /// Cycle mask colors through the label colors (AE preference, default true);
    /// off: every new mask is yellow.
    #[serde(default)]
    pub cycle_colors: Option<bool>,
}

/// Output of `mask.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaskOutput {
    pub mask: PrimPath,
}

impl Command for CreateMask {
    type Params = CreateMaskParams;
    type Output = MaskOutput;
    const ID: &'static str = "mask.create";
    const TITLE: &'static str = "New Mask";
    const DESCRIPTION: &'static str = "Add a mask to a layer (what AE's shape and Pen tools draw on a selected non-shape layer): a rectangle, rounded rectangle, ellipse, polygon, star or Bezier path in layer space, mode Add, named Mask N.";

    fn run(
        &self,
        tx: &mut bae_model::Transaction<'_>,
        p: Self::Params,
    ) -> Result<MaskOutput, CommandError> {
        let kind = query::layer_kind(tx.stage(), &p.layer)
            .ok_or_else(|| fail(format!("{} is not a layer", p.layer)))?;
        if !takes_masks(&kind) {
            return Err(fail(format!("{kind} layers cannot have masks")));
        }
        ensure_unlocked(tx.stage(), [&p.layer])?;
        let path = p.geometry.to_path(p.geometry.position)?;
        let masks = child(&p.layer, "masks")?;
        if !tx.stage().has_prim(&masks) {
            create(tx, &masks, PrimType::FOLDER, None)?;
        }
        let count = tx.stage().children(&masks).len();
        let name = match p.name.filter(|n| !n.trim().is_empty()) {
            Some(n) => n,
            // AE: "Mask N", N = the mask's index when it is created.
            None => format!("{} {}", crate::names::localized("Mask"), count + 1),
        };
        let mask = child(&masks, &query::unique_child_name(tx.stage(), &masks, &name))?;
        create(tx, &mask, PrimType::MASK, None)?;
        set(tx, &mask, "name", Value::String(name))?;
        set(tx, &mask, "path", Value::Bezier(path))?;
        let mode = p.mode.unwrap_or_else(|| "add".into());
        set(tx, &mask, "mode", Value::Enum(mode))?;
        if p.inverted {
            set(tx, &mask, "inverted", Value::Bool(true))?;
        }
        let color = p
            .color
            .unwrap_or_else(|| mask_color(count, p.cycle_colors.unwrap_or(true)));
        set(tx, &mask, "color", Value::Color(color))?;
        Ok(MaskOutput { mask })
    }
}
