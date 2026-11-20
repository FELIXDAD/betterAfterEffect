//! Display lists: what a shape layer (or an Ai artboard, or converted text) draws,
//! as a flat list of fills, strokes and isolated groups in layer space.
//!
//! [`VectorScene::from_group`] evaluates a [`ShapeGroup`] tree with AE's order of
//! operations (AE-SHP-003):
//!
//! 1. Path items generate geometry; groups pass their geometry to the parent
//!    transformed by the group matrix.
//! 2. A path operator modifies every path above it in its group, including paths
//!    in nested groups above it, top to bottom; operators of enclosing groups
//!    apply after those of inner groups. Operators act before any paint, so a
//!    paint above a Trim Paths still draws the trimmed path.
//! 3. A paint draws all paths above it in its group as **one** compound path.
//!    Paints above a Merge Paths draw nothing.
//! 4. Paints and groups are drawn bottom to top; "Above previous" paints move in
//!    front of the item above them (AE-SHP-004).
//! 5. Groups with opacity below 100 % or a non-normal blend mode are isolated
//!    (drawn into their own layer, AE-SHP-005/030); a group whose content is a
//!    single normal draw gets the opacity multiplied in instead.

use std::hash::{Hash, Hasher};

use kurbo::{Affine, BezPath, Rect, Shape};
use rustc_hash::FxHasher;

use crate::content::{Operator, ShapeGroup, ShapeItem};
use crate::ops::{merge_concat, trim_shapes};
use crate::paint::{
    BlendMode, Brush, Composite, FillRule, Paint, PaintKind, StrokeStyle, hash_f64,
};

/// A filled compound path.
#[derive(Clone, Debug, PartialEq)]
pub struct FillDraw {
    /// Geometry in the paint's group space.
    pub path: BezPath,
    /// Paint group space → layer space.
    pub transform: Affine,
    pub rule: FillRule,
    /// In group space (gradient points move with the group).
    pub brush: Brush,
    /// `[0, 1]`, enclosing group opacities included where they were folded in.
    pub opacity: f32,
}

/// A stroked compound path. The outline is computed in group space and then
/// transformed, so non-uniform group scale makes the stroke anisotropic.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeDraw {
    pub path: BezPath,
    pub transform: Affine,
    pub style: StrokeStyle,
    pub brush: Brush,
    pub opacity: f32,
}

/// Items drawn into an isolated transparent layer that is then composited with
/// `blend` at `opacity`.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerDraw {
    pub opacity: f32,
    pub blend: BlendMode,
    pub items: Vec<DrawItem>,
}

/// One entry of a display list.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawItem {
    Fill(FillDraw),
    Stroke(StrokeDraw),
    Layer(LayerDraw),
}

impl DrawItem {
    fn multiply_opacity(&mut self, k: f32) {
        match self {
            DrawItem::Fill(f) => f.opacity *= k,
            DrawItem::Stroke(s) => s.opacity *= k,
            DrawItem::Layer(l) => l.opacity *= k,
        }
    }

    fn is_normal(&self) -> bool {
        match self {
            DrawItem::Layer(l) => l.blend == BlendMode::Normal,
            _ => true,
        }
    }

    /// Layer-space bounds; `with_stroke` adds stroke extents (conservative for
    /// miter joins).
    fn bounds(&self, with_stroke: bool) -> Option<Rect> {
        match self {
            DrawItem::Fill(f) => non_empty((f.transform * &f.path).bounding_box()),
            DrawItem::Stroke(s) => {
                let r = (s.transform * &s.path).bounding_box();
                let r = non_empty(r).or_else(|| (!s.path.is_empty()).then_some(r))?;
                if with_stroke {
                    let k = max_scale(&s.transform);
                    Some(r.inflate(s.style.outset() * k, s.style.outset() * k))
                } else {
                    Some(r)
                }
            }
            DrawItem::Layer(l) => union_bounds(l.items.iter(), with_stroke),
        }
    }

    fn hash_into(&self, h: &mut impl Hasher) {
        match self {
            DrawItem::Fill(f) => {
                0u8.hash(h);
                hash_path(&f.path, h);
                hash_affine(&f.transform, h);
                f.rule.hash(h);
                f.brush.hash_into(h);
                f.opacity.to_bits().hash(h);
            }
            DrawItem::Stroke(s) => {
                1u8.hash(h);
                hash_path(&s.path, h);
                hash_affine(&s.transform, h);
                s.style.hash_into(h);
                s.brush.hash_into(h);
                s.opacity.to_bits().hash(h);
            }
            DrawItem::Layer(l) => {
                2u8.hash(h);
                l.opacity.to_bits().hash(h);
                l.blend.hash(h);
                l.items.len().hash(h);
                for i in &l.items {
                    i.hash_into(h);
                }
            }
        }
    }
}

fn non_empty(r: Rect) -> Option<Rect> {
    (r.width() > 0.0 || r.height() > 0.0).then_some(r)
}

fn max_scale(t: &Affine) -> f64 {
    let [a, b, c, d, _, _] = t.as_coeffs();
    (a * a + b * b).sqrt().max((c * c + d * d).sqrt())
}

fn union_bounds<'a>(items: impl Iterator<Item = &'a DrawItem>, with_stroke: bool) -> Option<Rect> {
    items
        .filter_map(|i| i.bounds(with_stroke))
        .reduce(|a, b| a.union(b))
}

fn hash_path(p: &BezPath, h: &mut impl Hasher) {
    p.elements().len().hash(h);
    for el in p.elements() {
        match *el {
            kurbo::PathEl::MoveTo(a) => {
                0u8.hash(h);
                hash_f64(a.x, h);
                hash_f64(a.y, h);
            }
            kurbo::PathEl::LineTo(a) => {
                1u8.hash(h);
                hash_f64(a.x, h);
                hash_f64(a.y, h);
            }
            kurbo::PathEl::QuadTo(a, b) => {
                2u8.hash(h);
                for p in [a, b] {
                    hash_f64(p.x, h);
                    hash_f64(p.y, h);
                }
            }
            kurbo::PathEl::CurveTo(a, b, c) => {
                3u8.hash(h);
                for p in [a, b, c] {
                    hash_f64(p.x, h);
                    hash_f64(p.y, h);
                }
            }
            kurbo::PathEl::ClosePath => 4u8.hash(h),
        }
    }
}

fn hash_affine(t: &Affine, h: &mut impl Hasher) {
    for v in t.as_coeffs() {
        hash_f64(v, h);
    }
}

/// A display list in layer space, drawn in order (first item at the back).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VectorScene {
    pub items: Vec<DrawItem>,
}

impl VectorScene {
    /// Nothing to draw.
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates a shape layer's contents (the root group; its own transform,
    /// opacity and blend mode are applied like any group's).
    pub fn from_group(root: &ShapeGroup) -> Self {
        let mut b = Builder::default();
        let plan = b.plan_group(&root.items, false);
        let items = b.emit(&plan, root.transform.matrix());
        Self {
            items: wrap_group(items, root.transform.opacity, root.blend),
        }
    }

    /// Whether nothing is drawn.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Layer-space bounds of everything drawn (`sourceRectAtTime`, AE-SHP-009):
    /// exact curve bounds; `with_stroke` adds stroke extents (half the width,
    /// times the miter limit for miter joins). `None` when nothing is drawn.
    pub fn bounds(&self, with_stroke: bool) -> Option<Rect> {
        union_bounds(self.items.iter(), with_stroke)
    }

    /// Hash of everything that affects the pixels (render cache key part).
    pub fn content_hash(&self) -> u64 {
        let mut h = FxHasher::default();
        self.items.len().hash(&mut h);
        for i in &self.items {
            i.hash_into(&mut h);
        }
        h.finish()
    }

    /// Prepends `t` to every transform (e.g. layer → comp or a fit transform).
    pub fn transform(&mut self, t: Affine) {
        for i in &mut self.items {
            transform_item(i, t);
        }
    }
}

fn transform_item(item: &mut DrawItem, t: Affine) {
    match item {
        DrawItem::Fill(f) => f.transform = t * f.transform,
        DrawItem::Stroke(s) => s.transform = t * s.transform,
        DrawItem::Layer(l) => {
            for i in &mut l.items {
                transform_item(i, t);
            }
        }
    }
}

/// Wraps a group's draws for its opacity (percent) and blend mode.
fn wrap_group(mut items: Vec<DrawItem>, opacity_percent: f64, blend: BlendMode) -> Vec<DrawItem> {
    let opacity = (opacity_percent / 100.0).clamp(0.0, 1.0) as f32;
    if items.is_empty() || opacity <= 0.0 {
        return Vec::new();
    }
    if blend == BlendMode::Normal {
        if opacity >= 1.0 {
            return items;
        }
        if items.len() == 1 && items[0].is_normal() {
            items[0].multiply_opacity(opacity);
            return items;
        }
    }
    vec![DrawItem::Layer(LayerDraw {
        opacity,
        blend,
        items,
    })]
}

/// Geometry of one path item while operators run, in its own group's space.
#[derive(Debug)]
struct Slot {
    path: BezPath,
}

/// A path slot visible in a group: slot index and slot space → group space.
type OpenSlot = (usize, Affine);

/// A group's evaluation plan (Timeline order).
#[derive(Debug, Default)]
struct GroupPlan {
    entries: Vec<PlanEntry>,
    /// Paths contributed to the parent (in this group's space).
    open: Vec<OpenSlot>,
}

#[derive(Debug)]
enum PlanEntry {
    Paint {
        paint: Paint,
        slots: Vec<OpenSlot>,
    },
    Group {
        plan: GroupPlan,
        matrix: Affine,
        opacity: f64,
        blend: BlendMode,
        composite: Composite,
    },
    /// A suppressed paint keeps its place (for "Above previous" bookkeeping).
    Nothing,
}

#[derive(Default)]
struct Builder {
    slots: Vec<Slot>,
}

impl Builder {
    /// Pass 1: geometry. Generates slots, runs operators in place, records which
    /// slots each paint draws. `suppressed` comes from a Merge Paths below an
    /// enclosing group. Operators of enclosing groups run later on the same
    /// slots, so nested paints see their result too.
    fn plan_group(&mut self, items: &[ShapeItem], suppressed: bool) -> GroupPlan {
        // Paints above a Merge Paths (in this group and nested groups) draw nothing.
        let mut suppress_at = vec![suppressed; items.len()];
        let mut s = suppressed;
        for i in (0..items.len()).rev() {
            suppress_at[i] = s;
            if matches!(items[i], ShapeItem::Operator(Operator::Merge(_))) {
                s = true;
            }
        }

        let mut plan = GroupPlan::default();
        for (i, item) in items.iter().enumerate() {
            match item {
                ShapeItem::Path(p) => {
                    self.slots.push(Slot {
                        path: p.to_bez_path(),
                    });
                    plan.open.push((self.slots.len() - 1, Affine::IDENTITY));
                }
                ShapeItem::Group(g) => {
                    let sub = self.plan_group(&g.items, suppress_at[i]);
                    let m = g.transform.matrix();
                    plan.open
                        .extend(sub.open.iter().map(|&(id, a)| (id, m * a)));
                    plan.entries.push(PlanEntry::Group {
                        plan: sub,
                        matrix: m,
                        opacity: g.transform.opacity,
                        blend: g.blend,
                        composite: Composite::Below,
                    });
                }
                ShapeItem::Operator(op) => self.apply(op, &mut plan.open),
                ShapeItem::Paint(paint) => {
                    plan.entries.push(if suppress_at[i] {
                        PlanEntry::Nothing
                    } else {
                        PlanEntry::Paint {
                            paint: paint.clone(),
                            slots: plan.open.clone(),
                        }
                    });
                }
            }
        }
        plan
    }

    fn apply(&mut self, op: &Operator, open: &mut Vec<OpenSlot>) {
        match op {
            Operator::Trim(trim) => {
                let shapes: Vec<BezPath> = open
                    .iter()
                    .map(|&(id, _)| self.slots[id].path.clone())
                    .collect();
                for ((id, _), path) in open.iter().zip(trim_shapes(&shapes, trim)) {
                    self.slots[*id].path = path;
                }
            }
            Operator::Merge(_) => {
                // Boolean modes need the path finder (planned); until then every
                // mode combines the paths like `merge` (the parser warns).
                let shapes: Vec<BezPath> = open
                    .iter()
                    .map(|&(id, a)| a * &self.slots[id].path)
                    .collect();
                for &(id, _) in open.iter() {
                    self.slots[id].path = BezPath::new();
                }
                self.slots.push(Slot {
                    path: merge_concat(&shapes),
                });
                *open = vec![(self.slots.len() - 1, Affine::IDENTITY)];
            }
            Operator::Unsupported(_) => {}
        }
    }

    /// Pass 2: draws, back to front, with `to_layer` = this group's space → layer.
    fn emit(&self, plan: &GroupPlan, to_layer: Affine) -> Vec<DrawItem> {
        // Timeline order → runs: an "Above previous" paint joins the run of the
        // item above it and is drawn right after it.
        let mut runs: Vec<Vec<DrawItem>> = Vec::new();
        for entry in &plan.entries {
            let (draws, composite) = match entry {
                PlanEntry::Nothing => (Vec::new(), Composite::Below),
                PlanEntry::Paint { paint, slots } => {
                    (self.paint_draw(paint, slots, to_layer), paint.composite)
                }
                PlanEntry::Group {
                    plan,
                    matrix,
                    opacity,
                    blend,
                    composite,
                } => (
                    wrap_group(self.emit(plan, to_layer * *matrix), *opacity, *blend),
                    *composite,
                ),
            };
            match (composite, runs.last_mut()) {
                (Composite::Above, Some(run)) => run.extend(draws),
                _ => runs.push(draws),
            }
        }
        runs.into_iter().rev().flatten().collect()
    }

    fn paint_draw(&self, paint: &Paint, slots: &[OpenSlot], to_layer: Affine) -> Vec<DrawItem> {
        let mut path = BezPath::new();
        for &(id, a) in slots {
            let p = &self.slots[id].path;
            if a == Affine::IDENTITY {
                path.extend(p.iter());
            } else {
                path.extend((a * p).iter());
            }
        }
        if path.is_empty() || paint.opacity <= 0.0 {
            return Vec::new();
        }
        let opacity = paint.opacity as f32;
        let item = match &paint.kind {
            PaintKind::Fill(rule) => DrawItem::Fill(FillDraw {
                path,
                transform: to_layer,
                rule: *rule,
                brush: paint.brush.clone(),
                opacity,
            }),
            PaintKind::Stroke(style) => {
                if style.width <= 0.0 {
                    return Vec::new();
                }
                DrawItem::Stroke(StrokeDraw {
                    path,
                    transform: to_layer,
                    style: style.clone(),
                    brush: paint.brush.clone(),
                    opacity,
                })
            }
        };
        if paint.blend == BlendMode::Normal {
            vec![item]
        } else {
            vec![DrawItem::Layer(LayerDraw {
                opacity: 1.0,
                blend: paint.blend,
                items: vec![item],
            })]
        }
    }
}

#[cfg(test)]
mod tests {
    use kurbo::Point;

    use super::*;
    use crate::content::{GroupTransform, MergeMode, PathItem, PathSource, TrimPaths};
    use crate::primitive::{Direction, EllipseShape, RectShape};

    fn rect(size: f64, x: f64) -> ShapeItem {
        ShapeItem::Path(PathItem {
            source: PathSource::Rect(RectShape {
                size: [size, size],
                position: [x, 0.0],
                roundness: 0.0,
            }),
            direction: Direction::Normal,
        })
    }

    fn fill(color: [f32; 4]) -> Paint {
        Paint {
            kind: PaintKind::Fill(FillRule::NonZero),
            brush: Brush::Solid(color),
            opacity: 1.0,
            blend: BlendMode::Normal,
            composite: Composite::Below,
        }
    }

    fn stroke(width: f64) -> Paint {
        Paint {
            kind: PaintKind::Stroke(StrokeStyle {
                width,
                ..StrokeStyle::default()
            }),
            ..fill([1.0; 4])
        }
    }

    const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

    fn color_of(item: &DrawItem) -> [f32; 4] {
        match item {
            DrawItem::Fill(FillDraw {
                brush: Brush::Solid(c),
                ..
            }) => *c,
            other => panic!("not a solid fill: {other:?}"),
        }
    }

    #[test]
    fn paints_draw_paths_above_them_bottom_to_top() {
        let root = ShapeGroup::new()
            .with(rect(10.0, 0.0))
            .with(ShapeItem::Paint(fill(RED)))
            .with(rect(10.0, 100.0))
            .with(ShapeItem::Paint(fill(BLUE)));
        let scene = VectorScene::from_group(&root);
        assert_eq!(scene.items.len(), 2);
        // Blue (lower in the list) is drawn first and sees both rectangles.
        assert_eq!(color_of(&scene.items[0]), BLUE);
        assert_eq!(color_of(&scene.items[1]), RED);
        let DrawItem::Fill(blue) = &scene.items[0] else {
            unreachable!()
        };
        assert_eq!(crate::path::subpath_count(&blue.path), 2);
        let DrawItem::Fill(red) = &scene.items[1] else {
            unreachable!()
        };
        assert_eq!(crate::path::subpath_count(&red.path), 1);
    }

    #[test]
    fn paints_above_all_paths_draw_nothing() {
        let root = ShapeGroup::new()
            .with(ShapeItem::Paint(fill(RED)))
            .with(rect(10.0, 0.0));
        assert!(VectorScene::from_group(&root).is_empty());
    }

    #[test]
    fn group_transform_and_parent_paint() {
        let group = ShapeGroup {
            transform: GroupTransform {
                position: [50.0, 0.0],
                scale: [200.0, 200.0],
                ..GroupTransform::default()
            },
            ..ShapeGroup::new()
        }
        .with(rect(10.0, 0.0))
        .with(ShapeItem::Paint(fill(RED)));
        let root = ShapeGroup::new()
            .with(ShapeItem::Group(group))
            .with(ShapeItem::Paint(fill(BLUE)));
        let scene = VectorScene::from_group(&root);
        assert_eq!(scene.items.len(), 2);
        // The parent's fill sees the group's rectangle transformed.
        let b = scene.items[0].bounds(false).unwrap();
        assert_eq!(b, Rect::new(40.0, -10.0, 60.0, 10.0));
        // The group's own fill carries the group matrix as its transform.
        let DrawItem::Fill(red) = &scene.items[1] else {
            unreachable!()
        };
        assert_eq!(red.transform * Point::new(5.0, 5.0), Point::new(60.0, 10.0));
        assert_eq!(
            scene.bounds(false),
            Some(Rect::new(40.0, -10.0, 60.0, 10.0))
        );
    }

    #[test]
    fn trim_below_a_paint_still_trims_it() {
        let root = ShapeGroup::new()
            .with(ShapeItem::Path(PathItem {
                source: PathSource::Ellipse(EllipseShape::default()),
                direction: Direction::Normal,
            }))
            .with(ShapeItem::Paint(stroke(4.0)))
            .with(ShapeItem::Operator(Operator::Trim(TrimPaths {
                end: 25.0,
                ..TrimPaths::default()
            })));
        let scene = VectorScene::from_group(&root);
        let DrawItem::Stroke(s) = &scene.items[0] else {
            panic!("expected a stroke")
        };
        // A quarter circle from 12 o'clock clockwise.
        let r = s.path.bounding_box();
        assert!(
            (r.x0 - 0.0).abs() < 1e-6 && (r.y1 - 0.0).abs() < 1e-3,
            "{r:?}"
        );
        assert!((r.x1 - 50.0).abs() < 1e-3 && (r.y0 + 50.0).abs() < 1e-6);
    }

    #[test]
    fn enclosing_operator_applies_to_nested_paints() {
        let group = ShapeGroup::new()
            .with(rect(100.0, 0.0))
            .with(ShapeItem::Paint(stroke(1.0)));
        let root = ShapeGroup::new()
            .with(ShapeItem::Group(group))
            .with(ShapeItem::Operator(Operator::Trim(TrimPaths {
                end: 0.0,
                ..TrimPaths::default()
            })));
        assert!(VectorScene::from_group(&root).is_empty());
    }

    #[test]
    fn merge_suppresses_paints_above_it() {
        let root = ShapeGroup::new()
            .with(rect(10.0, 0.0))
            .with(ShapeItem::Paint(fill(RED)))
            .with(rect(10.0, 50.0))
            .with(ShapeItem::Operator(Operator::Merge(MergeMode::Merge)))
            .with(ShapeItem::Paint(fill(BLUE)));
        let scene = VectorScene::from_group(&root);
        assert_eq!(scene.items.len(), 1);
        assert_eq!(color_of(&scene.items[0]), BLUE);
    }

    #[test]
    fn composite_above_moves_paint_in_front() {
        let mut blue = fill(BLUE);
        blue.composite = Composite::Above;
        let root = ShapeGroup::new()
            .with(rect(10.0, 0.0))
            .with(ShapeItem::Paint(fill(RED)))
            .with(ShapeItem::Paint(blue));
        let scene = VectorScene::from_group(&root);
        assert_eq!(color_of(&scene.items[0]), RED);
        assert_eq!(color_of(&scene.items[1]), BLUE);
    }

    #[test]
    fn group_opacity_isolates_only_when_needed() {
        let faded = |items: Vec<ShapeItem>| ShapeGroup {
            transform: GroupTransform {
                opacity: 50.0,
                ..GroupTransform::default()
            },
            items,
            ..ShapeGroup::new()
        };
        let single = ShapeGroup::new().with(ShapeItem::Group(faded(vec![
            rect(10.0, 0.0),
            ShapeItem::Paint(fill(RED)),
        ])));
        let scene = VectorScene::from_group(&single);
        let DrawItem::Fill(f) = &scene.items[0] else {
            panic!("single draw is not isolated")
        };
        assert_eq!(f.opacity, 0.5);
        let double = ShapeGroup::new().with(ShapeItem::Group(faded(vec![
            rect(10.0, 0.0),
            ShapeItem::Paint(fill(RED)),
            ShapeItem::Paint(stroke(2.0)),
        ])));
        let scene = VectorScene::from_group(&double);
        assert!(
            matches!(&scene.items[0], DrawItem::Layer(l) if l.opacity == 0.5 && l.items.len() == 2)
        );
    }

    #[test]
    fn bounds_include_stroke_extents() {
        let root = ShapeGroup::new()
            .with(rect(10.0, 0.0))
            .with(ShapeItem::Paint(Paint {
                kind: PaintKind::Stroke(StrokeStyle {
                    width: 4.0,
                    join: crate::paint::LineJoin::Round,
                    ..StrokeStyle::default()
                }),
                ..fill(RED)
            }));
        let scene = VectorScene::from_group(&root);
        assert_eq!(scene.bounds(false), Some(Rect::new(-5.0, -5.0, 5.0, 5.0)));
        assert_eq!(scene.bounds(true), Some(Rect::new(-7.0, -7.0, 7.0, 7.0)));
        let hash = scene.content_hash();
        let mut moved = scene.clone();
        moved.transform(Affine::translate((1.0, 0.0)));
        assert_ne!(moved.content_hash(), hash);
        assert_eq!(scene.clone().content_hash(), hash);
    }
}
