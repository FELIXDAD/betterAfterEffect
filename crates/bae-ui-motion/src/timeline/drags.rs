//! Drags that span rows: keyframes, marquee, pick whip, reorder.

use super::*;

impl TimelinePanel {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn finish_drags(
        &mut self,
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        axis: &TimeAxis,
        graph_body: Rect,
        key_rects: &[(Rect, KeyRef)],
        layer_rows: &[(Rect, PrimPath)],
        comp_layers: &[PrimPath],
    ) {
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let released = ui.input(|i| i.pointer.any_released());
        let frame = info.frame_duration();
        match self.drag.clone() {
            Some(Drag::Keys { origin, keys }) => {
                if released {
                    self.drag = None;
                    let x = pointer.map_or(origin, |p| p.x);
                    let d = Time((f64::from(x - origin) * axis.per_point()) as i64);
                    let d = Time((d.0 as f64 / frame.0 as f64).round() as i64 * frame.0);
                    if d != Time::ZERO {
                        // Keyframe time offset (stretch-aware) from the first key.
                        let first = &keys[0];
                        let ct = query::to_comp_time(cx.stage, &first.path, first.time);
                        let kd = query::to_key_time(cx.stage, &first.path, ct + d) - first.time;
                        cx.invoke_with("key.move", json!({"keys": keys, "delta": kd}));
                        session.lock().selected_keys = keys
                            .iter()
                            .map(|k| KeyRef::new(k.path.clone(), k.attr.clone(), k.time + kd))
                            .collect();
                    }
                }
            }
            Some(Drag::Marquee { origin, additive }) => {
                if let Some(p) = pointer {
                    let r = Rect::from_two_pos(origin, p).intersect(graph_body);
                    ui.painter().rect(
                        r,
                        0.0,
                        Color32::from_rgba_unmultiplied(88, 149, 237, 30),
                        Stroke::new(1.0, color::ACCENT),
                        egui::StrokeKind::Inside,
                    );
                    let inside: Vec<KeyRef> = key_rects
                        .iter()
                        .filter(|(kr, _)| r.intersects(*kr))
                        .map(|(_, k)| k.clone())
                        .collect();
                    let mut s = session.lock();
                    if additive {
                        for k in inside {
                            if !s.selected_keys.contains(&k) {
                                s.selected_keys.push(k);
                            }
                        }
                    } else {
                        s.selected_keys = inside;
                    }
                }
                if released {
                    self.drag = None;
                }
            }
            Some(Drag::PickWhip { layers, origin }) => {
                if let Some(p) = pointer {
                    ui.painter()
                        .line_segment([origin, p], Stroke::new(1.5, color::ACCENT));
                }
                if released {
                    self.drag = None;
                    if let Some(p) = pointer
                        && let Some((_, target)) = layer_rows.iter().find(|(r, _)| r.contains(p))
                        && !layers.contains(target)
                    {
                        let now = session.lock().time(&info.path);
                        cx.invoke_with(
                            "layer.set_parent",
                            json!({"layers": layers, "parent": target, "time": now}),
                        );
                    }
                }
            }
            Some(Drag::Reorder { layers }) => {
                if let Some(p) = pointer {
                    let target = layer_rows
                        .iter()
                        .position(|(r, _)| p.y < r.center().y)
                        .unwrap_or(layer_rows.len());
                    let y = layer_rows.get(target).map_or_else(
                        || {
                            layer_rows
                                .last()
                                .map_or(graph_body.top(), |(r, _)| r.bottom())
                        },
                        |(r, _)| r.top(),
                    );
                    ui.painter().hline(
                        graph_body.left() - 400.0..=graph_body.right(),
                        y,
                        Stroke::new(2.0, color::ACCENT),
                    );
                    if released {
                        self.drag = None;
                        // Index among the comp's layers excluding the moved ones.
                        let target_layer = layer_rows.get(target).map(|(_, l)| l.clone());
                        let rest: Vec<&PrimPath> =
                            comp_layers.iter().filter(|l| !layers.contains(l)).collect();
                        let index = match target_layer {
                            Some(t) => rest.iter().position(|l| **l == t).unwrap_or(rest.len()),
                            None => rest.len(),
                        };
                        let current: Vec<usize> = layers
                            .iter()
                            .filter_map(|l| comp_layers.iter().position(|x| x == l))
                            .collect();
                        let first = current.iter().min().copied().unwrap_or(0);
                        if index != first {
                            cx.invoke_with(
                                "layer.reorder",
                                json!({"layers": layers, "index": index}),
                            );
                        }
                    }
                } else if released {
                    self.drag = None;
                }
            }
            // Bar and work-area drags finish in their own widgets; if the row
            // scrolled out of view, drop the drag instead of leaving it stuck.
            Some(
                Drag::Cti
                | Drag::Move { .. }
                | Drag::TrimIn { .. }
                | Drag::TrimOut { .. }
                | Drag::WorkArea { .. },
            ) if released => self.drag = None,
            _ => {}
        }
    }
}
