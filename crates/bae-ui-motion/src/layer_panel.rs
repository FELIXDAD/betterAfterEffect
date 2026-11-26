//! Layer panel (AE's Layer viewer): one layer on its own, in its own space.
//! Double-clicking a layer's bar in the Timeline opens it (a pre-comp layer
//! opens its composition instead); Window ▸ Layer opens the selected layer.
//! The View popup chooses what the panel shows and edits: nothing, the masks,
//! or one effect. With Render on, the picture includes the effects up to the
//! chosen one; paint is drawn on the Paint effect. The time ruler under the
//! picture is the layer's: its in and out points (drag an end to trim the
//! layer; the { and } buttons set them to the current time) and the current
//! time, with AE's In, Out and Duration readouts.

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2};
use bae_model::{PrimPath, Time};
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
    eval: EvalCache,
    gpu: Option<GpuPreview>,
    gpu_tried: bool,
    pub(crate) show: Show,
    /// Render: the picture includes the effects up to the shown one.
    pub(crate) render: bool,
    /// An end of the layer being dragged on the ruler: the in point (`true`)
    /// or the out point, and where it would go.
    trim: Option<(bool, Time)>,
}

/// Double-clicking a layer (AE): a pre-comp layer opens its composition, any
/// other layer opens in the Layer panel.
pub(crate) fn open(cx: &mut PanelContext<'_>, session: &SharedSession, layer: &PrimPath) {
    let source = cx
        .stage
        .prim(layer)
        .and_then(|v| v.value("source").and_then(|s| s.as_prim_ref()).cloned())
        .filter(|s| query::comp_info(cx.stage, s).is_some());
    if let Some(comp) = source {
        session.lock().open_comp(&comp);
        cx.open_panel("timeline");
        cx.open_panel("composition");
    } else {
        session.lock().layer_panel = Some(layer.clone());
        cx.open_panel("layer");
    }
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
    fn tab(&self, cx: &PanelContext<'_>) -> TabInfo {
        let session = common::session(cx.resources);
        match shown_layer(&session) {
            Some(l) if cx.stage.prim(&l).is_some() => TabInfo::new(format!(
                "{} {}",
                tr("Layer"),
                query::display_name_of(cx.stage, &l)
            )),
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
        ui.painter().rect_filled(area, 0.0, Color32::from_gray(35));
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
        let eval = self.eval.get(cx.resources);
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
            self.picture(ui, cx, area, &one, min, evaluated);
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
        Some(serde_json::json!({ "render": self.render }))
    }
}

impl LayerPanel {
    pub fn new(state: Option<&serde_json::Value>) -> Self {
        LayerPanel {
            render: state
                .and_then(|s| s.get("render"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            ..LayerPanel::default()
        }
    }

    /// The layer's picture, fitted in `area`, and the masks in Masks view.
    fn picture(
        &mut self,
        ui: &Ui,
        cx: &PanelContext<'_>,
        area: Rect,
        one: &EvaluatedComp,
        min: DVec2,
        layer: &bae_anim::EvaluatedLayer,
    ) {
        let ppp = ui.ctx().pixels_per_point();
        let (w, h) = (one.width as f32, one.height as f32);
        let scale = ((area.width() - 24.0) / w)
            .min((area.height() - 24.0) / h)
            .clamp(0.01, 1.0 / ppp);
        let view = View {
            origin: area.center() - vec2(w, h) * scale / 2.0,
            scale,
            point: 6.0,
            camera: None,
        };
        let frame = render::comp_rect(&view, one.width, one.height);
        let painter = ui.painter_at(area);
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
        if self.show == Show::Masks {
            let to_panel = DMat4::from_translation((-min).extend(0.0));
            for m in &layer.masks {
                let stroke = Stroke::new(1.0, crate::viewer::mask_color(cx.stage, &m.path));
                painter.extend(crate::viewer::path_shapes(
                    &m.shape, &to_panel, &view, stroke,
                ));
            }
        }
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
