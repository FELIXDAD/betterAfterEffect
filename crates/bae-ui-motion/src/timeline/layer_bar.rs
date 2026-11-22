//! Layer duration bars: move and trim.

use super::*;

impl TimelinePanel {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn layer_bar(
        &mut self,
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        axis: &TimeAxis,
        layer: &PrimPath,
        rect: Rect,
        selected: bool,
    ) {
        let stage = cx.stage;
        let timing = query::layer_timing(stage, layer);
        let (mut a, mut b) = (
            timing.in_point.min(timing.out_point),
            timing.in_point.max(timing.out_point),
        );
        // Live preview of a bar drag.
        let pointer_x = ui.input(|i| i.pointer.interact_pos()).map(|p| p.x);
        let frame = info.frame_duration();
        let snap = |d: f32| -> Time {
            let t = Time((f64::from(d) * axis.per_point()) as i64);
            Time((t.0 as f64 / frame.0 as f64).round() as i64 * frame.0)
        };
        match (&self.drag, pointer_x) {
            (Some(Drag::Move { origin, layers }), Some(x)) if layers.contains(layer) => {
                let d = snap(x - origin);
                a += d;
                b += d;
            }
            (Some(Drag::TrimIn { origin, layer: l }), Some(x)) if l == layer => {
                a = (a + snap(x - origin)).min(b - frame);
            }
            (Some(Drag::TrimOut { origin, layer: l }), Some(x)) if l == layer => {
                b = (b + snap(x - origin)).max(a + frame);
            }
            _ => {}
        }
        let graph = Rect::from_min_max(pos2(axis.left, rect.top()), rect.max);
        let bar = Rect::from_min_max(
            pos2(axis.x(a), rect.top() + 2.0),
            pos2(axis.x(b), rect.bottom() - 2.0),
        )
        .intersect(graph);
        if bar.width() <= 0.0 {
            return;
        }
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
                &bae_ui::a11y::context(&[&self.layer_ctx(stage, layer), tr("Duration Bar")]),
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
        // A locked layer's bar does not move or trim (AE).
        if resp.drag_started()
            && !query::is_locked(stage, layer)
            && let Some(p) = resp.interact_pointer_pos()
        {
            if !selected {
                select_layer(session, stage, layer, egui::Modifiers::NONE);
            }
            self.drag = Some(match hover_edge {
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
        if resp.drag_stopped() {
            let x = pointer_x.unwrap_or(0.0);
            match self.drag.take() {
                Some(Drag::Move { origin, layers }) => {
                    let d = snap(x - origin);
                    if d != Time::ZERO {
                        cx.invoke_with("layer.shift_time", json!({"layers": layers, "delta": d}));
                    }
                }
                Some(Drag::TrimIn { origin, layer }) => {
                    let d = snap(x - origin);
                    if d != Time::ZERO {
                        cx.invoke_with("layer.trim", json!({"layers": [layer], "in_point": a}));
                    }
                    let _ = origin;
                }
                Some(Drag::TrimOut { origin, layer }) => {
                    let d = snap(x - origin);
                    if d != Time::ZERO {
                        cx.invoke_with("layer.trim", json!({"layers": [layer], "out_point": b}));
                    }
                    let _ = origin;
                }
                other => self.drag = other,
            }
        }
    }
}
