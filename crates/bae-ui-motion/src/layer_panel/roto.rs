use super::*;
use crate::tools::Tool;
use crate::vision_bake::{self, Job};
use bae_anim::{EvaluatedEffect, EvaluatedLayer};
use bae_vision::{FROZEN, ROTO, SPAN, STROKES};
use serde_json::json;

struct StrokeInput {
    layer: PrimPath,
    time: Time,
    radius: f64,
    background: bool,
    refine: bool,
    points: Vec<[f64; 2]>,
}

#[derive(Default)]
pub(super) struct Brush {
    stroke: Option<StrokeInput>,
    ring: Option<(egui::Pos2, f32, Color32)>,
    dragging: Option<(PrimPath, usize)>,
    job: Option<Job>,
    message: String,
}
impl Brush {
    pub fn cancel_stroke(&mut self) {
        self.stroke = None;
        self.ring = None;
    }
    pub fn cursor(&self, ui: &Ui, area: Rect) {
        if let Some((p, r, c)) = self.ring {
            ui.painter_at(area).circle_stroke(p, r, Stroke::new(1.5, c));
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &mut self,
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        area: Rect,
        view: &View,
        min: DVec2,
        layer: &EvaluatedLayer,
        preview: &mut EvaluatedLayer,
        tool: Tool,
    ) {
        let fx = layer.effects.iter().find(|e| e.match_name == ROTO);
        let refine = tool == Tool::RefineEdge;
        if (refine && fx.is_none())
            || fx.is_some_and(|e| e.param(FROZEN).and_then(Value::as_prim_ref).is_some())
        {
            self.cancel_stroke();
            return;
        }
        if self.stroke.as_ref().is_some_and(|s| {
            s.layer != layer.path || s.time != layer.layer_time || s.refine != refine
        }) {
            self.cancel_stroke();
        }
        let radius = crate::tools::shared(cx.resources)
            .lock()
            .options
            .paint
            .diameter
            .max(1.)
            * 0.5;
        let response = ui
            .interact(
                area,
                ui.id().with(("roto", layer.path.as_str())),
                Sense::click_and_drag(),
            )
            .labeled(
                egui::WidgetType::Other,
                tr(if refine {
                    "Refine Edge Canvas"
                } else {
                    "Roto Brush Canvas"
                }),
            );
        let bg = ui.input(|i| i.modifiers.alt);
        self.ring = response.hover_pos().map(|p| {
            (
                p,
                (radius as f32 * view.scale).max(1.),
                if refine {
                    Color32::LIGHT_BLUE
                } else if bg {
                    Color32::RED
                } else {
                    Color32::GREEN
                },
            )
        });
        let loc = |p| (view.to_comp(p) + min).to_array();
        if (response.drag_started_by(egui::PointerButton::Primary)
            || response.clicked_by(egui::PointerButton::Primary))
            && let Some(p) = ui
                .input(|i| i.pointer.press_origin())
                .or_else(|| response.interact_pointer_pos())
            && (!bg || fx.is_some())
        {
            self.stroke = Some(StrokeInput {
                layer: layer.path.clone(),
                time: layer.layer_time,
                radius,
                background: bg,
                refine,
                points: vec![loc(p)],
            });
        }
        let Some(s) = self.stroke.as_mut() else {
            return;
        };
        if let Some(p) = response.interact_pointer_pos() {
            let next = loc(p);
            if s.points
                .last()
                .is_none_or(|q| DVec2::from(*q).distance(DVec2::from(next)) >= 1.)
            {
                s.points.push(next);
            }
        }
        if ui.input(|i| i.pointer.primary_down()) {
            let index = preview
                .effects
                .iter()
                .position(|e| e.match_name == ROTO)
                .unwrap_or_else(|| {
                    let path = layer
                        .path
                        .child("effects")
                        .and_then(|p| p.child("RotoBrush"))
                        .expect("valid effect path");
                    preview.effects.push(EvaluatedEffect {
                        path,
                        match_name: ROTO.into(),
                        enabled: true,
                        params: Default::default(),
                        children: Vec::new(),
                    });
                    preview.effects.len() - 1
                });
            let value = preview.effects[index]
                .params
                .entry(format!(
                    "{:04}",
                    if s.refine {
                        bae_vision::EDGE_STROKES
                    } else {
                        STROKES
                    }
                ))
                .or_insert_with(|| Value::FloatList(Vec::new()));
            if let Value::FloatList(data) = value {
                bae_vision::append_stroke(
                    data,
                    s.time,
                    if s.background { -s.radius } else { s.radius },
                    &s.points,
                );
            }
            ui.ctx().request_repaint();
        } else {
            cx.invoke_with(
                "roto.stroke",
                json!({
                    "layer": s.layer, "time": s.time, "radius": s.radius,
                    "background": s.background, "refine": s.refine, "points": s.points
                }),
            );
            self.stroke = None;
        }
    }
    pub fn poll(&mut self, cx: &mut PanelContext<'_>) {
        if let Some(job) = &mut self.job
            && let Some(result) = job.poll(cx)
        {
            if let Err(e) = result {
                self.message = e;
            }
            self.job = None;
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn span(
        &mut self,
        ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        rect: Rect,
        layer: &EvaluatedLayer,
        fx: &EvaluatedEffect,
        info: &CompInfo,
        time: Time,
    ) {
        let timing = query::layer_timing(cx.stage, &layer.path);
        let t0 = timing.layer_time(timing.in_point);
        let t1 = timing.layer_time(timing.out_point);
        let lo = t0.min(t1);
        let hi = t0.max(t1);
        let span = bae_vision::span(fx.param(SPAN)).unwrap_or([lo, hi]);
        let frozen = fx.param(FROZEN).and_then(Value::as_prim_ref).is_some();
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        if let Some(job) = &self.job {
            child.label(format!("{} / {}", job.done, job.frames));
        } else if child
            .button(tr(if frozen { "Unfreeze" } else { "Freeze" }))
            .clicked()
        {
            self.message.clear();
            if frozen {
                cx.invoke_with("roto.freeze", json!({"effect":fx.path}));
            } else {
                let [a, b] = span.map(|t| timing.comp_time(t));
                match vision_bake::start(
                    cx,
                    ui.ctx(),
                    layer.path.clone(),
                    vision_bake::Mode::Freeze(fx.path.clone()),
                    [a.min(b), a.max(b)],
                ) {
                    Ok(job) => self.job = Some(job),
                    Err(e) => self.message = e,
                }
            }
        }
        if !self.message.is_empty() {
            child.label(&self.message);
        }
        child.label(tr("Propagation Span"));
        let (r, response) = child.allocate_exact_size(
            vec2(child.available_width().max(1.), 18.),
            Sense::click_and_drag(),
        );
        let r = r.shrink2(vec2(4., 4.));
        let width = (hi.0 - lo.0).max(1) as f64;
        let x = |t: Time| r.left() + (t.0 - lo.0) as f32 / width as f32 * r.width();
        let paint = child.painter();
        paint.rect_filled(r, 1., color::PANEL);
        paint.rect_filled(
            Rect::from_min_max(pos2(x(span[0]), r.top()), pos2(x(span[1]), r.bottom())),
            1.,
            if frozen {
                Color32::GRAY
            } else {
                Color32::DARK_GREEN
            },
        );
        for s in bae_vision::segments(fx.param(STROKES)).chunk_by(|a, b| a[0] == b[0]) {
            let x = x(Time(s[0][0] as i64));
            paint.vline(x, r.y_range(), Stroke::new(2., Color32::YELLOW));
        }
        paint.vline(
            x(layer.layer_time),
            r.y_range(),
            Stroke::new(1.5, color::ACCENT),
        );
        if frozen {
            return;
        }
        if response.drag_started()
            && let Some(p) = ui.input(|i| i.pointer.press_origin())
        {
            self.dragging = Some((
                fx.path.clone(),
                usize::from((p.x - x(span[1])).abs() < (p.x - x(span[0])).abs()),
            ));
        }
        if let Some((path, end)) = &self.dragging
            && *path == fx.path
            && let Some(p) = response.interact_pointer_pos()
        {
            let at = Time(
                lo.0 + (width * f64::from(((p.x - r.left()) / r.width().max(1.)).clamp(0., 1.)))
                    as i64,
            );
            let at = timing
                .layer_time(info.frame_rate.snap(timing.comp_time(at)))
                .clamp(lo, hi);
            let mut v = span;
            v[*end] = if *end == 0 {
                at.min(span[1])
            } else {
                at.max(span[0])
            };
            let value = Value::Vec2(v.map(|t| t.0 as f64));
            let edit = if response.drag_stopped() {
                self.dragging = None;
                common::Edit::Commit(value)
            } else {
                common::Edit::Live(value)
            };
            common::apply_edit(
                cx,
                session,
                &fx.path,
                &bae_model::effect::param_attr(SPAN),
                time,
                edit,
            );
            ui.ctx().request_repaint();
        }
    }
}
