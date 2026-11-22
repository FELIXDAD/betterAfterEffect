//! Composition panel (the comp viewer): the active comp at the current time, layer
//! selection and moving with the Selection tool, magnification and panning,
//! title/action-safe guides and a grid. Pixels come from [`crate::render`] until the
//! GPU compositor (04) publishes viewport textures.

use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_anim::{EvaluatedComp, Transform};
use bae_model::{LayerKind, PrimPath, Stage, Time, Value};
use bae_motion::query::{self, CompInfo};
use bae_ui::Accessible as _;
use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use bae_ui::{Invocation, Panel, PanelContext, TabInfo, tr};
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use serde_json::json;

use crate::common::{self, EvalCache};
use crate::gpu_preview::GpuPreview;
use crate::project::ItemDrag;
use crate::render::{self, Affine, View};
use crate::session::{Scrub, SharedSession};
use crate::tools::cursor::{self, Hover};
use crate::tools::{Tool, ViewRequest};

mod bar;
mod overlay;
mod shape_tool;
#[cfg(test)]
mod tests;

const MAGNIFICATIONS: [f32; 9] = [0.125, 0.25, 0.333, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0];

struct MoveDrag {
    /// `(layer, position at drag start, parent-space linear inverse)`.
    layers: Vec<(PrimPath, DVec3, DMat4)>,
    start: DVec2,
    delta: DVec2,
}

#[derive(Default)]
pub struct CompositionViewer {
    eval: EvalCache,
    /// Magnification (1 = 100%: one comp pixel per screen pixel); `None` = Fit.
    zoom: Option<f32>,
    pan: Vec2,
    safe: bool,
    grid: bool,
    checker: bool,
    locked: bool,
    drag: Option<MoveDrag>,
    title: String,
    /// Zoom step requested by a shortcut, applied in `ui` (needs the panel rect).
    pending_zoom: Option<bool>,
    gpu: Option<GpuPreview>,
    gpu_tried: bool,
    /// The last frame came from the GPU compositor.
    on_gpu: bool,
    /// Space is held over the viewer (temporary Hand tool): whether the comp was
    /// playing and its time when Space went down, so a Space-drag that pans does
    /// not leave the playback Space started behind.
    space_hold: Option<(bool, Time)>,
    /// A Hand-tool (or Space) drag is panning the view.
    panning: bool,
    /// A shape being drawn with a shape tool.
    shape: Option<shape_tool::ShapeDrag>,
    /// Comp → screen mapping of the last frame.
    last_view: Option<View>,
}

impl CompositionViewer {
    pub fn new(state: Option<&serde_json::Value>) -> Self {
        let mut v = CompositionViewer::default();
        if let Some(s) = state {
            v.safe = s.get("safe").and_then(|v| v.as_bool()).unwrap_or(false);
            v.grid = s.get("grid").and_then(|v| v.as_bool()).unwrap_or(false);
            v.checker = s.get("checker").and_then(|v| v.as_bool()).unwrap_or(false);
        }
        v
    }

    fn view(&self, area: Rect, info: &CompInfo, ppp: f32) -> (View, f32) {
        let (w, h) = (info.width as f32, info.height as f32);
        let fit = ((area.width() - 24.0) / w)
            .min((area.height() - 24.0) / h)
            .max(0.01);
        // AE "Fit up to 100%".
        let fit = fit.min(1.0 / ppp);
        let scale = self.zoom.map_or(fit, |m| m / ppp);
        let origin = area.center() - vec2(w, h) * scale / 2.0 + self.pan;
        (View { origin, scale }, scale * ppp)
    }

    fn zoom_by(
        &mut self,
        factor: f32,
        around: Option<Pos2>,
        area: Rect,
        info: &CompInfo,
        ppp: f32,
    ) {
        let (view, mag) = self.view(area, info, ppp);
        let new = (mag * factor).clamp(0.02, 32.0);
        let anchor = around.unwrap_or(area.center());
        let comp_pt = view.to_comp(anchor);
        self.zoom = Some(new);
        let (v2, _) = self.view(area, info, ppp);
        let moved = v2.to_screen(comp_pt);
        self.pan += anchor - moved;
    }

    fn step_zoom(&mut self, up: bool, area: Rect, info: &CompInfo, ppp: f32) {
        let (_, mag) = self.view(area, info, ppp);
        let next = if up {
            MAGNIFICATIONS.iter().copied().find(|m| *m > mag * 1.01)
        } else {
            MAGNIFICATIONS
                .iter()
                .rev()
                .copied()
                .find(|m| *m < mag * 0.99)
        };
        if let Some(m) = next {
            self.zoom_by(m / mag, None, area, info, ppp);
        }
    }

    /// Zoom tool click: the next magnification step, keeping `at` in place.
    fn step_zoom_at(&mut self, up: bool, at: Pos2, area: Rect, info: &CompInfo, ppp: f32) {
        let (_, mag) = self.view(area, info, ppp);
        let next = if up {
            MAGNIFICATIONS.iter().copied().find(|m| *m > mag * 1.01)
        } else {
            MAGNIFICATIONS
                .iter()
                .rev()
                .copied()
                .find(|m| *m < mag * 0.99)
        };
        if let Some(m) = next {
            self.zoom_by(m / mag, Some(at), area, info, ppp);
        }
    }
}

/// Apply live edits (timeline scrubs, viewer drags) to an evaluated comp and
/// recompute world matrices.
fn apply_overrides(comp: &mut EvaluatedComp, scrub: Option<&Scrub>, drag: Option<&MoveDrag>) {
    let mut changed = false;
    for layer in &mut comp.layers {
        let three_d = layer.switches.three_d;
        let mut t: Transform = layer.transform;
        let mut touched = false;
        if let Some(s) = scrub.filter(|s| s.path == layer.path) {
            touched = true;
            let v3 = |v: &Value| match v {
                Value::Vec3(a) => Some(DVec3::from_array(*a)),
                Value::Vec2(a) => Some(DVec3::new(a[0], a[1], 0.0)),
                _ => None,
            };
            match (s.attr.as_str(), &s.value) {
                ("transform:position", v) => t.position = v3(v).unwrap_or(t.position),
                ("transform:anchor_point", v) => t.anchor_point = v3(v).unwrap_or(t.anchor_point),
                ("transform:scale", v) => t.scale = v3(v).unwrap_or(t.scale),
                ("transform:rotation", Value::Float(r)) => t.rotation.z = *r,
                ("transform:opacity", Value::Float(o)) => {
                    t.opacity = *o;
                    layer.opacity = (o / 100.0).clamp(0.0, 1.0);
                }
                _ => touched = false,
            }
        }
        if let Some(d) = drag
            && let Some((_, start, inv)) = d.layers.iter().find(|(p, ..)| *p == layer.path)
        {
            touched = true;
            let local = inv.transform_vector3(DVec3::new(d.delta.x, d.delta.y, 0.0));
            t.position = *start + local;
        }
        if touched {
            layer.transform = t;
            layer.local_matrix = if matches!(layer.kind, LayerKind::Camera | LayerKind::Light) {
                t.camera_matrix()
            } else {
                t.matrix(three_d)
            };
            changed = true;
        }
    }
    if changed {
        // Parents come anywhere in the list: resolve recursively.
        let n = comp.layers.len();
        let mut done = vec![false; n];
        fn world(comp: &mut EvaluatedComp, i: usize, done: &mut [bool], depth: usize) -> DMat4 {
            if done[i] || depth > 64 {
                return comp.layers[i].world_matrix;
            }
            let local = comp.layers[i].local_matrix;
            let w = match comp.layers[i].parent {
                Some(p) if p != i => world(comp, p, done, depth + 1) * local,
                _ => local,
            };
            comp.layers[i].world_matrix = w;
            done[i] = true;
            w
        }
        for i in 0..n {
            world(comp, i, &mut done, 0);
        }
    }
}

/// Topmost layer under `pos` (screen), if any.
/// The topmost layer under `pos`; locked layers cannot be picked (AE: clicks go
/// through them).
fn hit_test(stage: &Stage, comp: &EvaluatedComp, view: &View, pos: Pos2) -> Option<PrimPath> {
    let outer = Affine::new(DMat4::IDENTITY);
    comp.layers
        .iter()
        .filter(|l| l.visible && !l.switches.guide || l.kind == LayerKind::Null)
        .filter(|l| !query::is_locked(stage, &l.path))
        .find(|l| {
            render::bounds_quad(l, view, &outer).is_some_and(|q| render::quad_contains(&q, pos))
        })
        .map(|l| l.path.clone())
}

fn checkerboard(p: &egui::Painter, r: Rect) {
    p.rect_filled(r, 0.0, Color32::from_gray(200));
    let s = 8.0;
    let mut y = r.top();
    let mut row = 0;
    while y < r.bottom() {
        let mut x = r.left() + if row % 2 == 0 { 0.0 } else { s };
        while x < r.right() {
            let cell = Rect::from_min_size(pos2(x, y), vec2(s, s)).intersect(r);
            p.rect_filled(cell, 0.0, Color32::from_gray(150));
            x += 2.0 * s;
        }
        y += s;
        row += 1;
    }
}

impl Panel for CompositionViewer {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        let title = if self.title.is_empty() {
            tr("Composition").to_owned()
        } else {
            format!("{} {}", tr("Composition"), self.title)
        };
        TabInfo::new(title).viewer().locked(self.locked)
    }

    fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        Some(json!({"safe": self.safe, "grid": self.grid, "checker": self.checker}))
    }

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        // While a shape is being drawn the keys belong to the drag: Space moves
        // the shape (not playback), arrows and Page Up / Down change it.
        if self.shape.is_some() {
            return true;
        }
        let session = common::session(cx.resources);
        match action.id.as_str() {
            "viewer.zoom_in" | "viewer.zoom_out" => {
                self.pending_zoom = Some(action.id == "viewer.zoom_in");
                true
            }
            "viewer.fit" => {
                self.zoom = None;
                self.pan = Vec2::ZERO;
                true
            }
            _ => crate::actions::handle(action, cx, &session),
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        session.lock().sync(cx.stage);
        let Some(info) = common::active_comp(cx.stage, &session) else {
            self.title.clear();
            empty_viewer(ui, cx, &session);
            return;
        };
        self.title = info.name.clone();
        if session.lock().tick(&info) {
            ui.ctx().request_repaint();
        }
        let time = session.lock().time(&info.path);
        let ppp = ui.ctx().pixels_per_point();

        let full = ui.available_rect_before_wrap();
        let bar_h = 28.0;
        let area = Rect::from_min_max(full.min, pos2(full.right(), full.bottom() - bar_h));
        let bar = Rect::from_min_max(pos2(full.left(), area.bottom()), full.max);
        let resp = ui
            .allocate_rect(area, Sense::click_and_drag())
            .labeled(egui::WidgetType::Image, tr("Composition"));

        if let Some(up) = self.pending_zoom.take() {
            self.step_zoom(up, area, &info, ppp);
        }
        // Double-clicking the Hand / Zoom tool buttons.
        let request = crate::tools::shared(cx.resources)
            .lock()
            .view_request
            .take();
        match request {
            Some(ViewRequest::Center) => self.pan = Vec2::ZERO,
            Some(ViewRequest::Actual) => {
                self.zoom = Some(1.0);
                self.pan = Vec2::ZERO;
            }
            None => {}
        }
        // Zoom: pinch / Cmd+scroll around the pointer; plain scroll pans.
        if resp.hovered() && self.shape.is_none() {
            let (zoom, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
            if zoom != 1.0 {
                self.zoom_by(zoom, resp.hover_pos(), area, &info, ppp);
            } else if scroll != Vec2::ZERO {
                self.pan += scroll;
            }
        }
        if resp.dragged_by(egui::PointerButton::Middle)
            || resp.dragged_by(egui::PointerButton::Secondary)
        {
            self.pan += resp.drag_delta();
        }
        let (view, mag) = self.view(area, &info, ppp);
        self.last_view = Some(view);

        // Evaluate.
        let eval = self.eval.get(cx.resources);
        let mut comp = match eval.evaluate_comp(cx.stage, &info.path, time) {
            Ok(c) => c,
            Err(e) => {
                ui.painter().text(
                    area.center(),
                    Align2::CENTER_CENTER,
                    e.to_string(),
                    theme::body(),
                    color::TEXT_DIM,
                );
                return;
            }
        };
        // A value drag that ended without a commit (e.g. cancelled) must not leave
        // its preview behind.
        if !ui.input(|i| i.pointer.any_down()) {
            session.lock().scrub = None;
        }
        let scrub = session.lock().scrub.clone();
        apply_overrides(&mut comp, scrub.as_ref(), self.drag.as_ref());
        // Double-clicking a shape tool button: a layer- or comp-sized shape.
        let tools = crate::tools::shared(cx.resources);
        let fill = tools.lock().fill_request.take();
        if let Some(request) = fill {
            shape_tool::fill_target(cx, &session, &info, &comp, request, &tools);
        }

        // Paint.
        let painter = ui.painter_at(area);
        painter.rect_filled(area, 0.0, Color32::from_gray(35));
        let comp_rect = render::comp_rect(&view, info.width, info.height);
        if self.checker {
            checkerboard(&painter, comp_rect.intersect(area));
        } else {
            let c = info.bg_color;
            painter.rect_filled(
                comp_rect,
                0.0,
                Color32::from_rgb(
                    (c[0] * 255.0) as u8,
                    (c[1] * 255.0) as u8,
                    (c[2] * 255.0) as u8,
                ),
            );
        }
        let clip = painter.with_clip_rect(comp_rect.intersect(area));
        // GPU preview through 04's compositor when the app has a GPU.
        if self
            .gpu
            .as_ref()
            .is_some_and(|g| !g.is_current(cx.resources))
        {
            self.gpu = None;
            self.gpu_tried = false;
        }
        if self.gpu.is_none() && !self.gpu_tried {
            self.gpu_tried = true;
            self.gpu = GpuPreview::new(cx.resources);
        }
        let resolution = session.lock().resolution;
        let image = self
            .gpu
            .as_mut()
            .and_then(|g| g.frame(cx.resources, cx.stage, &comp, resolution));
        self.on_gpu = image.is_some();
        let mut vector = std::borrow::Cow::Borrowed(&comp);
        if let Some(img) = image {
            let r = img.rect;
            let rect = Rect::from_min_max(
                view.to_screen(DVec2::new(f64::from(r.x0), f64::from(r.y0))),
                view.to_screen(DVec2::new(f64::from(r.x1), f64::from(r.y1))),
            );
            clip.image(
                egui::TextureId::User(img.handle),
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            // Until 05's text source is registered, text layers are drawn here.
            let has_text = self.gpu.as_ref().is_some_and(|g| g.has_text);
            let mut text_only = comp.clone();
            for l in &mut text_only.layers {
                if has_text || l.kind != LayerKind::Text {
                    l.visible = false;
                }
            }
            vector = std::borrow::Cow::Owned(text_only);
        }
        let mut shapes = Vec::new();
        let to_screen = |p: DVec2| view.to_screen(p);
        render::paint_comp(
            &vector,
            &clip,
            &to_screen,
            Affine::new(DMat4::IDENTITY),
            1.0,
            f64::from(view.scale),
            &|s| egui::FontId::proportional(s),
            &mut shapes,
        );
        clip.extend(shapes);

        // Preferences > Grids & Guides (bae-prefs, workstream D).
        let guides = bae_ui::settings::GridGuides::current(cx.resources);
        if self.grid {
            guides.paint_grid(&clip, comp_rect, view.scale);
        }
        if self.safe {
            guides.paint_safe(
                &painter,
                comp_rect,
                Stroke::new(1.0, Color32::from_gray(150)),
            );
            let c = comp_rect.center();
            painter.line_segment(
                [c - vec2(8.0, 0.0), c + vec2(8.0, 0.0)],
                Stroke::new(1.0, Color32::from_gray(150)),
            );
            painter.line_segment(
                [c - vec2(0.0, 8.0), c + vec2(0.0, 8.0)],
                Stroke::new(1.0, Color32::from_gray(150)),
            );
        }

        // Selection: bounding boxes, handles, anchor points (AE draws them in the
        // layer's label color).
        let selected = session.lock().selected_layers.clone();
        let outer = Affine::new(DMat4::IDENTITY);
        for l in comp.layers.iter().filter(|l| selected.contains(&l.path)) {
            let label = cx
                .stage
                .prim(&l.path)
                .and_then(|v| query::static_value(&v, "label"))
                .and_then(|v| v.as_f64())
                .map_or(8, |v| v as usize);
            let c = theme::label_color(label);
            if let Some(q) = render::bounds_quad(l, &view, &outer) {
                painter.add(egui::Shape::closed_line(q.to_vec(), Stroke::new(1.0, c)));
                for i in 0..4 {
                    let mid = q[i] + (q[(i + 1) % 4] - q[i]) / 2.0;
                    for p in [q[i], mid] {
                        painter.rect_stroke(
                            Rect::from_center_size(p, vec2(5.0, 5.0)),
                            0.0,
                            Stroke::new(1.0, c),
                            egui::StrokeKind::Middle,
                        );
                    }
                }
            }
            let a = l.world_matrix.transform_point3(l.transform.anchor_point);
            let ap = view.to_screen(DVec2::new(a.x, a.y));
            painter.circle_stroke(ap, 4.0, Stroke::new(1.0, c));
            painter.line_segment(
                [ap - vec2(7.0, 0.0), ap + vec2(7.0, 0.0)],
                Stroke::new(1.0, c),
            );
            painter.line_segment(
                [ap - vec2(0.0, 7.0), ap + vec2(0.0, 7.0)],
                Stroke::new(1.0, c),
            );
        }

        overlay::masks(&painter, cx.stage, &comp, &view, &selected);

        // Interaction by tool. Space held over the viewer is the Hand tool until
        // released (AE-UI-307), except while drawing a shape (Space moves it).
        let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
        let tool = tools.lock().current();
        let space = ui.input(|i| i.key_down(egui::Key::Space)) && !ui.ctx().text_edit_focused();
        if !space {
            self.space_hold = None;
        } else if self.space_hold.is_none() && resp.hovered() && self.shape.is_none() {
            let s = session.lock();
            self.space_hold = Some((s.is_playing(), s.time(&info.path)));
        }
        let hand = tool == Tool::Hand || self.space_hold.is_some() || self.panning;
        let effective = if hand { Tool::Hand } else { tool };
        if hand && resp.drag_started_by(egui::PointerButton::Primary) {
            self.panning = true;
            // Space started playback when pressed; a Space-drag only pans.
            if let Some((false, t0)) = self.space_hold {
                let mut s = session.lock();
                if s.is_playing() {
                    s.stop();
                    s.set_time(&info, t0);
                }
            }
        }
        if self.panning {
            if resp.dragged_by(egui::PointerButton::Primary) {
                self.pan += resp.drag_delta();
            }
            if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
                self.panning = false;
            }
        }
        if effective == Tool::Zoom
            && resp.clicked_by(egui::PointerButton::Primary)
            && let Some(pos) = pointer
        {
            let out = ui.input(|i| i.modifiers.alt);
            self.step_zoom_at(!out, pos, area, &info, ppp);
        }
        shape_tool::frame(
            &mut self.shape,
            ui,
            &resp,
            cx,
            &session,
            &info,
            &comp,
            &view,
            effective,
        );
        if let Some(d) = &self.shape {
            d.paint(&painter, &view);
        }
        if resp.hovered()
            && let Some(pos) = resp.hover_pos()
        {
            let hover = Hover {
                alt: ui.input(|i| i.modifiers.alt),
                dragging: self.panning,
            };
            cursor::show(ui, &painter, pos, cursor::for_tool(effective, hover));
            // A small star by the crosshair: the drag makes a shape, not a mask.
            if effective.is_shape() {
                let makes_shape = match &self.shape {
                    Some(d) => d.target.makes_shape(),
                    None => {
                        let selected = session.lock().selected_layers.clone();
                        let mask = tools.lock().options.creates_mask;
                        shape_tool::Target::of(cx.stage, &info.path, &selected, mask).makes_shape()
                    }
                };
                if makes_shape {
                    cursor::shape_star(&painter, pos);
                }
            }
        }
        // Clicking a layer selects it with the Selection, Rotation and Pan Behind
        // tools; only the Selection tool moves layers.
        let selects = matches!(
            effective,
            Tool::Selection | Tool::Rotation | Tool::PanBehind
        );
        let moves = effective == Tool::Selection;
        if selects
            && resp.clicked_by(egui::PointerButton::Primary)
            && let Some(pos) = pointer
        {
            let hit = hit_test(cx.stage, &comp, &view, pos);
            let shift = ui.input(|i| i.modifiers.shift);
            let mut s = session.lock();
            match hit {
                Some(h) if shift => {
                    if let Some(i) = s.selected_layers.iter().position(|p| *p == h) {
                        s.selected_layers.remove(i);
                    } else {
                        s.selected_layers.push(h);
                    }
                }
                Some(h) => {
                    s.selected_layers = vec![h];
                    s.selected_keys.clear();
                }
                None => {
                    s.selected_layers.clear();
                    s.selected_keys.clear();
                }
            }
        }
        if moves
            && resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let hit = hit_test(cx.stage, &comp, &view, pos);
            let mut s = session.lock();
            if let Some(h) = &hit
                && !s.selected_layers.contains(h)
            {
                s.selected_layers = vec![h.clone()];
            }
            if hit.is_some() {
                s.stop();
                let layers = s
                    .selected_layers
                    .iter()
                    .filter_map(|p| {
                        let l = comp.layer(p)?;
                        let parent_world = l
                            .parent
                            .map_or(DMat4::IDENTITY, |i| comp.layers[i].world_matrix);
                        Some((p.clone(), l.transform.position, parent_world.inverse()))
                    })
                    .collect();
                self.drag = Some(MoveDrag {
                    layers,
                    start: view.to_comp(pos),
                    delta: DVec2::ZERO,
                });
            }
        }
        if let Some(d) = &mut self.drag {
            if let Some(pos) = resp.interact_pointer_pos() {
                let mut delta = view.to_comp(pos) - d.start;
                if ui.input(|i| i.modifiers.shift) {
                    if delta.x.abs() > delta.y.abs() {
                        delta.y = 0.0;
                    } else {
                        delta.x = 0.0;
                    }
                }
                d.delta = delta;
            }
            if resp.drag_stopped() {
                let d = self.drag.take().expect("some");
                if d.delta.length_squared() > 0.0 {
                    let edits: Vec<serde_json::Value> = d
                        .layers
                        .iter()
                        .map(|(p, start, inv)| {
                            let local =
                                inv.transform_vector3(DVec3::new(d.delta.x, d.delta.y, 0.0));
                            let v = *start + local;
                            json!({
                                "path": p,
                                "attr": "transform:position",
                                "time": query::to_key_time(cx.stage, p, time),
                                "value": {"type": "vec3", "value": [v.x, v.y, v.z]},
                            })
                        })
                        .collect();
                    cx.invoke_with("attr.set_many", json!({"edits": edits}));
                }
            }
        }

        // Items dropped from the project panel become layers.
        if let Some(item) = resp.dnd_release_payload::<ItemDrag>() {
            add_item_as_layer(cx, &session, &item.0, pointer.map(|p| view.to_comp(p)));
        }

        bar::bottom_bar(ui, bar, self, &info, time, mag, &session);
    }
}

/// Add a project item to the active comp (drag and drop).
pub(crate) fn add_item_as_layer(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    item: &PrimPath,
    at: Option<DVec2>,
) {
    let stage = cx.stage;
    let Some(active) = session.lock().active_comp.clone() else {
        return;
    };
    let kind = if query::comp_info(stage, item).is_some() {
        if item == &active {
            return;
        }
        LayerKind::PreComp
    } else {
        match stage
            .prim(item)
            .and_then(|v| query::static_value(&v, "kind"))
        {
            Some(Value::Enum(k)) if k == "solid" => LayerKind::Solid,
            _ => LayerKind::Footage,
        }
    };
    let mut extra = json!({"source": item, "name": query::display_name_of(stage, item)});
    if let Some(p) = at {
        extra["position"] = json!([p.x, p.y]);
    }
    common::new_layer(cx, session, kind, extra);
}

fn empty_viewer(ui: &mut Ui, cx: &mut PanelContext<'_>, session: &SharedSession) {
    ui.add_space((ui.available_height() / 2.0 - 40.0).max(8.0));
    ui.vertical_centered(|ui| {
        icons::show(ui, Icon::Composition, 32.0, color::ICON_DIM);
        ui.add_space(8.0);
        if common::text_button(ui, tr("New Composition")).clicked() {
            common::new_comp(cx, session, None);
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(tr(
                "Open a composition from the Project panel, or create one.",
            ))
            .color(color::TEXT_DIM)
            .font(theme::small()),
        );
    });
}
