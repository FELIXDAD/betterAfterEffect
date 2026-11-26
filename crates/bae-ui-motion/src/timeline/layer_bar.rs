//! Layer duration bars (AE-UI-066 / -068): drag the body to move the selected
//! layers, an end to trim; layers with a time-based source show their untrimmed
//! source as a pale bar, and dragging it slip-edits (Alt-dragging the bar moves it,
//! as in AE 26). Shift
//! snaps the moving edge to the CTI, the work area, the comp's ends, other layers'
//! In / Out points, keyframes and markers. One command on release.

use bae_motion::layer::slip_limit;
use bae_motion::markers::markers_of;

use super::*;

/// Snapping distance (points).
const SNAP_PX: f32 = 8.0;
/// The pale slip-edit bar.
const SOURCE_BAR: Color32 = Color32::from_rgba_premultiplied(70, 70, 70, 140);

/// Where a bar drag puts things this frame: the comp-time offset (whole frames,
/// snapped, or clamped for a slip) and the snap target's x.
#[derive(Clone, Copy, Debug)]
pub(super) struct BarPlan {
    pub(super) delta: Time,
    pub(super) snap_x: Option<f32>,
}

/// Snap targets for bar edges: everything except the dragged layers.
fn bar_targets(stage: &Stage, info: &CompInfo, now: Time, exclude: &[PrimPath]) -> Vec<Time> {
    let mut out = vec![
        now,
        Time::ZERO,
        info.duration,
        info.work_area.start,
        info.work_area.end,
    ];
    for (m, t) in markers_of(stage, &info.path) {
        out.push(query::to_comp_time(stage, &m, t));
    }
    for l in query::comp_layers(stage, &info.path) {
        if exclude.contains(&l) {
            continue;
        }
        let t = query::layer_timing(stage, &l);
        out.push(t.in_point);
        out.push(t.out_point);
        out.extend(crate::actions::layer_key_times(stage, &l));
        for (m, t) in markers_of(stage, &l) {
            out.push(query::to_comp_time(stage, &m, t));
        }
    }
    out
}

impl TimelinePanel {
    /// The comp-time span a trim may reach: the untrimmed source of a
    /// time-based layer, unlimited once Time Remap is on (AE).
    fn trim_bounds(stage: &Stage, layer: &PrimPath) -> (Time, Time) {
        match source_bar(stage, layer) {
            Some((s, e)) => (s, e),
            None => (Time::MIN, Time::MAX),
        }
    }

    /// The bar drag's offset this frame (computed once, before the rows draw).
    pub(super) fn plan_bar_drag(
        &self,
        stage: &Stage,
        info: &CompInfo,
        axis: &TimeAxis,
        now: Time,
        pointer_x: Option<f32>,
        shift: bool,
    ) -> Option<BarPlan> {
        let timing = |l: &PrimPath| query::layer_timing(stage, l);
        let (origin, layers, edges): (f32, Vec<PrimPath>, Vec<Time>) = match &self.drag {
            Some(Drag::Move { origin, layers }) => (
                *origin,
                layers.clone(),
                layers
                    .iter()
                    .flat_map(|l| {
                        let t = timing(l);
                        [t.in_point, t.out_point]
                    })
                    .collect(),
            ),
            Some(Drag::TrimIn { origin, layer }) => {
                (*origin, vec![layer.clone()], vec![timing(layer).in_point])
            }
            Some(Drag::TrimOut { origin, layer }) => {
                (*origin, vec![layer.clone()], vec![timing(layer).out_point])
            }
            Some(Drag::Slip { origin, layer }) => (*origin, vec![layer.clone()], Vec::new()),
            _ => return None,
        };
        let frame = info.frame_duration();
        let raw = Time((f64::from(pointer_x.unwrap_or(origin) - origin) * axis.per_point()) as i64);
        let mut plan = BarPlan {
            delta: Time((raw.0 as f64 / frame.0 as f64).round() as i64 * frame.0),
            snap_x: None,
        };
        if shift && !edges.is_empty() {
            let targets = bar_targets(stage, info, now, &layers);
            let tol = Time((f64::from(SNAP_PX) * axis.per_point()) as i64);
            let mut best: Option<(i64, Time, Time)> = None;
            for e in &edges {
                if let Some(t) = crate::graph::snap_to(*e + raw, &targets, tol) {
                    let dist = (t - (*e + raw)).0.abs();
                    if best.is_none_or(|(d, _, _)| dist < d) {
                        best = Some((dist, t - *e, t));
                    }
                }
            }
            if let Some((_, d, t)) = best {
                plan.delta = d;
                plan.snap_x = Some(axis.x(t));
            }
        }
        if let Some(Drag::Slip { layer, .. }) = &self.drag {
            plan.delta = slip_limit(stage, layer, plan.delta);
        }
        Some(plan)
    }

    /// Release of a bar drag: one `layer.shift_time`, `layer.trim` or `layer.slip`.
    pub(super) fn commit_bar_drag(
        &self,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        drag: &Drag,
        plan: BarPlan,
    ) {
        let d = plan.delta;
        if d == Time::ZERO {
            return;
        }
        let stage = cx.stage;
        let frame = info.frame_duration();
        match drag {
            Drag::Move { layers, .. } => {
                cx.invoke_with("layer.shift_time", json!({"layers": layers, "delta": d}));
            }
            Drag::TrimIn { layer, .. } => {
                let t = query::layer_timing(stage, layer);
                let (a, b) = (t.in_point.min(t.out_point), t.in_point.max(t.out_point));
                let to = (a + d)
                    .min(b - frame)
                    .max(Self::trim_bounds(stage, layer).0);
                cx.invoke_with("layer.trim", json!({"layers": [layer], "in_point": to}));
            }
            Drag::TrimOut { layer, .. } => {
                let t = query::layer_timing(stage, layer);
                let (a, b) = (t.in_point.min(t.out_point), t.in_point.max(t.out_point));
                let to = (b + d)
                    .max(a + frame)
                    .min(Self::trim_bounds(stage, layer).1);
                cx.invoke_with("layer.trim", json!({"layers": [layer], "out_point": to}));
            }
            Drag::Slip { layer, .. } => {
                // Selected keyframes move with the source (AE).
                let keys: Vec<KeyRef> = session
                    .lock()
                    .selected_keys
                    .iter()
                    .filter(|k| query::owning_layer(stage, &k.path).as_ref() == Some(layer))
                    .cloned()
                    .collect();
                cx.invoke_with(
                    "layer.slip",
                    json!({"layers": [layer], "delta": d, "keys": keys}),
                );
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn layer_bar(
        &mut self,
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        axis: &TimeAxis,
        layer: &PrimPath,
        layer_ctx: &str,
        rect: Rect,
        selected: bool,
    ) {
        let stage = cx.stage;
        let timing = query::layer_timing(stage, layer);
        let (mut a, mut b) = (
            timing.in_point.min(timing.out_point),
            timing.in_point.max(timing.out_point),
        );
        let mut source = source_bar(stage, layer);
        let bounds = Self::trim_bounds(stage, layer);
        // Live preview of a bar drag.
        let frame = info.frame_duration();
        if let Some(plan) = self.bar_plan {
            let d = plan.delta;
            match &self.drag {
                Some(Drag::Move { layers, .. }) if layers.contains(layer) => {
                    a += d;
                    b += d;
                    source = source.map(|(s, e)| (s + d, e + d));
                }
                Some(Drag::TrimIn { layer: l, .. }) if l == layer => {
                    a = (a + d).min(b - frame).max(bounds.0)
                }
                Some(Drag::TrimOut { layer: l, .. }) if l == layer => {
                    b = (b + d).max(a + frame).min(bounds.1)
                }
                Some(Drag::Slip { layer: l, .. }) if l == layer => {
                    source = source.map(|(s, e)| (s + d, e + d));
                }
                _ => {}
            }
        }
        let graph = Rect::from_min_max(pos2(axis.left, rect.top()), rect.max);
        let bar = Rect::from_min_max(
            pos2(axis.x(a), rect.top() + 2.0),
            pos2(axis.x(b), rect.bottom() - 2.0),
        )
        .intersect(graph);
        let locked = query::is_locked(stage, layer);
        // The pale slip-edit bar over the untrimmed source (time-based sources).
        if let Some((s, e)) = source {
            let pale = Rect::from_min_max(
                pos2(axis.x(s), rect.top() + 4.0),
                pos2(axis.x(e), rect.bottom() - 4.0),
            )
            .intersect(graph);
            if pale.width() > 0.0 {
                ui.painter()
                    .with_clip_rect(graph)
                    .rect_filled(pale, 1.0, SOURCE_BAR);
                // The parts outside the bar take the slip drag.
                for (i, part) in [
                    Rect::from_min_max(pale.min, pos2(bar.left().min(pale.right()), pale.bottom())),
                    Rect::from_min_max(pos2(bar.right().max(pale.left()), pale.top()), pale.max),
                ]
                .into_iter()
                .enumerate()
                {
                    if part.width() <= 0.5 {
                        continue;
                    }
                    let resp = ui
                        .interact(
                            part,
                            ui.id().with(("slip", layer.as_str(), i)),
                            Sense::drag(),
                        )
                        .labeled(
                            egui::WidgetType::Other,
                            &bae_ui::a11y::context(&[layer_ctx, tr("Slip Edit")]),
                        );
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::Grab);
                    }
                    if resp.drag_started()
                        && !locked
                        && let Some(p) = ui
                            .input(|i| i.pointer.press_origin())
                            .or(resp.interact_pointer_pos())
                    {
                        if !selected {
                            select_layer(session, stage, layer, egui::Modifiers::NONE);
                        }
                        self.drag = Some(Drag::Slip {
                            origin: p.x,
                            layer: layer.clone(),
                        });
                    }
                }
            }
        }
        if bar.width() <= 0.0 {
            return;
        }
        #[cfg(test)]
        self.last_bars.push((layer.clone(), bar));
        let c = theme::layer_bar_color(label_of(stage, layer), selected);
        ui.painter().with_clip_rect(graph).rect_filled(bar, 1.0, c);
        let edge = 5.0;
        let resp = ui
            .interact(
                bar,
                ui.id().with(("bar", layer.as_str())),
                Sense::click_and_drag(),
            )
            .labeled_toggle(
                egui::WidgetType::Other,
                &bae_ui::a11y::context(&[layer_ctx, tr("Duration Bar")]),
                selected,
            );
        let hover_edge = resp.hover_pos().map(|p| {
            if (p.x - bar.left()).abs() <= edge {
                1
            } else if (p.x - bar.right()).abs() <= edge {
                2
            } else {
                0
            }
        });
        if matches!(hover_edge, Some(1 | 2)) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        if resp.clicked() {
            select_layer(session, stage, layer, ui.input(|i| i.modifiers));
        }
        if resp.double_clicked() {
            // A precomp opens its comp at the matching time, any other layer the
            // Layer panel (AE).
            if !self.open_precomp(cx, session, layer) {
                crate::layer_panel::open(cx, session, layer);
            }
        }
        // Right-click: the layer context menu (as on the layer name).
        let mut menu_out = None;
        crate::common::context_menu(&resp, &mut |ui| {
            if !session.lock().selected_layers.contains(layer) {
                select_layer(session, stage, layer, egui::Modifiers::NONE);
            }
            menu_out = menus::layer_menu(ui, cx, session, layer);
        });
        if menu_out == Some(menus::LayerMenuOut::Rename) {
            self.rename = Some((layer.clone(), query::display_name_of(stage, layer)));
        }
        // A locked layer's bar does not move or trim (AE). The drag starts where the
        // button went down.
        if resp.drag_started()
            && !locked
            && let Some(p) = ui
                .input(|i| i.pointer.press_origin())
                .or(resp.interact_pointer_pos())
        {
            let edge = if (p.x - bar.left()).abs() <= edge {
                Some(1)
            } else if (p.x - bar.right()).abs() <= edge {
                Some(2)
            } else {
                hover_edge
            };
            if !selected {
                select_layer(session, stage, layer, egui::Modifiers::NONE);
            }
            self.drag = Some(match edge {
                Some(1) => Drag::TrimIn {
                    origin: p.x,
                    layer: layer.clone(),
                },
                Some(2) => Drag::TrimOut {
                    origin: p.x,
                    layer: layer.clone(),
                },
                _ => Drag::Move {
                    origin: p.x,
                    layers: session.lock().selected_layers.clone(),
                },
            });
        }
    }
}

/// The pale slip-edit bar of a time-based layer (its untrimmed source in comp
/// time); none once Time Remap is on, when the layer can run past its source.
fn source_bar(stage: &Stage, layer: &PrimPath) -> Option<(Time, Time)> {
    if bool_attr(stage, layer, "time_remap_enabled") {
        return None;
    }
    query::source_span(stage, layer)
}
