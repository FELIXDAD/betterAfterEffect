use super::*;
use crate::render::View;
use bae_anim::glam::DVec2;
use bae_model::Time;
use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind, vec2};
#[derive(Clone, Copy)]
enum Handle {
    Center,
    Attach,
    Search,
    Size { outer: bool, x: f64, y: f64 },
}
#[derive(Clone, Copy)]
struct Geometry {
    center: DVec2,
    feature: DVec2,
    offset: DVec2,
    search: DVec2,
    attach: DVec2,
}
impl Geometry {
    fn moved(mut self, h: Handle, d: DVec2) -> Self {
        match h {
            Handle::Center => {
                self.center += d;
                self.attach += d;
            }
            Handle::Attach => self.attach += d,
            Handle::Search => self.offset += d,
            Handle::Size { outer, x, y } => {
                let change = DVec2::new(2. * x * d.x, 2. * y * d.y);
                if outer {
                    self.search = (self.search + change).max(self.feature + 2. * self.offset.abs());
                } else {
                    self.feature = (self.feature + change)
                        .max(DVec2::splat(2.))
                        .min((self.search - 2. * self.offset.abs()).max(DVec2::splat(2.)));
                }
            }
        }
        if matches!(h, Handle::Search) {
            let limit = ((self.search - self.feature) * 0.5).max(DVec2::ZERO);
            self.offset = self.offset.clamp(-limit, limit);
        }
        self
    }
    fn edits(self, h: Handle) -> Vec<(&'static str, DVec2)> {
        match h {
            Handle::Center => vec![
                ("feature_center", self.center),
                ("attach_point", self.attach),
            ],
            Handle::Attach => vec![
                ("attach_point", self.attach),
                ("attach_offset", self.attach - self.center),
            ],
            Handle::Search => vec![("search_offset", self.offset)],
            Handle::Size { outer: true, .. } => vec![("search_size", self.search)],
            Handle::Size { outer: false, .. } => vec![("feature_size", self.feature)],
        }
    }
}
struct Drag {
    point: PrimPath,
    handle: Handle,
    start: Pos2,
    base: Geometry,
    time: Time,
}
#[derive(Default)]
pub(crate) struct Overlay {
    drag: Option<Drag>,
}
impl Overlay {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &mut self,
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        area: Rect,
        view: &View,
        min: DVec2,
        layer: &PrimPath,
        magnify: impl Fn(&egui::Painter, Rect, DVec2, f32),
    ) {
        let Some(sh) = cx.resources.get::<Shared>() else {
            self.drag = None;
            return;
        };
        let s = sh.lock().unwrap();
        let Some(track) = s
            .current
            .as_ref()
            .filter(|p| query::owning_layer(cx.stage, p).as_ref() == Some(layer))
        else {
            self.drag = None;
            return;
        };
        let time = common::session(cx.resources).lock().active_time();
        let painter = ui.painter_at(area);
        let screen = |p: DVec2| view.to_screen(p - min);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let movable = s.job.is_none() && !query::is_locked(cx.stage, layer);
        let mut hit = None;
        for (i, pt) in model::points(cx.stage, track).iter().enumerate() {
            let at =
                |key| DVec2::from(model::vector_at(cx.stage, pt, key, time).unwrap_or_default());
            let mut g = Geometry {
                center: at("feature_center"),
                feature: at("feature_size"),
                offset: at("search_offset"),
                search: at("search_size"),
                attach: at("attach_point"),
            };
            let stroke = Stroke::new(1., Color32::YELLOW);
            if s.display.paths {
                let mut path = Vec::new();
                if let Some(a) = query::attr_spec(cx.stage, pt, "feature_center") {
                    for k in a.keyframes.values() {
                        if let Value::Vec2(x) = k.value {
                            path.push(screen(x.into()));
                        }
                    }
                }
                if path.len() > 1 {
                    painter.add(egui::Shape::line(path, stroke));
                }
                let mut live = Vec::new();
                for f in &s.live {
                    if let Some(v) = f.points.get(i) {
                        live.push(screen(v.center.into()));
                    }
                }
                if live.len() > 1 {
                    painter.add(egui::Shape::line(live, stroke));
                }
            }
            if let Some(v) = s.live.last().and_then(|f| f.points.get(i)) {
                g.center = v.center.into();
                g.attach = v.attach.map(DVec2::from).unwrap_or(g.center);
            }
            if let Some(d) = &self.drag
                && d.point == *pt
                && let Some(p) = pointer
            {
                let delta = (p - d.start) / view.scale;
                g = d
                    .base
                    .moved(d.handle, DVec2::new(f64::from(delta.x), f64::from(delta.y)));
            }
            let center = screen(g.center);
            if s.display.magnify && self.drag.as_ref().is_some_and(|drag| drag.point == *pt) {
                let scale = 4.0 / ui.ctx().pixels_per_point();
                let size = vec2(g.feature.x as f32, g.feature.y as f32) * scale;
                let lens = Rect::from_center_size(center, size);
                if lens.intersects(area) {
                    magnify(
                        &painter.with_clip_rect(area.intersect(lens)),
                        lens,
                        g.center - min,
                        scale,
                    );
                    painter.rect_stroke(lens, 0., stroke, StrokeKind::Inside);
                }
            }
            let attach = screen(g.attach);
            for outer in [true, false] {
                let c = if outer { g.center + g.offset } else { g.center };
                let size = if outer { g.search } else { g.feature };
                let rect = Rect::from_min_max(screen(c - size * 0.5), screen(c + size * 0.5));
                painter.rect_stroke(rect, 0., stroke, StrokeKind::Middle);
                if pointer.is_some_and(|p| {
                    rect.expand(3.).contains(p) && (!outer || !rect.shrink(4.).contains(p))
                }) {
                    hit = Some((
                        pt.clone(),
                        if outer {
                            Handle::Search
                        } else {
                            Handle::Center
                        },
                        g,
                    ));
                }
                for (x, y) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
                    let p = screen(c + size * 0.5 * DVec2::new(x, y));
                    painter.rect_filled(Rect::from_center_size(p, vec2(3., 3.)), 0., stroke.color);
                    if pointer.is_some_and(|m| m.distance(p) < 4.) {
                        hit = Some((pt.clone(), Handle::Size { outer, x, y }, g));
                    }
                }
            }
            painter.circle_stroke(center, 2., stroke);
            painter.line_segment([attach - vec2(5., 0.), attach + vec2(5., 0.)], stroke);
            painter.line_segment([attach - vec2(0., 5.), attach + vec2(0., 5.)], stroke);
            if pointer.is_some_and(|p| p.distance(attach) < 3.5) {
                hit = Some((pt.clone(), Handle::Attach, g));
            }
        }
        if !movable {
            self.drag = None;
            return;
        }
        if hit.is_none() && self.drag.is_none() {
            return;
        }
        let resp = ui.interact(area, ui.id().with("track_point_drag"), Sense::drag());
        if resp.is_pointer_button_down_on()
            && ui.input(|i| i.pointer.primary_pressed())
            && let Some((point, handle, base)) = hit
            && let Some(start) = pointer
        {
            self.drag = Some(Drag {
                point,
                handle,
                base,
                start,
                time,
            });
        }
        if ui.input(|i| i.pointer.primary_released())
            && let Some(d) = self.drag.take()
            && let Some(p) = pointer
            && p != d.start
        {
            let delta = (p - d.start) / view.scale;
            let new = d
                .base
                .moved(d.handle, DVec2::new(f64::from(delta.x), f64::from(delta.y)));
            let time = query::to_key_time(cx.stage, &d.point, d.time);
            let edits:Vec<_>=new.edits(d.handle).into_iter().map(|(attr,v)|json!({"path":d.point,"attr":attr,"value":Value::Vec2(v.to_array()),"time":time})).collect();
            cx.invoke_with("attr.set_many", json!({"edits":edits}));
        }
    }
}
