//! Outline rows: layer switches, names and Parent & Link; property rows with
//! stopwatches, values and keyframes; expression rows.

use super::*;

/// AE's name of a layer switch, for screen readers.
pub(super) fn switch_name(icon: Icon) -> &'static str {
    match icon {
        Icon::Eye => "Video",
        Icon::Speaker => "Audio",
        Icon::Solo => "Solo",
        Icon::Lock => "Lock",
        Icon::Shy => "Shy",
        Icon::Collapse => "Collapse Transformations",
        Icon::Quality => "Quality and Sampling",
        Icon::FrameBlend => "Frame Blending",
        Icon::MotionBlur => "Motion Blur",
        Icon::Adjustment => "Adjustment Layer",
        Icon::Cube => "3D Layer",
        _ => "Switch",
    }
}

/// A layer switch; `layer` is the accessibility context ("Layer “Solid 1”").
pub(super) fn switch(ui: &Ui, rect: Rect, on: bool, icon: Icon, id: Id, layer: &str) -> bool {
    let resp = ui.interact(rect, id, Sense::click()).labeled_toggle(
        egui::WidgetType::Checkbox,
        &bae_ui::a11y::context(&[layer, tr(switch_name(icon))]),
        on,
    );
    widgets::paint_switch(
        ui.painter(),
        rect.shrink2(vec2(1.0, 2.0)),
        on,
        icon,
        resp.hovered(),
    );
    resp.clicked()
}

impl TimelinePanel {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn row_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        cols: &Cols,
        axis: &TimeAxis,
        row: &Row,
        rect: Rect,
        comp_layers: &[PrimPath],
        key_rects: &mut Vec<(Rect, KeyRef)>,
    ) {
        let stage = cx.stage;
        let now = session.lock().time(&info.path);
        let l = rect.left();
        let cy = rect.center().y;
        let layer = &row.layer;
        let layer_selected = session.lock().selected_layers.contains(layer);
        let outline = Rect::from_min_max(rect.min, pos2(l + cols.end, rect.bottom()));
        let graph = Rect::from_min_max(
            pos2(axis.left, rect.top()),
            pos2(rect.right(), rect.bottom()),
        );
        {
            let p = ui.painter();
            let prop_selected = match &row.kind {
                RowKind::Prop { path, attr, .. } => session
                    .lock()
                    .selected_props
                    .iter()
                    .any(|(p, a)| p == path && a == attr),
                _ => false,
            };
            let bg = match row.kind {
                RowKind::Layer if layer_selected => color::ROW_SELECTED,
                RowKind::Layer => color::ROW,
                RowKind::Prop { .. } if prop_selected => color::ROW_SELECTED,
                _ => color::PANEL,
            };
            p.rect_filled(outline, 0.0, bg);
            p.rect_filled(
                graph,
                0.0,
                if matches!(row.kind, RowKind::Layer) {
                    color::TIMELINE_BG
                } else {
                    color::PANEL
                },
            );
            p.hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, color::SEPARATOR),
            );
        }
        let base = ui.id().with((layer.as_str(), row.depth, row_key(row)));
        let rid = |s: &str| base.with(s);
        // Screen readers tell the rows' controls apart by layer (issue #128).
        let layer_ctx = self.layer_ctx(stage, layer);

        match &row.kind {
            RowKind::Layer => {
                // A/V switches.
                let av = |i: usize| {
                    Rect::from_min_size(
                        pos2(l + cols.av + SW * i as f32, rect.top()),
                        vec2(SW, ROW_H),
                    )
                };
                for (i, attr, icon) in [
                    (0, "enabled", Icon::Eye),
                    (1, "audio_enabled", Icon::Speaker),
                    (2, "solo", Icon::Solo),
                    (3, "locked", Icon::Lock),
                ] {
                    let kind = query::layer_kind(stage, layer).unwrap_or_default();
                    if attr == "audio_enabled"
                        && !matches!(kind.as_str(), "footage" | "audio" | "pre_comp")
                    {
                        continue;
                    }
                    let on = bool_attr(stage, layer, attr);
                    if switch(ui, av(i), on, icon, rid(attr), &layer_ctx) {
                        cx.invoke_with("attr.set", json!({"path": layer, "attr": attr, "value": {"type": "bool", "value": !on}}));
                    }
                }
                let p = ui.painter();
                icons::swatch(
                    p,
                    pos2(l + cols.label + 7.0, cy),
                    9.0,
                    theme::label_color(label_of(stage, layer)),
                );
                let index = comp_layers
                    .iter()
                    .position(|x| x == layer)
                    .map_or(0, |i| i + 1);
                p.text(
                    pos2(l + cols.index + 4.0, cy),
                    Align2::LEFT_CENTER,
                    index.to_string(),
                    theme::small(),
                    color::TEXT,
                );
                // Twirl.
                let tw = Rect::from_center_size(pos2(l + cols.name + 6.0, cy), vec2(12.0, 12.0));
                let open = self.expanded.contains(layer.as_str())
                    || session.lock().reveal.contains_key(layer);
                icons::paint(
                    ui.painter(),
                    tw,
                    if open {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    },
                    color::ICON,
                );
                if ui
                    .interact(tw, rid("twirl"), Sense::click())
                    .labeled_toggle(
                        egui::WidgetType::CollapsingHeader,
                        &bae_ui::a11y::context(&[
                            &layer_ctx,
                            tr(if open { "Collapse" } else { "Expand" }),
                        ]),
                        open,
                    )
                    .clicked()
                    && session.lock().reveal.remove(layer).is_none()
                {
                    self.toggle(layer.as_str());
                }
                // Name (click selects, double-click renames, drag reorders).
                let name_rect = Rect::from_min_max(
                    pos2(l + cols.name + 14.0, rect.top()),
                    pos2(l + cols.switches - 2.0, rect.bottom()),
                );
                let name = query::display_name_of(stage, layer);
                if self.rename.as_ref().is_some_and(|(p, _)| p == layer) {
                    let mut done = None;
                    if let Some((_, buf)) = &mut self.rename {
                        done = common::rename_field(ui, name_rect, buf, rid("rename"));
                    }
                    if let Some(result) = done {
                        if let Some(new) = result.filter(|n| !n.trim().is_empty() && *n != name) {
                            cx.invoke_with("attr.set", json!({"path": layer, "attr": "name", "value": {"type": "string", "value": new}}));
                        }
                        self.rename = None;
                    }
                } else {
                    widgets::clipped_text(
                        ui.painter(),
                        pos2(name_rect.left(), cy),
                        &name,
                        theme::body(),
                        color::TEXT_STRONG,
                        name_rect.width(),
                    );
                    let resp = ui
                        .interact(name_rect, rid("name"), Sense::click_and_drag())
                        // "Layer 2 “Solid 1”": two layers may share a name (#128).
                        .labeled_toggle(
                            egui::WidgetType::SelectableLabel,
                            &layer_ctx,
                            layer_selected,
                        );
                    if resp.clicked() {
                        select_layer(session, stage, layer, ui.input(|i| i.modifiers));
                    }
                    let locked = query::is_locked(stage, layer);
                    if resp.double_clicked() && !locked {
                        self.rename = Some((layer.clone(), name.clone()));
                    }
                    if resp.drag_started() && !locked {
                        if !layer_selected {
                            select_layer(session, stage, layer, egui::Modifiers::NONE);
                        }
                        self.drag = Some(Drag::Reorder {
                            layers: session.lock().selected_layers.clone(),
                        });
                    }
                    let mut menu_out = None;
                    crate::common::context_menu(&resp, &mut |ui| {
                        if !session.lock().selected_layers.contains(layer) {
                            select_layer(session, stage, layer, egui::Modifiers::NONE);
                        }
                        menu_out = menus::layer_menu(ui, cx, session, layer);
                    });
                    if menu_out == Some(menus::LayerMenuOut::Rename) {
                        self.rename = Some((layer.clone(), name.clone()));
                    }
                }
                // Switches.
                let sw = |i: usize| {
                    Rect::from_min_size(
                        pos2(l + cols.switches + SW * i as f32, rect.top()),
                        vec2(SW, ROW_H),
                    )
                };
                for (i, attr, icon) in [
                    (0, "shy", Icon::Shy),
                    (1, "collapse_transformation", Icon::Collapse),
                    (5, "motion_blur", Icon::MotionBlur),
                    (6, "adjustment_layer", Icon::Adjustment),
                ] {
                    let on = bool_attr(stage, layer, attr);
                    if switch(ui, sw(i), on, icon, rid(attr), &layer_ctx) {
                        cx.invoke_with("attr.set", json!({"path": layer, "attr": attr, "value": {"type": "bool", "value": !on}}));
                    }
                }
                let draft = stage
                    .prim(layer)
                    .and_then(|v| query::static_value(&v, "quality"))
                    .is_some_and(|v| v.as_str() == Some("draft"));
                if switch(ui, sw(2), !draft, Icon::Quality, rid("quality"), &layer_ctx) {
                    let to = if draft { "best" } else { "draft" };
                    cx.invoke_with("attr.set", json!({"path": layer, "attr": "quality", "value": {"type": "enum", "value": to}}));
                }
                let three_d = bool_attr(stage, layer, "three_d");
                if switch(ui, sw(7), three_d, Icon::Cube, rid("three_d"), &layer_ctx) {
                    cx.invoke_with("attr.set", json!({"path": layer, "attr": "three_d", "value": {"type": "bool", "value": !three_d}}));
                }
                // Parent & Link: pick whip (left-aligned with the column title, as in
                // AE) and dropdown.
                let whip = Rect::from_min_size(
                    pos2(l + cols.parent + PARENT_PAD, cy - 7.0),
                    vec2(14.0, 14.0),
                );
                icons::paint(ui.painter(), whip, Icon::Pickwhip, color::ICON);
                let wresp = ui.interact(whip, rid("whip"), Sense::drag()).labeled(
                    egui::WidgetType::Button,
                    &bae_ui::a11y::context(&[&layer_ctx, tr("Parent Pick Whip")]),
                );
                if wresp.drag_started() {
                    let layers = if layer_selected {
                        session.lock().selected_layers.clone()
                    } else {
                        vec![layer.clone()]
                    };
                    self.drag = Some(Drag::PickWhip {
                        layers,
                        origin: whip.center(),
                    });
                }
                let parent = stage
                    .prim(layer)
                    .and_then(|v| v.value("parent").and_then(|v| v.as_prim_ref()).cloned());
                let mut names: Vec<String> = vec![tr("None").to_owned()];
                names.extend(
                    comp_layers
                        .iter()
                        .enumerate()
                        .map(|(i, p)| format!("{}. {}", i + 1, query::display_name_of(stage, p))),
                );
                let items: Vec<&str> = names.iter().map(String::as_str).collect();
                let mut sel = parent
                    .as_ref()
                    .and_then(|p| comp_layers.iter().position(|x| x == p))
                    .map_or(0, |i| i + 1);
                let before = sel;
                let drop_rect = Rect::from_min_max(
                    pos2(whip.right() + 4.0, rect.top() + 1.0),
                    pos2(l + cols.end - 4.0, rect.bottom() - 1.0),
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(drop_rect)
                        .id_salt(rid("parent")),
                );
                let current = items.get(sel).map(|s| s.to_string()).unwrap_or_default();
                widgets::dropdown(
                    &mut child,
                    rid("parent_dd"),
                    &mut sel,
                    &items,
                    drop_rect.width(),
                )
                .labeled_value(
                    egui::WidgetType::ComboBox,
                    &bae_ui::a11y::context(&[&layer_ctx, tr("Parent & Link")]),
                    &current,
                );
                if sel != before {
                    let parent = (sel > 0).then(|| comp_layers[sel - 1].clone());
                    if parent.as_ref() != Some(layer) {
                        cx.invoke_with(
                            "layer.set_parent",
                            json!({"layers": [layer], "parent": parent, "time": now}),
                        );
                    }
                }
                // Duration bar.
                if !self.graph_mode {
                    self.layer_bar(ui, cx, session, info, axis, layer, rect, layer_selected);
                    self.layer_markers_ui(ui, cx, session, info, axis, layer, rect);
                }
            }
            RowKind::Group { id, label, open } => {
                let x = l + cols.name + row.depth as f32 * 14.0;
                let tw = Rect::from_center_size(pos2(x + 6.0, cy), vec2(12.0, 12.0));
                icons::paint(
                    ui.painter(),
                    tw,
                    if *open {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    },
                    color::ICON,
                );
                let resp = ui
                    .interact(
                        Rect::from_min_max(
                            pos2(x, rect.top()),
                            pos2(l + cols.switches, rect.bottom()),
                        ),
                        rid("group"),
                        Sense::click(),
                    )
                    .labeled_toggle(
                        egui::WidgetType::CollapsingHeader,
                        &bae_ui::a11y::context(&[&layer_ctx, tr(label)]),
                        *open,
                    );
                if resp.clicked() {
                    self.toggle(id);
                }
                ui.painter().text(
                    pos2(x + 14.0, cy),
                    Align2::LEFT_CENTER,
                    label,
                    theme::body(),
                    color::TEXT,
                );
            }
            RowKind::Prop { path, attr, label } => {
                self.prop_row(
                    ui, cx, session, info, cols, axis, row, rect, path, attr, label, now, key_rects,
                );
            }
            RowKind::Expression { path, attr } => {
                self.expression_row(ui, cx, cols, axis, rect, path, attr, row.depth);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prop_row(
        &mut self,
        ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        cols: &Cols,
        axis: &TimeAxis,
        row: &Row,
        rect: Rect,
        path: &PrimPath,
        attr: &str,
        label: &str,
        now: Time,
        key_rects: &mut Vec<(Rect, KeyRef)>,
    ) {
        let stage = cx.stage;
        let l = rect.left();
        let cy = rect.center().y;
        let spec = query::attr_spec(stage, path, attr)
            .cloned()
            .unwrap_or_default();
        let animated = spec.is_animated();
        let key_now = query::to_key_time(stage, path, now);
        let id = ui.id().with(("prop", path.as_str(), attr));
        // What screen readers call this property: layer, shape group (when the
        // property is not on the layer itself), property (issue #128).
        let group = if *path == row.layer {
            String::new()
        } else {
            query::display_name_of(stage, path)
        };
        let prop_name =
            bae_ui::a11y::context(&[&self.layer_ctx(stage, &row.layer), &group, tr(label)]);
        // Keyframe navigator (A/V columns).
        if animated {
            let nav = Rect::from_min_size(
                pos2(l + cols.av, rect.top() + 1.0),
                vec2(3.0 * 13.0, ROW_H - 2.0),
            );
            let has_prev = spec.keyframes.range(..key_now).next_back().is_some();
            let has_next = spec.keyframes.range(key_now + Time(1)..).next().is_some();
            let on_key = spec.keyframes.contains_key(&key_now);
            let mut child =
                ui.new_child(egui::UiBuilder::new().max_rect(nav).id_salt(id.with("nav")));
            match widgets::keyframe_nav_for(&mut child, has_prev, on_key, has_next, &prop_name) {
                Some(KeyNav::Prev) => {
                    if let Some((t, _)) = spec.keyframes.range(..key_now).next_back() {
                        session
                            .lock()
                            .set_time(info, query::to_comp_time(stage, path, *t));
                    }
                }
                Some(KeyNav::Next) => {
                    if let Some((t, _)) = spec.keyframes.range(key_now + Time(1)..).next() {
                        session
                            .lock()
                            .set_time(info, query::to_comp_time(stage, path, *t));
                    }
                }
                Some(KeyNav::Toggle) => {
                    if on_key {
                        cx.invoke_with(
                            "key.delete",
                            json!({"keys": [KeyRef::new(path.clone(), attr, key_now)]}),
                        );
                    } else if let Some(v) =
                        common::current_value(self.eval.get(cx.resources), stage, path, attr, now)
                    {
                        cx.invoke_with(
                            "attr.set_at",
                            json!({"path": path, "attr": attr, "time": key_now, "value": v}),
                        );
                    }
                }
                None => {}
            }
        }
        // Stopwatch + name.
        let x = l + cols.name + row.depth as f32 * 14.0;
        let sw = Rect::from_center_size(pos2(x + 6.0, cy), vec2(16.0, 16.0));
        let mut on = animated;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(sw).id_salt(id.with("sw")));
        let animatable = stage
            .prim(path)
            .map(|v| bae_model::schema::is_animatable(v.prim_type(), attr))
            .unwrap_or(false);
        if animatable && widgets::stopwatch_for(&mut child, &mut on, &prop_name).clicked() {
            if ui.input(|i| i.modifiers.alt) {
                // Alt+click: add (or remove) an expression.
                let source = if spec.expression.is_some() {
                    None
                } else {
                    Some(default_expression(attr))
                };
                cx.invoke_with(
                    "attr.set_expression",
                    json!({"path": path, "attr": attr, "source": source}),
                );
            } else {
                cx.invoke_with(
                    "attr.set_animated",
                    json!({"path": path, "attr": attr, "animated": !animated, "time": key_now}),
                );
            }
        }
        let name_color = if spec.expression.is_some() {
            color::TEXT_STRONG
        } else {
            color::TEXT
        };
        // Graph Editor Set switch (animated properties), left of the stopwatch so the
        // stopwatch stays next to the name as in AE.
        let id_pair = (path.clone(), attr.to_owned());
        if animated {
            let gs = Rect::from_center_size(pos2(x - 8.0, cy), vec2(14.0, 14.0));
            let on = session.lock().graph_set.contains(&id_pair);
            let resp = ui
                .interact(gs, id.with("graph_set"), Sense::click())
                .labeled_toggle(
                    egui::WidgetType::Checkbox,
                    &bae_ui::a11y::context(&[&prop_name, tr("Graph Editor Set")]),
                    on,
                );
            icons::paint(
                ui.painter(),
                gs.shrink(1.0),
                Icon::GraphEditor,
                if on {
                    color::ACCENT
                } else if resp.hovered() {
                    color::ICON
                } else {
                    color::ICON_DIM
                },
            );
            if resp.clicked() {
                let mut s = session.lock();
                if let Some(i) = s.graph_set.iter().position(|p| *p == id_pair) {
                    s.graph_set.remove(i);
                } else {
                    s.graph_set.push(id_pair.clone());
                }
            }
        }
        ui.painter().text(
            pos2(x + 18.0, cy),
            Align2::LEFT_CENTER,
            label,
            theme::body(),
            name_color,
        );
        // Clicking the name selects the property and its keyframes (AE); Shift or
        // Cmd/Ctrl adds it.
        let name_rect = Rect::from_min_max(
            pos2(x + 16.0, rect.top()),
            pos2((l + cols.switches - 2.0).max(x + 22.0), rect.bottom()),
        );
        let selected_prop = session.lock().selected_props.contains(&id_pair);
        let nresp = ui
            .interact(name_rect, id.with("name"), Sense::click())
            .labeled_toggle(egui::WidgetType::SelectableLabel, &prop_name, selected_prop);
        if nresp.clicked() && query::locked_layer_of(stage, path).is_none() {
            let mods = ui.input(|i| i.modifiers);
            let keys: Vec<KeyRef> = spec
                .keyframes
                .keys()
                .map(|t| KeyRef::new(path.clone(), attr, *t))
                .collect();
            let mut s = session.lock();
            if mods.shift || mods.command {
                if let Some(i) = s.selected_props.iter().position(|p| *p == id_pair) {
                    s.selected_props.remove(i);
                    s.selected_keys
                        .retain(|k| !(k.path == *path && k.attr == attr));
                } else {
                    s.selected_props.push(id_pair.clone());
                    for k in keys {
                        if !s.selected_keys.contains(&k) {
                            s.selected_keys.push(k);
                        }
                    }
                }
            } else {
                s.selected_props = vec![id_pair.clone()];
                s.selected_keys = keys;
                s.selected_layers = vec![row.layer.clone()];
            }
        }
        // Value.
        let value = session
            .lock()
            .scrub
            .as_ref()
            .filter(|s| s.path == *path && s.attr == attr)
            .map(|s| s.value.clone())
            .or_else(|| common::current_value(self.eval.get(cx.resources), stage, path, attr, now));
        if let Some(value) = value {
            let vr = Rect::from_min_max(
                pos2(l + cols.switches, rect.top()),
                pos2(l + cols.end - 2.0, rect.bottom()),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(vr)
                    .layout(egui::Layout::left_to_right(egui::Align::Center))
                    .id_salt(id.with("val")),
            );
            let dims = if common::is_3d(stage, &row.layer) {
                3
            } else {
                2
            };
            let edit =
                common::value_editor(&mut child, id.with("ed"), attr, &value, dims, &prop_name);
            common::apply_edit(cx, session, path, attr, now, edit);
        }
        // Keyframes (layer bar mode; the Graph Editor draws its own).
        if !self.graph_mode {
            self.key_track(
                ui, cx, session, info, axis, rect, path, attr, &spec, &prop_name, key_rects,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn expression_row(
        &mut self,
        ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        cols: &Cols,
        axis: &TimeAxis,
        rect: Rect,
        path: &PrimPath,
        attr: &str,
        depth: usize,
    ) {
        let stage = cx.stage;
        let Some(expr) = query::attr_spec(stage, path, attr).and_then(|s| s.expression.clone())
        else {
            return;
        };
        let l = rect.left();
        let x = l + cols.name + depth as f32 * 14.0;
        let cy = rect.center().y;
        // "=" enable switch.
        let eq = Rect::from_center_size(pos2(x + 6.0, cy), vec2(14.0, 14.0));
        let resp = ui
            .interact(
                eq,
                ui.id().with(("expr_on", path.as_str(), attr)),
                Sense::click(),
            )
            .labeled(
                egui::WidgetType::Checkbox,
                &bae_ui::a11y::context(&[
                    &query::owning_layer(cx.stage, path)
                        .map(|l| self.layer_ctx(cx.stage, &l))
                        .unwrap_or_default(),
                    tr(&common::attr_label(None, attr, false)),
                    tr("Enable Expression"),
                ]),
            );
        ui.painter().text(
            eq.center(),
            Align2::CENTER_CENTER,
            "=",
            theme::body(),
            if expr.enabled {
                color::ACCENT
            } else {
                color::TEXT_DIM
            },
        );
        if resp.clicked() {
            cx.invoke_with("attr.set_expression", json!({"path": path, "attr": attr, "source": expr.source, "enabled": !expr.enabled}));
        }
        ui.painter().text(
            pos2(x + 18.0, cy),
            Align2::LEFT_CENTER,
            tr("Expression"),
            theme::small(),
            color::TEXT_DIM,
        );
        // Source field in the graph area.
        let field = Rect::from_min_max(
            pos2(axis.left + 4.0, rect.top() + 1.0),
            pos2(rect.right() - 4.0, rect.bottom() - 1.0),
        );
        let editing = self
            .expr_edit
            .as_ref()
            .is_some_and(|(p, a, _)| p == path && a == attr);
        let mut buf = if editing {
            self.expr_edit
                .as_ref()
                .map(|e| e.2.clone())
                .unwrap_or_default()
        } else {
            expr.source.clone()
        };
        let name = bae_ui::a11y::context(&[
            &query::owning_layer(cx.stage, path)
                .map(|l| self.layer_ctx(cx.stage, &l))
                .unwrap_or_default(),
            tr(&common::attr_label(None, attr, false)),
            tr("Expression"),
        ]);
        let (font, text, bg) = crate::common::expression_style(cx.resources);
        let r = ui
            .put(
                field,
                egui::TextEdit::singleline(&mut buf)
                    .font(font)
                    .text_color(text)
                    .background_color(bg),
            )
            .labeled(egui::WidgetType::TextEdit, &name);
        if r.changed() || r.has_focus() {
            self.expr_edit = Some((path.clone(), attr.to_owned(), buf.clone()));
        }
        if r.lost_focus() {
            self.expr_edit = None;
            if buf != expr.source {
                cx.invoke_with(
                    "attr.set_expression",
                    json!({"path": path, "attr": attr, "source": buf, "enabled": expr.enabled}),
                );
            }
        }
    }
}

/// AE's default expression text: the property's script name.
pub(super) fn default_expression(attr: &str) -> String {
    match attr {
        "transform:anchor_point" => "transform.anchorPoint".into(),
        "transform:position" => "transform.position".into(),
        "transform:scale" => "transform.scale".into(),
        "transform:rotation" => "transform.rotation".into(),
        "transform:opacity" => "transform.opacity".into(),
        other => format!("value // {other}"),
    }
}
