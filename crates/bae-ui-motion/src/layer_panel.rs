//! Layer panel (AE's Layer viewer): one layer on its own, in its own space.
//! Double-clicking a layer's bar in the Timeline opens it (a pre-comp layer
//! opens its composition instead); Window ▸ Layer opens the selected layer.
//! The View popup chooses what the panel shows and edits: nothing, the masks,
//! or one effect. With Render on, the picture includes the effects up to the
//! chosen one; paint is drawn on the Paint effect. The time ruler under the
//! picture is the layer's: its in and out points (drag an end to trim the
//! layer; the { and } buttons set them to the current time) and the current
//! time, with AE's In, Out and Duration readouts. The picture zooms and pans
//! like the Composition viewer: the bar's magnification popup, the mouse
//! wheel or a pinch around the pointer, trackpad scrolling, the Hand tool,
//! Space-drag or a middle drag, and Zoom tool clicks.
//!
//! The Brush, Clone Stamp and Eraser paint here: a drag becomes one stroke on
//! the layer's Paint effect (`paint.add_stroke`, one undo step) with the Paint
//! and Brushes panels' settings; Alt/Option-click sets the Clone Stamp source.

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2};
use bae_model::{PrimPath, Time, Value};
use bae_motion::query::{self, CompInfo};
use bae_ui::Accessible as _;
use bae_ui::theme::{self, color};
use bae_ui::{Panel, PanelContext, TabInfo, tr, widgets};
use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::common::{self, EvalCache};
use crate::gpu_preview::GpuPreview;
use crate::render::{self, Affine, View};
use crate::session::SharedSession;

/// What the View popup shows (AE: None, Masks, then each effect).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Show {
    #[default]
    Nothing,
    Masks,
    /// The effect at this index (in the layer's effect order).
    Effect(usize),
}

#[derive(Default)]
pub struct LayerPanel {
    display: crate::source_view_menu::SourceView,
    tracks: crate::tracker::overlay::Overlay,
    eval: EvalCache,
    gpu: Option<GpuPreview>,
    gpu_tried: bool,
    pub(crate) show: Show,
    /// Render: the picture includes the effects up to the shown one.
    pub(crate) render: bool,
    /// The stroke being painted: layer pixels, when it started.
    stroke: Option<(Vec<DVec2>, std::time::Instant)>,
    /// Clone Stamp: the source point (Alt/Option-click), layer pixels.
    clone_point: Option<DVec2>,
    /// Liquify: the mesh being brushed, the pointer's last position (layer
    /// pixels).
    liquify: Option<(bae_effects::effects::liquify::Mesh, DVec2)>,
    /// An end of the layer being dragged on the ruler: the in point (`true`)
    /// or the out point, and where it would go.
    trim: Option<(bool, Time)>,
    /// Magnification (1 = 100 %); `None` = Fit up to 100 %.
    pub(crate) zoom: Option<f32>,
    /// Legacy magnification: the part of a wheel notch not yet stepped.
    wheel_rest: f32,
    /// The picture's offset from centred, in points.
    pan: egui::Vec2,
}

/// Resolve a timeline or Composition viewer double-click with AE's Composition preferences.
pub(crate) fn open_clicked(
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    layer: &PrimPath,
    alt: bool,
    tool: crate::tools::Tool,
    eval: &mut EvalCache,
) {
    let prefs = crate::prefs::current(cx.resources);
    let paint =
        tool.is_layer_paint() && prefs.bool("composition.double_click_paint_roto_opens_layer");
    if !paint {
        if let Some(source) = query::precomp_source(cx.stage, layer) {
            if (prefs.str("composition.double_click_comp_layer") == "source_comp_panel") != alt {
                let mut s = session.lock();
                if prefs.bool("composition.synchronize_time_all_related_items")
                    && let Some(info) = query::comp_info(cx.stage, &source)
                {
                    let now = s.active_time();
                    let remap = cx
                        .stage
                        .prim(layer)
                        .and_then(|p| query::static_value(&p, "time_remap_enabled"))
                        .and_then(|v| v.as_bool())
                        == Some(true);
                    let time = if remap {
                        eval.get(cx.stage)
                            .evaluate_attribute(cx.stage, layer, "time_remap", now)
                            .ok()
                            .and_then(|v| {
                                v.as_time().or_else(|| v.as_f64().map(Time::from_seconds))
                            })
                    } else {
                        None
                    };
                    s.set_time(
                        &info,
                        time.unwrap_or_else(|| {
                            query::layer_timing(cx.stage, layer).layer_time(now)
                        }),
                    );
                }
                s.open_comp(&source);
                drop(s);
                cx.open_panel("composition");
                cx.open_panel("timeline");
                return;
            }
        } else if (prefs.str("composition.double_click_footage_layer") == "source_footage_panel")
            != alt
            && matches!(
                query::layer_kind(cx.stage, layer).as_deref(),
                Some("footage" | "audio")
            )
            && let Some(asset) = cx
                .stage
                .prim(layer)
                .and_then(|p| p.value("source")?.as_prim_ref().cloned())
        {
            session.lock().footage_view = Some(asset);
            cx.open_panel("footage");
            return;
        }
    }
    session.lock().layer_panel = Some(layer.clone());
    cx.open_panel("layer");
}

/// The layer the panel shows: the one opened last, or the selected one.
fn shown_layer(session: &SharedSession) -> Option<PrimPath> {
    let s = session.lock();
    s.layer_panel
        .clone()
        .or_else(|| s.selected_layers.first().cloned())
}

/// `layer` alone in a comp the size of its bounds, at the origin, with its
/// first `effects` effects and no masks, matte or parent; and where its bounds
/// start in layer space.
pub(crate) fn alone(
    comp: &EvaluatedComp,
    layer: &PrimPath,
    effects: usize,
) -> Option<(EvaluatedComp, DVec2)> {
    let mut l = comp.layer(layer)?.clone();
    let (min, max) = render::layer_bounds(&l)?;
    let size = (max - min).max(DVec2::ONE);
    let m = DMat4::from_translation((-min).extend(0.0));
    l.parent = None;
    l.local_matrix = m;
    l.world_matrix = m;
    l.opacity = 1.0;
    l.visible = true;
    l.active = true;
    l.blend_mode = "normal".into();
    l.track_matte = Default::default();
    l.track_matte_layer = None;
    l.switches.three_d = false;
    l.switches.solo = false;
    l.switches.adjustment = false;
    l.switches.motion_blur = false;
    l.masks.clear();
    l.effects.truncate(effects);
    let mut out = comp.clone();
    out.width = size.x.ceil() as u32;
    out.height = size.y.ceil() as u32;
    out.layers = vec![l];
    out.camera = None;
    out.bg_color = [0.0, 0.0, 0.0, 0.0];
    Some((out, min))
}

impl Panel for LayerPanel {
    fn menu(&mut self, ui: &mut Ui, _: &mut PanelContext<'_>) {
        self.display.menu(ui);
    }

    fn tab(&self, cx: &PanelContext<'_>) -> TabInfo {
        let session = common::session(cx.resources);
        match shown_layer(&session) {
            Some(l) if cx.stage.prim(&l).is_some() => crate::prefs::related_tab(
                cx,
                format!("{} {}", tr("Layer"), query::display_name_of(cx.stage, &l)),
                &l,
            ),
            _ => TabInfo::new(tr("Layer")),
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        session.lock().sync(cx.stage);
        let full = ui.available_rect_before_wrap();
        let bar_h = 28.0;
        let ruler_h = 22.0;
        let area = Rect::from_min_max(
            full.min,
            pos2(full.right(), full.bottom() - bar_h - ruler_h),
        );
        let ruler = Rect::from_min_max(
            pos2(full.left(), area.bottom()),
            pos2(full.right(), area.bottom() + ruler_h),
        );
        let bar = Rect::from_min_max(pos2(full.left(), ruler.bottom()), full.max);
        ui.allocate_rect(full, Sense::hover());
        ui.painter()
            .rect_filled(area, 0.0, self.display.background(ui));
        let info = common::active_comp(cx.stage, &session);
        let layer = shown_layer(&session).filter(|l| cx.stage.prim(l).is_some());
        let (Some(info), Some(layer)) = (info, layer) else {
            ui.painter().text(
                area.center(),
                Align2::CENTER_CENTER,
                tr("Double-click a layer in the Timeline to open it here."),
                theme::small(),
                color::TEXT_DIM,
            );
            return;
        };
        let time = session.lock().time(&info.path);
        let eval = crate::viewer::live_evaluator(&mut self.eval, cx, &session);
        if let Some(g) = &self.gpu
            && !eval
                .image_samples()
                .is_some_and(|s| std::sync::Arc::ptr_eq(s, g.samples()))
        {
            eval.set_image_samples(Some(g.samples().clone()));
        }
        let Ok(comp) = eval.evaluate_comp(cx.stage, &info.path, time) else {
            return;
        };
        let Some(evaluated) = comp.layer(&layer) else {
            return;
        };
        let effects = match (self.show, self.render) {
            (Show::Effect(i), true) => i + 1,
            _ => 0,
        };
        if let Some((one, min)) = alone(&comp, &layer, effects) {
            let source = cx
                .stage
                .prim(&evaluated.path)
                .and_then(|l| l.value("source").and_then(|v| v.as_prim_ref()).cloned());
            let aspect = self.display.aspect(cx.stage, source.as_ref());
            let size = vec2(one.width as f32 * aspect, one.height as f32);
            let panning = self.navigate(ui, cx, area, size);
            let (mut view, _) = self.view(area, size, ui.ctx().pixels_per_point());
            view.aspect = aspect;
            self.picture(ui, cx, area, &one, min, evaluated, &view);
            // Space pans whatever the tool (AE); a paint tool paints,
            // otherwise a shown Liquify effect brushes.
            if !panning && !self.paint(ui, cx, area, &view, min, evaluated) {
                self.liquify(ui, cx, &session, area, &view, min, evaluated, time);
            }
        }
        let in_out = (evaluated.timing.in_point, evaluated.timing.out_point);
        if let Some(edit) = self.time_ruler(ui, &session, &info, ruler, time, in_out) {
            trim(cx, &layer, edit);
        }
        let names: Vec<String> = evaluated
            .effects
            .iter()
            .map(|e| query::display_name_of(cx.stage, &e.path))
            .collect();
        if let Some(edit) = self.bar(ui, bar, &info, time, &names, in_out) {
            trim(cx, &layer, edit);
        }
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({ "render": self.render, "display": self.display }))
    }
}

impl LayerPanel {
    pub fn new(state: Option<&serde_json::Value>) -> Self {
        LayerPanel {
            display: state
                .and_then(|v| serde_json::from_value(v["display"].clone()).ok())
                .unwrap_or_default(),
            render: state
                .and_then(|s| s.get("render"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            ..LayerPanel::default()
        }
    }

    /// The picture's view of a `size` layer in `area`, and the magnification
    /// (AE: Fit up to 100 % until zoomed).
    fn view(&self, area: Rect, size: egui::Vec2, ppp: f32) -> (View, f32) {
        let fit = ((area.width() - 24.0) / size.x)
            .min((area.height() - 24.0) / size.y)
            .max(0.01)
            .min(1.0 / ppp);
        let scale = self.zoom.map_or(fit, |m| m / ppp);
        let view = View {
            origin: area.center() - size * scale / 2.0 + self.pan,
            aspect: 1.0,
            scale,
            point: 6.0,
            camera: None,
        };
        (view, scale * ppp)
    }

    /// Zoom by `factor` keeping the layer point under `around` in place.
    fn zoom_by(&mut self, factor: f32, around: egui::Pos2, area: Rect, size: egui::Vec2, ppp: f32) {
        let (view, mag) = self.view(area, size, ppp);
        let at = view.to_comp(around);
        self.zoom = Some((mag * factor).clamp(0.02, 32.0));
        let (moved, _) = self.view(area, size, ppp);
        self.pan += around - moved.to_screen(at);
    }

    /// Zoom and pan from the pointer, the wheel and the tools; `true` while
    /// Space is held (the picture pans instead of the tool working).
    fn navigate(&mut self, ui: &Ui, cx: &PanelContext<'_>, area: Rect, size: egui::Vec2) -> bool {
        let ppp = ui.ctx().pixels_per_point();
        let resp = ui.interact(
            area,
            ui.id().with("layer_navigate"),
            Sense::click_and_drag(),
        );
        // The viewer's wheel rules (#571): one shared reading.
        if let Some(pointer) = resp.hover_pos() {
            let m = crate::viewer::Magnification::of(cx.resources);
            let w = crate::viewer::wheel(ui, m);
            let around = if m.follows_pointer(w.alt) {
                pointer
            } else {
                area.center()
            };
            if cfg!(target_os = "macos") && crate::viewer::smart_zoom(ui.ctx()) {
                if self.zoom.is_some() {
                    self.zoom = None;
                    self.pan = egui::Vec2::ZERO;
                } else {
                    let (_, mag) = self.view(area, size, ppp);
                    self.zoom_by((1.0 / mag).max(2.0), pointer, area, size, ppp);
                }
            } else if w.pinch != 1.0 {
                self.zoom_by(w.pinch, pointer, area, size, ppp);
            } else if w.notches != 0.0 {
                let (_, mag) = self.view(area, size, ppp);
                let to = m.wheel_zoom(mag, w.notches, &mut self.wheel_rest);
                if to != mag {
                    self.zoom_by(to / mag, around, area, size, ppp);
                }
            }
            self.pan += w.pan;
        }
        let tool = crate::tools::shared(cx.resources).lock().current();
        let space = ui.input(|i| i.key_down(egui::Key::Space)) && !ui.ctx().text_edit_focused();
        let hand = tool == crate::tools::Tool::Hand || space;
        if resp.dragged_by(egui::PointerButton::Middle)
            || (hand && resp.dragged_by(egui::PointerButton::Primary))
        {
            self.pan += resp.drag_delta();
        }
        if tool == crate::tools::Tool::Zoom
            && !space
            && resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let (_, mag) = self.view(area, size, ppp);
            let out = ui.input(|i| i.modifiers.alt);
            // The viewer's steps: powers of two (#570).
            let next = crate::viewer::next_step(mag, !out);
            if let Some(m) = next {
                self.zoom_by(m / mag, p, area, size, ppp);
            }
        }
        space
    }

    /// The layer's picture, fitted in `area`, and the masks in Masks view.
    #[allow(clippy::too_many_arguments)]
    fn picture(
        &mut self,
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        area: Rect,
        one: &EvaluatedComp,
        min: DVec2,
        layer: &bae_anim::EvaluatedLayer,
        view: &View,
    ) {
        let frame = render::comp_rect(view, one.width, one.height);
        let painter = ui.painter_at(area);
        if self.display.grid {
            crate::viewer::checkerboard(&painter, frame.intersect(area));
        }
        if self.gpu.is_none() && !self.gpu_tried {
            self.gpu_tried = true;
            self.gpu = GpuPreview::new(cx.resources);
        }
        let image = self
            .gpu
            .as_mut()
            .and_then(|g| g.frame(cx.resources, cx.stage, one, 1, None, false));
        if self
            .gpu
            .as_ref()
            .is_some_and(GpuPreview::waiting_for_samples)
        {
            ui.ctx().request_repaint();
        }
        if let Some(img) = image {
            let r = img.rect;
            let at = |x: i32, y: i32| view.to_screen(DVec2::new(f64::from(x), f64::from(y)));
            painter.image(
                egui::TextureId::User(img.handle),
                Rect::from_min_max(at(r.x0, r.y0), at(r.x1, r.y1)),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            let mut shapes = Vec::new();
            let to_screen = |p: DVec2| view.to_screen(p);
            render::paint_comp(
                one,
                &painter,
                &to_screen,
                Affine::new(DMat4::IDENTITY),
                1.0,
                f64::from(view.scale),
                &|s| egui::FontId::proportional(s),
                &mut shapes,
            );
            painter.extend(shapes);
        }
        painter.rect_stroke(
            frame,
            0.0,
            Stroke::new(1.0, Color32::from_gray(90)),
            egui::StrokeKind::Outside,
        );
        self.tracks.draw(
            ui,
            cx,
            area,
            view,
            min,
            &layer.path,
            |painter, lens, source, scale| {
                let to_screen = |v: DVec2| {
                    lens.center()
                        + egui::vec2(
                            (v.x - source.x) as f32 * view.aspect,
                            (v.y - source.y) as f32,
                        ) * scale
                };
                painter.rect_filled(lens, 0., Color32::from_gray(35));
                if let Some(img) = image {
                    let r = img.rect;
                    let rect = Rect::from_min_max(
                        to_screen(DVec2::new(f64::from(r.x0), f64::from(r.y0))),
                        to_screen(DVec2::new(f64::from(r.x1), f64::from(r.y1))),
                    );
                    painter.image(
                        egui::TextureId::User(img.handle),
                        rect,
                        Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
                        Color32::WHITE,
                    );
                } else {
                    let mut shapes = Vec::new();
                    render::paint_comp(
                        one,
                        painter,
                        &to_screen,
                        Affine::new(DMat4::IDENTITY),
                        1.0,
                        f64::from(scale),
                        &|s| egui::FontId::proportional(s),
                        &mut shapes,
                    );
                    painter.extend(shapes);
                }
            },
        );
        if self.show == Show::Masks {
            let to_panel = DMat4::from_translation((-min).extend(0.0));
            for m in &layer.masks {
                let stroke = Stroke::new(1.0, crate::viewer::mask_color(cx.stage, &m.path));
                painter.extend(crate::viewer::path_shapes(
                    &m.shape, &to_panel, view, stroke,
                ));
            }
        }
    }

    /// The paint tools: drag to paint a stroke; Alt/Option-click with the
    /// Clone Stamp sets its source. `false` when no paint tool is active.
    fn paint(
        &mut self,
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        area: Rect,
        view: &View,
        min: DVec2,
        layer: &bae_anim::EvaluatedLayer,
    ) -> bool {
        use crate::tools::Tool;
        let shared = crate::tools::shared(cx.resources);
        let (tool, p) = {
            let t = shared.lock();
            (t.current(), t.options.paint.clone())
        };
        if !matches!(tool, Tool::Brush | Tool::CloneStamp | Tool::Eraser) {
            self.stroke = None;
            return false;
        }
        let resp = ui.interact(area, ui.id().with("paint"), Sense::click_and_drag());
        let to_layer = |pos: egui::Pos2| view.to_comp(pos) + min;
        let to_screen = |q: DVec2| view.to_screen(q - min);
        let alt = ui.input(|i| i.modifiers.alt);
        if tool == Tool::CloneStamp
            && alt
            && resp.clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            self.clone_point = Some(to_layer(pos));
            return true;
        }
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = ui.input(|i| i.pointer.press_origin())
        {
            self.stroke = Some((vec![to_layer(pos)], std::time::Instant::now()));
        }
        if let Some((points, _)) = &mut self.stroke
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let q = to_layer(pos);
            if points.last().is_none_or(|l| l.distance(q) >= 1.0) {
                points.push(q);
            }
        }
        // The brush outline at the pointer, the stroke so far.
        let painter = ui.painter_at(area);
        let radius = (p.diameter / 2.0) as f32 * view.scale;
        if let Some(pos) = resp.hover_pos() {
            painter.circle_stroke(
                pos,
                radius.max(1.0),
                Stroke::new(1.0, Color32::from_gray(200)),
            );
        }
        if let Some(c) = self.clone_point.filter(|_| tool == Tool::CloneStamp) {
            let s = to_screen(c);
            painter.line_segment(
                [s - vec2(6.0, 0.0), s + vec2(6.0, 0.0)],
                Stroke::new(1.0, color::ACCENT),
            );
            painter.line_segment(
                [s - vec2(0.0, 6.0), s + vec2(0.0, 6.0)],
                Stroke::new(1.0, color::ACCENT),
            );
        }
        if let Some((points, _)) = &self.stroke {
            let to = |v: f64| (v.clamp(0.0, 1.0) * 255.0) as u8;
            let c = match tool {
                Tool::Brush => Color32::from_rgb(
                    to(p.foreground[0]),
                    to(p.foreground[1]),
                    to(p.foreground[2]),
                ),
                _ => Color32::from_white_alpha(120),
            };
            let line: Vec<egui::Pos2> = points.iter().map(|q| to_screen(*q)).collect();
            painter.add(egui::Shape::line(
                line,
                Stroke::new((2.0 * radius).max(1.0), c),
            ));
        }
        if !ui.input(|i| i.pointer.primary_down())
            && let Some((points, started)) = self.stroke.take()
        {
            let first = points[0];
            let offset = match (tool, self.clone_point) {
                (Tool::CloneStamp, Some(c)) => {
                    let o = c - first;
                    // Aligned: later strokes keep this offset.
                    if p.aligned {
                        self.clone_point = None;
                        shared.lock().options.paint.clone_offset = [o.x, o.y];
                    }
                    o
                }
                _ => DVec2::from_array(p.clone_offset),
            };
            let flicks = |s: f64| Time::from_seconds(s).0;
            let params = serde_json::json!({
                "layer": layer.path,
                "tool": match tool { Tool::CloneStamp => "clone", Tool::Eraser => "eraser", _ => "brush" },
                "points": points.iter().map(|q| [q.x, q.y]).collect::<Vec<_>>(),
                "time": layer.layer_time.0,
                "duration": p.duration,
                "frames": p.frames,
                "draw_time": flicks(started.elapsed().as_secs_f64()),
                "color": [p.foreground[0], p.foreground[1], p.foreground[2], 1.0],
                "diameter": p.diameter,
                "angle": p.angle,
                "hardness": p.hardness,
                "roundness": p.roundness,
                "spacing": p.spacing,
                "opacity": p.opacity,
                "flow": p.flow,
                "channels": p.channels,
                "mode": p.mode,
                "erase": (tool == Tool::Eraser).then_some(&p.erase),
                "clone_source": p.clone_source,
                "clone_offset": [offset.x, offset.y],
                "clone_time_shift": flicks(p.clone_time_shift),
            });
            cx.invoke_with("paint.add_stroke", params);
            // AE shows the Paint effect once there is paint.
            let paint = layer
                .effects
                .iter()
                .position(|e| e.match_name == "ADBE Paint")
                .unwrap_or(layer.effects.len());
            self.show = Show::Effect(paint);
        }
        true
    }

    /// The Liquify brush: with a Liquify effect in the View popup, dragging
    /// applies its tool to the Distortion Mesh (previewed live, one undo step).
    #[allow(clippy::too_many_arguments)]
    fn liquify(
        &mut self,
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        area: Rect,
        view: &View,
        min: DVec2,
        layer: &bae_anim::EvaluatedLayer,
        time: Time,
    ) {
        use bae_effects::effects::liquify::{self as lq, Mesh};
        let Show::Effect(i) = self.show else {
            return;
        };
        let Some(fx) = layer
            .effects
            .get(i)
            .filter(|e| e.match_name == lq::MATCH_NAME)
        else {
            return;
        };
        let num = |id: u32, d: f64| fx.param(id).and_then(Value::as_f64).unwrap_or(d);
        let tool = (num(lq::TOOL, 1.0).round() as usize).saturating_sub(1);
        let radius = num(lq::BRUSH_SIZE, 64.0) / 2.0;
        let pressure = num(lq::BRUSH_PRESSURE, 50.0) / 100.0;
        let resp = ui.interact(area, ui.id().with("liquify"), Sense::click_and_drag());
        let to_layer = |p: egui::Pos2| view.to_comp(p) + min;
        if let Some(pos) = resp.hover_pos() {
            ui.painter_at(area).circle_stroke(
                pos,
                (radius as f32 * view.scale).max(1.0),
                Stroke::new(1.0, Color32::from_gray(220)),
            );
        }
        let attr = format!("param:{:04}", lq::MESH);
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = ui.input(|i| i.pointer.press_origin())
        {
            let mesh = fx.param(lq::MESH).and_then(Mesh::from_value).or_else(|| {
                let (a, b) = render::layer_bounds(layer)?;
                Some(Mesh::new(a, b - a))
            });
            self.liquify = mesh.map(|m| (m, to_layer(pos)));
        }
        let Some((mesh, last)) = &mut self.liquify else {
            return;
        };
        if let Some(pos) = resp.interact_pointer_pos() {
            let at = to_layer(pos);
            mesh.brush(tool, at, radius, pressure, at - *last);
            *last = at;
            ui.ctx().request_repaint();
        }
        let value = mesh.to_value();
        let done = !ui.input(|i| i.pointer.primary_down());
        let edit = if done {
            self.liquify = None;
            common::Edit::Commit(value)
        } else {
            common::Edit::Live(value)
        };
        common::apply_edit(cx, session, &fx.path, &attr, time, edit);
    }

    /// The bar under the ruler; returns a trim the { or } button asked for.
    fn bar(
        &mut self,
        ui: &mut Ui,
        bar: Rect,
        info: &CompInfo,
        time: Time,
        effects: &[String],
        (in_point, out_point): (Time, Time),
    ) -> Option<Trim> {
        ui.painter().rect_filled(bar, 0.0, color::PANEL);
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar.shrink2(vec2(6.0, 4.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui = &mut child;
        ui.spacing_mut().item_spacing.x = 6.0;
        // Magnification (as in the Composition viewer).
        let mags = crate::viewer::MAGNIFICATIONS;
        let mut labels: Vec<String> = vec![tr("Fit").to_owned()];
        labels.extend(mags.iter().map(|(_, label)| (*label).to_owned()));
        let items: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut sel = match self.zoom {
            None => 0,
            Some(z) => mags
                .iter()
                .position(|(m, _)| (m - z).abs() < 1e-3)
                .map_or(usize::MAX, |i| i + 1),
        };
        if widgets::dropdown(ui, "layer_mag", &mut sel, &items, 74.0)
            .on_hover_text(tr("Magnification"))
            .changed()
        {
            self.pan = egui::Vec2::ZERO;
            self.zoom = (sel > 0).then(|| mags[sel - 1].0);
        }
        if let Some(z) = self
            .zoom
            .filter(|z| !mags.iter().any(|(m, _)| (m - z).abs() < 1e-3))
        {
            ui.label(
                egui::RichText::new(format!("{:.1}%", z * 100.0))
                    .font(theme::small())
                    .color(color::TEXT),
            );
        }
        ui.label(
            egui::RichText::new(tr("View:"))
                .font(theme::small())
                .color(color::TEXT),
        );
        let mut labels = vec![tr("None").to_owned(), tr("Masks").to_owned()];
        labels.extend(effects.iter().cloned());
        let items: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut i = match self.show {
            Show::Nothing => 0,
            Show::Masks => 1,
            Show::Effect(k) => (k + 2).min(items.len() - 1),
        };
        if widgets::dropdown(ui, "layer_view", &mut i, &items, 110.0).changed() {
            self.show = match i {
                0 => Show::Nothing,
                1 => Show::Masks,
                k => Show::Effect(k - 2),
            };
        }
        widgets::checkbox(ui, &mut self.render, tr("Render"));
        ui.label(
            egui::RichText::new(common::timecode(info, time))
                .font(theme::body())
                .color(color::ACCENT),
        );
        // Set In / Out Point to Current Time, each with its time, then the
        // duration (the out point is exclusive; AE shows the last frame).
        let mut out = None;
        let frame = info.frame_duration();
        let last = |out_point: Time, in_point: Time| {
            if out_point > in_point + frame {
                out_point - frame
            } else {
                in_point
            }
        };
        let (shown_in, shown_out) = match self.trim {
            Some((true, t)) => (t, last(out_point, t)),
            Some((false, t)) => (in_point, last(t, in_point)),
            None => (in_point, last(out_point, in_point)),
        };
        let readout = |ui: &mut Ui, text: String| {
            ui.label(
                egui::RichText::new(text)
                    .font(theme::small())
                    .color(color::TEXT),
            );
        };
        for (glyph, tip, edit, shown) in [
            (
                "{",
                "Set In Point to Current Time",
                Trim::In(time),
                shown_in,
            ),
            (
                "}",
                "Set Out Point to Current Time",
                Trim::Out(time + frame),
                shown_out,
            ),
        ] {
            if ui
                .add(egui::Button::new(egui::RichText::new(glyph).font(theme::body())).small())
                .labeled(egui::WidgetType::Button, tr(tip))
                .on_hover_text(tr(tip))
                .clicked()
            {
                out = Some(edit);
            }
            readout(ui, common::timecode(info, shown));
        }
        readout(
            ui,
            format!("Δ {}", common::timecode(info, shown_out + frame - shown_in)),
        );
        out
    }
}

/// A trim the ruler or a button asks for (comp time; the out point exclusive).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Trim {
    In(Time),
    Out(Time),
}

/// Trim the layer (one `layer.trim`, one undo step).
fn trim(cx: &mut PanelContext<'_>, layer: &PrimPath, edit: Trim) {
    let t = query::layer_timing(cx.stage, layer);
    let params = match edit {
        Trim::In(at) if at < t.out_point && at != t.in_point => {
            serde_json::json!({"layers": [layer], "in_point": at})
        }
        Trim::Out(at) if at > t.in_point && at != t.out_point => {
            serde_json::json!({"layers": [layer], "out_point": at})
        }
        _ => return,
    };
    cx.invoke_with("layer.trim", params);
}

/// Pick distance for the ruler's in and out ends.
const END_PICK: f32 = 5.0;

impl LayerPanel {
    /// The layer's time ruler: the comp's duration, the layer's span between
    /// its in and out points, the current time. Clicking or dragging moves the
    /// time; dragging an end of the span trims the layer (on release).
    fn time_ruler(
        &mut self,
        ui: &Ui,
        session: &SharedSession,
        info: &CompInfo,
        rect: Rect,
        time: Time,
        (in_point, out_point): (Time, Time),
    ) -> Option<Trim> {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, color::PANEL);
        let track = rect.shrink2(vec2(10.0, 6.0));
        let total = info.duration.as_seconds().max(1e-6);
        let x = |t: Time| track.left() + (t.as_seconds() / total) as f32 * track.width();
        let frame = info.frame_duration().as_seconds().max(1e-9);
        let time_at = |px: f32| {
            let f = f64::from(((px - track.left()) / track.width()).clamp(0.0, 1.0));
            Time::from_seconds((f * total / frame).round() * frame)
        };
        let (a, b) = match self.trim {
            Some((true, t)) => (t, out_point),
            Some((false, t)) => (in_point, t),
            None => (in_point, out_point),
        };
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        painter.rect_filled(track, 2.0, Color32::from_gray(55));
        painter.rect_filled(
            Rect::from_min_max(pos2(x(a), track.top()), pos2(x(b), track.bottom())),
            2.0,
            Color32::from_gray(110),
        );
        // The ends: brackets the pointer can grab.
        for t in [a, b] {
            painter.line_segment(
                [
                    pos2(x(t), track.top() - 2.0),
                    pos2(x(t), track.bottom() + 2.0),
                ],
                Stroke::new(2.0, Color32::from_gray(170)),
            );
        }
        let cti = x(time);
        painter.line_segment(
            [pos2(cti, rect.top()), pos2(cti, rect.bottom())],
            Stroke::new(1.5, color::ACCENT),
        );
        let resp = ui.interact(rect, ui.id().with("layer_ruler"), Sense::click_and_drag());
        let near = |p: egui::Pos2, t: Time| (p.x - x(t)).abs() <= END_PICK;
        if let Some(p) = resp.hover_pos()
            && (self.trim.is_some() || near(p, in_point) || near(p, out_point))
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        // Where the press began (egui reports the drag a few points later).
        if resp.drag_started()
            && let Some(p) = ui.input(|i| i.pointer.press_origin())
        {
            if near(p, in_point) {
                self.trim = Some((true, in_point));
            } else if near(p, out_point) {
                self.trim = Some((false, out_point));
            }
        }
        if let Some((is_in, _)) = self.trim {
            if let Some(p) = resp.interact_pointer_pos() {
                self.trim = Some((is_in, time_at(p.x)));
            }
            if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
                let edit = self
                    .trim
                    .take()
                    .map(|(is_in, t)| if is_in { Trim::In(t) } else { Trim::Out(t) });
                return edit;
            }
            return None;
        }
        if (resp.clicked() || resp.dragged())
            && let Some(p) = resp.interact_pointer_pos()
        {
            let mut s = session.lock();
            s.stop();
            s.set_time(info, time_at(p.x));
        }
        None
    }
}

/// Registers this module's panel (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    crate::register_panel(
        app,
        "layer",
        bae_ui::PanelKind::new("Layer", |spawn| Box::new(LayerPanel::new(spawn.state))),
    );
}

/// Chinese (AE's zh_CN dictionary).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Set In Point to Current Time", "将入点设置为当前时间"),
    ("Set Out Point to Current Time", "将出点设置为当前时间"),
];
