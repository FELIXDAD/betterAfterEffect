//! Transform drags of the selected layers in the Composition viewer: move
//! (Selection tool, with snapping), scale by a bounding-box handle (Selection
//! tool), rotate (Rotation tool) and move the anchor point (Pan Behind tool).
//! The drag is previewed by overriding the evaluated transforms and becomes one
//! `attr.set_many` on release (one undo step). Arrow-key nudging is here too.

use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_anim::{EvaluatedComp, EvaluatedLayer, Transform};
use bae_model::{LayerKind, PrimPath, Stage, Time, Value};
use bae_motion::query::{self, CompInfo};
use bae_ui::PanelContext;
use egui::{Color32, Painter, Pos2, Rect, Stroke};
use serde_json::json;

use bae_render::Camera;

use crate::render::{self, Affine, View};
use crate::session::SharedSession;

/// Snapping distance on screen (points): AE 26 snaps at 13.3 DIP, not at 16
/// (R1, transform-tools.md §4).
pub(super) const SNAP_DISTANCE: f32 = 14.0;
/// Pick radius of bounding-box handles and the anchor point (points).
pub(super) const HANDLE_PICK: f32 = 5.0;
pub(super) const ANCHOR_PICK: f32 = 8.0;

/// One layer a drag changes.
pub(super) struct LayerEdit {
    pub path: PrimPath,
    pub start: Transform,
    pub now: Transform,
    /// Parent space → comp space.
    parent: DMat4,
    three_d: bool,
}

impl LayerEdit {
    fn of(stage: &Stage, comp: &EvaluatedComp, l: &EvaluatedLayer) -> Self {
        let _ = stage;
        LayerEdit {
            path: l.path.clone(),
            start: l.transform,
            now: l.transform,
            parent: l
                .parent
                .map_or(DMat4::IDENTITY, |i| comp.layers[i].world_matrix),
            three_d: l.switches.three_d,
        }
    }

    /// A comp-space vector in parent space.
    fn to_parent(&self, d: DVec2) -> DVec3 {
        self.parent
            .inverse()
            .transform_vector3(DVec3::new(d.x, d.y, 0.0))
    }

    /// Layer space → comp space at the start of the drag.
    fn world(&self) -> DMat4 {
        self.parent * self.start.matrix(self.three_d)
    }
}

/// A bounding-box handle: corners and edge midpoints clockwise from the top left
/// (0 top left, 1 top, 2 top right, 3 right, 4 bottom right, 5 bottom, 6 bottom
/// left, 7 left).
pub(super) type Handle = usize;

/// Layer-space position of `handle` on `bounds`, and which axes it scales.
fn handle_point(bounds: (DVec2, DVec2), handle: Handle) -> (DVec2, bool, bool) {
    let (a, b) = bounds;
    let c = [a, DVec2::new(b.x, a.y), b, DVec2::new(a.x, b.y)];
    let i = handle / 2;
    if handle.is_multiple_of(2) {
        (c[i], true, true)
    } else {
        let m = (c[i] + c[(i + 1) % 4]) / 2.0;
        // Handles 1 and 5 are on the top and bottom edges.
        (m, !i.is_multiple_of(2), i.is_multiple_of(2))
    }
}

/// The screen positions of a layer's handles (see [`Handle`]).
pub(super) fn handles(l: &EvaluatedLayer, view: &View) -> Option<[Pos2; 8]> {
    let q = render::bounds_quad(l, view, &Affine::new(DMat4::IDENTITY))?;
    let mut out = [Pos2::ZERO; 8];
    for i in 0..4 {
        out[2 * i] = q[i];
        out[2 * i + 1] = q[i] + (q[(i + 1) % 4] - q[i]) / 2.0;
    }
    Some(out)
}

/// The anchor point of a layer on screen (far off screen behind the camera).
pub(super) fn anchor_screen(l: &EvaluatedLayer, view: &View) -> Pos2 {
    let a = l.world_matrix.transform_point3(l.transform.anchor_point);
    view.project(a, render::is_3d(l))
        .unwrap_or(Pos2::new(f32::MAX, f32::MAX))
}

/// What a drag does.
pub(super) enum Kind {
    Move {
        start: DVec2,
    },
    Scale {
        handle: Handle,
        bounds: (DVec2, DVec2),
    },
    Rotate {
        center: DVec2,
        start: DVec2,
        /// Unwrapped angle change of the last frame (degrees).
        last: f64,
        /// Set Orientation (not Rotation) on 3D layers.
        orientation: bool,
    },
    Anchor {
        start: DVec2,
        /// Dragging the anchor point: Position compensates so the layer stays;
        /// otherwise the content pans behind the anchor.
        compensate: bool,
    },
    /// A 3D layer's axis arrow: Position along `dir` (comp space, unit).
    Axis {
        axis: usize,
        origin: DVec3,
        dir: DVec3,
        camera: Box<Camera>,
        size: DVec2,
        /// Where along the axis the drag started.
        start: f64,
    },
    /// A 3D layer's rotation ring: X / Y / Z Rotation (or Orientation) `axis`.
    Spin {
        axis: usize,
        center: DVec2,
        start: DVec2,
        last: f64,
        /// +1 or −1: a clockwise drag turns this way about the axis.
        sign: f64,
        orientation: bool,
    },
}

/// The plane a 3D layer moves in when dragged: parallel to the view at the
/// grabbed layer's depth.
#[derive(Clone, Copy)]
pub(super) struct ViewPlane {
    pub camera: Camera,
    pub size: DVec2,
    pub depth: f64,
}

/// The point that snaps and what it snaps to (comp space).
struct Snapping {
    feature: DVec2,
    targets: Vec<DVec2>,
}

/// The 9 snap features of a layer: bounds corners, edge midpoints and center
/// (comp space).
fn features(l: &EvaluatedLayer, out: &mut Vec<DVec2>) {
    if let Some((a, b)) = render::layer_bounds(l) {
        let m = Affine::new(l.world_matrix);
        let c = (a + b) / 2.0;
        for p in [
            a,
            DVec2::new(c.x, a.y),
            DVec2::new(b.x, a.y),
            DVec2::new(b.x, c.y),
            b,
            DVec2::new(c.x, b.y),
            DVec2::new(a.x, b.y),
            DVec2::new(a.x, c.y),
            c,
        ] {
            out.push(m.apply(p));
        }
    }
}

/// A drag of the selected layers.
pub(super) struct TransformDrag {
    pub edits: Vec<LayerEdit>,
    pub kind: Kind,
    snapping: Option<Snapping>,
    /// The point snapped to in the last frame (comp space).
    guide: Option<DVec2>,
    /// 3D layers move in this plane (Move).
    plane: Option<ViewPlane>,
}

impl TransformDrag {
    /// A drag of `layers` (they must be in `comp`).
    pub fn new(stage: &Stage, comp: &EvaluatedComp, layers: &[PrimPath], kind: Kind) -> Self {
        let edits = comp
            .layers
            .iter()
            .filter(|l| layers.contains(&l.path))
            .map(|l| LayerEdit::of(stage, comp, l))
            .collect();
        TransformDrag {
            edits,
            kind,
            snapping: None,
            guide: None,
            plane: None,
        }
    }

    /// 3D layers move parallel to the view (through `plane`'s camera).
    pub fn with_plane(mut self, plane: Option<ViewPlane>) -> Self {
        self.plane = plane;
        self
    }

    /// Snapping as AE 26 does it (R1, transform-tools.md §4): only one point
    /// snaps — for a move, the 9-point feature of the grabbed layer nearest the
    /// grab point; for an anchor drag, the anchor — point to point to the 9
    /// features of the other visible layers (and, dragging the anchor, of its own
    /// layer). The comp frame is not a target.
    pub fn with_snapping(mut self, comp: &EvaluatedComp, primary: &PrimPath, grab: DVec2) -> Self {
        let anchor = matches!(
            self.kind,
            Kind::Anchor {
                compensate: true,
                ..
            }
        );
        if !matches!(self.kind, Kind::Move { .. }) && !anchor {
            return self;
        }
        let moving: Vec<&PrimPath> = self.edits.iter().map(|e| &e.path).collect();
        let mut own = Vec::new();
        let mut targets = Vec::new();
        for l in &comp.layers {
            if moving.contains(&&l.path) {
                if &l.path == primary {
                    features(l, &mut own);
                }
            } else if l.visible && !matches!(l.kind, LayerKind::Camera | LayerKind::Light) {
                features(l, &mut targets);
            }
        }
        let feature = if anchor {
            targets.extend(own);
            grab
        } else {
            match own
                .iter()
                .min_by(|a, b| a.distance(grab).total_cmp(&b.distance(grab)))
            {
                Some(f) => *f,
                None => return self,
            }
        };
        self.snapping = Some(Snapping { feature, targets });
        self
    }

    /// Follow the pointer (comp space). `snap`: snapping distance in comp
    /// pixels, when snapping is on for this frame.
    pub fn update(&mut self, p: DVec2, shift: bool, snap: Option<f64>) {
        self.guide = None;
        // Move and anchor drags: the pointer offset, axis-locked with Shift
        // (moves), then snapped.
        let offset = match self.kind {
            Kind::Move { start } | Kind::Anchor { start, .. } => {
                let lock = shift && matches!(self.kind, Kind::Move { .. });
                let axis = |mut d: DVec2| {
                    if lock {
                        if (p - start).x.abs() > (p - start).y.abs() {
                            d.y = 0.0;
                        } else {
                            d.x = 0.0;
                        }
                    }
                    d
                };
                let mut d = axis(p - start);
                if let Some(t) = self.snap_target(d, snap) {
                    d = axis(t - self.snapping.as_ref().expect("snapping").feature);
                    self.guide = Some(t);
                }
                d
            }
            _ => DVec2::ZERO,
        };
        match &mut self.kind {
            Kind::Move { start } => {
                let d = offset;
                let start = *start;
                let world = self.plane.map(|v| {
                    v.camera.unproject(start + d, v.depth, v.size)
                        - v.camera.unproject(start, v.depth, v.size)
                });
                for e in &mut self.edits {
                    e.now.position = e.start.position
                        + match world {
                            Some(w) if e.three_d => e.parent.inverse().transform_vector3(w),
                            _ => e.to_parent(d),
                        };
                }
            }
            Kind::Axis {
                origin,
                dir,
                camera,
                size,
                start,
                ..
            } => {
                let Some(t) = super::gizmo3d::along(camera, *size, p, *origin, *dir) else {
                    return;
                };
                let w = *dir * (t - *start);
                for e in &mut self.edits {
                    e.now.position = e.start.position + e.parent.inverse().transform_vector3(w);
                }
            }
            Kind::Spin {
                axis,
                center,
                start,
                last,
                sign,
                orientation,
            } => {
                let a0 = (*start - *center).to_angle().to_degrees();
                let a1 = (p - *center).to_angle().to_degrees();
                let mut delta = a1 - a0;
                delta += ((*last - delta) / 360.0).round() * 360.0;
                *last = delta;
                delta *= *sign;
                let i = *axis;
                let pick = |t: &Transform| {
                    if *orientation {
                        t.orientation[i]
                    } else {
                        t.rotation[i]
                    }
                };
                if shift && let Some(e0) = self.edits.first() {
                    let r0 = pick(&e0.start);
                    delta = ((r0 + delta) / 45.0).round() * 45.0 - r0;
                }
                for e in &mut self.edits {
                    if *orientation {
                        e.now.orientation[i] = e.start.orientation[i] + delta;
                    } else {
                        e.now.rotation[i] = e.start.rotation[i] + delta;
                    }
                }
            }
            Kind::Scale { handle, bounds } => {
                let Some(e0) = self.edits.first() else {
                    return;
                };
                let (hp, ax, ay) = handle_point(*bounds, *handle);
                // The pointer in the layer's space as if its scale were 100 %.
                let mut unit = e0.start;
                unit.scale = DVec3::new(100.0, 100.0, unit.scale.z);
                let m = e0.parent * unit.matrix(e0.three_d);
                let x = m.inverse().transform_point3(DVec3::new(p.x, p.y, 0.0));
                let a = e0.start.anchor_point;
                let q = DVec2::new(x.x - a.x, x.y - a.y);
                let d = DVec2::new(hp.x - a.x, hp.y - a.y);
                let s0 = e0.start.scale;
                let (mut rx, mut ry) = (1.0, 1.0);
                if shift {
                    // Keep the proportions: the handle moves along its line
                    // through the anchor.
                    let dd = DVec2::new(
                        if ax { s0.x * d.x / 100.0 } else { 0.0 },
                        if ay { s0.y * d.y / 100.0 } else { 0.0 },
                    );
                    let qq = DVec2::new(if ax { q.x } else { 0.0 }, if ay { q.y } else { 0.0 });
                    if dd.length_squared() > 1e-12 {
                        let k = qq.dot(dd) / dd.length_squared();
                        (rx, ry) = (k, k);
                    }
                } else {
                    if ax && d.x.abs() > 1e-9 && s0.x.abs() > 1e-9 {
                        rx = 100.0 * q.x / d.x / s0.x;
                    }
                    if ay && d.y.abs() > 1e-9 && s0.y.abs() > 1e-9 {
                        ry = 100.0 * q.y / d.y / s0.y;
                    }
                }
                for e in &mut self.edits {
                    let s = e.start.scale;
                    e.now.scale = DVec3::new(s.x * rx, s.y * ry, s.z);
                }
            }
            Kind::Rotate {
                center,
                start,
                last,
                orientation,
            } => {
                let a0 = (*start - *center).to_angle().to_degrees();
                let a1 = (p - *center).to_angle().to_degrees();
                let mut delta = a1 - a0;
                // Unwrap: the change closest to last frame's.
                delta += ((*last - delta) / 360.0).round() * 360.0;
                *last = delta;
                if let Some(e0) = self.edits.first() {
                    let r0 = if *orientation && e0.three_d {
                        e0.start.orientation.z
                    } else {
                        e0.start.rotation.z
                    };
                    if shift {
                        // 45° steps of the resulting rotation.
                        delta = ((r0 + delta) / 45.0).round() * 45.0 - r0;
                    }
                }
                for e in &mut self.edits {
                    if *orientation && e.three_d {
                        e.now.orientation.z = e.start.orientation.z + delta;
                    } else {
                        e.now.rotation.z = e.start.rotation.z + delta;
                    }
                }
            }
            Kind::Anchor { compensate, .. } => {
                let compensate = *compensate;
                let d = offset;
                for e in &mut self.edits {
                    let local = e
                        .world()
                        .inverse()
                        .transform_vector3(DVec3::new(d.x, d.y, 0.0));
                    if compensate {
                        e.now.anchor_point = e.start.anchor_point + local;
                        e.now.position = e.start.position + e.to_parent(d);
                    } else {
                        e.now.anchor_point = e.start.anchor_point - local;
                    }
                }
            }
        }
    }

    /// The target within `snap` comp pixels of the snapping point moved by `d`.
    fn snap_target(&self, d: DVec2, snap: Option<f64>) -> Option<DVec2> {
        let (thr, s) = (snap?, self.snapping.as_ref()?);
        let f = s.feature + d;
        s.targets
            .iter()
            .filter(|t| t.distance(f) <= thr)
            .min_by(|a, b| a.distance(f).total_cmp(&b.distance(f)))
            .copied()
    }

    /// The snapped-to point: a small target mark.
    pub fn paint_guides(&self, painter: &Painter, view: &View, _info: &CompInfo) {
        let stroke = Stroke::new(1.0, Color32::from_rgb(0x52, 0xb8, 0xff));
        if let Some(t) = self.guide {
            let c = view.to_screen(t);
            painter.circle_stroke(c, 5.0, stroke);
            painter.line_segment([c - egui::vec2(8.0, 0.0), c + egui::vec2(8.0, 0.0)], stroke);
            painter.line_segment([c - egui::vec2(0.0, 8.0), c + egui::vec2(0.0, 8.0)], stroke);
        }
    }

    /// The gizmo axis an axis or ring drag uses.
    pub fn axis(&self) -> Option<usize> {
        match self.kind {
            Kind::Axis { axis, .. } | Kind::Spin { axis, .. } => Some(axis),
            _ => None,
        }
    }

    /// Whether anything changed.
    pub fn changed(&self) -> bool {
        self.edits.iter().any(|e| e.now != e.start)
    }

    /// The `attr.set_many` edits of the drag.
    pub fn commit(&self, stage: &Stage, time: Time) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        for e in &self.edits {
            let t = query::to_key_time(stage, &e.path, time);
            let mut push = |attr: &str, v: Value| {
                out.push(json!({"path": e.path, "attr": attr, "time": t, "value": v}));
            };
            let v3 = |v: DVec3| Value::Vec3([v.x, v.y, v.z]);
            if e.now.position != e.start.position {
                position_edits(stage, &e.path, e.now.position, &mut push);
            }
            if e.now.anchor_point != e.start.anchor_point {
                push("transform:anchor_point", v3(e.now.anchor_point));
            }
            if e.now.scale != e.start.scale {
                push("transform:scale", v3(e.now.scale));
            }
            if e.now.rotation.x != e.start.rotation.x {
                push("transform:rotation_x", Value::Float(e.now.rotation.x));
            }
            if e.now.rotation.y != e.start.rotation.y {
                push("transform:rotation_y", Value::Float(e.now.rotation.y));
            }
            if e.now.rotation.z != e.start.rotation.z {
                push("transform:rotation", Value::Float(e.now.rotation.z));
            }
            if e.now.orientation != e.start.orientation {
                push("transform:orientation", v3(e.now.orientation));
            }
        }
        out
    }
}

/// Position edits of a layer: `transform:position`, or X / Y / Z Position when
/// its dimensions are separated.
fn position_edits(stage: &Stage, layer: &PrimPath, p: DVec3, push: &mut impl FnMut(&str, Value)) {
    let separate = stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, "transform:separate_dimensions"))
        .and_then(|v| v.as_bool())
        == Some(true);
    if separate {
        push("transform:position_x", Value::Float(p.x));
        push("transform:position_y", Value::Float(p.y));
        push("transform:position_z", Value::Float(p.z));
    } else {
        push("transform:position", Value::Vec3([p.x, p.y, p.z]));
    }
}

/// Arrow keys (`motion.layer.nudge_position`): move the selected layers by
/// `(dx, dy)` screen pixels at magnification `mag` (AE: 1 pixel at the current
/// magnification, Shift 10), one undo step.
pub(crate) fn nudge_layers(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    dx: f64,
    dy: f64,
    mag: f64,
) -> bool {
    let (comp_path, layers, time) = {
        let s = session.lock();
        let Some(c) = s.active_comp.clone() else {
            return false;
        };
        let t = s.time(&c);
        (c, s.selected_layers.clone(), t)
    };
    if layers.is_empty() {
        return false;
    }
    let Ok(comp) = bae_anim::evaluate_comp(cx.stage, &comp_path, time) else {
        return false;
    };
    let mag = if mag > 0.0 { mag } else { 1.0 };
    let d = DVec2::new(dx / mag, dy / mag);
    let mut drag = TransformDrag::new(cx.stage, &comp, &layers, Kind::Move { start: DVec2::ZERO });
    drag.update(d, false, None);
    let edits = drag.commit(cx.stage, time);
    if edits.is_empty() {
        return false;
    }
    cx.invoke_with("attr.set_many", json!({ "edits": edits }));
    true
}

/// The thin outline AE shows around the layer under the pointer.
pub(super) fn hover_outline(painter: &Painter, l: &EvaluatedLayer, view: &View, color: Color32) {
    if let Some(q) = render::bounds_quad(l, view, &Affine::new(DMat4::IDENTITY)) {
        painter.add(egui::Shape::closed_line(
            q.to_vec(),
            Stroke::new(1.0, color.gamma_multiply(0.7)),
        ));
    }
}

/// The marquee rectangle.
pub(super) fn paint_marquee(painter: &Painter, r: Rect) {
    painter.rect_filled(r, 0.0, Color32::from_white_alpha(18));
    painter.rect_stroke(
        r,
        0.0,
        Stroke::new(1.0, Color32::from_gray(200)),
        egui::StrokeKind::Middle,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_points_and_axes() {
        let b = (DVec2::new(0.0, 0.0), DVec2::new(100.0, 50.0));
        assert_eq!(handle_point(b, 0), (DVec2::new(0.0, 0.0), true, true));
        assert_eq!(handle_point(b, 1), (DVec2::new(50.0, 0.0), false, true));
        assert_eq!(handle_point(b, 3), (DVec2::new(100.0, 25.0), true, false));
        assert_eq!(handle_point(b, 4), (DVec2::new(100.0, 50.0), true, true));
        assert_eq!(handle_point(b, 5), (DVec2::new(50.0, 50.0), false, true));
        assert_eq!(handle_point(b, 7), (DVec2::new(0.0, 25.0), true, false));
    }
}
