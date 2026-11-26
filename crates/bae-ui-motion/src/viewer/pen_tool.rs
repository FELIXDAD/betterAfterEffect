//! Pen tools in the Composition viewer (AE-SHP-055, AE-SHP-044).
//!
//! Pen: a click adds a corner vertex, click-drag a smooth vertex whose handles
//! follow the drag (Alt/Option-drag moves only the outgoing handle, Shift
//! constrains the segment or handle to 45°, Space moves the vertex being placed).
//! Clicking the first vertex closes the path. The first click creates the shape
//! (a "Shape N" group with "Path 1" in the same targets as the shape tools) or the
//! mask; every further click is one `path.set_vertices` (one undo step each, as in
//! AE). Switching away from the Pen, or deselecting the layer, ends an open path.
//!
//! Over the paths of selected layers the Pen works as the vertex tools: over a
//! segment Add Vertex, over a vertex Delete Vertex, Alt/Option over a vertex
//! Convert Vertex. Cmd/Ctrl makes it the Selection tool (handled by the viewer).

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_model::{BezierPath, PrimPath, PrimType, Value};
use bae_motion::path::{is_corner, roto_bezier, to_corner};
use bae_motion::query::{self, CompInfo};
use bae_motion::shape::{Geometry, ShapeKind};
use bae_ui::PanelContext;
use egui::{Color32, Key, Painter, Pos2, Rect, Response, Stroke, Ui, vec2};
use serde_json::json;

use super::overlay::{self, EditablePath, PathHit};
use super::shape_tool::{self, Target};
use crate::render::View;
use crate::session::SharedSession;
use crate::tools::{Tool, ToolOptions};

/// Screen distance under which a click on the first vertex closes the path.
const CLOSE_PICK: f32 = 6.0;
/// Pointer travel (points) after which a Pen press makes a smooth vertex.
const DRAG_START: f32 = 2.0;

/// An open path being drawn with the Pen.
pub(crate) struct PenPath {
    comp: PrimPath,
    target: Target,
    /// Captured when starting a new layer. Adding paths to it later keeps its anchor.
    center_new_anchor: bool,
    /// Path space (the target layer's space) → comp space.
    to_comp: DMat4,
    path: BezierPath,
    /// The mask or shape Path prim, once the creating command has run.
    prim: Option<PrimPath>,
    /// Where the new prim appears (`masks` or `contents` of the layer) and the
    /// children it had before.
    awaiting: Option<(PrimPath, Vec<PrimPath>)>,
    placing: Option<Placing>,
    color: Color32,
}

/// The vertex the pointer is placing (mouse button down).
struct Placing {
    index: usize,
    /// Press position, comp space (moves with Space).
    origin: DVec2,
    /// Pointer last frame, comp space.
    last: DVec2,
    /// Pressed on the first vertex: closes the path.
    closing: bool,
    dragged: bool,
}

/// Convert Vertex press on a vertex: a click makes a smooth vertex a corner, a
/// drag pulls out new symmetric handles.
struct ConvertDrag {
    path: EditablePath,
    index: usize,
    origin: Pos2,
    dragged: bool,
}

/// Pen-tool state of the viewer.
#[derive(Default)]
pub(crate) struct PenState {
    pub(crate) drawing: Option<PenPath>,
    convert: Option<ConvertDrag>,
}

impl PenState {
    /// The path a Convert Vertex drag is changing and its previewed value.
    pub(crate) fn path_preview(&self) -> Option<(PrimPath, bae_model::BezierPath)> {
        self.convert
            .as_ref()
            .filter(|c| c.dragged)
            .map(|c| (c.path.prim.clone(), c.path.path.clone()))
    }

    /// A Pen vertex or Convert Vertex drag is in progress: keys belong to it.
    pub(crate) fn busy(&self) -> bool {
        self.convert.is_some() || self.drawing.as_ref().is_some_and(|d| d.placing.is_some())
    }
}

/// What the pointer looks like over the viewer with a pen-group tool.
pub(crate) struct PenCursor {
    /// The tool whose pointer to show (the Pen becomes a vertex tool over paths).
    pub tool: Tool,
    /// Over the first vertex of the open path: the click closes it.
    pub close: bool,
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

/// `v` (comp space) constrained to the nearest multiple of 45°, keeping its
/// projection on that direction.
fn constrain45(v: DVec2) -> DVec2 {
    if v.length_squared() == 0.0 {
        return v;
    }
    let step = std::f64::consts::FRAC_PI_4;
    let a = (v.y.atan2(v.x) / step).round() * step;
    let d = DVec2::new(a.cos(), a.sin());
    d * v.dot(d)
}

impl PenPath {
    fn to_local(&self, p: DVec2) -> [f64; 2] {
        let v = self
            .to_comp
            .inverse()
            .transform_point3(DVec3::new(p.x, p.y, 0.0));
        [v.x, v.y]
    }

    fn local_vector(&self, d: DVec2) -> [f64; 2] {
        let v = self
            .to_comp
            .inverse()
            .transform_vector3(DVec3::new(d.x, d.y, 0.0));
        [v.x, v.y]
    }

    fn comp_point(&self, i: usize) -> DVec2 {
        let [x, y] = self.path.vertices[i];
        let v = self.to_comp.transform_point3(DVec3::new(x, y, 0.0));
        DVec2::new(v.x, v.y)
    }

    fn screen(&self, view: &View, i: usize) -> Pos2 {
        view.to_screen(self.comp_point(i))
    }

    /// The pointer is over the first vertex and the path can close there.
    fn over_first(&self, view: &View, pos: Pos2) -> bool {
        self.path.vertices.len() >= 2 && self.screen(view, 0).distance(pos) <= CLOSE_PICK
    }

    fn push_vertex(&mut self, at: DVec2) {
        let local = self.to_local(at);
        self.path.vertices.push(local);
        self.path.in_tangents.push([0.0, 0.0]);
        self.path.out_tangents.push([0.0, 0.0]);
    }

    /// The layer the path belongs to (once it exists).
    fn layer(&self, stage: &bae_model::Stage) -> Option<PrimPath> {
        match &self.target {
            Target::Shape(l) | Target::Mask(l) => Some(l.clone()),
            Target::NewLayer => self
                .prim
                .as_ref()
                .and_then(|p| query::owning_layer(stage, p)),
        }
    }
}

/// The first Path prim of a new shape group.
fn path_prim_of_group(stage: &bae_model::Stage, group: &PrimPath) -> Option<PrimPath> {
    let contents = group.child(query::CONTENTS).ok()?;
    stage
        .children(&contents)
        .into_iter()
        .find(|c| stage.prim(c).and_then(|v| v.prim_type().cloned()) == Some(PrimType::PATH))
}

/// Pen-group input for one viewer frame. `tool` is the tool in effect (the Pen
/// with Cmd/Ctrl held is the Selection tool and never gets here).
#[allow(clippy::too_many_arguments)]
pub(super) fn frame(
    state: &mut PenState,
    ui: &Ui,
    resp: &Response,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    comp: &EvaluatedComp,
    view: &View,
    tool: Tool,
    painter: &Painter,
) -> Option<PenCursor> {
    let stage = cx.stage;
    let tools = crate::tools::shared(cx.resources);
    let current = tools.lock().current();
    let selected = session.lock().selected_layers.clone();
    sync_open_path(state, stage, info, current, &selected);
    if !tool.is_pen() {
        state.convert = None;
        if let Some(d) = &state.drawing {
            paint_open_path(painter, view, d);
        }
        return None;
    }
    let options = tools.lock().options.clone();
    let prefs = super::prefs::ViewerPrefs::current(cx.resources);
    let paths = if session.lock().view_options.paths {
        overlay::editable_paths(stage, comp, &selected, Some(prefs))
    } else {
        Vec::new()
    };
    let (alt, shift, space, primary_down, pressed) = ui.input(|i| {
        (
            i.modifiers.alt,
            i.modifiers.shift,
            i.key_down(Key::Space),
            i.pointer.primary_down(),
            i.pointer.primary_pressed(),
        )
    });
    let hover = resp.hover_pos();
    let pointer = resp.interact_pointer_pos().or(hover);
    let drawing = state.drawing.as_ref();
    // The Pen over existing paths acts as the vertex tools.
    let path_hit = match (hover, drawing) {
        (Some(pos), None) => overlay::hit(&paths, view, pos),
        (Some(pos), Some(d)) if d.placing.is_none() && alt => {
            overlay::hit(&paths, view, pos).filter(|h| matches!(h, PathHit::Vertex { .. }))
        }
        _ => None,
    };
    let acting = if state.convert.is_some() {
        Tool::ConvertVertex
    } else {
        match (tool, path_hit) {
            (Tool::Pen, Some(PathHit::Vertex { .. })) if alt => Tool::ConvertVertex,
            (Tool::Pen, Some(PathHit::Vertex { .. })) => Tool::DeleteVertex,
            (Tool::Pen, Some(PathHit::Segment { .. })) => Tool::AddVertex,
            (t, _) => t,
        }
    };
    let close = hover.is_some_and(|p| drawing.is_some_and(|d| d.over_first(view, p)));

    // Presses.
    if pressed
        && resp.hovered()
        && let Some(pos) = hover
    {
        match (acting, path_hit) {
            (Tool::Pen, _) => {
                session.lock().stop();
                start_vertex(state, cx, session, info, comp, view, pos, shift);
            }
            (Tool::AddVertex, Some(PathHit::Segment { path, segment, t })) => {
                cx.invoke_with(
                    "path.insert_vertex",
                    json!({"path": paths[path].prim, "segment": segment, "t": t}),
                );
            }
            (Tool::DeleteVertex, Some(PathHit::Vertex { path, index })) => {
                if paths[path].path.vertices.len() > 1 {
                    cx.invoke_with(
                        "path.delete_vertex",
                        json!({"path": paths[path].prim, "index": index}),
                    );
                }
            }
            (Tool::ConvertVertex, Some(PathHit::Vertex { path, index })) => {
                state.convert = Some(ConvertDrag {
                    path: paths[path].clone(),
                    index,
                    origin: pos,
                    dragged: false,
                });
            }
            _ => {}
        }
    }

    // A vertex being placed follows the pointer.
    if let Some(d) = state.drawing.as_mut()
        && let Some(mut pl) = d.placing.take()
    {
        if let Some(pos) = pointer {
            let at = view.to_comp(pos);
            if space {
                // Space moves the vertex being placed.
                let delta = d.local_vector(at - pl.last);
                let v = &mut d.path.vertices[pl.index];
                *v = [v[0] + delta[0], v[1] + delta[1]];
                pl.origin += at - pl.last;
            } else if pl.dragged || view.to_screen(pl.origin).distance(pos) > DRAG_START {
                pl.dragged = true;
                let mut handle = at - pl.origin;
                if shift {
                    handle = constrain45(handle);
                }
                let out = d.local_vector(handle);
                d.path.out_tangents[pl.index] = out;
                if !alt {
                    d.path.in_tangents[pl.index] = [-out[0], -out[1]];
                }
            }
            pl.last = at;
        }
        d.placing = Some(pl);
    }
    if !primary_down
        && let Some(d) = state.drawing.as_mut()
        && let Some(pl) = d.placing.take()
    {
        if pl.closing {
            d.path.closed = true;
        }
        commit(d, cx, session, info, &options);
        if pl.closing {
            state.drawing = None;
        }
    }

    // Convert Vertex: drag pulls out handles, a click toggles.
    if let Some(cv) = state.convert.as_mut()
        && let Some(pos) = pointer
        && pos.distance(cv.origin) > DRAG_START
    {
        cv.dragged = true;
        let local = cv.path.to_local(view.to_comp(pos));
        let out = sub(local, cv.path.path.vertices[cv.index]);
        cv.path.path.out_tangents[cv.index] = out;
        cv.path.path.in_tangents[cv.index] = [-out[0], -out[1]];
    }
    if !primary_down && let Some(cv) = state.convert.take() {
        // A click makes a smooth vertex a corner (a corner stays one); a drag
        // pulls out symmetric handles (AE 26).
        let new = if cv.dragged {
            Some(cv.path.path.clone())
        } else if !is_corner(&cv.path.path, cv.index) {
            Some(to_corner(&cv.path.path, cv.index))
        } else {
            None
        };
        if let Some(new) = new {
            set_vertices(cx, session, info, &cv.path.prim, &new);
        }
    }

    // Paint: editable paths with their vertices, the open path, a convert drag.
    for p in &paths {
        if state.drawing.as_ref().and_then(|d| d.prim.as_ref()) == Some(&p.prim) {
            continue;
        }
        let stroke = Stroke::new(1.0, p.color);
        painter.extend(overlay::path_shapes(&p.path, &p.to_comp, view, stroke));
        overlay::vertices(painter, view, p, None);
    }
    if let Some(d) = &state.drawing {
        paint_open_path(painter, view, d);
    }
    if let Some(cv) = &state.convert {
        let stroke = Stroke::new(1.0, cv.path.color);
        painter.extend(overlay::path_shapes(
            &cv.path.path,
            &cv.path.to_comp,
            view,
            stroke,
        ));
        paint_handles(
            painter,
            view,
            &cv.path.path,
            &cv.path.to_comp,
            cv.index,
            cv.path.color,
        );
    }
    Some(PenCursor {
        tool: acting,
        close,
    })
}

/// End the open path when it no longer applies: the Pen is not the tool any
/// more, another comp is active, its layer was deselected, or it was undone.
/// Otherwise pick up the created prim and follow the model (undo of a vertex).
fn sync_open_path(
    state: &mut PenState,
    stage: &bae_model::Stage,
    info: &CompInfo,
    current: Tool,
    selected: &[PrimPath],
) {
    let Some(d) = state.drawing.as_mut() else {
        return;
    };
    if d.prim.is_none()
        && let Some((container, before)) = &d.awaiting
        && let Some(new) = stage
            .children(container)
            .into_iter()
            .find(|c| !before.contains(c))
    {
        d.prim = match d.target {
            Target::Mask(_) => Some(new),
            _ => path_prim_of_group(stage, &new),
        };
        d.awaiting = None;
    }
    let mut end = current != Tool::Pen || d.comp != info.path;
    if d.placing.is_none() {
        if let Some(prim) = &d.prim {
            match stage
                .prim(prim)
                .and_then(|v| query::static_value(&v, bae_motion::path::PATH_ATTR))
            {
                Some(Value::Bezier(b)) => {
                    if b != d.path {
                        d.path = b;
                    }
                }
                _ => end = true,
            }
        }
        if let Some(layer) = d.layer(stage)
            && (d.prim.is_some() || !matches!(d.target, Target::NewLayer))
            && !selected.contains(&layer)
        {
            end = true;
        }
    }
    if end {
        state.drawing = None;
    }
}

/// A Pen press at `pos`: close the open path on its first vertex, add a vertex
/// to it, or start a new path.
#[allow(clippy::too_many_arguments)]
fn start_vertex(
    state: &mut PenState,
    cx: &PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    comp: &EvaluatedComp,
    view: &View,
    pos: Pos2,
    shift: bool,
) {
    let mut at = view.to_comp(pos);
    if let Some(d) = state.drawing.as_mut() {
        if d.over_first(view, pos) {
            d.placing = Some(Placing {
                index: 0,
                origin: d.comp_point(0),
                last: at,
                closing: true,
                dragged: false,
            });
            return;
        }
        if shift && let Some(last) = d.path.vertices.len().checked_sub(1) {
            let prev = d.comp_point(last);
            at = prev + constrain45(at - prev);
        }
        d.push_vertex(at);
        let index = d.path.vertices.len() - 1;
        d.placing = Some(Placing {
            index,
            origin: at,
            last: at,
            closing: false,
            dragged: false,
        });
        return;
    }
    let tools = crate::tools::shared(cx.resources);
    let mask = tools.lock().options.creates_mask;
    let selected = session.lock().selected_layers.clone();
    let target = Target::of(cx.stage, &info.path, &selected, mask);
    let to_comp = shape_tool::target_matrix(info, comp, &target);
    let color = shape_tool::preview_color(cx.stage, &target);
    let center_new_anchor = matches!(target, Target::NewLayer)
        && super::prefs::ViewerPrefs::current(cx.resources).center_shape_anchor;
    let mut d = PenPath {
        comp: info.path.clone(),
        target,
        center_new_anchor,
        to_comp,
        path: BezierPath {
            vertices: Vec::new(),
            in_tangents: Vec::new(),
            out_tangents: Vec::new(),
            closed: false,
            feather: Vec::new(),
        },
        prim: None,
        awaiting: None,
        placing: None,
        color,
    };
    d.push_vertex(at);
    d.placing = Some(Placing {
        index: 0,
        origin: at,
        last: at,
        closing: false,
        dragged: false,
    });
    state.drawing = Some(d);
}

fn set_vertices(
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

/// Write the open path to the model: the first vertex creates the shape or mask,
/// later ones edit its path.
fn commit(
    d: &mut PenPath,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    options: &ToolOptions,
) {
    let path = if options.roto_bezier {
        roto_bezier(&d.path)
    } else {
        d.path.clone()
    };
    if let Some(prim) = &d.prim {
        if d.center_new_anchor
            && let Some(layer) = d.layer(cx.stage)
        {
            // The path remains in its original coordinates. Move the anchor and
            // compensate Position in the same undo step as each new vertex.
            let [x, y] = bae_motion::path::drawing_center(&path);
            let position = d.to_comp.transform_point3(DVec3::new(x, y, 0.0));
            let time = query::to_key_time(cx.stage, prim, session.lock().time(&info.path));
            cx.invoke_with("attr.set_many", json!({"edits": [
                {"path": prim, "attr": bae_motion::path::PATH_ATTR, "time": time, "value": Value::Bezier(path)},
                {"path": layer, "attr": "transform:anchor_point", "time": time, "value": Value::Vec3([x, y, 0.0])},
                {"path": layer, "attr": "transform:position", "time": time, "value": Value::Vec3(position.to_array())}
            ]}));
            return;
        }
        set_vertices(cx, session, info, prim, &path);
        return;
    }
    if d.awaiting.is_some() {
        // The creating command has not run yet; the next commit sends all.
        return;
    }
    let g = Geometry {
        kind: ShapeKind::Path,
        position: [0.0, 0.0],
        size: None,
        roundness: 0.0,
        points: None,
        outer_radius: None,
        inner_radius: None,
        rotation: 0.0,
        outer_roundness: 0.0,
        inner_roundness: 0.0,
        path: Some(path),
    };
    let container_name = match d.target {
        Target::Mask(_) => "masks",
        _ => query::CONTENTS,
    };
    let Some(layer) = shape_tool::run(cx, session, info, &d.target, g, false, options) else {
        return;
    };
    if let Ok(container) = layer.child(container_name) {
        let before = cx.stage.children(&container);
        d.awaiting = Some((container, before));
    }
}

fn paint_handles(
    painter: &Painter,
    view: &View,
    path: &BezierPath,
    to_comp: &DMat4,
    i: usize,
    color: Color32,
) {
    let v = path.vertices[i];
    let vs = overlay::to_screen(view, to_comp, v);
    for t in [path.in_tangents[i], path.out_tangents[i]] {
        if t[0].hypot(t[1]) < 1e-9 {
            continue;
        }
        let h = overlay::to_screen(view, to_comp, [v[0] + t[0], v[1] + t[1]]);
        painter.line_segment([vs, h], Stroke::new(1.0, color));
        painter.circle_filled(h, view.point / 2.0, color);
    }
}

/// The open path: its segments, hollow vertices with the last one filled, and the
/// handles of the vertex being placed.
fn paint_open_path(painter: &Painter, view: &View, d: &PenPath) {
    let stroke = Stroke::new(1.0, d.color);
    painter.extend(overlay::path_shapes(&d.path, &d.to_comp, view, stroke));
    let n = d.path.vertices.len();
    for i in 0..n {
        let r = Rect::from_center_size(d.screen(view, i), vec2(view.point, view.point));
        if i + 1 == n {
            painter.rect_filled(r, 0.0, d.color);
        } else {
            painter.rect_filled(r, 0.0, Color32::from_black_alpha(160));
            painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
        }
    }
    if let Some(pl) = &d.placing {
        paint_handles(painter, view, &d.path, &d.to_comp, pl.index, d.color);
    }
}
