//! Composition panel (the comp viewer): the active comp at the current time, layer
//! selection and moving with the Selection tool, magnification and panning,
//! title/action-safe guides and a grid. Pixels come from [`crate::render`] until the
//! GPU compositor (04) publishes viewport textures.

use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_anim::{EvaluatedComp, Transform};
use bae_model::{LayerKind, PrimPath, Time, Value};
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
pub(crate) mod errors;
mod free_transform;
mod menu;
mod motion_path;
mod overlay;
mod path_edit;
mod pen_tool;
pub(crate) mod prefs;
mod select_tool;
mod shape_tool;
#[cfg(test)]
mod tests;
mod transform;
mod type_tool;
pub(crate) mod view3d;

pub(crate) use transform::nudge_layers;

const MAGNIFICATIONS: [f32; 9] = [0.125, 0.25, 0.333, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0];
/// Magnification change per mouse-wheel notch, in and out (AE 26 Smooth Zoom:
/// ×9/8 and ×7/8, R1).
const WHEEL_ZOOM_IN: f32 = 9.0 / 8.0;
const WHEEL_ZOOM_OUT: f32 = 7.0 / 8.0;

#[derive(Default)]
pub struct CompositionViewer {
    eval: EvalCache,
    /// The expression error banner.
    banner: errors::Banner,
    /// Magnification (1 = 100%: one comp pixel per screen pixel); `None` = Fit.
    zoom: Option<f32>,
    pan: Vec2,
    safe: bool,
    grid: bool,
    checker: bool,
    locked: bool,
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
    /// Pen tools: the open path being drawn, a Convert Vertex drag.
    pen: pen_tool::PenState,
    /// Selection, Rotation and Pan Behind tools: transform drags, marquee,
    /// selected mask vertices.
    select: select_tool::SelectState,
    /// Magnification of the last frame (arrow keys nudge by screen pixels).
    mag: f32,
    /// Layers under the pointer when the context menu opened (topmost first).
    menu_layers: Vec<PrimPath>,
    /// Pixel step the last frame rendered at (1 Full, 2 Half …).
    render_factor: u32,
    /// Type tool: text being typed or edited.
    type_tool: type_tool::TypeState,
    /// Settings read at the start of each frame.
    prefs: prefs::ViewerPrefs,
    /// Comp → screen mapping of the last frame.
    last_view: Option<View>,
    /// Motion paths of the selected layers (C).
    motion_path: motion_path::MotionPathState,
    /// Something in the viewer uses Esc (a drag, text, Free Transform, a
    /// menu): Esc goes to it instead of switching to the last 3D view.
    esc_taken: bool,
    /// Esc to give back to the viewer's widgets this frame.
    escape: bool,
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
        let point = self.prefs.point_size;
        (
            View {
                origin,
                scale,
                point,
                camera: None,
            },
            scale * ppp,
        )
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

    fn nudge_vertices(
        &mut self,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        d: DVec2,
    ) -> bool {
        let Some(comp) = self.evaluate(cx, session, info) else {
            return false;
        };
        let selected = session.lock().selected_layers.clone();
        let paths = overlay::editable_paths(cx.stage, &comp, &selected, None);
        path_edit::nudge(cx, session, info, &paths, &self.select.vertices, d)
    }

    fn delete_vertices(
        &mut self,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
    ) {
        let Some(comp) = self.evaluate(cx, session, info) else {
            return;
        };
        let selected = session.lock().selected_layers.clone();
        let paths = overlay::editable_paths(cx.stage, &comp, &selected, None);
        if path_edit::delete(cx, &paths, &self.select.vertices) {
            self.select.vertices.clear();
        }
    }

    fn evaluate(
        &mut self,
        cx: &PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
    ) -> Option<EvaluatedComp> {
        let time = session.lock().time(&info.path);
        self.eval
            .get(cx.resources)
            .evaluate_comp(cx.stage, &info.path, time)
            .ok()
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
fn apply_overrides(
    comp: &mut EvaluatedComp,
    scrub: Option<&Scrub>,
    drag: Option<&transform::TransformDrag>,
) {
    let mut changed = false;
    for layer in &mut comp.layers {
        let three_d = layer.switches.three_d;
        let mut t: Transform = layer.transform;
        let mut touched = false;
        // A scrubbed effect parameter of this layer (Effect Controls, timeline).
        if let Some(s) = scrub.filter(|s| s.attr.starts_with("param:"))
            && crate::effects::layer_of(&s.path).as_ref() == Some(&layer.path)
            && let Some(e) = layer.effects.iter_mut().find(|e| e.path == s.path)
        {
            e.params.insert(
                s.attr.trim_start_matches("param:").to_owned(),
                s.value.clone(),
            );
        }
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
        if let Some(e) = drag.and_then(|d| d.edits.iter().find(|e| e.path == layer.path)) {
            touched = true;
            t.position = e.now.position;
            t.anchor_point = e.now.anchor_point;
            t.scale = e.now.scale;
            t.rotation = e.now.rotation;
            t.orientation = e.now.orientation;
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
    fn tab(&self, cx: &PanelContext<'_>) -> TabInfo {
        // Read from the session every frame, so New / Open / Close Project
        // update the tab (and its accessible name) at once (issue #215).
        let title = match common::active_comp(cx.stage, &common::session(cx.resources)) {
            Some(info) => format!("{} {}", tr("Composition"), info.name),
            None => tr("Composition").to_owned(),
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
        if action.id == view3d::ids::LAST && self.esc_taken {
            self.escape = true;
            return true;
        }
        if view3d::handle(action, cx) {
            return true;
        }
        // While a shape is being drawn the keys belong to the drag: Space moves
        // the shape (not playback), arrows and Page Up / Down change it.
        if self.shape.is_some() || self.pen.busy() || self.select.busy() {
            return true;
        }
        let session = common::session(cx.resources);
        match action.id.as_str() {
            crate::tools::ids::NUDGE => {
                let p = |k: &str| action.params.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
                let mag = f64::from(if self.mag > 0.0 { self.mag } else { 1.0 });
                if !self.select.vertices.is_empty()
                    && let Some(info) = common::active_comp(cx.stage, &session)
                {
                    let d = DVec2::new(p("dx") / mag, p("dy") / mag);
                    return self.nudge_vertices(cx, &session, &info, d);
                }
                nudge_layers(cx, &session, p("dx"), p("dy"), mag);
                true
            }
            crate::actions::ids::DELETE if !self.select.vertices.is_empty() => {
                if let Some(info) = common::active_comp(cx.stage, &session) {
                    self.delete_vertices(cx, &session, &info);
                }
                true
            }
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
        self.prefs = prefs::ViewerPrefs::current(cx.resources);
        let Some(info) = common::active_comp(cx.stage, &session) else {
            empty_viewer(ui, cx, &session);
            return;
        };
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
        if std::mem::take(&mut self.escape) {
            ui.input_mut(|i| {
                i.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            });
        }

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
        // Wheel (AE-UI-011): a mouse wheel zooms around the pointer (Alt/Option:
        // around the view's center; Shift: scrolls sideways); trackpad scrolling
        // pans (AE 25.3+ on macOS); pinch and Cmd/Ctrl+scroll zoom.
        if resp.hovered() && self.shape.is_none() {
            let (zoom, wheel) = ui.input(|i| {
                let mut notches = 0.0f32;
                let mut pan = Vec2::ZERO;
                for e in &i.events {
                    if let egui::Event::MouseWheel {
                        unit,
                        delta,
                        modifiers,
                        ..
                    } = e
                    {
                        match unit {
                            egui::MouseWheelUnit::Point => pan += *delta,
                            _ if modifiers.shift => pan.x += (delta.x + delta.y) * 40.0,
                            egui::MouseWheelUnit::Line => notches += delta.y,
                            egui::MouseWheelUnit::Page => notches += delta.y * 4.0,
                        }
                    }
                }
                (i.zoom_delta(), (notches, pan, i.modifiers.alt))
            });
            let (notches, pan, alt) = wheel;
            if zoom != 1.0 {
                self.zoom_by(zoom, resp.hover_pos(), area, &info, ppp);
            } else {
                if notches != 0.0 {
                    let around = if alt { None } else { resp.hover_pos() };
                    let step = if notches > 0.0 {
                        WHEEL_ZOOM_IN
                    } else {
                        WHEEL_ZOOM_OUT
                    };
                    self.zoom_by(step.powf(notches.abs()), around, area, &info, ppp);
                }
                self.pan += pan;
            }
        }
        if resp.dragged_by(egui::PointerButton::Middle)
            || resp.dragged_by(egui::PointerButton::Secondary)
        {
            self.pan += resp.drag_delta();
        }
        let (view, mag) = self.view(area, &info, ppp);
        self.last_view = Some(view);
        if self.mag != mag {
            self.mag = mag;
            crate::tools::shared(cx.resources).lock().magnification = mag;
        }

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
        // Expression errors at this time: the banner and the timeline's icons.
        {
            let mut s = session.lock();
            if s.expr_errors != comp.errors {
                s.expr_errors = comp.errors.clone();
            }
        }
        // A value drag that ended without a commit (e.g. cancelled) must not leave
        // its preview behind.
        if !ui.input(|i| i.pointer.any_down()) {
            session.lock().scrub = None;
        }
        let scrub = session.lock().scrub.clone();
        apply_overrides(&mut comp, scrub.as_ref(), self.select.drag.as_ref());
        // The 3D view: 3D layers are seen through its camera.
        let size = DVec2::new(f64::from(info.width), f64::from(info.height));
        let view_camera = session
            .lock()
            .views3d
            .get(&info.path)
            .and_then(|v| v.camera(size));
        let view = View {
            camera: Some((
                view_camera.unwrap_or_else(|| view3d::active_camera(&comp, size)),
                size,
            )),
            ..view
        };
        self.last_view = Some(view);
        // Text being edited is drawn by the text field, not the layer.
        if let Some(edited) = self.type_tool.edited_layer()
            && let Some(l) = comp.layers.iter_mut().find(|l| &l.path == edited)
        {
            l.visible = false;
        }
        // Double-clicking a shape tool button: a layer- or comp-sized shape.
        let tools = crate::tools::shared(cx.resources);
        let fill = tools.lock().fill_request.take();
        if let Some(request) = fill {
            shape_tool::fill_target(cx, &session, &info, &comp, request, &tools);
        }
        // Cmd/Ctrl double-click on the Pan Behind tool button.
        let center = std::mem::take(&mut tools.lock().center_anchor_request);
        if center {
            select_tool::center_anchors(cx, &session, &info, &comp);
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
        let resolution = render_factor(cx.stage, &info.path, session.lock().auto_resolution, mag);
        self.render_factor = resolution;
        let image = self
            .gpu
            .as_mut()
            .and_then(|g| g.frame(cx.resources, cx.stage, &comp, resolution, view_camera));
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
            let c = self.prefs.handle_color(theme::label_color(label));
            if let Some(q) = render::bounds_quad(l, &view, &outer) {
                painter.add(egui::Shape::closed_line(q.to_vec(), Stroke::new(1.0, c)));
                for i in 0..4 {
                    let mid = q[i] + (q[(i + 1) % 4] - q[i]) / 2.0;
                    for p in [q[i], mid] {
                        painter.rect_stroke(
                            Rect::from_center_size(p, vec2(view.point, view.point)),
                            0.0,
                            Stroke::new(1.0, c),
                            egui::StrokeKind::Middle,
                        );
                    }
                }
            }
            let a = l.world_matrix.transform_point3(l.transform.anchor_point);
            let Some(ap) = view.project(a, render::is_3d(l)) else {
                continue;
            };
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
        } else if self.space_hold.is_none()
            && resp.hovered()
            && self.shape.is_none()
            && !self.pen.busy()
            && !self.select.busy()
        {
            let s = session.lock();
            self.space_hold = Some((s.is_playing(), s.time(&info.path)));
        }
        let hand = tool == Tool::Hand || self.space_hold.is_some() || self.panning;
        // Cmd/Ctrl makes the pen tools the Selection tool while held (AE-SHP-044).
        let command = ui.input(|i| i.modifiers.command);
        let effective = if hand {
            Tool::Hand
        } else if tool.is_pen() && command && !self.pen.busy() {
            Tool::Selection
        } else {
            tool
        };
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
        let pen = pen_tool::frame(
            &mut self.pen,
            ui,
            &resp,
            cx,
            &session,
            &info,
            &comp,
            &view,
            effective,
            &painter,
        );
        type_tool::frame(
            &mut self.type_tool,
            ui,
            &resp,
            area,
            cx,
            &session,
            &info,
            &comp,
            &view,
            effective,
            &painter,
        );
        // Motion paths (C): drawn for the selected layers; a press on a keyframe
        // square or handle is theirs, not the Selection tool's.
        let path = motion_path::frame(
            &mut self.motion_path,
            ui,
            &resp,
            cx,
            &session,
            &info,
            &comp,
            &view,
            effective,
            &painter,
            &self.prefs,
        );
        let select_cursor = if path.owns {
            path.cursor
        } else {
            path.cursor.or(select_tool::frame(
                &mut self.select,
                ui,
                &resp,
                cx,
                &session,
                &info,
                &comp,
                &view,
                effective,
                &painter,
            ))
        };
        if resp.hovered()
            && let Some(pos) = resp.hover_pos()
        {
            let hover = Hover {
                alt: ui.input(|i| i.modifiers.alt),
                dragging: self.panning,
            };
            let shown = pen.as_ref().map_or(effective, |p| p.tool);
            match select_cursor {
                Some(icon) => ui.ctx().set_cursor_icon(icon),
                None => cursor::show(ui, &painter, pos, cursor::for_tool(shown, hover)),
            }
            if pen.as_ref().is_some_and(|p| p.close) {
                cursor::close_mark(&painter, pos);
            }
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
        menu::frame(&mut self.menu_layers, &resp, cx, &session, &comp, &view);

        // Items dropped from the project panel become layers.
        if let Some(item) = resp.dnd_release_payload::<ItemDrag>() {
            add_item_as_layer(cx, &session, &item.0, pointer.map(|p| view.to_comp(p)));
        }
        // An effect dropped on a layer (Effects & Presets) is applied to it.
        if let Some(fx) = resp.dnd_release_payload::<crate::effects::EffectDrag>()
            && let Some(p) = pointer
            && let Some(layer) = select_tool::hit_layer(cx.stage, &comp, &view, p)
        {
            crate::effects::apply(cx, &[layer], &fx.0);
        }

        self.banner.ui(ui, cx, &session, area);
        bar::bottom_bar(ui, cx, bar, self, &info, time, mag, &session);
        self.esc_taken = self.shape.is_some()
            || self.pen.busy()
            || self.select.busy()
            || self.select.free.is_some()
            || ui.ctx().text_edit_focused()
            || egui::Popup::is_any_open(ui.ctx());
    }
}

/// The comp's resolution as a pixel step (Composition Settings > Resolution:
/// 1 Full, 2 Half, 3 Third, 4 Quarter, more Custom).
pub(crate) fn comp_resolution(stage: &bae_model::Stage, comp: &PrimPath) -> u32 {
    match stage
        .prim(comp)
        .and_then(|v| query::static_value(&v, "resolution"))
    {
        Some(Value::Vec2([h, _])) => (h as u32).max(1),
        _ => 1,
    }
}

/// The pixel step the viewer renders at: the comp's resolution, or on Auto the
/// coarsest step the magnification hides (50 % → Half, 25 % → Quarter).
fn render_factor(stage: &bae_model::Stage, comp: &PrimPath, auto: bool, mag: f32) -> u32 {
    if auto {
        (1.0 / mag.max(0.01) + 0.01).floor().max(1.0) as u32
    } else {
        comp_resolution(stage, comp)
    }
}

/// Set the active comp's resolution (View > Resolution, the viewer's menu) and,
/// with `nested`, of the comps nested in it (AE's "… Recurse" shortcuts); turns
/// Auto off. One `comp.set_settings` per comp, like Composition Settings.
pub(crate) fn set_resolution(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    factor: u32,
    nested: bool,
) {
    let Some(comp) = session.lock().active_comp.clone() else {
        return;
    };
    session.lock().auto_resolution = false;
    let mut comps = vec![comp];
    let mut i = 0;
    while nested && i < comps.len() {
        for l in query::comp_layers(cx.stage, &comps[i]) {
            if let Some(Value::PrimRef(src)) = cx
                .stage
                .prim(&l)
                .and_then(|v| query::static_value(&v, "source"))
                && query::comp_info(cx.stage, &src).is_some()
                && !comps.contains(&src)
            {
                comps.push(src);
            }
        }
        i += 1;
    }
    for c in comps {
        if comp_resolution(cx.stage, &c) != factor {
            cx.invoke_with(
                "comp.set_settings",
                json!({"comp": c, "resolution": [factor, factor]}),
            );
        }
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

fn empty_viewer(ui: &mut Ui, cx: &PanelContext<'_>, session: &SharedSession) {
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
