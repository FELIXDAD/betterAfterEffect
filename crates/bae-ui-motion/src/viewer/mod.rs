//! Composition panel (the comp viewer): the active comp at the current time, layer
//! selection and moving with the Selection tool, magnification and panning,
//! title/action-safe guides and a grid. Pixels come from [`crate::render`] until the
//! GPU compositor (04) publishes viewport textures.

use bae_anim::glam::{DMat4, DVec2};
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
use crate::project::{DropPlacement, ItemDrag};
use crate::render::{self, Affine, View};
use crate::session::SharedSession;
use crate::tools::cursor::{self, Hover};
use crate::tools::{Tool, ViewRequest};

mod bar;
mod camera_tool;
pub(crate) mod display;
mod effect_points;
pub(crate) mod errors;
mod feather_tool;
mod free_transform;
mod gizmo3d;
pub(crate) mod guides;
mod layout;
mod look_at;
mod menu;
mod motion_path;
mod motion_sketch;
mod navigator;
mod overlay;
mod particle_guides;
mod path_edit;
mod pen_tool;
pub(crate) mod prefs;
mod scene_icons;
mod select_tool;
mod shape_tool;
#[cfg(test)]
mod tests;
pub(crate) mod transform;
mod type_tool;
pub(crate) mod view3d;
pub(crate) mod view_options;

pub(crate) use overlay::{mask_color, path_shapes};
pub(crate) use transform::nudge_layers;

/// The magnification popup (AE 26): `(magnification, label)`.
pub(crate) const MAGNIFICATIONS: [(f32, &str); 14] = [
    (0.015625, "1.5%"),
    (0.03125, "3.1%"),
    (0.0625, "6.25%"),
    (0.125, "12.5%"),
    (0.25, "25%"),
    (0.333, "33.3%"),
    (0.5, "50%"),
    (1.0, "100%"),
    (2.0, "200%"),
    (4.0, "400%"),
    (8.0, "800%"),
    (16.0, "1600%"),
    (32.0, "3200%"),
    (64.0, "6400%"),
];
/// Zoom In / Zoom Out (. and ,) and Zoom tool clicks step by powers of two
/// (AE: 100 % → 50 % → 25 %, no 33.3 %).
const ZOOM_STEPS: [f32; 13] = [
    0.015625, 0.03125, 0.0625, 0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0,
];

/// Zoom to 100 % (/ in the Composition panel).
pub(crate) const ZOOM_100: &str = "motion.viewer.zoom_100";
/// View ▸ Fit up to 100% (Alt/Option+/).
pub(crate) const FIT_UP_TO_100: &str = "motion.viewer.fit_up_to_100";

/// The next zoom step above (`up`) or below `mag`.
pub(crate) fn next_step(mag: f32, up: bool) -> Option<f32> {
    if up {
        ZOOM_STEPS.iter().copied().find(|m| *m > mag * 1.01)
    } else {
        ZOOM_STEPS.iter().rev().copied().find(|m| *m < mag * 0.99)
    }
}

/// Associate the native smart-magnify event with the next input pass.
pub fn request_smart_zoom(ctx: &egui::Context) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("mac.smart_zoom"), pass));
}

pub(crate) fn smart_zoom(ctx: &egui::Context) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<u64>(egui::Id::new("mac.smart_zoom"))) == Some(pass)
}

/// What the mouse wheel and trackpad did this frame over a picture (the
/// viewer and the Layer panel share it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Wheel {
    /// Pinch or Cmd/Ctrl+scroll (egui's zoom factor; 1 = none).
    pub pinch: f32,
    /// Wheel notches up (> 0) or down: zoom by AE's ×9/8 per notch.
    pub notches: f32,
    /// Scrolling to pan (macOS trackpads, Shift+wheel), points.
    pub pan: Vec2,
    /// Alt/Option: zoom around the centre instead of the pointer.
    pub alt: bool,
}

/// Preferences ▸ Previews ▸ Magnification (AE 26): how the wheel and the
/// trackpad zoom the Composition and Layer panels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Magnification {
    /// Type: Legacy — each wheel notch steps through the zoom menu's levels
    /// (instead of Smooth's ×9/8).
    pub legacy: bool,
    /// Behavior: Centered — zoom around the view's center instead of
    /// following the pointer (Alt/Option swaps the two).
    pub centered: bool,
    /// Trackpad Scroll: Zoom — two-finger scrolling zooms instead of panning.
    pub trackpad_zooms: bool,
}

impl Magnification {
    pub(crate) fn of(resources: &bae_app::Resources) -> Self {
        let p = crate::prefs::current(resources);
        Magnification {
            legacy: p.str("previews.magnification_type") == "legacy",
            centered: p.str("previews.magnification_behavior") == "centered",
            trackpad_zooms: p.str("previews.magnification_trackpad_scroll") == "zoom",
        }
    }

    /// Whether the wheel zooms around the pointer (`alt`: Alt/Option held).
    pub(crate) fn follows_pointer(self, alt: bool) -> bool {
        self.centered == alt
    }

    /// The magnification after `notches` wheel notches (up > 0) from `mag`.
    /// Legacy steps whole notches; `rest` carries the fraction between frames
    /// (trackpads and high-resolution wheels).
    pub(crate) fn wheel_zoom(self, mag: f32, notches: f32, rest: &mut f32) -> f32 {
        if !self.legacy {
            *rest = 0.0;
            let step = if notches > 0.0 {
                WHEEL_ZOOM_IN
            } else {
                WHEEL_ZOOM_OUT
            };
            return mag * step.powf(notches.abs());
        }
        *rest += notches;
        let whole = rest.trunc();
        *rest -= whole;
        let mut m = mag;
        for _ in 0..whole.abs() as usize {
            if let Some(next) = next_step(m, whole > 0.0) {
                m = next;
            }
        }
        m
    }
}

/// Points of pixel-precise scrolling that make one wheel notch (Windows and
/// Linux high-resolution wheels report pixels).
const POINTS_PER_NOTCH: f32 = 50.0;

/// Read this frame's wheel input. A mouse wheel zooms; Shift+wheel scrolls
/// sideways; pixel-precise scrolling pans on macOS (AE 25.3+ trackpads; it
/// zooms with Trackpad Scroll: Zoom) and zooms elsewhere, where it comes from
/// high-resolution wheels (#571).
pub(crate) fn wheel(ui: &Ui, m: Magnification) -> Wheel {
    ui.input(|i| {
        let mut w = Wheel {
            pinch: i.zoom_delta(),
            alt: i.modifiers.alt,
            ..Wheel::default()
        };
        for e in &i.events {
            if let egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } = e
            {
                match unit {
                    _ if modifiers.shift => {
                        let lines = if *unit == egui::MouseWheelUnit::Point {
                            1.0
                        } else {
                            40.0
                        };
                        w.pan.x += (delta.x + delta.y) * lines;
                    }
                    egui::MouseWheelUnit::Point if cfg!(target_os = "macos") => {
                        if m.trackpad_zooms {
                            w.notches += delta.y / POINTS_PER_NOTCH;
                        } else {
                            w.pan += *delta;
                        }
                    }
                    egui::MouseWheelUnit::Point => w.notches += delta.y / POINTS_PER_NOTCH,
                    egui::MouseWheelUnit::Line => w.notches += delta.y,
                    egui::MouseWheelUnit::Page => w.notches += delta.y * 4.0,
                }
            }
        }
        w
    })
}
/// Magnification change per mouse-wheel notch, in and out (AE 26 Smooth Zoom:
/// ×9/8 and ×7/8, R1).
pub(crate) const WHEEL_ZOOM_IN: f32 = 9.0 / 8.0;
pub(crate) const WHEEL_ZOOM_OUT: f32 = 7.0 / 8.0;

#[derive(Default)]
pub struct CompositionViewer {
    eval: EvalCache,
    preview_3d: Option<(PrimPath, u64, bool)>,
    /// The expression error banner.
    banner: errors::Banner,
    /// Magnification (1 = 100%: one comp pixel per screen pixel); `None` =
    /// fitted to the panel.
    zoom: Option<f32>,
    /// Fitted with AE's Fit (any magnification) rather than Fit up to 100%.
    fit_all: bool,
    /// Legacy magnification: the part of a wheel notch not yet stepped.
    wheel_rest: f32,
    pan: Vec2,
    /// Title/action safe and grid from the saved panel state, given to the
    /// session's view options on the first frame.
    restore: Option<(bool, bool)>,
    checker: bool,
    /// A guide being dragged out of a ruler or moved.
    guide_drag: Option<guides::GuideDrag>,
    /// Show Channel, Exposure, Region of Interest.
    display: display::Display,
    /// The region of interest being drawn: where the drag started (comp).
    roi_draw: Option<DVec2>,
    /// Show Snapshot: the slot shown while its key (or the button) is held.
    snapshot_shown: Option<usize>,
    /// The Show Snapshot button is held.
    snapshot_button: bool,
    /// The view options of the last frame (saved with the panel).
    saved: guides::ViewOptions,
    locked: bool,
    /// Zoom step requested by a shortcut, applied in `ui` (needs the panel rect).
    pending_zoom: Option<bool>,
    /// View ▸ Zoom to 100 % (/) waits for the frame's area.
    pending_100: bool,
    gpu: Option<GpuPreview>,
    gpu_tried: bool,
    /// GPU previews of the other views of the layout (by view index).
    extra_gpu: Vec<Option<GpuPreview>>,
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
    feather: feather_tool::FeatherState,
    sketch: motion_sketch::SketchState,
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
    /// Point controls of the selected effects.
    effect_points: effect_points::EffectPoints,
    /// Something in the viewer uses Esc (a drag, text, Free Transform, a
    /// menu): Esc goes to it instead of switching to the last 3D view.
    esc_taken: bool,
    /// Esc to give back to the viewer's widgets this frame.
    escape: bool,
    /// The current time and since when it has not changed (idle prefetch).
    idle: Option<(Time, std::time::Instant)>,
    /// A camera tool drag.
    camera_drag: Option<camera_tool::CameraDrag>,
}

/// How long the current time stays put before the frames playback would
/// start with are prepared.
const IDLE_PREFETCH: std::time::Duration = std::time::Duration::from_millis(300);
/// Frames prepared from where playback would start.
const PREFETCH_FRAMES: i64 = 6;

impl CompositionViewer {
    pub fn new(state: Option<&serde_json::Value>) -> Self {
        let mut v = CompositionViewer::default();
        if let Some(s) = state {
            let b = |k: &str| s.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
            v.restore = Some((b("safe"), b("grid")));
            v.checker = s.get("checker").and_then(|v| v.as_bool()).unwrap_or(false);
        }
        v
    }

    fn paint_guides(
        cx: &PanelContext<'_>,
        painter: &egui::Painter,
        clip: &egui::Painter,
        comp_rect: Rect,
        view: &View,
        opts: guides::ViewOptions,
    ) {
        // Preferences > Grids & Guides (bae-prefs, workstream D).
        let guides = bae_ui::settings::GridGuides::current(cx.resources);
        if opts.grid {
            guides.paint_grid(clip, comp_rect, view.scale);
        }
        if opts.proportional {
            guides::paint_proportional(clip, comp_rect, guides.proportional, guides.grid_color);
        }
        if opts.safe {
            guides.paint_safe(
                painter,
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
    }

    fn view(&self, area: Rect, info: &CompInfo, ppp: f32) -> (View, f32) {
        let (w, h) = (info.width as f32, info.height as f32);
        let fit = ((area.width() - 24.0) / w)
            .min((area.height() - 24.0) / h)
            .max(0.01);
        // AE's Fit, or Fit up to 100% (the default).
        let fit = if self.fit_all {
            fit
        } else {
            fit.min(1.0 / ppp)
        };
        let scale = self.zoom.map_or(fit, |m| m / ppp);
        let origin = area.center() - vec2(w, h) * scale / 2.0 + self.pan;
        // At native size a fractional texture edge blends adjacent texels.
        let origin = if cfg!(target_os = "macos") && scale == 1.0 / ppp {
            use egui::emath::GuiRounding as _;
            origin.round_to_pixels(ppp)
        } else {
            origin
        };
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
        if let Some(m) = next_step(mag, up) {
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
        live_evaluator(&mut self.eval, cx, session)
            .evaluate_comp(cx.stage, &info.path, time)
            .ok()
    }

    /// The region of interest being drawn. Returns true while it takes the
    /// pointer.
    fn roi_frame(
        &mut self,
        ui: &Ui,
        resp: &egui::Response,
        view: &View,
        painter: &egui::Painter,
        info: &CompInfo,
    ) -> bool {
        let waiting = self.display.roi_on && self.display.roi.is_none();
        if !waiting && self.roi_draw.is_none() {
            return false;
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = ui.input(|i| i.pointer.press_origin())
        {
            self.roi_draw = Some(view.to_comp(p));
        }
        let Some(a) = self.roi_draw else {
            return true;
        };
        let b = resp
            .interact_pointer_pos()
            .or(resp.hover_pos())
            .map_or(a, |p| view.to_comp(p));
        let (w, h) = (f64::from(info.width), f64::from(info.height));
        let clamp = |p: DVec2| DVec2::new(p.x.clamp(0.0, w), p.y.clamp(0.0, h));
        let (a, b) = (clamp(a), clamp(b));
        let (lo, hi) = (a.min(b), a.max(b));
        let screen = Rect::from_min_max(view.to_screen(lo), view.to_screen(hi));
        painter.rect_stroke(
            screen,
            0.0,
            Stroke::new(1.0, Color32::WHITE),
            egui::StrokeKind::Middle,
        );
        if !ui.input(|i| i.pointer.primary_down()) {
            self.roi_draw = None;
            let r = bae_render::IRect::new(
                lo.x.floor() as i32,
                lo.y.floor() as i32,
                hi.x.ceil() as i32,
                hi.y.ceil() as i32,
            );
            if r.width() > 0 && r.height() > 0 {
                self.display.roi = Some(r);
            }
        }
        true
    }

    /// Zoom tool click: the next magnification step, keeping `at` in place.
    fn step_zoom_at(&mut self, up: bool, at: Pos2, area: Rect, info: &CompInfo, ppp: f32) {
        let (_, mag) = self.view(area, info, ppp);
        if let Some(m) = next_step(mag, up) {
            self.zoom_by(m / mag, Some(at), area, info, ppp);
        }
    }
}

/// The panel's evaluator showing the value drag in progress (timeline, Effect
/// Controls, Properties …): its targets evaluated as the commit will leave
/// them, so children, expressions, masks and effects follow live (#593).
pub(crate) fn live_evaluator<'a>(
    eval: &'a mut crate::common::EvalCache,
    cx: &PanelContext<'_>,
    session: &SharedSession,
) -> &'a mut bae_anim::Evaluator {
    let overrides = session
        .lock()
        .scrub
        .as_ref()
        .map(|s| s.overrides(cx.stage))
        .unwrap_or_default();
    let eval = eval.get(cx.resources);
    eval.set_overrides(overrides);
    eval
}

/// Apply the viewer's own transform drag to an evaluated comp and recompute
/// world matrices (value drags in panels are evaluated as overrides instead:
/// [`live_evaluator`]).
fn apply_overrides(comp: &mut EvaluatedComp, drag: Option<&transform::TransformDrag>) {
    let mut changed = false;
    for layer in &mut comp.layers {
        let three_d = layer.switches.three_d;
        let mut t: Transform = layer.transform;
        let mut touched = false;
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
        let (safe, grid) = self.restore.unwrap_or((self.saved.safe, self.saved.grid));
        Some(json!({"safe": safe, "grid": grid, "checker": self.checker}))
    }

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        if action.id == view3d::ids::LAST && self.esc_taken {
            self.escape = true;
            return true;
        }
        if view3d::handle(action, cx) {
            return true;
        }
        // With a camera tool, F is Look at Selected Layers (AE's camera tool
        // shortcut; elsewhere F reveals Mask Feather).
        if action.id == "motion.reveal"
            && action.params.get("what").and_then(|v| v.as_str()) == Some("mask_feather")
            && matches!(
                crate::tools::shared(cx.resources).lock().current().group(),
                crate::tools::Group::Orbit | crate::tools::Group::Pan | crate::tools::Group::Dolly
            )
        {
            let look = Invocation::new(look_at::ids::SELECTED);
            return look_at::handle(&look, cx);
        }
        // While a shape is being drawn the keys belong to the drag: Space moves
        // the shape (not playback), arrows and Page Up / Down change it.
        if self.shape.is_some() || self.pen.busy() || self.select.busy() {
            return true;
        }
        let session = common::session(cx.resources);
        match action.id.as_str() {
            display::SHOW_CHANNEL => {
                self.display.shortcut(&action.params);
                true
            }
            display::FAST_PREVIEWS => {
                // Draft 3D is the 3D renderer's switch (not here yet).
                if let Some(f) = action
                    .params
                    .get("mode")
                    .and_then(|m| m.as_str())
                    .and_then(display::Fast::parse)
                {
                    self.display.fast = f;
                }
                true
            }
            display::TAKE_SNAPSHOT | display::SHOW_SNAPSHOT | display::PURGE_SNAPSHOT => {
                let slot = action
                    .params
                    .get("slot")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1);
                let slot = (slot.clamp(1, 4) - 1) as usize;
                if let Some(g) = self.gpu.as_mut() {
                    match action.id.as_str() {
                        display::TAKE_SNAPSHOT => {
                            g.take_snapshot(slot);
                        }
                        display::PURGE_SNAPSHOT => g.purge_snapshot(slot),
                        _ => self.snapshot_shown = Some(slot),
                    }
                }
                true
            }
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
                self.fit_all = true;
                self.pan = Vec2::ZERO;
                true
            }
            ZOOM_100 => {
                self.pending_100 = true;
                true
            }
            FIT_UP_TO_100 => {
                self.zoom = None;
                self.fit_all = false;
                self.pan = Vec2::ZERO;
                true
            }
            _ => crate::actions::handle(action, cx, &session),
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        session.lock().sync(cx.stage);
        if let Some((safe, grid)) = self.restore.take() {
            let mut s = session.lock();
            s.view_options.safe = safe;
            s.view_options.grid = grid;
        }
        let opts = session.lock().view_options;
        self.saved = opts;
        self.prefs = prefs::ViewerPrefs::current(cx.resources);
        crate::import::drop_on_panel(ui, cx);
        let Some(info) = common::active_comp(cx.stage, &session) else {
            empty_viewer(ui, cx, &session);
            return;
        };
        // The shell ticks the clock; this keeps the audio mix current.
        session.lock().update_preview(cx, &info);
        let time = session.lock().time(&info.path);
        let ppp = ui.ctx().pixels_per_point();

        let full = ui.available_rect_before_wrap();
        let bar_h = 28.0;
        // The Composition Navigator along the top.
        let nav = Rect::from_min_max(full.min, pos2(full.right(), full.top() + navigator::HEIGHT));
        let area = Rect::from_min_max(
            pos2(full.left(), nav.bottom()),
            pos2(full.right(), full.bottom() - bar_h),
        );
        let bar = Rect::from_min_max(pos2(full.left(), area.bottom()), full.max);
        // View layout: the active view is the viewer, the others are drawn below.
        let (panes, active) = {
            let s = session.lock();
            s.views3d
                .get(&info.path)
                .map_or((vec![view3d::View3d::Active], 0), |v| {
                    let panes = v.panes();
                    let active = layout::preview_pane(
                        &panes,
                        v.active(),
                        s.is_playing() && s.preview.favor_active_camera,
                        || {
                            let revision = cx.stage.revision();
                            if let Some((comp, rev, result)) = &self.preview_3d
                                && comp == &info.path
                                && *rev == revision
                            {
                                return *result;
                            }
                            let result =
                                query::comp_layers(cx.stage, &info.path)
                                    .iter()
                                    .any(|layer| {
                                        cx.stage
                                            .prim(layer)
                                            .and_then(|p| query::static_value(&p, "three_d"))
                                            .and_then(|v| v.as_bool())
                                            .unwrap_or(false)
                                    });
                            self.preview_3d = Some((info.path.clone(), revision, result));
                            result
                        },
                    );
                    (panes, active)
                })
        };
        let pane_rects = layout::rects(area, panes.len());
        let area = pane_rects[active.min(pane_rects.len() - 1)];
        // View ▸ Show Rulers: along the top and left of the viewer.
        let (rulers, area) = guides::layout(area, opts.rulers);
        let resp = ui
            .allocate_rect(area, Sense::click_and_drag())
            .labeled(egui::WidgetType::Image, tr("Composition"));
        navigator::bar(ui, cx, &session, nav);
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

        // Zoom to 100 % around the middle of the view (#569).
        if std::mem::take(&mut self.pending_100) {
            let (_, mag) = self.view(area, &info, ppp);
            self.zoom_by(1.0 / mag, None, area, &info, ppp);
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
            Some(ViewRequest::FitUpTo100) => {
                self.zoom = None;
                self.fit_all = false;
                self.pan = Vec2::ZERO;
            }
            None => {}
        }
        // Wheel (AE-UI-011): a mouse wheel zooms around the pointer (Alt/Option:
        // around the view's center; Shift: scrolls sideways); trackpad scrolling
        // pans (AE 25.3+ on macOS); pinch and Cmd/Ctrl+scroll zoom.
        if resp.hovered() && self.shape.is_none() {
            let m = Magnification::of(cx.resources);
            let w = wheel(ui, m);
            if smart_zoom(ui.ctx()) {
                if self.zoom.is_some() {
                    self.zoom = None;
                    self.pan = Vec2::ZERO;
                } else {
                    let (_, mag) = self.view(area, &info, ppp);
                    self.zoom_by((1.0 / mag).max(2.0), resp.hover_pos(), area, &info, ppp);
                }
            } else if w.pinch != 1.0 {
                self.zoom_by(w.pinch, resp.hover_pos(), area, &info, ppp);
            } else {
                if w.notches != 0.0 {
                    let around = if m.follows_pointer(w.alt) {
                        resp.hover_pos()
                    } else {
                        None
                    };
                    let (_, mag) = self.view(area, &info, ppp);
                    let to = m.wheel_zoom(mag, w.notches, &mut self.wheel_rest);
                    if to != mag {
                        self.zoom_by(to / mag, around, area, &info, ppp);
                    }
                }
                self.pan += w.pan;
            }
        }
        // With a camera tool these drags pan and dolly the camera.
        let camera_tool = crate::tools::shared(cx.resources).lock().current();
        if (resp.dragged_by(egui::PointerButton::Middle)
            || resp.dragged_by(egui::PointerButton::Secondary))
            && !camera_tool.is_camera()
        {
            self.pan += resp.drag_delta();
        }
        let (view, mag) = self.view(area, &info, ppp);
        self.last_view = Some(view);
        if self.mag != mag {
            self.mag = mag;
            crate::tools::shared(cx.resources).lock().magnification = mag;
        }

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
        let mut resolution = {
            let s = session.lock();
            // Preview ▸ Resolution overrides the viewer's while playing.
            match s.preview.resolution.filter(|_| s.is_playing()) {
                Some(r) => r,
                None => render_factor(cx.stage, &info.path, s.auto_resolution, mag),
            }
        };
        // Fast Previews while something is dragged.
        let interacting = resp.dragged() || self.select.busy() || session.lock().scrub.is_some();
        if interacting && self.display.fast == display::Fast::Adaptive {
            resolution = resolution.max(4);
        }
        let wireframe = interacting && self.display.fast == display::Fast::Wireframe;
        self.render_factor = resolution;
        // Paint.
        let painter = ui.painter_at(area);
        let controls = session.lock().layer_controls;
        let pasteboard = session.lock().panel_background.color();
        painter.rect_filled(area, 0.0, pasteboard);
        let comp_rect = render::comp_rect(&view, info.width, info.height);
        // Region of interest: only it shows the comp.
        let comp_rect = match self.display.roi() {
            Some(r) => Rect::from_min_max(
                view.to_screen(DVec2::new(f64::from(r.x0), f64::from(r.y0))),
                view.to_screen(DVec2::new(f64::from(r.x1), f64::from(r.y1))),
            )
            .intersect(comp_rect),
            None => comp_rect,
        };
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
        if resp.dragged() || resp.clicked() {
            session.lock().stop();
        }
        // A value drag that ended without a commit (e.g. cancelled) must not leave
        // its preview behind (a committed one stays until its command has run).
        if !ui.input(|i| i.pointer.any_down()) {
            let mut s = session.lock();
            if s.scrub.as_ref().is_some_and(|s| s.committed.is_none()) {
                s.scrub = None;
            }
        }
        let scrub = session.lock().scrub.clone();
        let editing = scrub.is_some()
            || self.select.drag.is_some()
            || self.type_tool.edited_layer().is_some();
        let playback = session
            .lock()
            .playback
            .clone()
            .filter(|pb| pb.comp == info.path);
        let mut completed = None;
        if let Some(gpu) = self.gpu.as_mut() {
            if editing {
                gpu.playback.stop();
            } else {
                let range = playback.as_ref().map_or(
                    bae_model::TimeRange::new(
                        time,
                        info.frame_rate
                            .frame_to_time(info.frame_rate.time_to_frame(time) + 1),
                    ),
                    |pb| pb.range,
                );
                let looping = playback.is_some() && session.lock().loop_playback;
                if let Some((image, actual, errors)) = gpu.playback.poll(
                    cx.stage,
                    cx.resources,
                    &info.path,
                    time,
                    info.frame_rate,
                    range,
                    looping,
                    resolution,
                    ui.ctx(),
                ) {
                    completed = Some(image);
                    if actual == time || playback.is_some() {
                        let mut s = session.lock();
                        if playback.is_some() {
                            s.drew_preview(actual);
                        }
                        if s.expr_errors != errors {
                            s.expr_errors = errors.to_vec();
                        }
                    }
                }
                // Idle (the current time still for a moment): prepare the first
                // frames playback would show, so pressing play shows them at
                // once while the pipeline fills.
                if playback.is_none() {
                    let now = std::time::Instant::now();
                    match self.idle {
                        Some((at, since)) if at == time => {
                            if now.duration_since(since) >= IDLE_PREFETCH {
                                let (from, range) = session.lock().play_start(&info);
                                let rate = info.frame_rate;
                                let first = rate.time_to_frame(from);
                                let times = (0..PREFETCH_FRAMES)
                                    .map(|k| rate.frame_to_time(first + k))
                                    .filter(|t| *t != time && range.contains(*t))
                                    .collect();
                                gpu.playback.prefetch(times);
                            } else {
                                ui.ctx().request_repaint_after(IDLE_PREFETCH);
                            }
                        }
                        _ => {
                            self.idle = Some((time, now));
                            ui.ctx().request_repaint_after(IDLE_PREFETCH);
                        }
                    }
                }
                gpu.playback.publish_ranges(&session, info.frame_rate);
            }
        }
        if playback.is_some() && self.gpu.is_some() && !editing {
            self.on_gpu = completed.is_some();
            if let Some(image) = completed {
                paint_image(&clip, &view, image);
            }
            Self::paint_guides(cx, &painter, &clip, comp_rect, &view, opts);
            let (fps, dropped) = {
                let s = session.lock();
                (s.playback_stats.fps, s.playback_stats.dropped)
            };
            let label = format!("{fps:.1} fps   {} {dropped}", tr("Dropped frames"));
            painter.text(
                area.right_top() + vec2(-8., 8.),
                Align2::RIGHT_TOP,
                label,
                theme::body(),
                color::TEXT_STRONG,
            );
            self.banner.ui(ui, cx, &session, area);
            bar::bottom_bar(ui, cx, bar, self, &info, time, mag, &session, None);
            return;
        }

        // Evaluate, with the value a panel drag has reached (live, uncommitted);
        // sampleImage reads the pixels the GPU preview reads back.
        let eval = live_evaluator(&mut self.eval, cx, &session);
        if let Some(g) = &self.gpu
            && !eval
                .image_samples()
                .is_some_and(|s| std::sync::Arc::ptr_eq(s, g.samples()))
        {
            eval.set_image_samples(Some(g.samples().clone()));
        }
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
        apply_overrides(&mut comp, self.select.drag.as_ref());
        // A mask or shape path being edited shows in the picture, not only as
        // its outline (#593).
        if let Some((prim, path)) = self
            .select
            .path_preview()
            .or(self.pen.path_preview())
            .or(self.feather.path_preview())
        {
            overlay::apply_path_preview(&mut comp, &prim, &path);
        }
        // The 3D view: 3D layers are seen through its camera.
        let size = DVec2::new(f64::from(info.width), f64::from(info.height));
        if camera_tool.is_camera() || self.camera_drag.is_some() {
            camera_tool::frame(
                &mut self.camera_drag,
                &resp,
                ui,
                cx,
                &session,
                &info.path,
                &comp,
                size,
                &view,
                camera_tool,
                time,
            );
        }
        let layer_camera = self.camera_drag.as_ref().and_then(|d| d.layer_camera());
        let view_camera = layer_camera.or_else(|| {
            session
                .lock()
                .views3d
                .get(&info.path)
                .and_then(|v| v.camera_of(panes[active], size))
        });
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

        if let Some(g) = self.gpu.as_mut() {
            g.display = self.display.render();
            g.roi = self.display.roi();
        }
        let frame_nr = ui.ctx().cumulative_frame_nr();
        let measure = session
            .lock()
            .timing_column_frame
            .is_some_and(|shown| frame_nr.saturating_sub(shown) <= 1);
        // Live edits, other 3D views than the active camera, the Render Time
        // column (it times this render), Show Channel / Exposure and a region
        // of interest render here; otherwise the frame comes from the
        // background renderer and its cache. Wireframe draws no picture.
        let direct = editing
            || view_camera.is_some()
            || measure
            || self.display.render() != bae_render::Display::default()
            || self.display.roi().is_some();
        let image = if wireframe {
            None
        } else if direct {
            self.gpu.as_mut().and_then(|g| {
                g.frame(
                    cx.resources,
                    cx.stage,
                    &comp,
                    resolution,
                    view_camera,
                    measure,
                )
            })
        } else {
            if completed.is_some()
                && let Some(g) = self.gpu.as_mut()
                && let Some((texture, rect)) = g.playback.shown_texture()
            {
                g.show_cached(texture, rect);
            }
            completed
        };
        // Show Snapshot while F5–F8 (or the bar button) is held.
        let keys = [egui::Key::F5, egui::Key::F6, egui::Key::F7, egui::Key::F8];
        if let Some(s) = self.snapshot_shown
            && !ui.input(|i| i.key_down(keys[s]))
            && !self.snapshot_button
        {
            self.snapshot_shown = None;
        }
        let image = match (self.snapshot_shown, self.gpu.as_mut()) {
            (Some(s), Some(g)) => g.show_snapshot(s).or(image),
            _ => image,
        };
        self.on_gpu = image.is_some();
        if self
            .gpu
            .as_ref()
            .is_some_and(GpuPreview::waiting_for_samples)
        {
            ui.ctx().request_repaint();
        }
        if let Some(frame) = self.gpu.as_ref().and_then(|g| g.output()) {
            let mut s = session.lock();
            if s.scope_frame.as_ref().map(|f| f.1) != Some(frame.1) {
                s.scope_frame = Some(frame);
            }
        }
        // The Info panel: the pointer in comp pixels and, while the panel shows
        // it, the color under it (one texel read back from the GPU frame).
        if let Some(h) = resp.hover_pos() {
            let p = view.to_comp(h);
            let mut s = session.lock();
            s.info.pos = Some([p.x, p.y]);
            let shown = ui
                .ctx()
                .cumulative_frame_nr()
                .saturating_sub(s.info.shown_at)
                < 3;
            if shown && let Some(g) = self.gpu.as_mut() {
                // Outside the rendered frame the comp is transparent (AE: A 0).
                s.info.color = Some(g.pixel([p.x, p.y]).unwrap_or([0; 4]));
            }
        }
        let mut vector = std::borrow::Cow::Borrowed(&comp);
        if let Some(img) = image {
            paint_image(&clip, &view, img);
            // Until 05's text source is registered, text layers are drawn here.
            let has_text = cx
                .resources
                .get::<crate::PreviewSources>()
                .is_some_and(|sources| {
                    sources
                        .0
                        .snapshot()
                        .iter()
                        .any(|p| p.item().id() == bae_render::anim::TEXT_PROVIDER)
                });
            let mut text_only = comp.clone();
            for l in &mut text_only.layers {
                if has_text || l.kind != LayerKind::Text {
                    l.visible = false;
                }
            }
            vector = std::borrow::Cow::Owned(text_only);
        }
        if wireframe {
            // Wireframe: the layers' outlines only.
            let outer = Affine::new(DMat4::IDENTITY);
            for l in comp.layers.iter().filter(|l| l.visible) {
                if let Some(q) = render::bounds_quad(l, &view, &outer) {
                    clip.add(egui::Shape::closed_line(
                        q.to_vec(),
                        Stroke::new(1.0, Color32::WHITE),
                    ));
                }
            }
            vector = std::borrow::Cow::Owned(EvaluatedComp {
                layers: Vec::new(),
                ..comp.clone()
            });
        }
        // Something is still warming up (an effect pipeline, the text
        // renderer): the last complete frame stays (none yet: nothing, not the
        // simplified drawing) with a small hint, like AE's render progress.
        let preparing = self.gpu.as_ref().is_some_and(|g| g.preparing);
        if !(preparing && !self.on_gpu) {
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
        }
        if preparing {
            painter.text(
                area.right_bottom() + vec2(-8.0, -8.0),
                Align2::RIGHT_BOTTOM,
                tr("Preparing…"),
                theme::small(),
                color::TEXT_DIM,
            );
        }

        Self::paint_guides(cx, &painter, &clip, comp_rect, &view, opts);

        // Selection: bounding boxes, handles, anchor points (AE draws them in the
        // layer's label color).
        let selected = session.lock().selected_layers.clone();
        let outer = Affine::new(DMat4::IDENTITY);
        // View ▸ Show Layer Controls off: the tools still work, unseen.
        let hidden = painter.with_clip_rect(Rect::NOTHING);
        let painter = if opts.layer_controls { painter } else { hidden };
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
                for i in (0..4).filter(|_| controls.handles) {
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

        // View Options' masks and the bar's Toggle Mask and Shape Path Visibility.
        if controls.masks && opts.paths {
            overlay::masks(&painter, cx.stage, &comp, &view, &selected);
        }
        particle_guides::paint(&clip, cx.stage, eval, &comp, &view, &selected, view_camera);
        // Cameras and lights in the 3D views (not the camera looked through).
        let through = match session.lock().views3d.get(&info.path).map(|v| v.current) {
            None | Some(view3d::View3d::Active) => comp.camera.map(|i| comp.layers[i].path.clone()),
            Some(_) => None,
        };
        scene_icons::paint(
            &painter,
            cx.stage,
            &comp,
            &view,
            &selected,
            through.as_ref(),
        );

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
        // Region of interest on without a region: a drag draws it, with any
        // tool (the tools wait).
        let roi_busy = self.roi_frame(ui, &resp, &view, &painter, &info);
        let hand = !roi_busy && (tool == Tool::Hand || self.space_hold.is_some() || self.panning);
        // Cmd/Ctrl makes the pen tools the Selection tool while held (AE-SHP-044).
        let command = ui.input(|i| i.modifiers.command);
        let effective = if roi_busy || hand {
            Tool::Hand
        } else if tool.is_pen() && command && !self.pen.busy() {
            Tool::Selection
        } else {
            tool
        };
        if hand && resp.drag_started_by(egui::PointerButton::Primary) {
            self.panning = true;
            // Space started playback when pressed; a Space-drag only pans:
            // back to the time before Space (the drag may already have
            // stopped playback above, after it advanced).
            if let Some((false, t0)) = self.space_hold {
                let mut s = session.lock();
                s.stop();
                s.set_time(&info, t0);
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
        feather_tool::frame(
            &mut self.feather,
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
        // Effect point controls: a press on a crosshair is theirs.
        let points = effect_points::frame(
            &mut self.effect_points,
            ui,
            &resp,
            cx,
            &session,
            &comp,
            &view,
            effective,
            &painter,
            &self.prefs,
            time,
        );
        // The paint tools paint in the Layer panel: a double-click on a layer
        // opens it there (AE).
        if matches!(effective, Tool::Brush | Tool::CloneStamp | Tool::Eraser)
            && resp.double_clicked()
            && let Some(pos) = resp.interact_pointer_pos()
            && let Some(layer) = select_tool::hit_layer(cx.stage, &comp, &view, pos)
        {
            session.lock().selected_layers = vec![layer.clone()];
            crate::layer_panel::open(cx, &session, &layer);
        }
        // Motion paths (C): drawn for the selected layers; a press on a keyframe
        // square or handle is theirs, not the Selection tool's.
        // Motion Sketch: an armed capture owns the next drag.
        let sketching = motion_sketch::frame(
            &mut self.sketch,
            ui,
            &resp,
            cx,
            &session,
            &info,
            &view,
            &painter,
        );
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
        let guide_busy = guides::frame(
            &mut self.guide_drag,
            ui,
            cx,
            &info,
            &view,
            area,
            rulers,
            &opts,
            effective == Tool::Selection && !path.owns && !points.owns,
        );
        let select_cursor = if points.owns {
            points.cursor
        } else if path.owns || sketching || guide_busy {
            path.cursor
        } else {
            points.cursor.or(path.cursor).or(select_tool::frame(
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
            item.add_as_layer(
                cx,
                &session,
                DropPlacement::Viewer(pointer.map(|p| view.to_comp(p))),
            );
        }
        // An effect dropped on a layer (Effects & Presets) is applied to it.
        if let Some(fx) = resp.dnd_release_payload::<crate::effects::EffectDrag>()
            && let Some(p) = pointer
            && let Some(layer) = select_tool::hit_layer(cx.stage, &comp, &view, p)
        {
            crate::effects::apply(cx, &[layer], &fx.0);
        }

        // 3D Reference Axes: the world axes as the view's camera sees them.
        let opts = session.lock().view_options;
        if opts.axes
            && opts.layer_controls
            && comp.layers.iter().any(render::is_3d)
            && let Some((camera, _)) = view.camera.as_ref()
        {
            reference_axes(&ui.painter_at(area), area, &camera.world);
        }
        self.banner.ui(ui, cx, &session, area);
        self.other_views(ui, cx, &session, &info, &comp, &pane_rects, &panes, active);
        let camera = comp.camera.and_then(|i| comp.layers.get(i));
        let camera = camera.map(|l| l.name.clone());
        bar::bottom_bar(
            ui,
            cx,
            bar,
            self,
            &info,
            time,
            mag,
            &session,
            camera.as_deref(),
        );
        self.esc_taken = self.shape.is_some()
            || self.pen.busy()
            || self.select.busy()
            || self.select.free.is_some()
            || ui.ctx().text_edit_focused()
            || egui::Popup::is_any_open(ui.ctx());
    }
}

/// The 3D Reference Axes in `area`'s lower-left corner: world X (red), Y
/// (green) and Z (blue) as seen by a camera with `world` (camera → comp).
fn reference_axes(painter: &egui::Painter, area: Rect, world: &DMat4) {
    let origin = pos2(area.left() + 28.0, area.bottom() - 28.0);
    let to_camera = world.inverse();
    let axes = [
        (
            bae_anim::glam::DVec3::X,
            "X",
            Color32::from_rgb(230, 70, 70),
        ),
        (
            bae_anim::glam::DVec3::Y,
            "Y",
            Color32::from_rgb(90, 200, 90),
        ),
        (
            bae_anim::glam::DVec3::Z,
            "Z",
            Color32::from_rgb(80, 140, 240),
        ),
    ];
    for (axis, name, color) in axes {
        // Camera space: +x right, +y down (screen), +z away.
        let d = to_camera.transform_vector3(axis).normalize_or_zero();
        let tip = origin + vec2(d.x as f32, d.y as f32) * 18.0;
        painter.line_segment([origin, tip], Stroke::new(1.5, color));
        let label = origin + vec2(d.x as f32, d.y as f32) * 25.0;
        painter.text(label, Align2::CENTER_CENTER, name, theme::small(), color);
    }
}

/// View ▸ Look at Selected / All Layers (see [`look_at`]).
pub(crate) fn look_at(inv: &bae_ui::Invocation, cx: &mut PanelContext<'_>) -> bool {
    look_at::handle(inv, cx)
}

pub(crate) fn look_at_actions() -> impl Iterator<Item = (&'static str, &'static str)> {
    look_at::ACTIONS.iter().copied()
}

pub(crate) fn look_at_enabled(resources: &bae_app::Resources, all: bool) -> bool {
    look_at::enabled(resources, all)
}

fn paint_image(clip: &egui::Painter, view: &View, img: crate::gpu_preview::PreviewImage) {
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

/// The comp's resolution as pixel steps (horizontal, vertical).
pub(crate) fn comp_steps(stage: &bae_model::Stage, comp: &PrimPath) -> [u32; 2] {
    match stage
        .prim(comp)
        .and_then(|v| query::static_value(&v, "resolution"))
    {
        Some(Value::Vec2([h, v])) => [(h as u32).max(1), (v as u32).max(1)],
        _ => [1, 1],
    }
}

/// Set the active comp's resolution as pixel steps (View > Resolution, the
/// viewer's menu, Custom Resolution) and,
/// with `nested`, of the comps nested in it (AE's "… Recurse" shortcuts); turns
/// Auto off. One `comp.set_settings` per comp, like Composition Settings.
pub(crate) fn set_resolution(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    steps: [u32; 2],
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
        if comp_steps(cx.stage, &c) != steps {
            cx.invoke_with("comp.set_settings", json!({"comp": c, "resolution": steps}));
        }
    }
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

mod zh;
pub(crate) use zh::ZH;

/// Registers this module's Chinese strings and panel (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    bae_ui::i18n::register(view_options::ZH);
    bae_ui::i18n::register(display::ZH);
    bae_ui::i18n::register(navigator::ZH);
    crate::register_panel(
        app,
        "composition",
        bae_ui::PanelKind::new("Composition", |spawn| {
            Box::new(CompositionViewer::new(spawn.state))
        }),
    );
}
