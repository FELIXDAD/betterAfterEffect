use super::*;
use bae_anim::{EvaluatedEffect, EvaluatedLayer};
use bae_effects::effects::liquify::{self as lq, Brush, BrushStroke, Mesh};

#[derive(Default)]
pub(super) struct Editor {
    stroke: Option<Gesture>,
    prepared: Option<(PrimPath, Time, u64, Preparation)>,
    clone: Option<(PrimPath, DVec2)>,
}

type Preparation = std::thread::JoinHandle<Result<BrushStroke, String>>;

struct Gesture {
    gpu: Option<BrushStroke>,
    pending: Option<Preparation>,
    dabs: Vec<Brush>,
    effect: PrimPath,
    time: Time,
    auto_key: bool,
    start: DVec2,
    last: DVec2,
    released: bool,
}

impl Editor {
    pub(super) fn update(&mut self, ui: &Ui, cx: &mut PanelContext<'_>, session: &SharedSession) {
        let Some(g) = self.stroke.as_mut() else {
            return;
        };
        g.released |= !ui.input(|i| i.pointer.primary_down());
        if let Some(job) = &g.pending {
            if !job.is_finished() {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(8));
                return;
            }
            match g.pending.take().unwrap().join() {
                Ok(Ok(mut gpu)) => {
                    for dab in g.dabs.drain(..) {
                        gpu.dab(&dab);
                    }
                    g.gpu = Some(gpu);
                }
                _ => {
                    tracing::warn!("Liquify brush preparation failed");
                    self.stroke = None;
                    return;
                }
            }
            ui.ctx().request_repaint();
        }
        let Some(gpu) = g.gpu.as_mut() else {
            return;
        };
        let attr = bae_model::effect::param_attr(lq::MESH);
        let live = gpu.preview_value();
        if let Some(value) = live.as_ref() {
            let mut s = session.lock();
            if s.scrub
                .as_ref()
                .is_none_or(|old| old.path != g.effect || old.value != *value)
            {
                s.scrub = Some(crate::session::Scrub {
                    auto_key: g.auto_key,
                    path: g.effect.clone(),
                    time: g.time,
                    value: value.clone(),
                    attr: attr.clone(),
                    peers: Vec::new(),
                    committed: None,
                });
            }
        }
        if !g.released {
            return;
        }
        if live.is_none() {
            self.stroke = None;
            return;
        }
        let clear = || {
            let mut session = session.lock();
            if session.scrub.as_ref().is_some_and(|s| {
                s.path == g.effect && s.attr == attr && Some(&s.value) == live.as_ref()
            }) {
                session.scrub = None;
            }
        };
        match gpu.committed() {
            Ok(None) => {
                ui.ctx().request_repaint();
            }
            Ok(Some(value)) => {
                clear();
                let time = query::to_key_time(cx.stage, &g.effect, g.time);
                cx.invoke_with(
                    "attr.set_at",
                    serde_json::json!({
                        "path":g.effect, "attr":bae_model::effect::param_attr(lq::MESH),
                        "time": time, "value":value, "auto_key":g.auto_key,
                    }),
                );
                self.stroke = None;
            }
            Err(err) => {
                clear();
                tracing::error!(%err, "Liquify commit failed");
                self.stroke = None;
            }
        }
    }

    /// Grid lines are local to the layer viewer.
    pub(super) fn preview_overlay(effect: &mut EvaluatedEffect) {
        if effect.match_name != lq::MATCH_NAME {
            return;
        }
        let number = |id, default| effect.param(id).and_then(Value::as_f64).unwrap_or(default);
        let stride = if effect.param(10).and_then(Value::as_bool).unwrap_or(false) {
            1 << (number(11, 2.) as u32).clamp(1, 3)
        } else {
            0
        };
        let color = (number(12, 7.) as u32).clamp(1, 7) - 1;
        if stride != 0 {
            effect.params.insert(
                lq::VIEW_GRID.to_string(),
                Value::Int(i64::from(stride * 8 + color)),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn show(
        &mut self,
        ui: &Ui,
        cx: &PanelContext<'_>,
        session: &SharedSession,
        area: Rect,
        view: &View,
        min: DVec2,
        layer: &EvaluatedLayer,
        fx: &EvaluatedEffect,
        time: Time,
    ) {
        let num = |id, default| fx.param(id).and_then(Value::as_f64).unwrap_or(default);
        let flag = |id| fx.param(id).and_then(Value::as_bool).unwrap_or(false);
        let offset = match fx.param(lq::MESH_OFFSET) {
            Some(Value::Vec2(v)) => DVec2::from_array(*v),
            _ => DVec2::ZERO,
        };
        let painter = ui.painter_at(area);
        let resp = ui
            .interact(area, ui.id().with("liquify"), Sense::drag())
            .labeled(egui::WidgetType::Other, tr("Liquify"));
        let tool = session.lock().liquify_tool;
        let radius = num(lq::BRUSH_SIZE, 64.) * 0.5;
        let at = |pos| view.to_comp(pos) + min - offset;
        // A selected effect prepares one mesh in the background. When the user
        // drags immediately, the gesture keeps its samples until that mesh is ready.
        let revision = cx.stage.revision();
        if session.lock().playback.is_none()
            && self.stroke.is_none()
            && !self
                .prepared
                .as_ref()
                .is_some_and(|(e, t, v, _)| *e == fx.path && *t == time && *v == revision)
        {
            let running = self
                .prepared
                .as_ref()
                .is_some_and(|(_, _, _, j)| !j.is_finished());
            if (!running || resp.drag_started())
                && let Some(device) = cx.resources.get::<std::sync::Arc<bae_gpu::GpuContext>>()
                && let Some((lo, hi)) = render::layer_bounds(layer)
            {
                let device = device.clone();
                let value = fx.param(lq::MESH).cloned();
                let mask = fx
                    .param(7)
                    .and_then(Value::as_prim_ref)
                    .and_then(|p| layer.masks.iter().find(|m| m.path == *p && m.shape.closed))
                    .cloned();
                let context = ui.ctx().clone();
                let job = std::thread::spawn(move || {
                    let mesh = value
                        .as_ref()
                        .and_then(Mesh::from_value)
                        .unwrap_or_else(|| Mesh::new(lo, hi - lo));
                    let frozen = mask.as_ref().map(|m| {
                        lq::unfrozen_weight(&m.shape, m.opacity, m.feather, m.expansion, m.inverted)
                    });
                    let result = BrushStroke::new(&device, mesh, |p| {
                        frozen.as_ref().map_or(1., |weight| weight(p + offset))
                    })
                    .map_err(|e| e.to_string());
                    context.request_repaint();
                    result
                });
                self.prepared = Some((fx.path.clone(), time, revision, job));
            }
        }
        if let Some(pos) = resp.hover_pos() {
            let radius = (radius as f32 * view.scale).max(1.);
            painter.add(egui::Shape::ellipse_stroke(
                pos,
                egui::vec2(radius * view.aspect, radius),
                Stroke::new(1., Color32::WHITE),
            ));
        }
        if tool == 8
            && let Some(p) = ui.input(|i| {
                (i.pointer.primary_pressed() && i.modifiers.alt)
                    .then(|| i.pointer.press_origin())
                    .flatten()
                    .filter(|p| area.contains(*p))
            })
        {
            self.clone = Some((fx.path.clone(), at(p)));
        }
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = ui.input(|i| i.pointer.press_origin())
        {
            let start = at(pos);
            if tool == 8 && ui.input(|i| i.modifiers.alt) {
                return;
            }
            if self.stroke.is_some()
                || (tool == 8 && !self.clone.as_ref().is_some_and(|(p, _)| p == &fx.path))
            {
                return;
            }
            let Some(pending) = self.prepared.take() else {
                return;
            };
            if pending.0 != fx.path || pending.1 != time || pending.2 != revision {
                return;
            }
            self.stroke = Some(Gesture {
                gpu: None,
                pending: Some(pending.3),
                dabs: Vec::new(),
                effect: fx.path.clone(),
                time,
                auto_key: session.lock().auto_key,
                start,
                last: start,
                released: false,
            });
        }
        let Some(g) = &mut self.stroke else { return };
        if g.released || g.effect != fx.path || g.time != time {
            return;
        }
        if let Some(pos) = resp.interact_pointer_pos() {
            let center = at(pos);
            let drag = center - g.last;
            let source = if tool == 8 {
                self.clone.as_ref().map_or(g.start, |(_, p)| *p)
                    + if flag(17) {
                        center - g.start
                    } else {
                        DVec2::ZERO
                    }
            } else {
                g.start
            };
            if drag != DVec2::ZERO || !matches!(tool, 0 | 6 | 7) {
                let dab = Brush {
                    tool: tool as u32,
                    center,
                    drag,
                    radius,
                    pressure: num(lq::BRUSH_PRESSURE, 50.) / 100.,
                    jitter: num(5, 70.) / 100.,
                    source,
                    reconstruction: (num(6, 1.) as u32).saturating_sub(1),
                };
                if let Some(gpu) = &mut g.gpu {
                    gpu.dab(&dab);
                } else {
                    g.dabs.push(dab);
                }
                g.last = center;
            }
        }
        ui.ctx().request_repaint();
    }
}
