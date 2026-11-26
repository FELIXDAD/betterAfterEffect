use super::*;
use bae_anim::{EvaluatedEffect, EvaluatedLayer};
use bae_effects::effects::liquify::{self as lq, Brush, BrushStroke, Mesh};

#[derive(Default)]
pub(super) struct Editor {
    stroke: Option<Gesture>,
    clone: Option<(PrimPath, DVec2)>,
}

struct Gesture {
    gpu: BrushStroke,
    effect: PrimPath,
    time: Time,
    start: DVec2,
    last: DVec2,
    latest: Option<Value>,
    released: bool,
}

impl Editor {
    pub(super) fn update(&mut self, ui: &Ui, cx: &mut PanelContext<'_>, session: &SharedSession) {
        let Some(g) = self.stroke.as_mut() else {
            return;
        };
        g.released |= !ui.input(|i| i.pointer.primary_down());
        let attr = bae_model::effect::param_attr(lq::MESH);
        match g.gpu.poll() {
            Ok(Some((mesh, current))) => {
                if current {
                    g.latest = Some(mesh.clone());
                }
                if !(current && g.released) {
                    common::apply_edit(
                        cx,
                        session,
                        &g.effect,
                        &attr,
                        g.time,
                        common::Edit::Live(mesh),
                    );
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::error!(%error, "Liquify mesh readback failed");
                session.lock().scrub = None;
                self.stroke = None;
                return;
            }
        }
        if g.released
            && let Some(value) = g.latest.take()
        {
            common::apply_edit(
                cx,
                session,
                &g.effect,
                &attr,
                g.time,
                common::Edit::Commit(value),
            );
            self.stroke = None;
        } else {
            ui.ctx().request_repaint();
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
        let mesh = || {
            fx.param(lq::MESH).and_then(Mesh::from_value).or_else(|| {
                let (lo, hi) = render::layer_bounds(layer)?;
                Some(Mesh::new(lo, hi - lo))
            })
        };
        let painter = ui.painter_at(area);
        if flag(10)
            && let Some(m) = mesh()
        {
            let color = [
                Color32::RED,
                Color32::YELLOW,
                Color32::GREEN,
                Color32::CYAN,
                Color32::BLUE,
                Color32::MAGENTA,
                Color32::GRAY,
            ][(num(12, 7.) as usize).clamp(1, 7) - 1];
            let stride = 1 << (num(11, 2.) as usize).clamp(1, 3);
            let pct = num(lq::PERCENT, 100.) / 100.;
            let pos = |x: usize, y: usize| {
                view.to_screen(
                    m.origin + offset - min + m.cell * DVec2::new(x as f64, y as f64)
                        - m.offsets[y * m.cols + x] * pct,
                )
            };
            let stroke = Stroke::new(1., color);
            for y in (0..m.rows).step_by(stride) {
                painter.add(egui::Shape::line(
                    (0..m.cols).map(|x| pos(x, y)).collect(),
                    stroke,
                ));
            }
            for x in (0..m.cols).step_by(stride) {
                painter.add(egui::Shape::line(
                    (0..m.rows).map(|y| pos(x, y)).collect(),
                    stroke,
                ));
            }
        }
        let resp = ui
            .interact(area, ui.id().with("liquify"), Sense::drag())
            .labeled(egui::WidgetType::Other, tr("Liquify"));
        let tool = session.lock().liquify_tool;
        let radius = num(lq::BRUSH_SIZE, 64.) * 0.5;
        let at = |pos| view.to_comp(pos) + min - offset;
        if let Some(pos) = resp.hover_pos() {
            painter.circle_stroke(
                pos,
                (radius as f32 * view.scale).max(1.),
                Stroke::new(1., Color32::WHITE),
            );
        }
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = ui.input(|i| i.pointer.press_origin())
        {
            let start = at(pos);
            if tool == 8 && ui.input(|i| i.modifiers.alt) {
                self.clone = Some((fx.path.clone(), start));
                return;
            }
            if self.stroke.is_some()
                || (tool == 8 && !self.clone.as_ref().is_some_and(|(p, _)| p == &fx.path))
            {
                return;
            }
            let Some(device) = cx.resources.get::<std::sync::Arc<bae_gpu::GpuContext>>() else {
                return;
            };
            let Some(m) = mesh() else { return };
            let frozen = fx
                .param(7)
                .and_then(Value::as_prim_ref)
                .and_then(|path| {
                    layer
                        .masks
                        .iter()
                        .find(|m| m.path == *path && m.shape.closed)
                })
                .map(|m| {
                    lq::unfrozen_weight(&m.shape, m.opacity, m.feather, m.expansion, m.inverted)
                });
            let gpu = BrushStroke::new(device, m, |point| {
                frozen.as_ref().map_or(1., |weight| weight(point + offset))
            });
            match gpu {
                Ok(gpu) => {
                    self.stroke = Some(Gesture {
                        gpu,
                        effect: fx.path.clone(),
                        time,
                        start,
                        last: start,
                        latest: None,
                        released: false,
                    })
                }
                Err(e) => {
                    tracing::warn!(%e, "Liquify brush unavailable");
                    return;
                }
            }
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
            if drag != DVec2::ZERO || !matches!(tool, 0 | 6 | 7) || resp.drag_started() {
                g.latest = None;
                g.gpu.dab(&Brush {
                    tool: tool as u32,
                    center,
                    drag,
                    radius,
                    pressure: num(lq::BRUSH_PRESSURE, 50.) / 100.,
                    jitter: num(5, 70.) / 100.,
                    source,
                    reconstruction: (num(6, 1.) as u32).saturating_sub(1),
                });
                g.latest = None;
                g.last = center;
            }
        }
        ui.ctx().request_repaint();
    }
}
