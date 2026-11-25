//! Selection, Rotation and Pan Behind tools in the Composition viewer.
//!
//! Selection tool: click a layer to select it (Shift adds or removes), click
//! empty space to deselect; drag a layer to move the selected layers (Shift
//! constrains to one axis; snapping per the Tools panel switch, Cmd/Ctrl
//! inverts it while held); drag a bounding-box handle to scale (Shift keeps the
//! proportions); drag in empty space for a marquee (Shift adds); click and drag
//! mask vertices and handles of the selected layers ([`super::path_edit`]).
//! The layer under the pointer gets a thin outline.
//!
//! Rotation tool (W): drag a layer to rotate the selected layers around their
//! anchor points, Shift in 45° steps. Pan Behind tool (Y): drag the anchor point
//! to move it while the layer stays in place; drag elsewhere on the layer to pan
//! its content behind the anchor point.

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2};
use bae_model::PrimPath;
use bae_motion::query::{self, CompInfo};
use bae_ui::PanelContext;
use bae_ui::theme;
use egui::{CursorIcon, Painter, PointerButton, Pos2, Rect, Response, Ui};
use serde_json::json;

use super::free_transform::FreeTransform;
use super::gizmo3d::{self, Gizmo};
use super::overlay::{self, EditablePath};
use super::path_edit::{self, PathDrag, VertexSelection};
use super::transform::{self, Kind, TransformDrag};
use crate::render::{self, Affine, View};
use crate::session::SharedSession;
use crate::tools::Tool;

/// What a press landed on.
#[derive(Clone, Debug, PartialEq)]
enum Press {
    /// On the free-transform box.
    Free,
    Path(path_edit::Hit),
    Handle(PrimPath, transform::Handle),
    /// A 3D layer's axis arrow or rotation ring (0 X, 1 Y, 2 Z).
    Axis(PrimPath, usize),
    Anchor(PrimPath),
    Layer(PrimPath),
    Empty,
}

struct Marquee {
    origin: Pos2,
    /// Selection when the marquee started (kept with Shift).
    base: Vec<PrimPath>,
}

/// State of the selection-type tools.
#[derive(Default)]
pub(crate) struct SelectState {
    pub(crate) drag: Option<TransformDrag>,
    pub(crate) vertices: VertexSelection,
    press: Option<(Press, Pos2)>,
    marquee: Option<Marquee>,
    path_drag: Option<PathDrag>,
    /// Free Transform Points box.
    pub(crate) free: Option<FreeTransform>,
}

impl SelectState {
    /// A drag is in progress.
    pub(crate) fn busy(&self) -> bool {
        self.drag.is_some()
            || self.marquee.is_some()
            || self.path_drag.is_some()
            || self.free.as_ref().is_some_and(|f| f.dragging())
    }
}

/// The topmost unlocked layer under `pos`.
pub(super) fn hit_layer(
    stage: &bae_model::Stage,
    comp: &EvaluatedComp,
    view: &View,
    pos: Pos2,
) -> Option<PrimPath> {
    layers_at(stage, comp, view, pos).next()
}

/// The unlocked layers under `pos`, topmost first.
pub(super) fn layers_at<'a>(
    stage: &'a bae_model::Stage,
    comp: &'a EvaluatedComp,
    view: &'a View,
    pos: Pos2,
) -> impl Iterator<Item = PrimPath> + 'a {
    let outer = Affine::new(DMat4::IDENTITY);
    comp.layers
        .iter()
        .filter(|l| l.visible && !l.switches.guide || l.kind == bae_model::LayerKind::Null)
        .filter(|l| !query::is_locked(stage, &l.path))
        .filter(move |l| {
            render::bounds_quad(l, view, &outer).is_some_and(|q| render::quad_contains(&q, pos))
        })
        .map(|l| l.path.clone())
}

/// Whether a convex quad and a rectangle overlap (separating axes).
fn quad_overlaps(q: &[Pos2; 4], r: Rect) -> bool {
    let rc = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
    ];
    let mut axes = vec![egui::vec2(1.0, 0.0), egui::vec2(0.0, 1.0)];
    for i in 0..4 {
        let e = q[(i + 1) % 4] - q[i];
        axes.push(egui::vec2(-e.y, e.x));
    }
    axes.iter().all(|a| {
        let proj = |pts: &[Pos2]| {
            pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                let d = p.x * a.x + p.y * a.y;
                (lo.min(d), hi.max(d))
            })
        };
        let (a0, a1) = proj(q);
        let (b0, b1) = proj(&rc);
        a0 <= b1 && b0 <= a1
    })
}

fn label_color(stage: &bae_model::Stage, layer: &PrimPath) -> egui::Color32 {
    let label = stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, "label"))
        .and_then(|v| v.as_f64())
        .map_or(8, |v| v as usize);
    theme::label_color(label)
}

fn handle_cursor(h: transform::Handle) -> CursorIcon {
    match h {
        0 | 4 => CursorIcon::ResizeNwSe,
        2 | 6 => CursorIcon::ResizeNeSw,
        1 | 5 => CursorIcon::ResizeVertical,
        _ => CursorIcon::ResizeHorizontal,
    }
}

/// Input of the Selection, Rotation and Pan Behind tools for one viewer frame.
/// Returns a cursor to show instead of the tool's own.
#[allow(clippy::too_many_arguments)]
pub(super) fn frame(
    state: &mut SelectState,
    ui: &Ui,
    resp: &Response,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    comp: &EvaluatedComp,
    view: &View,
    tool: Tool,
    painter: &Painter,
) -> Option<CursorIcon> {
    if !matches!(tool, Tool::Selection | Tool::Rotation | Tool::PanBehind) {
        state.press = None;
        state.marquee = None;
        return None;
    }
    let stage = cx.stage;
    let tools = crate::tools::shared(cx.resources);
    let options = tools.lock().options.clone();
    let (shift, command, primary_down, pressed) = ui.input(|i| {
        (
            i.modifiers.shift,
            i.modifiers.command,
            i.pointer.primary_down(),
            i.pointer.primary_pressed(),
        )
    });
    let selected = session.lock().selected_layers.clone();
    let paths: Vec<EditablePath> = if tool == Tool::Selection {
        overlay::editable_paths(stage, comp, &selected, None)
    } else {
        Vec::new()
    };
    if state
        .vertices
        .prim
        .as_ref()
        .is_some_and(|p| !paths.iter().any(|e| &e.prim == p))
    {
        state.vertices.clear();
    }
    let hover = resp.hover_pos();
    let pointer = resp.interact_pointer_pos().or(hover);

    // Free Transform Points: follow the model; Enter / Esc end it; the menu
    // command or a double-click on a mask path starts it.
    if let Some(f) = state.free.as_mut() {
        match paths.iter().find(|p| p.prim == f.prim) {
            Some(p) => f.sync(p),
            None => state.free = None,
        }
    }
    let request = std::mem::take(&mut tools.lock().free_transform_request);
    if request && tool == Tool::Selection {
        let prim = state
            .vertices
            .prim
            .clone()
            .or_else(|| paths.first().map(|p| p.prim.clone()));
        if let Some(p) = prim.and_then(|prim| paths.iter().find(|p| p.prim == prim)) {
            let sel = if state.vertices.prim.as_ref() == Some(&p.prim) {
                state.vertices.indices.clone()
            } else {
                Vec::new()
            };
            state.free = FreeTransform::new(p, &sel);
        }
    }
    if state.free.as_ref().is_some_and(|f| !f.dragging())
        && ui.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter))
    {
        state.free = None;
    }
    if tool == Tool::Selection
        && resp.double_clicked()
        && let Some(pos) = hover
        && let Some(hit) = overlay::hit(&paths, view, pos)
    {
        let pi = match hit {
            overlay::PathHit::Vertex { path, .. } | overlay::PathHit::Segment { path, .. } => path,
        };
        let sel = if state.vertices.prim.as_ref() == Some(&paths[pi].prim) {
            state.vertices.indices.clone()
        } else {
            Vec::new()
        };
        state.free = FreeTransform::new(&paths[pi], &sel);
        state.path_drag = None;
    }

    // What is under the pointer.
    let target = |pos: Pos2, state: &SelectState| -> Press {
        if tool == Tool::Selection
            && let Some(h) = path_edit::hit(&paths, &state.vertices, view, pos)
        {
            return Press::Path(h);
        }
        for l in comp.layers.iter().filter(|l| selected.contains(&l.path)) {
            if matches!(tool, Tool::Selection | Tool::Rotation)
                && let Some(g) = Gizmo::of(l, view)
                && let Some(i) = g.hit(view, pos, tool == Tool::Rotation)
            {
                return Press::Axis(l.path.clone(), i);
            }
            if tool == Tool::PanBehind
                && transform::anchor_screen(l, view).distance(pos) <= transform::ANCHOR_PICK
            {
                return Press::Anchor(l.path.clone());
            }
            if tool == Tool::Selection
                && let Some(hs) = transform::handles(l, view)
                && let Some(h) = hs
                    .iter()
                    .position(|p| p.distance(pos) <= transform::HANDLE_PICK)
            {
                return Press::Handle(l.path.clone(), h);
            }
        }
        match hit_layer(stage, comp, view, pos) {
            Some(l) => Press::Layer(l),
            None => Press::Empty,
        }
    };

    if pressed
        && resp.hovered()
        && let Some(pos) = hover
    {
        let on_box = match state.free.as_mut() {
            Some(f) => f.press(view, pos),
            None => false,
        };
        if !on_box && state.free.is_some() {
            // A click away from the box ends free transform.
            state.free = None;
        }
        let t = if on_box {
            Press::Free
        } else {
            target(pos, state)
        };
        match &t {
            Press::Path(path_edit::Hit::Vertex { path, index }) => {
                path_edit::press_vertex(&mut state.vertices, &paths[*path].prim, *index, shift);
            }
            Press::Path(_) => {}
            _ if !shift => state.vertices.clear(),
            _ => {}
        }
        state.press = Some((t, pos));
    }

    // Clicks (press and release without a drag): layer selection.
    if resp.clicked_by(PointerButton::Primary)
        && let Some((press, _)) = state.press.clone()
    {
        let mut s = session.lock();
        match press {
            Press::Layer(h) if shift => {
                if let Some(i) = s.selected_layers.iter().position(|p| *p == h) {
                    s.selected_layers.remove(i);
                } else {
                    s.selected_layers.push(h);
                }
            }
            Press::Layer(h) => {
                s.selected_layers = vec![h];
                s.selected_keys.clear();
            }
            Press::Empty if !shift => {
                s.selected_layers.clear();
                s.selected_keys.clear();
            }
            _ => {}
        }
    }

    // Drags start.
    if resp.drag_started_by(PointerButton::Primary)
        && let Some((press, origin)) = state.press.clone()
    {
        let at = view.to_comp(origin);
        session.lock().stop();
        match press {
            Press::Free => {}
            Press::Path(h) => {
                let pi = match h {
                    path_edit::Hit::Handle { path, .. } | path_edit::Hit::Vertex { path, .. } => {
                        path
                    }
                };
                state.path_drag = Some(PathDrag::new(paths[pi].clone(), h, &state.vertices, at));
            }
            Press::Handle(layer, handle) => {
                if let Some(bounds) = comp.layer(&layer).and_then(render::layer_bounds) {
                    let mut layers = vec![layer.clone()];
                    layers.extend(selected.iter().filter(|l| **l != layer).cloned());
                    state.drag = Some(primary_first(
                        TransformDrag::new(stage, comp, &layers, Kind::Scale { handle, bounds }),
                        &layer,
                    ));
                }
            }
            Press::Axis(layer, axis) => {
                if let Some(l) = comp.layer(&layer)
                    && let Some(g) = Gizmo::of(l, view)
                    && let Some((camera, size)) = view.camera
                {
                    let kind = if tool == Tool::Rotation {
                        let center = view.project(g.origin, true).map_or(at, |c| view.to_comp(c));
                        Kind::Spin {
                            axis,
                            center,
                            start: at,
                            last: 0.0,
                            sign: gizmo3d::spin_sign(&camera, g.origin, g.axes[axis]),
                            orientation: options.rotation_sets_orientation,
                        }
                    } else {
                        let dir = g.axes[axis];
                        Kind::Axis {
                            axis,
                            origin: g.origin,
                            dir,
                            camera: Box::new(camera),
                            size,
                            start: gizmo3d::along(&camera, size, at, g.origin, dir).unwrap_or(0.0),
                        }
                    };
                    let mut layers = vec![layer.clone()];
                    layers.extend(
                        selected
                            .iter()
                            .filter(|p| **p != layer && comp.layer(p).is_some_and(render::is_3d))
                            .cloned(),
                    );
                    state.drag = Some(primary_first(
                        TransformDrag::new(stage, comp, &layers, kind),
                        &layer,
                    ));
                }
            }
            Press::Anchor(layer) => {
                let anchor = comp
                    .layer(&layer)
                    .map(|l| {
                        let a = l.world_matrix.transform_point3(l.transform.anchor_point);
                        DVec2::new(a.x, a.y)
                    })
                    .unwrap_or(at);
                let drag = TransformDrag::new(
                    stage,
                    comp,
                    std::slice::from_ref(&layer),
                    Kind::Anchor {
                        start: at,
                        compensate: true,
                    },
                );
                state.drag = Some(drag.with_snapping(comp, &layer, anchor));
            }
            Press::Layer(layer) => {
                let layers = {
                    let mut s = session.lock();
                    if !s.selected_layers.contains(&layer) {
                        if shift {
                            s.selected_layers.push(layer.clone());
                        } else {
                            s.selected_layers = vec![layer.clone()];
                        }
                    }
                    s.selected_layers.clone()
                };
                let grabbed = comp.layer(&layer);
                let kind = match tool {
                    Tool::Rotation => {
                        let center = grabbed
                            .map(|l| view.to_comp(transform::anchor_screen(l, view)))
                            .unwrap_or(at);
                        Kind::Rotate {
                            center,
                            start: at,
                            last: 0.0,
                            orientation: options.rotation_sets_orientation,
                        }
                    }
                    Tool::PanBehind => Kind::Anchor {
                        start: at,
                        compensate: false,
                    },
                    _ => Kind::Move { start: at },
                };
                // A 3D layer moves parallel to the view, at its depth.
                let plane = grabbed.filter(|l| render::is_3d(l)).and_then(|l| {
                    let (camera, size) = view.camera?;
                    let a = l.world_matrix.transform_point3(l.transform.anchor_point);
                    Some(transform::ViewPlane {
                        camera,
                        size,
                        depth: camera.view().transform_point3(a).z,
                    })
                });
                let drag = TransformDrag::new(stage, comp, &layers, kind).with_plane(plane);
                state.drag = Some(primary_first(drag.with_snapping(comp, &layer, at), &layer));
            }
            Press::Empty if tool == Tool::Selection => {
                state.marquee = Some(Marquee {
                    origin,
                    base: if shift { selected.clone() } else { Vec::new() },
                });
            }
            Press::Empty => {}
        }
    }

    // Drags follow the pointer.
    let snap = (options.snapping != command)
        .then(|| f64::from(transform::SNAP_DISTANCE / view.scale.max(1e-6)));
    if let Some(d) = state.drag.as_mut()
        && let Some(pos) = pointer
    {
        d.update(view.to_comp(pos), shift, snap);
    }
    if let Some(d) = state.path_drag.as_mut()
        && let Some(pos) = pointer
    {
        d.update(view.to_comp(pos), shift);
    }
    if let Some(f) = state.free.as_mut()
        && f.dragging()
        && let Some(pos) = pointer
    {
        f.update(view, pos, shift);
    }
    if let Some(m) = &state.marquee
        && let Some(pos) = pointer
    {
        let r = Rect::from_two_pos(m.origin, pos);
        let mut sel = m.base.clone();
        let outer = Affine::new(DMat4::IDENTITY);
        for l in &comp.layers {
            if query::is_locked(stage, &l.path) || sel.contains(&l.path) {
                continue;
            }
            if render::bounds_quad(l, view, &outer).is_some_and(|q| quad_overlaps(&q, r)) {
                sel.push(l.path.clone());
            }
        }
        // Keep the comp's stacking order.
        let order = query::comp_layers(stage, &info.path);
        sel.sort_by_key(|l| order.iter().position(|x| x == l));
        let mut s = session.lock();
        if s.selected_layers != sel {
            s.selected_layers = sel;
            s.selected_keys.clear();
        }
        transform::paint_marquee(painter, r);
    }

    // Drags end.
    if !primary_down {
        let time = session.lock().time(&info.path);
        if let Some(d) = state.drag.take()
            && d.changed()
        {
            let edits = d.commit(stage, time);
            if !edits.is_empty() {
                cx.invoke_with("attr.set_many", json!({ "edits": edits }));
            }
        }
        if let Some(d) = state.path_drag.take()
            && d.changed()
        {
            d.commit(cx, session, info);
        }
        if let Some(f) = state.free.as_mut() {
            f.release(cx, session, info);
        }
        state.marquee = None;
        state.press = None;
    }

    // Paint: mask vertices, snap guides, the hover outline.
    let mut shown = paths.clone();
    if let Some(f) = &state.free {
        if let Some(p) = shown.iter_mut().find(|p| p.prim == f.prim) {
            p.path = f.preview().clone();
        }
        f.paint(painter, view);
    }
    if let Some(d) = &state.path_drag {
        let preview = d.preview();
        if let Some(p) = shown.iter_mut().find(|p| &p.prim == d.prim()) {
            *p = preview.clone();
        }
        let stroke = egui::Stroke::new(1.0, preview.color);
        painter.extend(overlay::path_shapes(
            &preview.path,
            &preview.to_comp,
            view,
            stroke,
        ));
    }
    path_edit::paint(painter, view, &shown, &state.vertices);
    if let Some(d) = &state.drag {
        d.paint_guides(painter, view, info);
    }
    // The axis gizmo of the selected 3D layers (rings for the Rotation tool).
    if matches!(tool, Tool::Selection | Tool::Rotation) {
        let active = match (&state.drag, hover) {
            (Some(d), _) => d
                .axis()
                .zip(d.edits.first())
                .map(|(a, e)| (e.path.clone(), a)),
            (None, Some(pos)) => match target(pos, state) {
                Press::Axis(l, a) => Some((l, a)),
                _ => None,
            },
            _ => None,
        };
        for l in comp.layers.iter().filter(|l| selected.contains(&l.path)) {
            if let Some(g) = Gizmo::of(l, view) {
                let on = active
                    .as_ref()
                    .filter(|(p, _)| *p == l.path)
                    .map(|(_, a)| *a);
                g.paint(painter, view, tool == Tool::Rotation, on);
            }
        }
    }
    let mut cursor = None;
    if let Some(f) = &state.free
        && let Some(pos) = hover
        && let Some(c) = f.cursor(view, pos)
    {
        return Some(c);
    }
    if !state.busy()
        && let Some(pos) = hover
    {
        match target(pos, state) {
            Press::Handle(_, h) => cursor = Some(handle_cursor(h)),
            Press::Layer(l) if !selected.contains(&l) && tool == Tool::Selection => {
                if let Some(el) = comp.layer(&l) {
                    transform::hover_outline(painter, el, view, label_color(stage, &l));
                }
            }
            _ => {}
        }
    }
    cursor
}

/// Put the edit of `primary` first (scale and rotate measure on it).
fn primary_first(mut d: TransformDrag, primary: &PrimPath) -> TransformDrag {
    if let Some(i) = d.edits.iter().position(|e| &e.path == primary) {
        d.edits.swap(0, i);
    }
    d
}

/// Cmd/Ctrl double-click on the Pan Behind tool button (AE): the anchor point of
/// each selected layer moves to the center of its content, the layer stays in
/// place (Layer > Transform > Center Anchor Point in Layer Content).
pub(super) fn center_anchors(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    comp: &EvaluatedComp,
) {
    let (selected, time) = {
        let s = session.lock();
        (s.selected_layers.clone(), s.time(&info.path))
    };
    let mut edits = Vec::new();
    for l in comp.layers.iter().filter(|l| selected.contains(&l.path)) {
        let Some((a, b)) = render::layer_bounds(l) else {
            continue;
        };
        let c = Affine::new(l.world_matrix).apply((a + b) / 2.0);
        let anchor = l.world_matrix.transform_point3(l.transform.anchor_point);
        let start = DVec2::new(anchor.x, anchor.y);
        let mut d = TransformDrag::new(
            cx.stage,
            comp,
            std::slice::from_ref(&l.path),
            Kind::Anchor {
                start,
                compensate: true,
            },
        );
        d.update(c, false, None);
        edits.extend(d.commit(cx.stage, time));
    }
    if !edits.is_empty() {
        cx.invoke_with("attr.set_many", json!({ "edits": edits }));
    }
}
