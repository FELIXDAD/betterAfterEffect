//! BAE vector graphics (workstream 05): AE shape layers and the path geometry
//! shared with the design workspace (13) and Illustrator interchange (16).
//!
//! ```text
//! bae-anim EvaluatedLayer.contents ──► ShapeGroup (typed tree, content.rs)
//!     ──► VectorScene::from_group (AE order of operations, operators) ──► VectorScene
//!     ──► render::encode (Vello) ──► render::ShapeSource (bae-render LayerSource)
//! ```
//!
//! Public interface:
//!
//! * [`path`] – AE vertex/tangent paths ([`bae_model::BezierPath`]) ↔ kurbo
//!   [`BezPath`](kurbo::BezPath); reversing.
//! * [`primitive`] – Rectangle, Ellipse and Polystar with AE's exact vertex order.
//! * [`paint`] – fills, strokes, gradients, blend modes ([`Paint`], [`Brush`],
//!   [`StrokeStyle`]).
//! * [`content`] – the shape tree ([`ShapeGroup`], [`ShapeItem`], operators),
//!   parsed from bae-anim with [`ShapeGroup::from_evaluated`] or built in code.
//! * [`scene`] – the result type: [`VectorScene`], a flat display list of
//!   [`FillDraw`] / [`StrokeDraw`] / [`LayerDraw`] in layer space, with bounds
//!   (`sourceRectAtTime`) and a content hash.
//! * [`ops`] – path operators on kurbo paths (Trim Paths; more to come).
//! * `render` (feature `render`) – Vello encoding of a [`VectorScene`], the shared
//!   [`render::VelloRenderer`], and the bae-render layer source / provider
//!   ([`SHAPE_PROVIDER`]).
//!
//! Coordinates are AE layer space: x right, y down, pixels; angles in degrees,
//! positive clockwise. Geometry is `f64` until the Vello boundary. Without the
//! `render` feature this crate has no GPU, window or platform dependencies.

pub mod content;
pub mod ops;
pub mod paint;
pub mod path;
pub mod primitive;
#[cfg(feature = "render")]
pub mod render;
pub mod scene;

pub use content::{
    GroupTransform, MergeMode, Operator, PathItem, PathSource, ShapeGroup, ShapeItem, TrimMode,
    TrimPaths,
};
pub use kurbo;
pub use paint::{
    BlendMode, Brush, Composite, FillRule, Gradient, GradientKind, GradientStop, LineCap, LineJoin,
    Paint, PaintKind, Rgba, StrokeStyle,
};
pub use primitive::{Direction, EllipseShape, RectShape, StarShape, StarType};
pub use scene::{DrawItem, FillDraw, LayerDraw, StrokeDraw, VectorScene};

use bae_anim::EvaluatedLayer;

/// [`bae_render::SourceProvider`] ID for shape layers. `CustomContent::data` holds
/// either an `Arc<bae_anim::EvaluatedLayer>` (the shape layer as 03 evaluated it)
/// or an `Arc<VectorScene>` (a display list built elsewhere, e.g. an Ai artboard).
pub const SHAPE_PROVIDER: &str = "bae.shape";

/// The display list of a shape layer evaluated by 03. Problems (unsupported
/// operators or blend modes) are appended to `warnings`.
pub fn shape_layer_scene(layer: &EvaluatedLayer, warnings: &mut Vec<String>) -> VectorScene {
    VectorScene::from_group(&ShapeGroup::from_evaluated(&layer.contents, warnings))
}
