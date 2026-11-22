//! Editing mask paths with the Selection tool (AE-SHP-044): click a vertex to
//! select it (Shift adds or removes), drag selected vertices, drag a selected
//! vertex's direction handle (a smooth vertex keeps its opposite handle in line),
//! arrow keys nudge and Delete removes selected vertices. Each gesture is one
//! `path.set_vertices` / `path.delete_vertex` (one undo step).

use bae_anim::glam::{DVec2, DVec3};
use bae_model::{BezierPath, PrimPath};
use bae_motion::query::{self, CompInfo};
use bae_ui::PanelContext;
use egui::{Painter, Pos2, Stroke};
use serde_json::json;

use super::overlay::{self, EditablePath};
use crate::render::View;
use crate::session::SharedSession;

/// Selected vertices of one path.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct VertexSelection {
    pub prim: Option<PrimPath>,
    pub indices: Vec<usize>,
}

impl VertexSelection {
    pub fn clear(&mut self) {
        self.prim = None;
        self.indices.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.prim.is_none() || self.indices.is_empty()
    }

    fn has(&self, prim: &PrimPath, i: usize) -> bool {
        self.prim.as_ref() == Some(prim) && self.indices.contains(&i)
    }
}

/// What the pointer is over on the editable paths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Hit {
    /// A direction handle of a selected vertex.
    Handle {
        path: usize,
        index: usize,
        out: bool,
    },
    Vertex {
        path: usize,
        index: usize,
    },
}

/// The handle (of a selected vertex) or vertex under `pos`.
pub(super) fn hit(
    paths: &[EditablePath],
    sel: &VertexSelection,
    view: &View,
    pos: Pos2,
) -> Option<Hit> {
    for (pi, p) in paths.iter().enumerate() {
        if sel.prim.as_ref() != Some(&p.prim) {
            continue;
        }
        for &i in &sel.indices {
            if i >= p.path.vertices.len() {
                continue;
            }
            for (out, t) in [
                (false, p.path.in_tangents[i]),
                (true, p.path.out_tangents[i]),
            ] {
                if t[0].hypot(t[1]) < 1e-9 {
                    continue;
                }
                let v = p.path.vertices[i];
                let h = overlay::to_screen(view, &p.to_comp, [v[0] + t[0], v[1] + t[1]]);
                if h.distance(pos) <= 5.0 {
                    return Some(Hit::Handle {
                        path: pi,
                        index: i,
                        out,
                    });
                }
            }
        }
    }
    match overlay::hit(paths, view, pos) {
        Some(overlay::PathHit::Vertex { path, index }) => Some(Hit::Vertex { path, index }),
        _ => None,
    }
}

/// A press on a vertex: select it (Shift toggles; an already selected vertex
/// keeps the selection so several can be dragged).
pub(super) fn press_vertex(sel: &mut VertexSelection, prim: &PrimPath, i: usize, shift: bool) {
    if sel.prim.as_ref() != Some(prim) {
        sel.prim = Some(prim.clone());
        sel.indices.clear();
    }
    if shift {
        if let Some(k) = sel.indices.iter().position(|x| *x == i) {
            sel.indices.remove(k);
        } else {
            sel.indices.push(i);
        }
    } else if !sel.indices.contains(&i) {
        sel.indices = vec![i];
    }
}

/// A drag of vertices or a handle.
pub(super) struct PathDrag {
    path: EditablePath,
    now: BezierPath,
    what: Hit,
    indices: Vec<usize>,
    start: DVec2,
}

impl PathDrag {
    pub fn new(path: EditablePath, what: Hit, sel: &VertexSelection, start: DVec2) -> Self {
        PathDrag {
            now: path.path.clone(),
            path,
            what,
            indices: sel.indices.clone(),
            start,
        }
    }

    fn local_vector(&self, d: DVec2) -> [f64; 2] {
        let v = self
            .path
            .to_comp
            .inverse()
            .transform_vector3(DVec3::new(d.x, d.y, 0.0));
        [v.x, v.y]
    }

    /// Follow the pointer (comp space).
    pub fn update(&mut self, p: DVec2, shift: bool) {
        let mut d = p - self.start;
        let base = &self.path.path;
        self.now = base.clone();
        match self.what {
            Hit::Vertex { .. } => {
                if shift {
                    if d.x.abs() > d.y.abs() {
                        d.y = 0.0;
                    } else {
                        d.x = 0.0;
                    }
                }
                let l = self.local_vector(d);
                for &i in &self.indices {
                    if let Some(v) = self.now.vertices.get_mut(i) {
                        *v = [v[0] + l[0], v[1] + l[1]];
                    }
                }
            }
            Hit::Handle { index, out, .. } => {
                let v = base.vertices[index];
                let (own, other) = if out {
                    (base.out_tangents[index], base.in_tangents[index])
                } else {
                    (base.in_tangents[index], base.out_tangents[index])
                };
                let l = self.local_vector(d);
                let new = [own[0] + l[0], own[1] + l[1]];
                // A smooth vertex (handles in line) keeps the opposite handle in
                // line, with its own length.
                let cross = own[0] * other[1] - own[1] * other[0];
                let dot = own[0] * other[0] + own[1] * other[1];
                let lo = other[0].hypot(other[1]);
                let ln = new[0].hypot(new[1]);
                let smooth = lo > 1e-9
                    && own[0].hypot(own[1]) > 1e-9
                    && dot < 0.0
                    && cross.abs() < 1e-6 * lo.max(1.0) * own[0].hypot(own[1]).max(1.0);
                let opposite =
                    (smooth && ln > 1e-9).then(|| [-new[0] / ln * lo, -new[1] / ln * lo]);
                let _ = v;
                if out {
                    self.now.out_tangents[index] = new;
                    if let Some(o) = opposite {
                        self.now.in_tangents[index] = o;
                    }
                } else {
                    self.now.in_tangents[index] = new;
                    if let Some(o) = opposite {
                        self.now.out_tangents[index] = o;
                    }
                }
            }
        }
    }

    pub fn changed(&self) -> bool {
        self.now != self.path.path
    }

    pub fn commit(&self, cx: &mut PanelContext<'_>, session: &SharedSession, info: &CompInfo) {
        set_vertices(cx, session, info, &self.path.prim, &self.now);
    }

    /// The path being edited, for drawing.
    pub fn preview(&self) -> EditablePath {
        EditablePath {
            path: self.now.clone(),
            ..self.path.clone()
        }
    }

    pub fn prim(&self) -> &PrimPath {
        &self.path.prim
    }
}

pub(super) fn set_vertices(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    prim: &PrimPath,
    path: &BezierPath,
) {
    let time = session.lock().time(&info.path);
    let mut params = serde_json::to_value(path).unwrap_or_default();
    params["path"] = json!(prim);
    params["time"] = json!(query::to_key_time(cx.stage, prim, time));
    cx.invoke_with("path.set_vertices", params);
}

/// Arrow keys with vertices selected: move them by `(dx, dy)` comp pixels.
pub(super) fn nudge(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    paths: &[EditablePath],
    sel: &VertexSelection,
    d: DVec2,
) -> bool {
    let Some(p) = paths.iter().find(|p| sel.prim.as_ref() == Some(&p.prim)) else {
        return false;
    };
    let mut drag = PathDrag::new(
        p.clone(),
        Hit::Vertex { path: 0, index: 0 },
        sel,
        DVec2::ZERO,
    );
    drag.update(d, false);
    if !drag.changed() {
        return false;
    }
    drag.commit(cx, session, info);
    true
}

/// Delete with vertices selected: remove them (highest index first, one
/// command each; AE keeps at least one vertex).
pub(super) fn delete(
    cx: &mut PanelContext<'_>,
    paths: &[EditablePath],
    sel: &VertexSelection,
) -> bool {
    let Some(p) = paths.iter().find(|p| sel.prim.as_ref() == Some(&p.prim)) else {
        return false;
    };
    let mut idx: Vec<usize> = sel
        .indices
        .iter()
        .copied()
        .filter(|i| *i < p.path.vertices.len())
        .collect();
    idx.sort_unstable();
    idx.dedup();
    if idx.is_empty() {
        return false;
    }
    let keep = p.path.vertices.len().saturating_sub(idx.len());
    if keep == 0 {
        idx.remove(0);
    }
    let mut path = p.path.clone();
    for &i in idx.iter().rev() {
        path = bae_motion::path::remove(&path, i);
    }
    // One undo step: the whole path at once.
    let mut params = serde_json::to_value(&path).unwrap_or_default();
    params["path"] = json!(p.prim);
    cx.invoke_with("path.set_vertices", params);
    true
}

/// Mask paths of the selected layers with their vertices (selected ones filled)
/// and the handles of the selected vertices.
pub(super) fn paint(painter: &Painter, view: &View, paths: &[EditablePath], sel: &VertexSelection) {
    for p in paths {
        for i in 0..p.path.vertices.len() {
            let r =
                egui::Rect::from_center_size(p.screen(view, i), egui::vec2(view.point, view.point));
            if sel.has(&p.prim, i) {
                painter.rect_filled(r, 0.0, p.color);
                let v = p.path.vertices[i];
                let vs = p.screen(view, i);
                for t in [p.path.in_tangents[i], p.path.out_tangents[i]] {
                    if t[0].hypot(t[1]) < 1e-9 {
                        continue;
                    }
                    let h = overlay::to_screen(view, &p.to_comp, [v[0] + t[0], v[1] + t[1]]);
                    painter.line_segment([vs, h], Stroke::new(1.0, p.color));
                    painter.circle_filled(h, view.point / 2.0, p.color);
                }
            } else {
                painter.rect_filled(r, 0.0, egui::Color32::from_black_alpha(160));
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.color), egui::StrokeKind::Middle);
            }
        }
    }
}
