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
        Icon::Fx => "Effects",
        Icon::FrameBlend => "Frame Blending",
        Icon::MotionBlur => "Motion Blur",
        Icon::Adjustment => "Adjustment Layer",
        Icon::Cube => "3D Layer",
        _ => "Switch",
    }
}

impl TimelinePanel {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn row_ui(
        &mut self,
        ui: &mut Ui,
        graph_ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        cols: &Cols,
        axis: &TimeAxis,
        row: &Row,
        rect: Rect,
        switch_input: switches::SwitchInput,
        layers: &super::TimelineLayers,
        parent_items: &[&str],
        key_rects: &mut Vec<(Rect, KeyRef)>,
    ) {
        let stage = cx.stage;
        let now = session.lock().time(&info.path);
        let l = rect.left();
        let cy = rect.center().y;
        let layer = &layers.paths[row.layer_index];
        let layer_selected = session.lock().selected_layers.contains(layer);
        let outline = Rect::from_min_max(rect.min, pos2(axis.left, rect.bottom()));
        let graph = Rect::from_min_max(
            pos2(axis.left, rect.top()),
            pos2(rect.right(), rect.bottom()),
        );
        {
            let p = graph_ui.painter();
            let prop_selected = match &row.kind {
                RowKind::Prop { path, attr, .. } => session
                    .lock()
                    .selected_props
                    .iter()
                    .any(|(p, a)| p == path && a == attr),
                _ => false,
            };
            let bg = match &row.kind {
                RowKind::Layer if layer_selected => color::ROW_SELECTED,
                RowKind::Layer => color::ROW,
                RowKind::Prop { .. } if prop_selected => color::ROW_SELECTED,
                RowKind::Group { id, .. } if session.lock().selected_groups.contains(id) => {
                    color::ROW_SELECTED
                }
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
        let layer_ctx = bae_ui::a11y::layer(
            Some(row.layer_index + 1),
            &query::display_name_of(stage, layer),
        );

        match &row.kind {
            RowKind::Layer => {
                // A/V switches.
                let av = |i: usize| {
                    Rect::from_min_size(
                        pos2(l + cols.x(Column::Av) + SW * i as f32, rect.top()),
                        vec2(SW, ROW_H),
                    )
                };
                let locked = query::is_locked(stage, layer);
                for (i, attr, icon) in [
                    (0, "enabled", Icon::Eye),
                    (1, "audio_enabled", Icon::Speaker),
                    (2, "solo", Icon::Solo),
                    (3, "locked", Icon::Lock),
                ] {
                    if !cols.visible(Column::Av) {
                        break;
                    }
                    let kind = query::layer_kind(stage, layer).unwrap_or_default();
                    if attr == "audio_enabled"
                        && !matches!(kind.as_str(), "footage" | "audio" | "pre_comp")
                    {
                        continue;
                    }
                    let on = bool_attr(stage, layer, attr);
                    self.switch_cell(
                        ui,
                        switch_input,
                        cx,
                        av(i),
                        rid(attr),
                        layer,
                        &layer_ctx,
                        attr,
                        icon,
                        on,
                        locked,
                    );
                }
                let p = ui.painter();
                if cols.visible(Column::Label) {
                    icons::swatch(
                        p,
                        pos2(l + cols.x(Column::Label) + 7.0, cy),
                        9.0,
                        theme::label_color(label_of(stage, layer)),
                    );
                }
                if cols.visible(Column::Index) {
                    let index = row.layer_index + 1;
                    p.text(
                        pos2(l + cols.x(Column::Index) + 4.0, cy),
                        Align2::LEFT_CENTER,
                        index.to_string(),
                        theme::small(),
                        color::TEXT,
                    );
                }
                // Twirl.
                let tw = Rect::from_center_size(
                    pos2(l + cols.right(Column::Av) + 7.0, cy),
                    vec2(12.0, 12.0),
                );
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
                // Double-click opens a precomp or renames an ordinary layer.
                let name_rect = Rect::from_min_max(
                    pos2(l + cols.x(Column::Name) + 2.0, rect.top()),
                    pos2(l + cols.right(Column::Name) - 2.0, rect.bottom()),
                );
                let name = query::display_name_of(stage, layer);
                let source_name = stage
                    .prim(layer)
                    .and_then(|p| p.value("source").and_then(bae_model::Value::as_prim_ref))
                    .map(|p| query::display_name_of(stage, p));
                let display_name = match source_name {
                    Some(source) if !self.columns.layer_names => source,
                    Some(source) if source == name => format!("[{source}]"),
                    _ => name.clone(),
                };
                if self.rename.as_ref().is_some_and(|(p, _)| p == layer) {
                    let mut done = None;
                    if let Some((_, buf)) = &mut self.rename {
                        done = common::rename_field(ui, name_rect, buf, rid("rename"));
                    }
                    if let Some(result) = done {
                        if let Some(new) = result.filter(|n| !n.trim().is_empty() && *n != name) {
                            self.columns.layer_names = true;
                            cx.invoke_with("attr.set", json!({"path": layer, "attr": "name", "value": {"type": "string", "value": new}}));
                        }
                        self.rename = None;
                    }
                } else {
                    widgets::clipped_text(
                        ui.painter(),
                        pos2(name_rect.left(), cy),
                        &display_name,
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
                    // An effect dragged from Effects & Presets onto the layer.
                    if let Some(fx) = resp.dnd_release_payload::<crate::effects::EffectDrag>() {
                        crate::effects::apply(cx, std::slice::from_ref(layer), &fx.0);
                    }
                    if resp.double_clicked() && !self.open_precomp(cx, session, layer) && !locked {
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
                if cols.visible(Column::Switches) {
                    // Switches.
                    let sw = |i: usize| {
                        Rect::from_min_size(
                            pos2(l + cols.x(Column::Switches) + SW * i as f32, rect.top()),
                            vec2(SW, ROW_H),
                        )
                    };
                    let mut cell = |rect: Rect, attr, icon, on| {
                        self.switch_cell(
                            ui,
                            switch_input,
                            cx,
                            rect,
                            rid(attr),
                            layer,
                            &layer_ctx,
                            attr,
                            icon,
                            on,
                            locked,
                        );
                    };
                    for (i, attr, icon) in [
                        (0, "shy", Icon::Shy),
                        (1, "collapse_transformation", Icon::Collapse),
                        (5, "motion_blur", Icon::MotionBlur),
                        (6, "adjustment_layer", Icon::Adjustment),
                        (7, "three_d", Icon::Cube),
                    ] {
                        cell(sw(i), attr, icon, bool_attr(stage, layer, attr));
                    }
                    // Effect switch (fx): only on layers with effects, like AE.
                    if !bae_motion::effect::effects_of(stage, layer).is_empty() {
                        let on = stage
                            .prim(layer)
                            .and_then(|v| query::static_value(&v, "effects_enabled"))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);
                        cell(sw(3), "effects_enabled", Icon::Fx, on);
                    }
                    let draft = stage
                        .prim(layer)
                        .and_then(|v| query::static_value(&v, "quality"))
                        .is_some_and(|v| v.as_str() == Some("draft"));
                    cell(sw(2), "quality", Icon::Quality, !draft);
                }
                // Modes: Mode, T, Track Matte.
                if cols.visible(Column::Modes) {
                    self.modes_cells(
                        ui,
                        cx,
                        session,
                        cols,
                        rect,
                        layer,
                        &layers.paths,
                        layer_selected,
                        &layer_ctx,
                        base,
                    );
                }
                if cols.visible(Column::Parent) {
                    // Parent & Link: pick whip (left-aligned with the column title, as in
                    // AE) and dropdown.
                    let whip = Rect::from_min_size(
                        pos2(l + cols.x(Column::Parent) + PARENT_PAD, cy - 7.0),
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
                    let mut sel = layers.parent_numbers[row.layer_index];
                    let before = sel;
                    let drop_rect = Rect::from_min_max(
                        pos2(whip.right() + 4.0, rect.top() + 1.0),
                        pos2(l + cols.right(Column::Parent) - 4.0, rect.bottom() - 1.0),
                    );
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(drop_rect)
                            .id_salt(rid("parent")),
                    );
                    let current = parent_items[sel];
                    widgets::dropdown(
                        &mut child,
                        rid("parent_dd"),
                        &mut sel,
                        parent_items,
                        drop_rect.width(),
                    )
                    .labeled_value(
                        egui::WidgetType::ComboBox,
                        &bae_ui::a11y::context(&[&layer_ctx, tr("Parent & Link")]),
                        current,
                    );
                    if sel != before {
                        let parent = (sel > 0).then(|| layers.paths[sel - 1].clone());
                        if parent.as_ref() != Some(layer) {
                            cx.invoke_with(
                                "layer.set_parent",
                                json!({"layers": [layer], "parent": parent, "time": now}),
                            );
                        }
                    }
                }
                if cols.visible(Column::Comment) {
                    let r = Rect::from_min_max(
                        pos2(l + cols.x(Column::Comment) + 2.0, rect.top()),
                        pos2(l + cols.right(Column::Comment) - 2.0, rect.bottom()),
                    );
                    let comment = stage
                        .prim(layer)
                        .and_then(|p| {
                            p.value("comment")
                                .and_then(|v| v.as_str())
                                .map(str::to_owned)
                        })
                        .unwrap_or_default();
                    if let Some((path, buffer)) = &mut self.comment_edit
                        && path == layer
                    {
                        if let Some(result) =
                            common::rename_field(ui, r, buffer, rid("edit_comment"))
                        {
                            if let Some(text) = result
                                && text != comment
                            {
                                cx.invoke_with("attr.set",json!({"path":layer,"attr":"comment","value":{"type":"string","value":text}}));
                            }
                            self.comment_edit = None;
                        }
                    } else {
                        widgets::clipped_text(
                            ui.painter(),
                            r.left_center(),
                            &comment,
                            theme::small(),
                            color::TEXT,
                            r.width(),
                        );
                        if ui
                            .interact(r, rid("comment"), Sense::click())
                            .labeled_value(egui::WidgetType::Label, tr("Comment"), &comment)
                            .double_clicked()
                            && !query::is_locked(stage, layer)
                        {
                            self.comment_edit = Some((layer.clone(), comment));
                        }
                    }
                }
                if cols.on_screen(Column::RenderTime) {
                    let elapsed = session.lock().layer_times.get(layer).copied();
                    let text = elapsed.map_or_else(
                        || "—".into(),
                        |d| format!("{:.2} {}", d.as_secs_f64() * 1000.0, tr("ms")),
                    );
                    widgets::clipped_text(
                        ui.painter(),
                        pos2(l + cols.x(Column::RenderTime) + 3.0, cy),
                        &text,
                        theme::small(),
                        color::TEXT,
                        cols.width(Column::RenderTime) - 6.0,
                    );
                }
                cols.extra_cells(ui, stage, info, layer, rect);
                // Duration bar.
                if !self.graph_mode {
                    self.layer_bar(
                        graph_ui,
                        cx,
                        session,
                        info,
                        axis,
                        layer,
                        &layer_ctx,
                        rect,
                        layer_selected,
                    );
                    self.layer_markers_ui(graph_ui, cx, session, info, axis, layer, rect);
                }
            }
            RowKind::Waveform => {
                ui.painter().text(
                    pos2(l + cols.x(Column::Name) + 14.0, cy),
                    Align2::LEFT_CENTER,
                    tr("Waveform"),
                    theme::body(),
                    color::TEXT,
                );
                self.waveforms.draw(ui, stage, layer, axis, graph);
            }
            RowKind::Group { id, label, open } => {
                let x = l + cols.x(Column::Name) + row.depth as f32 * 14.0;
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
                // AE: only the arrow expands; the name selects the row (Shift or
                // Cmd/Ctrl adds), for Copy, Paste, Duplicate and Delete.
                let twirl = ui
                    .interact(tw.expand(3.0), rid("group"), Sense::click())
                    .labeled_toggle(
                        egui::WidgetType::CollapsingHeader,
                        &bae_ui::a11y::context(&[&layer_ctx, tr(label)]),
                        *open,
                    );
                if twirl.clicked() {
                    self.toggle(id);
                }
                let selected = session.lock().selected_groups.contains(id);
                let name = ui
                    .interact(
                        Rect::from_min_max(
                            pos2(x + 12.0, rect.top()),
                            pos2(l + cols.x(Column::Switches), rect.bottom()),
                        ),
                        rid("group_name"),
                        Sense::click(),
                    )
                    .labeled_toggle(
                        egui::WidgetType::SelectableLabel,
                        &bae_ui::a11y::context(&[&layer_ctx, tr(label), tr("Select")]),
                        selected,
                    );
                #[cfg(test)]
                self.last_groups
                    .push((id.clone(), tw.center(), pos2(x + 30.0, cy)));
                if name.clicked() && query::locked_layer_of(stage, layer).is_none() {
                    let mods = ui.input(|i| i.modifiers);
                    let mut s = session.lock();
                    if mods.shift || mods.command {
                        if let Some(i) = s.selected_groups.iter().position(|g| g == id) {
                            s.selected_groups.remove(i);
                        } else {
                            s.selected_groups.push(id.clone());
                        }
                    } else {
                        s.selected_groups = vec![id.clone()];
                        s.selected_props.clear();
                        s.selected_keys.clear();
                    }
                    s.selected_layers = vec![layer.clone()];
                }
                ui.painter().text(
                    pos2(x + 14.0, cy),
                    Align2::LEFT_CENTER,
                    label,
                    theme::body(),
                    color::TEXT,
                );
                // Text: "Animate:" (a new animator); an animator: "Add:" (AE).
                let animator = id.strip_suffix("#animator");
                if (id.ends_with("#text") || animator.is_some()) && !query::is_locked(stage, layer)
                {
                    let title = if animator.is_some() {
                        "Add:"
                    } else {
                        "Animate:"
                    };
                    let b = Rect::from_min_size(
                        pos2(l + cols.x(Column::Switches) + 4.0, rect.top() + 2.0),
                        vec2(72.0, ROW_H - 4.0),
                    );
                    let bresp = ui.interact(b, rid("animate"), Sense::click()).labeled(
                        egui::WidgetType::Button,
                        &bae_ui::a11y::context(&[&layer_ctx, tr(title)]),
                    );
                    ui.painter().text(
                        b.left_center(),
                        Align2::LEFT_CENTER,
                        format!("{} {}", tr(title), widgets::SUBMENU_ARROW),
                        theme::small(),
                        if bresp.hovered() {
                            color::TEXT_STRONG
                        } else {
                            color::TEXT_DIM
                        },
                    );
                    if bresp.clicked() && !layer_selected {
                        select_layer(session, stage, layer, egui::Modifiers::NONE);
                    }
                    let animator = animator.and_then(|a| PrimPath::new(a).ok());
                    crate::common::popup_menu(&bresp, &mut |ui| match &animator {
                        Some(a) => menus::animator_add_menu(ui, cx, a),
                        None => {
                            if let Some(n) = menus::catalog_entry("Animation", "Animate Text")
                                && let crate::catalog::Node::Submenu(sub) = n
                            {
                                menus::catalog_nodes(ui, cx, sub.children);
                            }
                        }
                    });
                }
            }
            RowKind::Prop { path, attr, label } => {
                self.prop_row(
                    ui, graph_ui, cx, session, info, cols, axis, row, layer, &layer_ctx, rect,
                    path, attr, label, now, key_rects,
                );
            }
            RowKind::Expression {
                path,
                attr,
                line,
                lines,
            } => {
                self.expression_row(
                    ui,
                    graph_ui,
                    cx,
                    session,
                    cols,
                    axis,
                    rect,
                    path,
                    attr,
                    row.depth,
                    layer,
                    &layer_ctx,
                    (*line, *lines),
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prop_row(
        &mut self,
        ui: &mut Ui,
        graph_ui: &Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        cols: &Cols,
        axis: &TimeAxis,
        row: &Row,
        layer: &PrimPath,
        layer_ctx: &str,
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
        let group = if path == layer {
            String::new()
        } else {
            query::display_name_of(stage, path)
        };
        let prop_name = bae_ui::a11y::context(&[layer_ctx, &group, tr(label)]);
        // Keyframe navigator (A/V columns).
        if animated && (cols.visible(Column::Keys) || cols.visible(Column::Av)) {
            let nav = Rect::from_min_size(
                pos2(
                    l + cols.x(if cols.visible(Column::Keys) {
                        Column::Keys
                    } else {
                        Column::Av
                    }),
                    rect.top() + 1.0,
                ),
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
                        common::current_value(self.eval.get(cx.stage), stage, path, attr, now)
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
        let x = l + cols.x(Column::Name) + row.depth as f32 * 14.0;
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
                    Some(crate::expr::default_expression(stage, path, attr))
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
            pos2(
                (l + cols.x(Column::Switches) - 2.0).max(x + 22.0),
                rect.bottom(),
            ),
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
                s.selected_groups.clear();
                s.selected_keys = keys;
                s.selected_layers = vec![layer.clone()];
            }
        }
        // Value.
        let value = session
            .lock()
            .scrub
            .as_ref()
            .and_then(|s| s.value_for(path, attr))
            .cloned()
            .or_else(|| common::current_value(self.eval.get(cx.stage), stage, path, attr, now));
        if attr == "text:path" {
            // Path Options > Path: a popup of the layer's masks, even when unset.
            let vr = Rect::from_min_max(
                pos2(l + cols.x(Column::Switches), rect.top()),
                pos2(l + cols.end - 2.0, rect.bottom()),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(vr)
                    .layout(egui::Layout::left_to_right(egui::Align::Center))
                    .id_salt(id.with("val")),
            );
            common::text_path_picker(&mut child, cx, path, id.with("path"));
        } else if let Some(value) = value {
            let vr = Rect::from_min_max(
                pos2(l + cols.x(Column::Switches), rect.top()),
                pos2(l + cols.end - 2.0, rect.bottom()),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(vr)
                    .layout(egui::Layout::left_to_right(egui::Align::Center))
                    .id_salt(id.with("val")),
            );
            let dims = if common::is_3d(stage, layer) { 3 } else { 2 };
            // Choices (a text selector's Units, Based On, Mode, Shape): a dropdown.
            let options = match &value {
                bae_model::Value::Enum(_) => stage
                    .prim(path)
                    .and_then(|v| v.prim_type().cloned())
                    .and_then(|t| bae_model::schema::attr_def(&t, attr))
                    .map(|d| d.options)
                    .unwrap_or_default(),
                _ => &[],
            };
            let expression = spec.expression.as_ref().is_some_and(|e| e.enabled);
            let edit = if let bae_model::Value::Bool(on) = value {
                // AE's timeline switches On/Off with hot text, not a checkbox.
                let text = tr(if on { "On" } else { "Off" });
                if widgets::link(&mut child, text)
                    .labeled_toggle(egui::WidgetType::Checkbox, &prop_name, on)
                    .clicked()
                {
                    common::Edit::Commit(bae_model::Value::Bool(!on))
                } else {
                    common::Edit::None
                }
            } else if let (bae_model::Value::Enum(cur), false) = (&value, options.is_empty()) {
                let names: Vec<&str> = options.iter().map(|o| option_label(o)).collect();
                let mut sel = options.iter().position(|o| o == cur).unwrap_or(0);
                let before = sel;
                widgets::dropdown(&mut child, id.with("choice"), &mut sel, &names, 160.0)
                    .labeled_value(egui::WidgetType::ComboBox, &prop_name, names[sel]);
                if sel != before {
                    common::Edit::Commit(bae_model::Value::Enum(options[sel].into()))
                } else {
                    common::Edit::None
                }
            } else {
                common::value_editor(
                    &mut child,
                    id.with("ed"),
                    attr,
                    &value,
                    dims,
                    &prop_name,
                    expression,
                )
            };
            common::apply_edit(cx, session, path, attr, now, edit);
        }
        // Keyframes (layer bar mode; the Graph Editor draws its own).
        if !self.graph_mode {
            self.key_track(
                graph_ui, cx, session, info, axis, rect, path, attr, &spec, &prop_name, key_rects,
            );
        }
    }

    /// An expression row: line 0 has "Expression: <property>", the Enable switch
    /// and the pick whip; the last line draws the field over all the lines.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn expression_row(
        &mut self,
        ui: &Ui,
        graph_ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        cols: &Cols,
        axis: &TimeAxis,
        rect: Rect,
        path: &PrimPath,
        attr: &str,
        depth: usize,
        layer: &PrimPath,
        layer_ctx: &str,
        (line, lines): (usize, usize),
    ) {
        let stage = cx.stage;
        let Some(expr) = query::attr_spec(stage, path, attr).and_then(|s| s.expression.clone())
        else {
            return;
        };
        let l = rect.left();
        let cy = rect.center().y;
        let three_d = common::is_3d(stage, layer);
        let prop = tr(&common::attr_label(None, attr, three_d)).to_owned();
        let name = bae_ui::a11y::context(&[layer_ctx, &prop]);
        let id = ui.id().with(("expr", path.as_str(), attr));
        let field_id = id.with("field");
        if line == 0 {
            let x = l + cols.x(Column::Name) + depth as f32 * 14.0;
            widgets::clipped_text(
                ui.painter(),
                pos2(x + 4.0, cy),
                &format!("{} {prop}", tr("Expression:")),
                theme::body(),
                color::TEXT,
                (l + cols.x(Column::Switches) - x - 8.0).max(0.0),
            );
            // Switches column: Enable Expression (= / ≠), pick whip.
            let ix = l + cols.x(Column::Switches);
            let eq = Rect::from_center_size(pos2(ix + SW / 2.0, cy), vec2(16.0, 16.0));
            let resp = ui
                .interact(eq, id.with("on"), Sense::click())
                .labeled_toggle(
                    egui::WidgetType::Checkbox,
                    &bae_ui::a11y::context(&[&name, tr("Enable Expression")]),
                    expr.enabled,
                );
            ui.painter().text(
                eq.center(),
                Align2::CENTER_CENTER,
                if expr.enabled { "=" } else { "≠" },
                theme::body(),
                if expr.enabled {
                    color::ACCENT
                } else {
                    color::TEXT_DIM
                },
            );
            if resp.clicked() {
                cx.invoke_with(
                    "attr.set_expression",
                    json!({"path": path, "attr": attr, "source": expr.source, "enabled": !expr.enabled}),
                );
            }
            resp.on_hover_text(tr("Enable Expression"));
            let whip = Rect::from_center_size(pos2(ix + SW * 1.5 + 2.0, cy), vec2(14.0, 14.0));
            icons::paint(ui.painter(), whip, Icon::Pickwhip, color::ICON);
            let tip = tr("Expression pick whip (insert reference to target)");
            let wresp = ui.interact(whip, id.with("whip"), Sense::drag()).labeled(
                egui::WidgetType::Button,
                &bae_ui::a11y::context(&[&name, tip]),
            );
            #[cfg(test)]
            self.last_expr
                .push((path.clone(), attr.to_owned(), eq.center(), whip.center()));
            if ui.input(|i| i.pointer.primary_pressed()) && wresp.contains_pointer() {
                // The field loses focus on this press: keep its selection.
                self.expr_press = self.caret(path, attr);
            }
            if wresp.drag_started() {
                let w = crate::expr::Whip {
                    path: path.clone(),
                    attr: attr.to_owned(),
                    insert: self.expr_press.take(),
                };
                egui::DragAndDrop::set_payload(ui.ctx(), w.clone());
                self.drag = Some(Drag::ExprWhip {
                    whip: w,
                    origin: whip.center(),
                });
            }
            wresp.on_hover_text(tip);
            // Expression language menu.
            let mr = Rect::from_center_size(pos2(ix + SW * 2.5 + 4.0, cy), vec2(16.0, 16.0));
            icons::paint(ui.painter(), mr, Icon::PlayCircle, color::ICON);
            let mtip = tr("Expression language menu");
            let mresp = ui.interact(mr, id.with("lang"), Sense::click()).labeled(
                egui::WidgetType::Button,
                &bae_ui::a11y::context(&[&name, mtip]),
            );
            if ui.input(|i| i.pointer.primary_pressed()) && mresp.contains_pointer() {
                self.lang_press = Some((path.clone(), attr.to_owned(), self.caret(path, attr)));
            }
            let mut chosen = None;
            common::popup_menu(&mresp, &mut |ui| crate::expr_menu::ui(ui, &mut chosen));
            if let Some(item) = chosen {
                let insert = self
                    .lang_press
                    .take()
                    .filter(|(p, a, _)| p == path && a == attr)
                    .and_then(|(.., i)| i);
                insert_expression(cx, path, attr, &expr, insert, item);
            }
            mresp.on_hover_text(mtip);
            #[cfg(test)]
            self.last_lang
                .push((path.clone(), attr.to_owned(), mr.center()));
            // An error in this expression (the viewer's evaluation).
            let error = session
                .lock()
                .expr_errors
                .iter()
                .find(|e| e.prim == *path && e.attr == attr)
                .cloned();
            if let Some(e) = error {
                let wr = Rect::from_center_size(pos2(ix + SW * 3.5 + 6.0, cy), vec2(14.0, 14.0));
                icons::paint(ui.painter(), wr, Icon::Alert, crate::viewer::errors::YELLOW);
                let what =
                    tr("Indicates there is an error in the expression applied to this property.");
                ui.interact(wr, id.with("error"), Sense::hover())
                    .labeled(
                        egui::WidgetType::Label,
                        &bae_ui::a11y::context(&[&name, what]),
                    )
                    .on_hover_text(format!(
                        "{what}\n{}",
                        crate::viewer::errors::describe(stage, &e)
                    ));
            }
        }
        if line + 1 < lines {
            return;
        }
        let ui = graph_ui;
        // The field, in the graph area over the rows of all the lines.
        let field = Rect::from_min_max(
            pos2(axis.left + 4.0, rect.bottom() - lines as f32 * ROW_H + 1.0),
            pos2(rect.right() - 4.0, rect.bottom() - 1.0),
        );
        let mut buf = match &self.expr_edit {
            Some((p, a, b)) if p == path && a == attr => b.clone(),
            _ => expr.source.clone(),
        };
        // Keypad Enter ends editing (Return adds a line).
        let keypad = ui.memory(|m| m.has_focus(field_id))
            && bae_ui::keypad_enter()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let (font, text, bg) = crate::common::expression_style(cx.resources);
        let shown = ui
            .scope_builder(egui::UiBuilder::new().max_rect(field), |ui| {
                egui::TextEdit::multiline(&mut buf)
                    .id(field_id)
                    .font(font)
                    .text_color(text)
                    .background_color(bg)
                    .desired_width(field.width())
                    .desired_rows(lines)
                    .min_size(field.size())
                    .show(ui)
            })
            .inner;
        let r = shown.response.response.labeled(
            egui::WidgetType::TextEdit,
            &bae_ui::a11y::context(&[&name, tr("Expression")]),
        );
        if r.has_focus() {
            if let Some(c) = shown.cursor_range {
                let (a, b): (usize, usize) = (c.primary.index.into(), c.secondary.index.into());
                self.expr_caret = Some((
                    path.clone(),
                    attr.to_owned(),
                    buf.clone(),
                    a.min(b)..a.max(b),
                ));
            }
            self.expr_edit = Some((path.clone(), attr.to_owned(), buf.clone()));
        }
        if keypad {
            r.surrender_focus();
        }
        if r.lost_focus() || keypad {
            self.expr_edit = None;
            self.expr_caret = None;
            if buf != expr.source {
                cx.invoke_with(
                    "attr.set_expression",
                    json!({"path": path, "attr": attr, "source": buf, "enabled": expr.enabled}),
                );
            }
        }
    }

    /// The text and selection of `path.attr`'s field while it is being edited.
    fn caret(&self, path: &PrimPath, attr: &str) -> Option<crate::expr::Selection> {
        self.expr_caret
            .as_ref()
            .filter(|(p, a, ..)| p == path && a == attr)
            .map(|(_, _, t, r)| (t.clone(), r.clone()))
    }
}

/// Insert `text` (the language menu) at the selection of the field being
/// edited, or replace the expression.
fn insert_expression(
    cx: &mut PanelContext<'_>,
    path: &PrimPath,
    attr: &str,
    expr: &bae_model::Expression,
    insert: Option<crate::expr::Selection>,
    text: &str,
) {
    let source = match insert {
        Some((buf, sel)) => crate::expr::splice(&buf, &sel, text),
        None => text.to_owned(),
    };
    cx.invoke_with(
        "attr.set_expression",
        json!({"path": path, "attr": attr, "source": source, "enabled": expr.enabled}),
    );
}

/// The name of a choice token in the UI language. A table of its own: some of
/// these words read differently elsewhere in AE's Chinese UI (a text selector's
/// Intersect is 相交, a mask's 交集; its Add is 相加, the shape tools' 添加).
fn option_label(token: &str) -> &'static str {
    const NAMES: [(&str, &str, &str); 18] = [
        ("percentage", "Percentage", "百分比"),
        ("index", "Index", "索引"),
        ("characters", "Characters", "字符"),
        (
            "characters_excluding_spaces",
            "Characters Excluding Spaces",
            "不包含空格的字符",
        ),
        ("words", "Words", "词"),
        ("lines", "Lines", "行"),
        ("add", "Add", "相加"),
        ("subtract", "Subtract", "相减"),
        ("intersect", "Intersect", "相交"),
        ("min", "Min", "最小值"),
        ("max", "Max", "最大值"),
        ("difference", "Difference", "差值"),
        ("square", "Square", "正方形"),
        ("ramp_up", "Ramp Up", "上斜坡"),
        ("ramp_down", "Ramp Down", "下斜坡"),
        ("triangle", "Triangle", "三角形"),
        ("round", "Round", "圆形"),
        ("smooth", "Smooth", "平滑"),
    ];
    let zh = bae_ui::i18n::lang() == bae_ui::i18n::Lang::Zh;
    NAMES
        .iter()
        .find(|(t, _, _)| *t == token)
        .map_or("", |(_, en, z)| if zh { z } else { en })
}
