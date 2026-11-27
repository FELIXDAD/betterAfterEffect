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
        graph_body: Rect,
        key_rects: &[(Rect, KeyRef)],
        layer_rows: &[(Rect, PrimPath)],
        prop_rows: &[(Rect, PrimPath, String)],
        comp_layers: &[PrimPath],
    ) {
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let released = ui.input(|i| i.pointer.any_released());
        match self.drag.clone() {
            Some(Drag::Keys { .. }) => {
                let plan = self.key_plan.clone();
                if let Some(x) = plan.as_ref().and_then(|p| p.snap_x) {
                    ui.painter().with_clip_rect(graph_body).vline(
                        x,
                        graph_body.y_range(),
                        Stroke::new(1.0, keys::SNAP),
                    );
                }
                if released {
                    self.drag = None;
                    if let Some(plan) = plan {
                        self.commit_key_drag(cx, session, &plan);
                    }
                }
            }
            Some(
                drag @ (Drag::Move { .. }
                | Drag::TrimIn { .. }
                | Drag::TrimOut { .. }
                | Drag::Slip { .. }),
            ) => {
                if let Some(x) = self.bar_plan.and_then(|p| p.snap_x) {
                    ui.painter().with_clip_rect(graph_body).vline(
                        x,
                        graph_body.y_range(),
                        Stroke::new(1.0, keys::SNAP),
                    );
                }
                if released {
                    self.drag = None;
                    if let Some(plan) = self.bar_plan {
                        self.commit_bar_drag(cx, session, info, &drag, plan);
                    }
                }
            }
            Some(Drag::Marker { .. }) => {
                if released {
                    self.drag = None;
                    if let Some((m, d)) = self.marker_plan.clone() {
                        self.commit_marker_drag(cx, &m, d);
                    }
                }
            }
            Some(Drag::Marquee { origin, mode, base }) => {
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
                    session.lock().selected_keys = mode.apply(&base, inside);
                }
                if released {
                    self.drag = None;
                }
            }
            Some(Drag::LayerMarquee { origin, mode, base }) => {
                if let Some(p) = pointer {
                    // The outline only: left of the time graph.
                    let outline = Rect::from_min_max(
                        pos2(ui.max_rect().left(), graph_body.top()),
                        pos2(graph_body.left(), graph_body.bottom()),
                    );
                    let r = Rect::from_two_pos(origin, p).intersect(outline);
                    ui.painter().rect(
                        r,
                        0.0,
                        Color32::from_rgba_unmultiplied(88, 149, 237, 30),
                        Stroke::new(1.0, color::ACCENT),
                        egui::StrokeKind::Inside,
                    );
                    let inside: Vec<PrimPath> = layer_rows
                        .iter()
                        .filter(|(row, _)| r.y_range().intersects(row.y_range()))
                        .map(|(_, l)| l.clone())
                        .filter(|l| !query::is_locked(cx.stage, l))
                        .collect();
                    let mut s = session.lock();
                    s.selected_layers = mode.apply(&base, inside);
                    s.selected_keys.clear();
                }
                if released {
                    self.drag = None;
                }
            }
            Some(Drag::MatteWhip { layer, origin }) => {
                if let Some(p) = pointer {
                    ui.painter()
                        .line_segment([origin, p], Stroke::new(1.5, color::ACCENT));
                }
                if released {
                    self.drag = None;
                    if let Some(p) = pointer
                        && let Some((_, target)) = layer_rows.iter().find(|(r, _)| r.contains(p))
                        && *target != layer
                    {
                        let matte = cx
                            .stage
                            .prim(&layer)
                            .and_then(|v| query::static_value(&v, "track_matte"))
                            .and_then(|v| v.as_str().map(str::to_owned))
                            .filter(|m| m != "none")
                            .unwrap_or_else(|| "alpha".into());
                        cx.invoke_with(
                            "layer.set_track_matte",
                            json!({"layers": [layer], "matte": matte, "source": target}),
                        );
                    }
                }
            }
            Some(Drag::PickWhip { layers, origin }) => {
                if let Some(p) = pointer {
                    ui.painter()
                        .line_segment([origin, p], Stroke::new(1.5, color::ACCENT));
                    // The layer it would parent to is outlined (AE).
                    if let Some((r, target)) = layer_rows.iter().find(|(r, _)| r.contains(p))
                        && !layers.contains(target)
                    {
                        outline(ui, *r);
                    }
                }
                if released {
                    self.drag = None;
                    if let Some(p) = pointer
                        && let Some((_, target)) = layer_rows.iter().find(|(r, _)| r.contains(p))
                        && !layers.contains(target)
                    {
                        let now = session.lock().time(&info.path);
                        let mods = ui.input(|i| i.modifiers);
                        cx.invoke_with(
                            "layer.set_parent",
                            super::rows::parent_params(&layers, Some(target), now, mods),
                        );
                    }
                }
            }
            Some(Drag::ExprWhip { whip, origin }) => {
                if let Some(p) = pointer {
                    ui.painter()
                        .line_segment([origin, p], Stroke::new(1.5, color::ACCENT));
                    if let Some(r) = prop_rows
                        .iter()
                        .map(|(r, ..)| r)
                        .chain(layer_rows.iter().map(|(r, _)| r))
                        .find(|r| r.contains(p))
                    {
                        outline(ui, *r);
                    }
                }
                if released {
                    self.drag = None;
                    let now = session.lock().time(&info.path);
                    let eval = self.eval.get(cx.stage);
                    if let Some(p) = pointer {
                        if let Some((_, path, attr)) =
                            prop_rows.iter().find(|(r, ..)| r.contains(p))
                        {
                            let attr = (!attr.is_empty()).then_some(attr.as_str());
                            crate::expr::link(cx, eval, &whip, path, attr, now);
                        } else if let Some((_, layer)) =
                            layer_rows.iter().find(|(r, _)| r.contains(p))
                        {
                            crate::expr::link(cx, eval, &whip, layer, None, now);
                        }
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
            Some(Drag::Cti | Drag::WorkArea { .. } | Drag::MarkerFromBin) if released => {
                self.drag = None
            }
            _ => {}
        }
    }
}

/// The row a pick whip is over.
fn outline(ui: &Ui, r: Rect) {
    ui.painter().rect_stroke(
        r.shrink(1.0),
        0.0,
        Stroke::new(1.0, color::ACCENT),
        egui::StrokeKind::Inside,
    );
}
