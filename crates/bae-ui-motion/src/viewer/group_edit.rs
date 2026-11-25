//! A shape group selected in the viewer (AE: double-click a shape with the
//! Selection tool). Its box shows with handles: dragging inside moves the
//! group, a corner or edge handle scales it around the group's anchor (Shift:
//! the larger ratio on both axes), the Rotation tool rotates it. One
//! `attr.set_many` per drag on the group's Transform.

use bae_anim::glam::{DMat4, DVec2};
use bae_anim::{EvaluatedLayer, EvaluatedPrim};
use bae_model::{PrimPath, PrimType, Stage, Time, Value};
use bae_motion::query;
use egui::{Color32, Painter, Pos2, Rect, Stroke, vec2};
use serde_json::json;

use crate::render::{self, View};

/// A shape group and where it sits: parent space → layer space.
pub(super) struct Found<'a> {
    pub group: &'a EvaluatedPrim,
    pub parent: DMat4,
}

fn find<'a>(items: &'a [EvaluatedPrim], path: &PrimPath, m: DMat4) -> Option<Found<'a>> {
    for item in items {
        if &item.path == path {
            return Some(Found {
                group: item,
                parent: m,
            });
        }
        let inner = if item.prim_type.as_ref() == Some(&PrimType::SHAPE_GROUP) {
            m * render::group_matrix(item)
        } else {
            m
        };
        if let Some(f) = find(&item.children, path, inner) {
            return Some(f);
        }
    }
    None
}

/// The group `path` in `layer`'s contents.
pub(super) fn group<'a>(layer: &'a EvaluatedLayer, path: &PrimPath) -> Option<Found<'a>> {
    find(&layer.contents, path, DMat4::IDENTITY)
}

/// The group's content bounds in its own space.
fn bounds(g: &EvaluatedPrim) -> Option<(DVec2, DVec2)> {
    let polys = render::shape_paths(&g.children);
    let mut pts = polys.iter().flat_map(|p| p.points.iter().copied());
    let first = pts.next()?;
    Some(pts.fold((first, first), |(a, b), p| (a.min(p), b.max(p))))
}

/// The group's box on screen: corners clockwise from the top left.
pub(super) fn quad(layer: &EvaluatedLayer, found: &Found<'_>, view: &View) -> Option<[Pos2; 4]> {
    let (a, b) = bounds(found.group)?;
    let m = layer.world_matrix * found.parent * render::group_matrix(found.group);
    let c = [a, DVec2::new(b.x, a.y), b, DVec2::new(a.x, b.y)];
    Some(c.map(|p| {
        let q = m.transform_point3(p.extend(0.0));
        view.to_screen(DVec2::new(q.x, q.y))
    }))
}

/// The topmost top-level group of `layer` whose box contains `pos`.
pub(super) fn group_at(layer: &EvaluatedLayer, view: &View, pos: Pos2) -> Option<PrimPath> {
    fn groups<'a>(items: &'a [EvaluatedPrim], out: &mut Vec<&'a EvaluatedPrim>) {
        for item in items {
            if item.prim_type.as_ref() == Some(&PrimType::SHAPE_GROUP) {
                out.push(item);
            } else if item.prim_type.as_ref() == Some(&PrimType::FOLDER) || item.prim_type.is_none()
            {
                groups(&item.children, out);
            }
        }
    }
    let mut all = Vec::new();
    groups(&layer.contents, &mut all);
    all.into_iter().find_map(|g| {
        let f = group(layer, &g.path)?;
        let q = quad(layer, &f, view)?;
        render::quad_contains(&q, pos).then(|| g.path.clone())
    })
}

/// The eight handles (corners and edge midpoints, clockwise from the top left).
pub(super) fn handles(q: &[Pos2; 4]) -> [Pos2; 8] {
    let mut out = [Pos2::ZERO; 8];
    for i in 0..4 {
        out[2 * i] = q[i];
        out[2 * i + 1] = q[i] + (q[(i + 1) % 4] - q[i]) / 2.0;
    }
    out
}

pub(super) fn paint(painter: &Painter, q: &[Pos2; 4], view: &View, color: Color32) {
    painter.add(egui::Shape::closed_line(
        q.to_vec(),
        Stroke::new(1.0, color),
    ));
    for h in handles(q) {
        painter.rect_stroke(
            Rect::from_center_size(h, vec2(view.point, view.point)),
            0.0,
            Stroke::new(1.0, color),
            egui::StrokeKind::Middle,
        );
    }
}

fn v2(p: &EvaluatedPrim, attr: &str, d: [f64; 2]) -> DVec2 {
    match p.get(attr) {
        Some(Value::Vec2(v)) => DVec2::from_array(*v),
        _ => DVec2::from_array(d),
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Move,
    /// Handle index (see [`handles`]).
    Scale(usize),
    Rotate,
}

/// A drag of the selected group.
pub(super) struct GroupDrag {
    pub layer: PrimPath,
    pub group: PrimPath,
    kind: Kind,
    /// Comp pixels where it started.
    start: DVec2,
    /// Layer world × parent: the group's parent space → comp.
    parent_world: DMat4,
    position: DVec2,
    anchor: DVec2,
    scale: DVec2,
    rotation: f64,
    /// The grabbed handle in the group's own space (Scale).
    handle: DVec2,
    last: f64,
    pub now: (DVec2, DVec2, f64),
}

impl GroupDrag {
    pub fn new(
        layer: &EvaluatedLayer,
        found: &Found<'_>,
        kind: Kind,
        start: DVec2,
    ) -> Option<Self> {
        let g = found.group;
        let (a, b) = bounds(g)?;
        let handle = match kind {
            Kind::Scale(h) => {
                let c = [a, DVec2::new(b.x, a.y), b, DVec2::new(a.x, b.y)];
                let i = h / 2;
                if h % 2 == 0 {
                    c[i]
                } else {
                    (c[i] + c[(i + 1) % 4]) / 2.0
                }
            }
            _ => DVec2::ZERO,
        };
        let position = v2(g, "transform:position", [0.0; 2]);
        let scale = v2(g, "transform:scale", [100.0; 2]);
        let rotation = g.f64("transform:rotation").unwrap_or(0.0);
        Some(GroupDrag {
            layer: layer.path.clone(),
            group: g.path.clone(),
            kind,
            start,
            parent_world: layer.world_matrix * found.parent,
            position,
            anchor: v2(g, "transform:anchor_point", [0.0; 2]),
            scale,
            rotation,
            handle,
            last: 0.0,
            now: (position, scale, rotation),
        })
    }

    /// Comp point → the group's parent space.
    fn to_parent(&self, p: DVec2) -> DVec2 {
        let q = self.parent_world.inverse().transform_point3(p.extend(0.0));
        DVec2::new(q.x, q.y)
    }

    pub fn update(&mut self, p: DVec2, shift: bool) {
        let (pos, mut scale, mut rot) = (self.position, self.scale, self.rotation);
        let mut new_pos = pos;
        match self.kind {
            Kind::Move => {
                let mut d = self.to_parent(p) - self.to_parent(self.start);
                if shift {
                    // AE 26: |dx| ≥ |dy| locks horizontally.
                    if (p - self.start).x.abs() >= (p - self.start).y.abs() {
                        d.y = 0.0;
                    } else {
                        d.x = 0.0;
                    }
                }
                new_pos = pos + d;
            }
            Kind::Scale(h) => {
                // The pointer in the group's rotated, unscaled frame at its anchor.
                let q = DVec2::from_angle(-rot.to_radians()).rotate(self.to_parent(p) - pos);
                let d = self.handle - self.anchor;
                let ratio = |q: f64, d: f64, s: f64| if d.abs() > 1e-9 { q / d * 100.0 } else { s };
                let (sx, sy) = (ratio(q.x, d.x, scale.x), ratio(q.y, d.y, scale.y));
                let corner = h % 2 == 0;
                let horizontal = h == 3 || h == 7;
                if corner {
                    scale = if shift {
                        let k = if (sx / self.scale.x).abs() >= (sy / self.scale.y).abs() {
                            sx / self.scale.x
                        } else {
                            sy / self.scale.y
                        };
                        self.scale * k
                    } else {
                        DVec2::new(sx, sy)
                    };
                } else if horizontal {
                    scale.x = sx;
                    if shift {
                        scale.y = self.scale.y * sx / self.scale.x;
                    }
                } else {
                    scale.y = sy;
                    if shift {
                        scale.x = self.scale.x * sy / self.scale.y;
                    }
                }
            }
            Kind::Rotate => {
                let c = self.parent_world.transform_point3(pos.extend(0.0));
                let c = DVec2::new(c.x, c.y);
                let a0 = (self.start - c).to_angle().to_degrees();
                let mut delta = (p - c).to_angle().to_degrees() - a0;
                delta += ((self.last - delta) / 360.0).round() * 360.0;
                self.last = delta;
                rot = self.rotation + delta;
                if shift {
                    rot = (rot / 45.0).round() * 45.0;
                }
            }
        }
        self.now = (new_pos, scale, rot);
    }

    pub fn changed(&self) -> bool {
        self.now != (self.position, self.scale, self.rotation)
    }

    /// Show the drag: the group's Transform in `layer`'s evaluated contents
    /// (copied on write: the evaluator shares static contents).
    pub fn preview(&self, layer: &mut EvaluatedLayer) {
        fn holds(items: &[EvaluatedPrim], group: &PrimPath) -> bool {
            items
                .iter()
                .any(|i| i.path == *group || holds(&i.children, group))
        }
        fn patch(items: &mut std::sync::Arc<[EvaluatedPrim]>, d: &GroupDrag) -> bool {
            let Some(i) = items
                .iter()
                .position(|item| item.path == d.group || holds(&item.children, &d.group))
            else {
                return false;
            };
            let item = &mut std::sync::Arc::make_mut(items)[i];
            if item.path != d.group {
                return patch(&mut item.children, d);
            }
            let (p, s, r) = d.now;
            let attrs = std::sync::Arc::make_mut(&mut item.attrs);
            attrs.insert("transform:position".into(), Value::Vec2(p.to_array()));
            attrs.insert("transform:scale".into(), Value::Vec2(s.to_array()));
            attrs.insert("transform:rotation".into(), Value::Float(r));
            true
        }
        patch(&mut layer.contents, self);
    }

    /// The `attr.set_many` edits.
    pub fn commit(&self, stage: &Stage, time: Time) -> Vec<serde_json::Value> {
        let t = query::to_key_time(stage, &self.layer, time);
        let (p, s, r) = self.now;
        let mut out = Vec::new();
        let path = &self.group;
        if p != self.position {
            out.push(json!({"path": path, "attr": "transform:position", "time": t, "value": Value::Vec2(p.to_array())}));
        }
        if s != self.scale {
            out.push(json!({"path": path, "attr": "transform:scale", "time": t, "value": Value::Vec2(s.to_array())}));
        }
        if r != self.rotation {
            out.push(json!({"path": path, "attr": "transform:rotation", "time": t, "value": Value::Float(r)}));
        }
        out
    }
}
