//! Free Transform Points (AE-SHP-044): double-click a mask path with the
//! Selection tool (or Layer > Mask and Shape Path > Free Transform Points,
//! `motion.layer.free_transform_points`) to put a box around its vertices
//! (the selected ones, or all). Drag inside the box to move them, drag a handle
//! to scale around the box's center (Shift keeps the proportions), drag just
//! outside to rotate (Shift: 45° steps). Each drag is one `path.set_vertices`.
//! Enter, Esc or a click away from the box ends it.

use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_model::{BezierPath, PrimPath};
use bae_motion::query::CompInfo;
use bae_ui::PanelContext;
use egui::{Color32, CursorIcon, Painter, Pos2, Rect, Stroke, vec2};

use super::overlay::{self, EditablePath};
use super::path_edit;
use crate::render::View;
use crate::session::SharedSession;

/// Screen distance outside the box within which a drag rotates.
const ROTATE_RING: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    Move,
    Scale(usize),
    Rotate,
}

struct Drag {
    grab: Grab,
    start: DVec2,
    last_angle: f64,
}

/// A free-transform box on one path.
pub(crate) struct FreeTransform {
    pub prim: PrimPath,
    /// The path when the box was made or after the last drag.
    base: BezierPath,
    now: BezierPath,
    /// Transformed vertices.
    indices: Vec<usize>,
    /// Path space → comp space.
    to_comp: DMat4,
    color: Color32,
    /// Box in path space (min, max) and its center (the transform's anchor).
    lo: DVec2,
    hi: DVec2,
    drag: Option<Drag>,
}

fn bounds(p: &BezierPath, idx: &[usize]) -> (DVec2, DVec2) {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for &i in idx {
        if let Some(v) = p.vertices.get(i) {
            let v = DVec2::new(v[0], v[1]);
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    (lo, hi)
}

impl FreeTransform {
    pub fn new(path: &EditablePath, selected: &[usize]) -> Option<Self> {
        let n = path.path.vertices.len();
        let indices: Vec<usize> = if selected.len() >= 2 {
            selected.iter().copied().filter(|i| *i < n).collect()
        } else {
            (0..n).collect()
        };
        if indices.len() < 2 {
            return None;
        }
        let (lo, hi) = bounds(&path.path, &indices);
        Some(FreeTransform {
            prim: path.prim.clone(),
            base: path.path.clone(),
            now: path.path.clone(),
            indices,
            to_comp: path.to_comp,
            color: path.color,
            lo,
            hi,
            drag: None,
        })
    }

    fn screen(&self, view: &View, p: DVec2) -> Pos2 {
        overlay::to_screen(view, &self.to_comp, [p.x, p.y])
    }

    fn local(&self, p: DVec2) -> DVec2 {
        let v = self
            .to_comp
            .inverse()
            .transform_point3(DVec3::new(p.x, p.y, 0.0));
        DVec2::new(v.x, v.y)
    }

    fn corners(&self) -> [DVec2; 4] {
        let (a, b) = (self.lo, self.hi);
        [a, DVec2::new(b.x, a.y), b, DVec2::new(a.x, b.y)]
    }

    /// Handles clockwise from the top left (as the layer handles).
    fn handles(&self) -> [DVec2; 8] {
        let c = self.corners();
        let mut out = [DVec2::ZERO; 8];
        for i in 0..4 {
            out[2 * i] = c[i];
            out[2 * i + 1] = (c[i] + c[(i + 1) % 4]) / 2.0;
        }
        out
    }

    fn center(&self) -> DVec2 {
        (self.lo + self.hi) / 2.0
    }

    fn quad(&self, view: &View) -> [Pos2; 4] {
        self.corners().map(|c| self.screen(view, c))
    }

    fn grab_at(&self, view: &View, pos: Pos2) -> Option<Grab> {
        if let Some(h) = self
            .handles()
            .iter()
            .position(|h| self.screen(view, *h).distance(pos) <= 5.0)
        {
            return Some(Grab::Scale(h));
        }
        let q = self.quad(view);
        if crate::render::quad_contains(&q, pos) {
            return Some(Grab::Move);
        }
        let r = Rect::from_points(&q).expand(ROTATE_RING);
        r.contains(pos).then_some(Grab::Rotate)
    }

    /// The cursor over the box.
    pub fn cursor(&self, view: &View, pos: Pos2) -> Option<CursorIcon> {
        match self.grab_at(view, pos)? {
            Grab::Move => Some(CursorIcon::Move),
            Grab::Scale(0 | 4) => Some(CursorIcon::ResizeNwSe),
            Grab::Scale(2 | 6) => Some(CursorIcon::ResizeNeSw),
            Grab::Scale(1 | 5) => Some(CursorIcon::ResizeVertical),
            Grab::Scale(_) => Some(CursorIcon::ResizeHorizontal),
            Grab::Rotate => Some(CursorIcon::Alias),
        }
    }

    /// A press at `pos`: starts a drag on the box, or `false` (away from it).
    pub fn press(&mut self, view: &View, pos: Pos2) -> bool {
        match self.grab_at(view, pos) {
            Some(grab) => {
                self.drag = Some(Drag {
                    grab,
                    start: self.local(view.to_comp(pos)),
                    last_angle: 0.0,
                });
                true
            }
            None => false,
        }
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Follow the pointer (screen).
    pub fn update(&mut self, view: &View, pos: Pos2, shift: bool) {
        let Some(d) = self.drag.as_mut() else {
            return;
        };
        let p = {
            let v = self.to_comp.inverse().transform_point3(DVec3::new(
                view.to_comp(pos).x,
                view.to_comp(pos).y,
                0.0,
            ));
            DVec2::new(v.x, v.y)
        };
        let c = (self.lo + self.hi) / 2.0;
        // The 2D affine map in path space: x' = m * (x - c) + c + t.
        let (m, t) = match d.grab {
            Grab::Move => {
                let mut dv = p - d.start;
                if shift {
                    if dv.x.abs() > dv.y.abs() {
                        dv.y = 0.0;
                    } else {
                        dv.x = 0.0;
                    }
                }
                ([1.0, 0.0, 0.0, 1.0], dv)
            }
            Grab::Scale(h) => {
                let hp = {
                    let (a, b) = (self.lo, self.hi);
                    let corners = [a, DVec2::new(b.x, a.y), b, DVec2::new(a.x, b.y)];
                    let i = h / 2;
                    if h.is_multiple_of(2) {
                        corners[i]
                    } else {
                        (corners[i] + corners[(i + 1) % 4]) / 2.0
                    }
                };
                let (ax, ay) = match h {
                    1 | 5 => (false, true),
                    3 | 7 => (true, false),
                    _ => (true, true),
                };
                let dd = hp - c;
                let q = p - c;
                let mut sx = if ax && dd.x.abs() > 1e-9 {
                    q.x / dd.x
                } else {
                    1.0
                };
                let mut sy = if ay && dd.y.abs() > 1e-9 {
                    q.y / dd.y
                } else {
                    1.0
                };
                if shift {
                    let dm = DVec2::new(if ax { dd.x } else { 0.0 }, if ay { dd.y } else { 0.0 });
                    let qm = DVec2::new(if ax { q.x } else { 0.0 }, if ay { q.y } else { 0.0 });
                    if dm.length_squared() > 1e-12 {
                        let k = qm.dot(dm) / dm.length_squared();
                        (sx, sy) = (k, k);
                    }
                }
                ([sx, 0.0, 0.0, sy], DVec2::ZERO)
            }
            Grab::Rotate => {
                let a0 = (d.start - c).y.atan2((d.start - c).x).to_degrees();
                let a1 = (p - c).y.atan2((p - c).x).to_degrees();
                let mut a = a1 - a0;
                a += ((d.last_angle - a) / 360.0).round() * 360.0;
                d.last_angle = a;
                if shift {
                    a = (a / 45.0).round() * 45.0;
                }
                let (s, co) = a.to_radians().sin_cos();
                ([co, -s, s, co], DVec2::ZERO)
            }
        };
        let apply_v = |v: [f64; 2]| {
            let x = DVec2::new(v[0], v[1]) - c;
            [
                m[0] * x.x + m[1] * x.y + c.x + t.x,
                m[2] * x.x + m[3] * x.y + c.y + t.y,
            ]
        };
        let apply_t = |v: [f64; 2]| [m[0] * v[0] + m[1] * v[1], m[2] * v[0] + m[3] * v[1]];
        self.now = self.base.clone();
        for &i in &self.indices {
            self.now.vertices[i] = apply_v(self.base.vertices[i]);
            self.now.in_tangents[i] = apply_t(self.base.in_tangents[i]);
            self.now.out_tangents[i] = apply_t(self.base.out_tangents[i]);
        }
    }

    /// The drag ended: write the path; the box follows the new vertices.
    pub fn release(&mut self, cx: &mut PanelContext<'_>, session: &SharedSession, info: &CompInfo) {
        if self.drag.take().is_some() && self.now != self.base {
            path_edit::set_vertices(cx, session, info, &self.prim, &self.now);
            self.base = self.now.clone();
            let (lo, hi) = bounds(&self.base, &self.indices);
            (self.lo, self.hi) = (lo, hi);
        }
    }

    /// Follow the model (undo, edits elsewhere) between drags.
    pub fn sync(&mut self, path: &EditablePath) {
        if self.drag.is_none() && path.path != self.base {
            if path.path.vertices.len() != self.base.vertices.len() {
                self.indices.retain(|i| *i < path.path.vertices.len());
            }
            self.base = path.path.clone();
            self.now = self.base.clone();
            let (lo, hi) = bounds(&self.base, &self.indices);
            (self.lo, self.hi) = (lo, hi);
        }
        self.to_comp = path.to_comp;
    }

    /// The path as previewed.
    pub fn preview(&self) -> &BezierPath {
        &self.now
    }

    pub fn paint(&self, painter: &Painter, view: &View) {
        let stroke = Stroke::new(1.0, self.color);
        painter.extend(overlay::path_shapes(&self.now, &self.to_comp, view, stroke));
        // The box follows the previewed points.
        let shown = FreeTransform {
            prim: self.prim.clone(),
            base: self.now.clone(),
            now: self.now.clone(),
            indices: self.indices.clone(),
            to_comp: self.to_comp,
            color: self.color,
            lo: DVec2::ZERO,
            hi: DVec2::ZERO,
            drag: None,
        };
        let (lo, hi) = match self.drag.as_ref().map(|d| d.grab) {
            Some(Grab::Rotate) | None => (self.lo, self.hi),
            _ => bounds(&self.now, &self.indices),
        };
        let shown = FreeTransform { lo, hi, ..shown };
        let q = shown.quad(view);
        painter.add(egui::Shape::closed_line(q.to_vec(), stroke));
        for h in shown.handles() {
            painter.rect_stroke(
                Rect::from_center_size(shown.screen(view, h), vec2(view.point, view.point)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        let c = shown.screen(view, shown.center());
        painter.circle_stroke(c, 4.0, stroke);
        painter.line_segment([c - vec2(6.0, 0.0), c + vec2(6.0, 0.0)], stroke);
        painter.line_segment([c - vec2(0.0, 6.0), c + vec2(0.0, 6.0)], stroke);
    }
}
