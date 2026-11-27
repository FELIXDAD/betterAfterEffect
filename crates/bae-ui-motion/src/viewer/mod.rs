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
mod group_edit;
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
pub(crate) mod puppet_tool;
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
pub(crate) use puppet_tool::record_options_ui as puppet_record_options_ui;
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
    /// CC Particle World producer paths (expressions evaluated once per time).
    particle_paths: particle_guides::PathCache,
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
    /// The comp a locked viewer keeps showing (AE's viewer lock).
    pinned: Option<PrimPath>,
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
    /// Arrow-key nudges while a key is held (one undo step on release).
    nudge: Option<Nudge>,
    /// The motion path keyframe the context menu opened on (AE: the keyframe
    /// menu instead of the layer menu).
    menu_key: Option<bae_motion::KeyRef>,
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
    /// Puppet tools: the pin being dragged.
    puppet: puppet_tool::PuppetState,
    /// Something in the viewer uses Esc (a drag, text, Free Transform, a
    /// menu): Esc goes to it instead of switching to the last 3D view.
    esc_taken: bool,
    /// Esc to give back to the viewer's widgets this frame.
    escape: bool,
    /// The current time and since when it has not changed (idle prefetch).
    idle: Option<(PrimPath, Time, u64, std::time::Instant)>,
    /// Pixel at primary press, before the pointer crosses the drag threshold.
    mask_start_color: Option<[u8; 4]>,
    /// A camera tool drag.
    camera_drag: Option<camera_tool::CameraDrag>,
}

impl CompositionViewer {
    fn has_3d_layers(&mut self, stage: &bae_model::Stage, path: &bae_model::PrimPath) -> bool {
        let revision = stage.revision();
        if let Some((comp, rev, result)) = &self.preview_3d
            && comp == path
            && *rev == revision
        {
            return *result;
        }
        let result = query::comp_layers(stage, path)
            .iter()
            .any(|layer| common::is_3d(stage, layer));
        self.preview_3d = Some((path.clone(), revision, result));
        result
    }

    pub fn new(state: Option<&serde_json::Value>) -> Self {
        let mut v = CompositionViewer::default();
        if let Some(s) = state {
            let b = |k: &str| s.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
            v.restore = Some((b("safe"), b("grid")));
            v.checker = s.get("checker").and_then(|v| v.as_bool()).unwrap_or(false);
            v.pinned = s
                .get("pinned")
                .and_then(|p| p.as_str())
                .and_then(|p| PrimPath::new(p).ok());
            v.locked = v.pinned.is_some();
        }
        v
    }

    /// The comp this viewer shows: the one it is locked to, else the active
    /// comp.
    fn shown_comp(&self, stage: &bae_model::Stage, session: &SharedSession) -> Option<CompInfo> {
        self.pinned
            .as_ref()
            .filter(|_| self.locked)
            .and_then(|p| query::comp_info(stage, p))
            .or_else(|| common::active_comp(stage, session))
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
                aspect: 1.0,
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

    /// An arrow-key nudge by `d` comp pixels: the selected vertices, the
    /// anchor point with the Pan Behind tool, or the selected layers. While an
    /// arrow key stays down the nudges add up, previewed live, and become one
    /// undo step when the keys come up ([`Self::finish_nudge`]).
    fn nudge_by(
        &mut self,
        cx: &PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        d: DVec2,
    ) -> bool {
        match &mut self.nudge {
            Some(Nudge::Vertices(drag, total)) => {
                *total += d;
                drag.update(*total, false);
                return true;
            }
            Some(Nudge::Layers(drag, total)) => {
                *total += d;
                drag.update(*total, false, None);
                return true;
            }
            None => {}
        }
        let Some(comp) = self.evaluate(cx, session, info) else {
            return false;
        };
        let selected = session.lock().selected_layers.clone();
        if !self.select.vertices.is_empty() {
            let paths = overlay::editable_paths(cx.stage, &comp, &selected, None);
            let Some(p) = paths
                .iter()
                .find(|p| self.select.vertices.prim.as_ref() == Some(&p.prim))
            else {
                return false;
            };
            let mut drag = path_edit::PathDrag::new(
                p.clone(),
                path_edit::Hit::Vertex { path: 0, index: 0 },
                &self.select.vertices,
                DVec2::ZERO,
            );
            drag.update(d, false);
            self.nudge = Some(Nudge::Vertices(drag, d));
            return true;
        }
        if selected.is_empty() {
            return false;
        }
        // Pan Behind: the anchor point moves, the layer stays (AE).
        let kind = if crate::tools::shared(cx.resources).lock().current() == Tool::PanBehind {
            transform::Kind::Anchor {
                start: DVec2::ZERO,
                compensate: true,
            }
        } else {
            transform::Kind::Move { start: DVec2::ZERO }
        };
        let mut drag = transform::TransformDrag::new(cx.stage, &comp, &selected, kind);
        drag.update(d, false, None);
        self.nudge = Some(Nudge::Layers(drag, d));
        true
    }

    /// The arrow keys are up: the nudges so far become one undo step.
    fn finish_nudge(
        &mut self,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
    ) {
        match self.nudge.take() {
            Some(Nudge::Vertices(drag, _)) if drag.changed() => drag.commit(cx, session, info),
            Some(Nudge::Layers(drag, _)) => {
                let time = session.lock().time(&info.path);
                let edits = drag.commit(cx.stage, time);
                if !edits.is_empty() {
                    cx.invoke_with("attr.set_many", json!({ "edits": edits }));
                }
            }
            _ => {}
        }
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
        if path_edit::delete(cx, session, info, &paths, &self.select.vertices) {
            self.select.vertices.clear();
        }
    }

    fn evaluate(
        &mut self,
        cx: &PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
    ) -> Option<EvaluatedComp> {
        let time = {
            let s = session.lock();
            s.held_viewer_time
                .as_ref()
                .filter(|(p, _)| p == &info.path)
                .map_or_else(|| s.time(&info.path), |(_, t)| *t)
        };
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
    let overrides = session.lock().live_overrides(cx.stage);
    let eval = eval.get(cx.stage);
    eval.set_overrides(overrides);
    eval
}

/// Arrow-key nudges held down: the drag they add up to and its total offset
/// (comp pixels).
enum Nudge {
    Layers(transform::TransformDrag, DVec2),
    Vertices(path_edit::PathDrag, DVec2),
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

/// The transparency grid: one rectangle tiled with a cached 2×2 texture
/// (a rectangle per cell was tens of thousands of shapes at the small size).
pub(crate) fn checkerboard(p: &egui::Painter, r: Rect, resources: &bae_app::Resources) {
    let prefs = prefs::ViewerPrefs::current(resources);
    let s = prefs.checker_size.max(1.0);
    let colors = prefs.checker_colors;
    let ctx = p.ctx();
    let id = egui::Id::new("bae.checkerboard");
    let cached = ctx
        .data(|d| d.get_temp::<([egui::Color32; 2], egui::TextureHandle)>(id))
        .filter(|(c, _)| *c == colors)
        .map(|(_, t)| t);
    let texture = cached.unwrap_or_else(|| {
        let image = egui::ColorImage::new([2, 2], vec![colors[0], colors[1], colors[1], colors[0]]);
        let texture = ctx.load_texture(
            "bae.checkerboard",
            image,
            egui::TextureOptions {
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Nearest,
                wrap_mode: egui::TextureWrapMode::Repeat,
                mipmap_mode: None,
            },
        );
        ctx.data_mut(|d| d.insert_temp(id, (colors, texture.clone())));
        texture
    });
    // Each texel is one cell: 2 cells per texture repeat.
    let uv = Rect::from_min_max(
        pos2(0.0, 0.0),
        pos2(r.width() / (2.0 * s), r.height() / (2.0 * s)),
    );
    p.image(texture.id(), r, uv, egui::Color32::WHITE);
}

impl Panel for CompositionViewer {
    fn tab(&self, cx: &PanelContext<'_>) -> TabInfo {
        // Read from the session every frame, so New / Open / Close Project
        // update the tab (and its accessible name) at once (issue #215).
        match self.shown_comp(cx.stage, &common::session(cx.resources)) {
            Some(info) => common::related_tab(cx, "Composition", &info.path, &info.name),
            None => TabInfo::new(tr("Composition")),
        }
        .viewer()
        .locked(self.locked)
    }

    fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        let (safe, grid) = self.restore.unwrap_or((self.saved.safe, self.saved.grid));
        let pinned = self.pinned.as_ref().filter(|_| self.locked);
        Some(json!({"safe": safe, "grid": grid, "checker": self.checker, "pinned": pinned}))
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
                let d = DVec2::new(p("dx") / mag, p("dy") / mag);
                match common::active_comp(cx.stage, &session) {
                    Some(info) => self.nudge_by(cx, &session, &info, d),
                    None => false,
                }
            }
            crate::actions::ids::DELETE if self.puppet.has_selection() => {
                self.puppet.delete_selected(cx);
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
        // A viewer opened locked takes its comp; locking keeps the comp shown.
        if let Some(p) = session.lock().viewer_pins.remove(cx.panel.as_str()) {
            self.pinned = Some(p);
            self.locked = true;
        }
        if !self.locked {
            self.pinned = None;
        } else if self.pinned.is_none() {
            self.pinned = session.lock().active_comp.clone();
        }
        // Working in a locked viewer makes its comp the active one (the
        // Timeline and the other panels follow, AE).
        if let Some(p) = self.pinned.clone().filter(|_| self.locked)
            && query::comp_info(cx.stage, &p).is_some()
            && session.lock().active_comp.as_ref() != Some(&p)
            && (cx.focused || ui.ui_contains_pointer() && ui.input(|i| i.pointer.any_pressed()))
        {
            session.lock().open_comp(&p);
        }
        let Some(info) = self.shown_comp(cx.stage, &session) else {
            empty_viewer(ui, cx, &session);
            return;
        };
        // The shell ticks the clock; this keeps the audio mix current.
        session.lock().update_preview(cx, &info);
        let time = {
            let mut s = session.lock();
            if !ui.input(|i| i.pointer.primary_down()) {
                s.held_viewer_time = None;
            }
            s.held_viewer_time
                .as_ref()
                .filter(|(p, _)| p == &info.path)
                .map_or_else(|| s.time(&info.path), |(_, t)| *t)
        };
        let ppp = ui.ctx().pixels_per_point();

        let full = ui.available_rect_before_wrap();
        let fullscreen = session.lock().fullscreen_preview();
        let bar_h = if fullscreen { 0.0 } else { 28.0 };
        // The Composition Navigator along the top.
        let nav = Rect::from_min_max(
            full.min,
            pos2(
                full.right(),
                full.top() + if fullscreen { 0.0 } else { navigator::HEIGHT },
            ),
        );
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
                        || self.has_3d_layers(cx.stage, &info.path),
                    );
                    (panes, active)
                })
        };
        let pane_rects = layout::rects(area, panes.len());
        let area = pane_rects[active.min(pane_rects.len() - 1)];
        // View ▸ Show Rulers: along the top and left of the viewer.
        let (rulers, area) = guides::layout(area, opts.rulers && !fullscreen);
        let resp = ui
            .allocate_rect(area, Sense::click_and_drag())
            .labeled(egui::WidgetType::Image, tr("Composition"));
        if !fullscreen {
            navigator::bar(ui, cx, &session, nav);
        }
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
        let mut dolly_notches = 0.0;
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
                if w.notches != 0.0
                    && self.prefs.dolly_wheel != 0.0
                    && self.has_3d_layers(cx.stage, &info.path)
                {
                    dolly_notches = w.notches * self.prefs.dolly_wheel;
                } else if w.notches != 0.0 {
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
        let selected_tool = crate::tools::shared(cx.resources).lock().current();
        let alt_drag = self.prefs.alt_camera_navigation
            && resp.dragged()
            && ui.input(|i| i.modifiers.alt && !i.key_down(egui::Key::Space))
            && self.has_3d_layers(cx.stage, &info.path);
        let navigating = alt_drag || self.camera_drag.is_some();
        let camera_tool = if alt_drag || (navigating && !selected_tool.is_camera()) {
            Tool::OrbitCursor
        } else {
            selected_tool
        };
        if (resp.dragged_by(egui::PointerButton::Middle)
            || resp.dragged_by(egui::PointerButton::Secondary))
            && !camera_tool.is_camera()
            && !navigating
        {
            self.pan += resp.drag_delta();
        }
        let (mut view, mut mag) = self.view(area, &info, ppp);
        if fullscreen {
            let size = vec2(info.width as f32, info.height as f32);
            view.scale = (area.width() / size.x).min(area.height() / size.y);
            view.origin = area.center() - size * view.scale * 0.5;
            mag = ppp * view.scale;
        }
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
            self.gpu = GpuPreview::new(cx.resources, ui.ctx());
        }
        let mut resolution = {
            let s = session.lock();
            // Preview ▸ Resolution overrides the viewer's while playing.
            match s.preview_options().resolution.filter(|_| s.is_playing()) {
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
        let audio_only = {
            let s = session.lock();
            s.is_playing() && !s.preview_options().video
        };
        if audio_only {
            if let Some(gpu) = self.gpu.as_mut() {
                gpu.playback.stop();
            }
            ui.painter().rect_filled(area, 0.0, Color32::BLACK);
            bar::bottom_bar(ui, cx, bar, self, &info, time, mag, &session, None);
            return;
        }
        // Paint.
        let painter = ui.painter_at(area);
        let controls = view_options::shown_controls(&session.lock());
        // Region of interest for Crop Comp to Region of Interest (AE turns it
        // off once the comp is cropped to it).
        {
            let mut s = session.lock();
            if std::mem::take(&mut s.roi_cropped) {
                self.display.roi_on = false;
                self.display.roi = None;
            }
            match self.display.roi() {
                Some(r) => {
                    let rect = [r.x0, r.y0, r.x1, r.y1].map(f64::from);
                    s.roi = Some((info.path.clone(), rect));
                }
                None if s.roi.as_ref().is_some_and(|(c, _)| *c == info.path) => s.roi = None,
                None => {}
            }
        }
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
            checkerboard(&painter, comp_rect.intersect(area), cx.resources);
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
        // its preview behind (a committed one stays until its command has run;
        // text being typed stays while its field has focus).
        if !ui.input(|i| i.pointer.any_down()) && ui.memory(|m| m.focused().is_none()) {
            let mut s = session.lock();
            if s.scrub.as_ref().is_some_and(|s| s.committed.is_none()) {
                s.scrub = None;
            }
        }
        // Nudges end when no arrow key is down (one undo step).
        let arrows = [
            egui::Key::ArrowLeft,
            egui::Key::ArrowRight,
            egui::Key::ArrowUp,
            egui::Key::ArrowDown,
        ];
        if self.nudge.is_some() && !ui.input(|i| arrows.iter().any(|k| i.key_down(*k))) {
            self.finish_nudge(cx, &session, &info);
        }
        // Ctrl+T and Layer ▸ Mask and Shape Path know which vertices are
        // selected.
        {
            let mut s = session.lock();
            if let Some(indices) = s.vertex_request.take()
                && self.select.vertices.prim.is_some()
            {
                self.select.vertices.indices = indices;
            }
            s.vertex_path = self
                .select
                .vertices
                .prim
                .clone()
                .filter(|_| !self.select.vertices.is_empty());
            s.vertex_indices = if s.vertex_path.is_some() {
                self.select.vertices.indices.clone()
            } else {
                Vec::new()
            };
        }
        let scrub = session.lock().scrub.clone();
        let editing = scrub.is_some()
            || self.nudge.is_some()
            || self.select.drag.is_some()
            || self.type_tool.edited_layer().is_some();
        let playback = session
            .lock()
            .playback
            .clone()
            .filter(|pb| pb.comp == info.path);
        let mut completed = None;
        if let Some(gpu) = self.gpu.as_mut() {
            if let Some((generation, item)) = &session.lock().solid_reload {
                gpu.reload_solid(*generation, item);
            }
            gpu.purge(session.lock().purge);
            if editing {
                self.idle = None;
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
                let caching = playback.as_ref().and_then(|pb| pb.caching.as_ref());
                let frame_time = caching.map_or(time, |p| info.frame_rate.frame_to_time(p.first));
                let poll_range = if caching.is_some() {
                    bae_model::TimeRange::new(frame_time, frame_time + info.frame_duration())
                } else {
                    range
                };
                let looping = playback.is_some()
                    && caching.is_none()
                    && session.lock().preview_options().repeat;
                let mut shown_now = false;
                if let Some((image, actual, errors)) = gpu.playback.poll(
                    cx.stage,
                    cx.resources,
                    &info.path,
                    frame_time,
                    info.frame_rate,
                    poll_range,
                    looping,
                    if playback.is_some() {
                        session.lock().preview_options().skip
                    } else {
                        0
                    },
                    resolution,
                    ui.ctx(),
                ) {
                    completed = Some(image);
                    shown_now = actual == time;
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
                if let Some(pass) = caching {
                    let mut pass = pass.clone();
                    let bytes = u64::from(info.width.div_ceil(resolution))
                        * u64::from(info.height.div_ceil(resolution))
                        * 4;
                    let done = gpu.playback.fill_cache(&mut pass, info.frame_rate, bytes);
                    let mut s = session.lock();
                    if let Some(pb) = s.playback.as_mut() {
                        pb.caching = Some(pass);
                    }
                    if done {
                        s.finish_cache(&info, true);
                    }
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(25));
                } else if playback.is_some() {
                    // A cold start: wait until the first playback frame is ready.
                    session.lock().hold_first_frame(shown_now);
                }
                let prefs = crate::prefs::current(cx.resources);
                let source_focused = cx
                    .focused_panel
                    .is_some_and(|id| matches!(id.kind(), "footage" | "layer"));
                if playback.is_none()
                    && !source_focused
                    && prefs.bool(bae_prefs::keys::CACHE_FRAMES_WHEN_IDLE)
                {
                    let now = std::time::Instant::now();
                    let revision = cx.stage.revision();
                    let active = ui.input(|i| !i.raw.events.is_empty());
                    let since = match &self.idle {
                        Some((comp, t, rev, since))
                            if !active && comp == &info.path && *t == time && *rev == revision =>
                        {
                            *since
                        }
                        _ => {
                            self.idle = Some((info.path.clone(), time, revision, now));
                            now
                        }
                    };
                    let delay = std::time::Duration::from_secs(
                        prefs.int("previews.idle_delay_before_caching") as u64,
                    );
                    let elapsed = now.duration_since(since);
                    let plan = if !active && elapsed >= delay {
                        crate::playback::idle::Plan::from_prefs(&prefs, &info, time)
                    } else {
                        ui.ctx()
                            .request_repaint_after(delay.saturating_sub(elapsed));
                        None
                    };
                    gpu.playback.prefetch(plan);
                } else {
                    self.idle = None;
                    gpu.playback.prefetch(None);
                }
                gpu.playback.publish_ranges(&session, info.frame_rate);
            }
        }
        if self.gpu.is_none() && playback.as_ref().is_some_and(|pb| pb.caching.is_some()) {
            session.lock().finish_cache(&info, false);
        }
        if playback.is_some() && self.gpu.is_some() && !editing {
            self.on_gpu = completed.is_some();
            if let Some(image) = completed {
                paint_image(&clip, &view, image);
            }
            if fullscreen {
                return;
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
            crate::tracker::camera::banner(ui, cx, area);
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
        let nudged = match &self.nudge {
            Some(Nudge::Layers(d, _)) => Some(d),
            _ => None,
        };
        apply_overrides(&mut comp, self.select.drag.as_ref().or(nudged));
        // A shape group being dragged.
        if let Some(d) = &self.select.group_drag
            && let Some(l) = comp.layers.iter_mut().find(|l| l.path == d.layer)
        {
            d.preview(l);
        }
        // A mask or shape path being edited shows in the picture, not only as
        // its outline (#593).
        let nudged_path = match &self.nudge {
            Some(Nudge::Vertices(d, _)) => Some((d.prim().clone(), d.preview().path)),
            _ => None,
        };
        if let Some((prim, path)) = self
            .select
            .path_preview()
            .or(self.pen.path_preview())
            .or(self.feather.path_preview())
            .or(nudged_path)
        {
            overlay::apply_path_preview(&mut comp, &prim, &path);
        }
        // The 3D view: 3D layers are seen through its camera.
        let size = DVec2::new(f64::from(info.width), f64::from(info.height));
        if camera_tool.is_camera() || self.camera_drag.is_some() || dolly_notches != 0.0 {
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
                dolly_notches,
                self.prefs.reverse_dolly_drag,
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
                && let Some(frame) = g.playback.shown_texture()
            {
                session.lock().channel_frame = g.playback.shown_channels();
                g.show_cached(frame);
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
        if let Some(gpu) = self.gpu.as_ref() {
            let mut s = session.lock();
            if let Some(texture) = self
                .snapshot_shown
                .and_then(|slot| gpu.snapshot_texture(slot))
            {
                use std::hash::{Hash, Hasher};
                let mut key = std::collections::hash_map::DefaultHasher::new();
                texture.hash(&mut key);
                s.scope_frame = Some((texture, key.finish()));
                s.scope_source = None;
            } else if let Some(frame) = gpu.output() {
                s.scope_frame = Some(frame);
                s.scope_source = crate::scopes::needs_working_frame(ui.ctx())
                    .then(|| gpu.scope_source())
                    .flatten();
            } else if !self.on_gpu {
                s.scope_frame = None;
                s.scope_source = None;
            }
        }
        // The Info panel: the pointer in comp pixels and, while the panel shows
        // it, the color under it (one texel read back from the GPU frame).
        let tool = tools.lock().current();
        if let Some(h) = resp.interact_pointer_pos().or(resp.hover_pos()) {
            let p = view.to_comp(h);
            let mut s = session.lock();
            s.info.pos = Some([p.x, p.y]);
            let shown = ui
                .ctx()
                .cumulative_frame_nr()
                .saturating_sub(s.info.shown_at)
                < 3;
            let mask = self.prefs.contrasting_masks
                && (tool.is_shape() || tool == Tool::Pen)
                && self.shape.is_none()
                && !self.pen.busy()
                && !s.selected_layers.is_empty();
            if shown || mask {
                let origin = ui.input(|i| i.pointer.press_origin());
                let pressed = ui.input(|i| i.pointer.primary_pressed());
                let down = ui.input(|i| i.pointer.primary_down());
                if pressed {
                    self.mask_start_color = None;
                }
                if mask
                    && down
                    && self.mask_start_color.is_none()
                    && let Some(origin) = origin
                {
                    let p = view.to_comp(origin);
                    self.mask_start_color = self
                        .gpu
                        .as_mut()
                        .and_then(|g| g.pixel([p.x, p.y], ui.ctx()));
                }
                if shown || !down || pressed {
                    s.info.color = self
                        .gpu
                        .as_mut()
                        .and_then(|g| g.pixel([p.x, p.y], ui.ctx()));
                }
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
            if self.prefs.show_precomp_bounds {
                overlay::precomp_bounds(&painter, l, &view, &outer, false, c);
            }
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
        particle_guides::paint(
            &clip,
            cx.stage,
            eval,
            &comp,
            &view,
            &selected,
            view_camera,
            &mut self.particle_paths,
        );
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
        if let Some(drag) = &self.camera_drag {
            drag.paint_point(
                &painter,
                &view,
                self.prefs.camera_point,
                resp.interact_pointer_pos(),
            );
        }

        // Interaction by tool. Space held over the viewer is the Hand tool until
        // released (AE-UI-307), except while drawing a shape (Space moves it).
        let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
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
        let hand = !roi_busy
            && !navigating
            && (tool == Tool::Hand || self.space_hold.is_some() || self.panning);
        // Cmd/Ctrl makes the pen tools the Selection tool while held (AE-SHP-044).
        let command = ui.input(|i| i.modifiers.command);
        let effective = if navigating {
            camera_tool
        } else if roi_busy || hand {
            Tool::Hand
        } else if tool.is_pen() && command && !self.pen.busy() {
            Tool::Selection
        } else {
            tool
        };
        if resp.double_clicked()
            && effective.is_layer_paint()
            && crate::prefs::current(cx.resources)
                .bool("composition.double_click_paint_roto_opens_layer")
            && let Some(layer) =
                pointer.and_then(|pos| select_tool::hit_layer(cx.stage, &comp, &view, pos))
        {
            session.lock().selected_layers = vec![layer.clone()];
            crate::layer_panel::open_clicked(
                cx,
                &session,
                &layer,
                ui.input(|i| i.modifiers.alt),
                effective,
                &mut self.eval,
            );
        }
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
            self.mask_start_color,
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
        // Puppet tools: pins added and dragged.
        let selected_layers = session.lock().selected_layers.clone();
        let puppet = puppet_tool::frame(
            &mut self.puppet,
            ui,
            &resp,
            cx,
            &session,
            &comp,
            &view,
            effective,
            &painter,
            time,
            &selected_layers,
        );
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
        let select_cursor = if puppet.owns {
            puppet.cursor
        } else if points.owns {
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
                &mut self.eval,
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
        if resp.secondary_clicked() {
            self.menu_key = resp
                .interact_pointer_pos()
                .and_then(|p| self.motion_path.key_at(p));
            if let Some(k) = &self.menu_key {
                let mut s = session.lock();
                if !s.selected_keys.contains(k) {
                    s.selected_keys = vec![k.clone()];
                }
            }
        }
        menu::frame(
            &mut self.menu_layers,
            self.menu_key.as_ref(),
            &resp,
            cx,
            &session,
            &comp,
            &view,
            self.type_tool.editing.is_some(),
        );

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
        crate::tracker::camera::banner(ui, cx, area);
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
        })
        .multi_instance(),
    );
}
