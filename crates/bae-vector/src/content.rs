//! Shape layer contents as a typed tree (AE-SHP-001/002): groups, paths, paints
//! and path operators, in Timeline order (index 0 = top).
//!
//! Built from bae-anim's evaluated prims ([`ShapeGroup::from_evaluated`]) or
//! directly in code (Ai import, design workspace). [`crate::scene`] turns the
//! tree into a draw list following AE's order of operations.

use bae_anim::EvaluatedPrim;
use bae_model::{BezierPath, PrimPath, PrimType, Value};
use kurbo::{Affine, BezPath};

use crate::paint::{
    BlendMode, Brush, Composite, FillRule, Gradient, GradientKind, LineCap, LineJoin, Paint,
    PaintKind, StrokeStyle, paint_color,
};
use crate::path::{point, reversed, to_bez_path};
use crate::primitive::{Direction, EllipseShape, RectShape, StarShape, StarType};

/// A group transform (AE-SHP-029). Units as in AE: pixels, percent, degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupTransform {
    pub anchor_point: [f64; 2],
    pub position: [f64; 2],
    /// Percent.
    pub scale: [f64; 2],
    /// Degrees.
    pub skew: f64,
    /// Degrees.
    pub skew_axis: f64,
    /// Degrees, clockwise.
    pub rotation: f64,
    /// Percent.
    pub opacity: f64,
}

impl Default for GroupTransform {
    fn default() -> Self {
        Self {
            anchor_point: [0.0; 2],
            position: [0.0; 2],
            scale: [100.0; 2],
            skew: 0.0,
            skew_axis: 0.0,
            rotation: 0.0,
            opacity: 100.0,
        }
    }
}

impl GroupTransform {
    /// Group space → parent space:
    /// `T(position) · R(rotation) · K(skew, axis) · S(scale) · T(−anchor)`, with
    /// `K = R(−axis) · shear(tan(−skew)) · R(axis)` (lottie-web's order).
    pub fn matrix(&self) -> Affine {
        let skew = if self.skew == 0.0 {
            Affine::IDENTITY
        } else {
            let axis = self.skew_axis.to_radians();
            let shear = Affine::new([1.0, 0.0, (-self.skew.to_radians()).tan(), 1.0, 0.0, 0.0]);
            Affine::rotate(-axis) * shear * Affine::rotate(axis)
        };
        Affine::translate((self.position[0], self.position[1]))
            * Affine::rotate(self.rotation.to_radians())
            * skew
            * Affine::scale_non_uniform(self.scale[0] / 100.0, self.scale[1] / 100.0)
            * Affine::translate((-self.anchor_point[0], -self.anchor_point[1]))
    }
}

/// Where a path item's geometry comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum PathSource {
    Rect(RectShape),
    Ellipse(EllipseShape),
    Star(StarShape),
    /// Freeform path (AE "Path", AE-SHP-013).
    Bezier(BezierPath),
}

/// A path item: generates geometry, invisible on its own.
#[derive(Clone, Debug, PartialEq)]
pub struct PathItem {
    pub source: PathSource,
    pub direction: Direction,
}

impl PathItem {
    /// The generated path in AE vertex form, direction applied.
    pub fn to_bezier(&self) -> BezierPath {
        match &self.source {
            PathSource::Rect(r) => r.to_path(self.direction),
            PathSource::Ellipse(e) => e.to_path(self.direction),
            PathSource::Star(s) => s.to_path(self.direction),
            PathSource::Bezier(b) => match self.direction {
                Direction::Normal => b.clone(),
                Direction::Reversed => reversed(b),
            },
        }
    }

    /// The generated path as a kurbo path in group space.
    pub fn to_bez_path(&self) -> BezPath {
        to_bez_path(&self.to_bezier())
    }
}

/// Trim Paths "Trim Multiple Shapes" (AE-SHP-036).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TrimMode {
    /// Every shape is trimmed with the same range of its own length.
    #[default]
    Simultaneously,
    /// All shapes are trimmed as one sequence in Timeline order.
    Individually,
}

/// Trim Paths parameters (AE-SHP-036).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrimPaths {
    /// Percent, clamped to 0–100.
    pub start: f64,
    /// Percent, clamped to 0–100.
    pub end: f64,
    /// Degrees; 360° = one full length.
    pub offset: f64,
    pub mode: TrimMode,
}

impl Default for TrimPaths {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: 100.0,
            offset: 0.0,
            mode: TrimMode::Simultaneously,
        }
    }
}

/// Merge Paths mode (AE-SHP-031).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MergeMode {
    /// Combine into one compound path (the paint's fill rule decides holes).
    #[default]
    Merge,
    Add,
    Subtract,
    Intersect,
    Exclude,
}

impl MergeMode {
    /// From the project token.
    pub fn from_token(token: &str) -> Self {
        match token {
            "add" => MergeMode::Add,
            "subtract" => MergeMode::Subtract,
            "intersect" => MergeMode::Intersect,
            "exclude" => MergeMode::Exclude,
            _ => MergeMode::Merge,
        }
    }
}

/// A path operator: modifies the paths above it in its group.
#[derive(Clone, Debug, PartialEq)]
pub enum Operator {
    Trim(TrimPaths),
    Merge(MergeMode),
    /// An operator that is not implemented yet (token from the `operator`
    /// attribute); it passes geometry through unchanged.
    Unsupported(String),
}

/// One item of a group's contents.
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeItem {
    Group(ShapeGroup),
    Path(PathItem),
    Paint(Paint),
    Operator(Operator),
}

/// A group (or the root Contents of a shape layer, with identity transform).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeGroup {
    /// The prim this group came from (`None` for the root and for code-built trees).
    pub path: Option<PrimPath>,
    pub transform: GroupTransform,
    pub blend: BlendMode,
    /// Timeline order: index 0 is the top item.
    pub items: Vec<ShapeItem>,
}

impl ShapeGroup {
    /// An empty group with identity transform.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an item (builder style).
    #[must_use]
    pub fn with(mut self, item: ShapeItem) -> Self {
        self.items.push(item);
        self
    }

    /// The root group of a shape layer from bae-anim's evaluated contents
    /// ([`bae_anim::EvaluatedLayer::contents`]). Unknown prims and disabled items
    /// (`enabled = false`) are skipped; problems are appended to `warnings`.
    pub fn from_evaluated(contents: &[EvaluatedPrim], warnings: &mut Vec<String>) -> Self {
        let mut root = ShapeGroup::new();
        parse_items(contents, &mut root.items, warnings, 0);
        root
    }
}

const MAX_DEPTH: usize = 64;

fn parse_items(
    prims: &[EvaluatedPrim],
    out: &mut Vec<ShapeItem>,
    warnings: &mut Vec<String>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        warnings.push("shape groups nested too deeply".to_owned());
        return;
    }
    for prim in prims {
        if prim.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let a = Attrs(prim);
        let Some(ty) = &prim.prim_type else {
            // Untyped prims are folders of items (e.g. a group's `contents`).
            parse_items(&prim.children, out, warnings, depth + 1);
            continue;
        };
        let item = if *ty == PrimType::FOLDER {
            parse_items(&prim.children, out, warnings, depth + 1);
            continue;
        } else if *ty == PrimType::SHAPE_GROUP {
            let mut group = ShapeGroup {
                path: Some(prim.path.clone()),
                transform: GroupTransform {
                    anchor_point: a.vec2("transform:anchor_point", [0.0; 2]),
                    position: a.vec2("transform:position", [0.0; 2]),
                    scale: a.vec2("transform:scale", [100.0; 2]),
                    skew: a.f64("transform:skew", 0.0),
                    skew_axis: a.f64("transform:skew_axis", 0.0),
                    rotation: a.f64("transform:rotation", 0.0),
                    opacity: a.f64("transform:opacity", 100.0),
                },
                blend: a.blend(warnings),
                items: Vec::new(),
            };
            parse_items(&prim.children, &mut group.items, warnings, depth + 1);
            ShapeItem::Group(group)
        } else if *ty == PrimType::PATH {
            let source = match prim.get("path") {
                Some(Value::Bezier(b)) => b.clone(),
                _ => BezierPath::default(),
            };
            path_item(&a, PathSource::Bezier(source))
        } else if *ty == PrimType::RECT {
            path_item(
                &a,
                PathSource::Rect(RectShape {
                    size: a.vec2("size", [100.0; 2]),
                    position: a.vec2("position", [0.0; 2]),
                    roundness: a.f64("roundness", 0.0),
                }),
            )
        } else if *ty == PrimType::ELLIPSE {
            path_item(
                &a,
                PathSource::Ellipse(EllipseShape {
                    size: a.vec2("size", [100.0; 2]),
                    position: a.vec2("position", [0.0; 2]),
                }),
            )
        } else if *ty == PrimType::STAR {
            path_item(
                &a,
                PathSource::Star(StarShape {
                    star_type: match a.token("star_type").as_deref() {
                        Some("polygon") => StarType::Polygon,
                        _ => StarType::Star,
                    },
                    points: a.f64("points", 5.0),
                    position: a.vec2("position", [0.0; 2]),
                    rotation: a.f64("rotation", 0.0),
                    inner_radius: a.f64("inner_radius", 50.0),
                    outer_radius: a.f64("outer_radius", 100.0),
                    inner_roundness: a.f64("inner_roundness", 0.0),
                    outer_roundness: a.f64("outer_roundness", 0.0),
                }),
            )
        } else if *ty == PrimType::FILL {
            ShapeItem::Paint(Paint {
                kind: PaintKind::Fill(a.fill_rule()),
                brush: Brush::Solid(paint_color(a.color("color", [1.0, 0.0, 0.0, 1.0]))),
                opacity: a.opacity(),
                blend: a.blend(warnings),
                composite: a.composite(),
            })
        } else if *ty == PrimType::STROKE {
            ShapeItem::Paint(Paint {
                kind: PaintKind::Stroke(a.stroke_style()),
                brush: Brush::Solid(paint_color(a.color("color", [1.0; 4]))),
                opacity: a.opacity(),
                blend: a.blend(warnings),
                composite: a.composite(),
            })
        } else if *ty == PrimType::GRADIENT_FILL {
            ShapeItem::Paint(Paint {
                kind: PaintKind::Fill(a.fill_rule()),
                brush: Brush::Gradient(a.gradient()),
                opacity: a.opacity(),
                blend: a.blend(warnings),
                composite: a.composite(),
            })
        } else if *ty == PrimType::GRADIENT_STROKE {
            ShapeItem::Paint(Paint {
                kind: PaintKind::Stroke(a.stroke_style()),
                brush: Brush::Gradient(a.gradient()),
                opacity: a.opacity(),
                blend: a.blend(warnings),
                composite: a.composite(),
            })
        } else if *ty == PrimType::SHAPE_OPERATOR {
            let token = a.token("operator").unwrap_or_else(|| "trim".to_owned());
            ShapeItem::Operator(match token.as_str() {
                "trim" => Operator::Trim(TrimPaths {
                    start: a.f64("trim:start", 0.0),
                    end: a.f64("trim:end", 100.0),
                    offset: a.f64("trim:offset", 0.0),
                    mode: match a.token("trim:mode").as_deref() {
                        Some("individually") => TrimMode::Individually,
                        _ => TrimMode::Simultaneously,
                    },
                }),
                "merge" => {
                    let mode =
                        MergeMode::from_token(a.token("merge:mode").as_deref().unwrap_or("merge"));
                    if mode != MergeMode::Merge {
                        warnings.push(format!(
                            "{}: merge mode {mode:?} is not implemented yet, paths are combined",
                            prim.path
                        ));
                    }
                    Operator::Merge(mode)
                }
                other => {
                    warnings.push(format!(
                        "{}: shape operator `{other}` is not implemented yet",
                        prim.path
                    ));
                    Operator::Unsupported(other.to_owned())
                }
            })
        } else {
            warnings.push(format!(
                "{}: `{}` is not a shape item",
                prim.path,
                ty.as_str()
            ));
            continue;
        };
        out.push(item);
    }
}

fn path_item(a: &Attrs<'_>, source: PathSource) -> ShapeItem {
    ShapeItem::Path(PathItem {
        source,
        direction: Direction::from_token(a.token("direction").as_deref().unwrap_or("normal")),
    })
}

/// Typed attribute access with schema defaults.
struct Attrs<'a>(&'a EvaluatedPrim);

impl Attrs<'_> {
    fn f64(&self, name: &str, default: f64) -> f64 {
        self.0
            .f64(name)
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }

    fn vec2(&self, name: &str, default: [f64; 2]) -> [f64; 2] {
        match self.0.get(name) {
            Some(Value::Vec2(v)) => *v,
            Some(Value::Vec3(v)) => [v[0], v[1]],
            Some(Value::Float(v)) => [*v, *v],
            _ => default,
        }
    }

    fn color(&self, name: &str, default: [f64; 4]) -> [f64; 4] {
        match self.0.get(name) {
            Some(Value::Color(c)) => *c,
            Some(Value::Vec3(c)) => [c[0], c[1], c[2], 1.0],
            _ => default,
        }
    }

    fn token(&self, name: &str) -> Option<String> {
        self.0.get(name).and_then(Value::as_str).map(str::to_owned)
    }

    fn opacity(&self) -> f64 {
        (self.f64("opacity", 100.0) / 100.0).clamp(0.0, 1.0)
    }

    fn fill_rule(&self) -> FillRule {
        FillRule::from_token(self.token("fill_rule").as_deref().unwrap_or("non_zero"))
    }

    fn composite(&self) -> Composite {
        Composite::from_token(self.token("composite").as_deref().unwrap_or("below"))
    }

    fn blend(&self, warnings: &mut Vec<String>) -> BlendMode {
        match self.token("blend_mode") {
            None => BlendMode::Normal,
            Some(t) => BlendMode::from_token(&t).unwrap_or_else(|| {
                warnings.push(format!(
                    "{}: blend mode `{t}` is not supported inside shape layers yet, using normal",
                    self.0.path
                ));
                BlendMode::Normal
            }),
        }
    }

    fn stroke_style(&self) -> StrokeStyle {
        let dashes = match self.0.get("dashes") {
            Some(Value::FloatList(l)) => l.clone(),
            _ => Vec::new(),
        };
        StrokeStyle {
            width: self.f64("width", 2.0),
            cap: LineCap::from_token(self.token("line_cap").as_deref().unwrap_or("butt")),
            join: LineJoin::from_token(self.token("line_join").as_deref().unwrap_or("miter")),
            miter_limit: self.f64("miter_limit", 4.0),
            dashes,
            dash_offset: self.f64("dash_offset", 0.0),
        }
    }

    fn gradient(&self) -> Gradient {
        let colors = match self.0.get("colors") {
            Some(Value::FloatList(l)) => l.as_slice(),
            _ => &[],
        };
        let color_stops = self.f64("color_stops", 2.0).max(0.0) as usize;
        let kind = match self.token("gradient_type").as_deref() {
            Some("radial") => GradientKind::Radial {
                highlight_length: self.f64("highlight_length", 0.0),
                highlight_angle: self.f64("highlight_angle", 0.0),
            },
            _ => GradientKind::Linear,
        };
        Gradient {
            kind,
            start: point(self.vec2("start_point", [0.0, 0.0])),
            end: point(self.vec2("end_point", [100.0, 0.0])),
            stops: Gradient::stops_from_ae(colors, color_stops),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use kurbo::Point;

    use super::*;

    fn prim(path: &str, ty: PrimType, attrs: &[(&str, Value)]) -> EvaluatedPrim {
        EvaluatedPrim {
            path: PrimPath::new(path).unwrap(),
            prim_type: Some(ty),
            attrs: attrs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect::<BTreeMap<_, _>>(),
            children: Vec::new(),
        }
    }

    #[test]
    fn group_matrix_follows_ae_order() {
        let t = GroupTransform {
            anchor_point: [10.0, 0.0],
            position: [100.0, 50.0],
            scale: [200.0, 100.0],
            rotation: 90.0,
            ..GroupTransform::default()
        };
        // Anchor → origin, scale x2 in x, rotate 90° clockwise (y down), move.
        let p = t.matrix() * Point::new(20.0, 0.0);
        assert!(
            (p.x - 100.0).abs() < 1e-9 && (p.y - 70.0).abs() < 1e-9,
            "{p:?}"
        );
        // Positive skew with axis 0 shears the top to the right (y up = negative).
        let s = GroupTransform {
            skew: 45.0,
            ..GroupTransform::default()
        };
        let q = s.matrix() * Point::new(0.0, -10.0);
        assert!((q.x - 10.0).abs() < 1e-9, "{q:?}");
    }

    #[test]
    fn parses_evaluated_contents() {
        let mut group = prim(
            "/c/l/contents/g",
            PrimType::SHAPE_GROUP,
            &[("transform:position", Value::Vec2([5.0, 6.0]))],
        );
        let mut folder = prim("/c/l/contents/g/contents", PrimType::FOLDER, &[]);
        folder.children = vec![
            prim(
                "/c/l/contents/g/contents/rect",
                PrimType::RECT,
                &[("size", Value::Vec2([20.0, 10.0]))],
            ),
            prim(
                "/c/l/contents/g/contents/fill",
                PrimType::FILL,
                &[
                    ("color", Value::Color([0.0, 1.0, 0.0, 1.0])),
                    ("opacity", Value::Float(50.0)),
                    ("fill_rule", Value::Enum("even_odd".into())),
                ],
            ),
            prim(
                "/c/l/contents/g/contents/hidden",
                PrimType::STROKE,
                &[("enabled", Value::Bool(false))],
            ),
            prim(
                "/c/l/contents/g/contents/wiggle",
                PrimType::SHAPE_OPERATOR,
                &[("operator", Value::Enum("wiggle_paths".into()))],
            ),
        ];
        group.children = vec![folder];
        let mut warnings = Vec::new();
        let root = ShapeGroup::from_evaluated(&[group], &mut warnings);
        let ShapeItem::Group(g) = &root.items[0] else {
            panic!("expected a group")
        };
        assert_eq!(g.transform.position, [5.0, 6.0]);
        assert_eq!(g.items.len(), 3, "disabled stroke skipped");
        assert!(matches!(
            &g.items[0],
            ShapeItem::Path(PathItem {
                source: PathSource::Rect(r),
                ..
            }) if r.size == [20.0, 10.0]
        ));
        let ShapeItem::Paint(fill) = &g.items[1] else {
            panic!("expected a paint")
        };
        assert_eq!(fill.opacity, 0.5);
        assert_eq!(fill.kind, PaintKind::Fill(FillRule::EvenOdd));
        assert_eq!(fill.brush, Brush::Solid([0.0, 1.0, 0.0, 1.0]));
        assert_eq!(warnings.len(), 1, "{warnings:?}");
    }
}
