//! Comp tabs, the time header (current time, ruler, work area, CTI) and the footer.

use super::*;

/// Ruler step in frames so labels are at least `min_px` apart.
pub(super) fn ruler_step(axis: &TimeAxis, info: &CompInfo, min_px: f32) -> i64 {
    let fps = info.frame_rate.as_f64().round().max(1.0) as i64;
    let frame_px = (info.frame_duration().0 as f64 / axis.per_point()) as f32;
    let candidates = [
        1,
        2,
        5,
        10,
        fps / 2,
        fps,
        2 * fps,
        5 * fps,
        10 * fps,
        30 * fps,
        60 * fps,
        300 * fps,
        600 * fps,
    ];
    candidates
        .into_iter()
        .filter(|c| *c > 0)
        .find(|c| *c as f32 * frame_px >= min_px)
        .unwrap_or(600 * fps)
}

pub(super) fn ruler_label(frame: i64, info: &CompInfo) -> String {
    let fps = info.frame_rate.as_f64().round().max(1.0) as i64;
    if frame % fps == 0 {
        format!("{}s", frame / fps)
    } else {
        format!("{}f", frame % fps)
    }
}

pub(super) fn comp_tabs(ui: &Ui, rect: Rect, stage: &Stage, session: &SharedSession) {
    let (open, active) = {
        let s = session.lock();
        (s.open_comps.clone(), s.active_comp.clone())
    };
    let mut x = rect.left() + 6.0;
    let mut close = None;
    let mut activate = None;
    for c in &open {
        let name = query::display_name_of(stage, c);
        let galley = ui
            .painter()
            .layout_no_wrap(name.clone(), theme::body(), color::TEXT);
        let w = galley.size().x + 34.0;
        let tab = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(w, rect.height() - 2.0));
        let is_active = active.as_ref() == Some(c);
        let resp = ui
            .interact(tab, ui.id().with(("comp_tab", c.as_str())), Sense::click())
            .labeled_toggle(egui::WidgetType::SelectableLabel, &name, is_active);
        let p = ui.painter();
        icons::paint(
            p,
            Rect::from_center_size(pos2(tab.left() + 9.0, tab.center().y), vec2(12.0, 12.0)),
            Icon::Composition,
            color::ICON_DIM,
        );
        p.text(
            pos2(tab.left() + 18.0, tab.center().y),
            Align2::LEFT_CENTER,
            &name,
            theme::body(),
            if is_active {
                color::TAB_ACTIVE
            } else {
                color::TAB_INACTIVE
            },
        );
        if is_active {
            p.hline(
                tab.x_range(),
                tab.bottom() - 1.0,
                Stroke::new(1.5, color::TAB_ACTIVE),
            );
        }
        let xr = Rect::from_center_size(pos2(tab.right() - 8.0, tab.center().y), vec2(10.0, 10.0));
        let xresp = ui
            .interact(xr, ui.id().with(("comp_tab_x", c.as_str())), Sense::click())
            .labeled(
                egui::WidgetType::Button,
                &bae_ui::a11y::context(&[&name, tr("Close")]),
            );
        if resp.hovered() || is_active {
            icons::paint(ui.painter(), xr, Icon::Close, color::TAB_CLOSE);
        }
        if xresp.clicked() {
            close = Some(c.clone());
        } else if resp.clicked() {
            activate = Some(c.clone());
        }
        x += w + 4.0;
    }
    let mut s = session.lock();
    if let Some(c) = close {
        s.close_comp(&c);
    }
    if let Some(c) = activate {
        s.open_comp(&c);
    }
}

impl TimelinePanel {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn header(
        &mut self,
        ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        head: Rect,
        colrow: Rect,
        cols: &Cols,
        axis: &TimeAxis,
        now: Time,
    ) {
        let p = ui.painter().clone();
        // Current time: big blue timecode, scrubbable.
        let tc = Rect::from_min_size(head.min + vec2(8.0, 2.0), vec2(120.0, head.height() - 4.0));
        p.text(
            pos2(tc.left(), tc.center().y - 4.0),
            Align2::LEFT_CENTER,
            common::timecode(info, now),
            theme::font(17.0),
            color::ACCENT,
        );
        p.text(
            pos2(tc.left(), tc.bottom() - 4.0),
            Align2::LEFT_CENTER,
            format!(
                "{:05} ({:.2} fps)",
                common::frame_of(info, now),
                info.frame_rate.as_f64()
            ),
            theme::small(),
            color::TEXT_DIM,
        );
        let tresp = ui
            .interact(tc, ui.id().with("cti_scrub"), Sense::drag())
            .labeled_value(
                egui::WidgetType::DragValue,
                tr("Current Time"),
                &common::timecode(info, now),
            );
        if tresp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        if tresp.dragged() {
            let frames = (tresp.drag_delta().x / 3.0).round() as i64;
            if frames != 0 {
                let mut s = session.lock();
                s.stop();
                s.set_time(info, now + Time(info.frame_duration().0 * frames));
            }
        }

        // Search field, between the current time and the comp switches.
        let left = tc.right() + 12.0;
        let right = (left + 170.0).min(head.left() + cols.end - 140.0);
        if right - left > 60.0 {
            self.search_ui(
                ui,
                Rect::from_min_max(
                    pos2(left, head.center().y - 10.0),
                    pos2(right, head.center().y + 10.0),
                ),
            );
        }

        // Graph Editor switch (AE's comp button; Shift+F3).
        let ge = Rect::from_center_size(
            pos2(head.left() + cols.viewport - 14.0, head.center().y),
            vec2(20.0, 20.0),
        );
        let gresp = ui.interact(ge, ui.id().with("graph_editor_switch"), Sense::click());
        if gresp.clicked() {
            self.graph_mode = !self.graph_mode;
            session.lock().graph_editor = self.graph_mode;
        }
        let on = self.graph_mode;
        let gresp = gresp.labeled_toggle(egui::WidgetType::Button, tr("Graph Editor"), on);
        if on {
            ui.painter().rect_filled(ge, 2.0, color::BUTTON_ON);
        }
        icons::paint(
            ui.painter(),
            ge.shrink(3.0),
            Icon::GraphEditor,
            if on {
                color::ACCENT
            } else if gresp.hovered() {
                color::ICON
            } else {
                color::ICON_DIM
            },
        );
        gresp.on_hover_text(tr("Graph Editor"));

        let flow = ge.translate(vec2(-5.0 * 26.0, 0.0));
        let title = tr("Composition Mini-Flowchart");
        let button = ui
            .interact(flow, ui.id().with("mini_flow"), Sense::click())
            .labeled(egui::WidgetType::Button, title);
        icons::paint(&p, flow.shrink(3.0), Icon::MiniFlowchart, color::ICON);
        if button.on_hover_text(title).clicked() {
            cx.invoke(ids::MINI_FLOW);
        }

        // The comp switches left of it, AE's order: Draft 3D, Hide Shy Layers,
        // Frame Blending, Motion Blur.
        for (i, (attr, icon, name, tip, enabled)) in [
            (
                "motion_blur",
                Icon::MotionBlur,
                "Enable Motion Blur",
                "Enables Motion Blur for all layers with the Motion Blur switch set",
                true,
            ),
            (
                "frame_blending",
                Icon::FrameBlend,
                "Enable Frame Blending",
                "Enables Frame Blending for all layers with the Frame Blend switch set",
                true,
            ),
            (
                "hide_shy_layers",
                Icon::Shy,
                "Hide Shy Layers",
                "Hides all layers for which the Shy switch is set",
                true,
            ),
            (
                "draft_3d",
                Icon::Cube,
                "Draft 3D",
                "Turn fast 3D previews on or off",
                false,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let r = ge.translate(vec2(-26.0 * (i as f32 + 1.0), 0.0));
            let on = enabled && bool_attr(cx.stage, &info.path, attr);
            let mut cell = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(r)
                    .id_salt(("comp_switch", attr)),
            );
            if !enabled {
                cell.disable();
            }
            let resp = cell
                .interact(r, cell.id().with("button"), Sense::click())
                .labeled_toggle(egui::WidgetType::Button, tr(name), on);
            if on {
                ui.painter().rect_filled(r, 2.0, color::BUTTON_ON);
            }
            let c = if !enabled {
                color::TEXT_DISABLED
            } else if on {
                color::ACCENT
            } else if resp.hovered() {
                color::ICON
            } else {
                color::ICON_DIM
            };
            icons::paint(ui.painter(), r.shrink(3.0), icon, c);
            if enabled && resp.clicked() {
                cx.invoke_with(
                    "attr.set",
                    json!({"path": info.path, "attr": attr, "value": {"type": "bool", "value": !on}}),
                );
            }
            resp.on_hover_text(tr(tip));
        }

        // Column headers (right-click: AE's Hide This / Columns ▸ menu).
        let clip = ui.clip_rect();
        let mut outline_clip = clip;
        outline_clip.max.x = outline_clip.max.x.min(axis.left - 2.0);
        ui.set_clip_rect(outline_clip);
        let p = ui.painter();
        p.rect_filled(colrow, 0.0, color::PANEL);
        p.hline(
            colrow.x_range(),
            colrow.bottom() - 0.5,
            Stroke::new(1.0, color::SEPARATOR),
        );
        let l = colrow.left();
        let cy = colrow.center().y;
        for (i, icon) in [Icon::Eye, Icon::Speaker, Icon::Solo, Icon::Lock]
            .into_iter()
            .enumerate()
            .filter(|_| cols.visible(Column::Av))
        {
            icons::paint(
                p,
                Rect::from_center_size(
                    pos2(l + cols.x(Column::Av) + SW * (i as f32 + 0.5), cy),
                    vec2(12.0, 12.0),
                ),
                icon,
                color::ICON_DIM,
            );
        }
        if cols.visible(Column::Label) {
            icons::paint(
                p,
                Rect::from_center_size(pos2(l + cols.x(Column::Label) + 7.0, cy), vec2(12.0, 12.0)),
                Icon::Tag,
                color::ICON_DIM,
            );
        }
        if cols.visible(Column::Index) {
            p.text(
                pos2(l + cols.x(Column::Index) + 4.0, cy),
                Align2::LEFT_CENTER,
                "#",
                theme::small(),
                color::TEXT,
            );
        }
        p.text(
            pos2(l + cols.x(Column::Name) + 2.0, cy),
            Align2::LEFT_CENTER,
            tr(if self.columns.layer_names {
                "Layer Name"
            } else {
                "Source Name"
            }),
            theme::small(),
            color::TEXT,
        );
        let switch_icons = [
            Icon::Shy,
            Icon::Collapse,
            Icon::Quality,
            Icon::Fx,
            Icon::FrameBlend,
            Icon::MotionBlur,
            Icon::Adjustment,
            Icon::Cube,
        ];
        for (i, icon) in switch_icons
            .into_iter()
            .enumerate()
            .filter(|_| cols.visible(Column::Switches))
        {
            icons::paint(
                p,
                Rect::from_center_size(
                    pos2(l + cols.x(Column::Switches) + SW * (i as f32 + 0.5), cy),
                    vec2(12.0, 12.0),
                ),
                icon,
                color::ICON_DIM,
            );
        }
        if cols.visible(Column::Modes) {
            for (x, title) in [
                (4.0, "Mode"),
                (96.0 + 4.0 + 5.0, "T"),
                (96.0 + SW + 8.0, "Track Matte"),
            ] {
                p.text(
                    pos2(l + cols.x(Column::Modes) + x, cy),
                    Align2::LEFT_CENTER,
                    tr(title),
                    theme::small(),
                    color::TEXT,
                );
            }
        }
        if cols.visible(Column::Parent) {
            p.text(
                pos2(l + cols.x(Column::Parent) + PARENT_PAD, cy),
                Align2::LEFT_CENTER,
                tr("Parent & Link"),
                theme::small(),
                color::TEXT,
            );
        }

        for c in [
            Column::Comment,
            Column::Keys,
            Column::RenderTime,
            Column::In,
            Column::Out,
            Column::Duration,
            Column::Stretch,
        ] {
            if cols.visible(c) {
                widgets::clipped_text(
                    p,
                    pos2(l + cols.x(c) + 3.0, cy),
                    tr(c.title()),
                    theme::small(),
                    color::TEXT,
                    cols.width(c) - 6.0,
                );
            }
        }
        self.columns.header_controls(ui, colrow, cols);

        ui.set_clip_rect(clip);

        // Ruler and work area (right part of both header rows).
        let ruler = Rect::from_min_max(
            pos2(axis.left, head.top()),
            pos2(head.right(), colrow.bottom()),
        );
        let p = ui.painter().with_clip_rect(ruler);
        p.rect_filled(ruler, 0.0, color::TIMELINE_BG);
        // Work area bar.
        let wa_top = head.top() + 14.0;
        let wa = Rect::from_min_max(
            pos2(axis.x(info.work_area.start), wa_top),
            pos2(axis.x(info.work_area.end), wa_top + 8.0),
        );
        let (mut ws, mut we) = (info.work_area.start, info.work_area.end);
        if let Some(Drag::WorkArea { origin, start, end }) = &self.drag
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            let d = Time(((pos.x - origin) as f64 * axis.per_point()) as i64);
            let snap = |t: Time| info.frame_rate.snap(t + Time(info.frame_duration().0 / 2));
            if *start {
                ws = snap(ws + d);
            }
            if *end {
                we = snap(we + d);
            }
        }
        let wa_draw = Rect::from_min_max(pos2(axis.x(ws), wa.top()), pos2(axis.x(we), wa.bottom()));
        p.rect_filled(wa_draw, 2.0, color::WORK_AREA);
        // Ticks.
        let step = ruler_step(axis, info, 60.0);
        let first = info.frame_rate.time_to_frame(axis.start) / step * step;
        let mut f = first;
        let tick_y = colrow.bottom();
        loop {
            let t = info.frame_rate.frame_to_time(f);
            if t > axis.start + axis.span {
                break;
            }
            let x = axis.x(t);
            p.vline(
                x,
                (tick_y - 8.0)..=tick_y,
                Stroke::new(1.0, color::TEXT_DIM),
            );
            p.text(
                pos2(x + 3.0, tick_y - 12.0),
                Align2::LEFT_CENTER,
                ruler_label(f, info),
                theme::small(),
                color::TEXT,
            );
            let minor = (step / 5).max(1);
            for k in 1..5 {
                let xm = axis.x(info.frame_rate.frame_to_time(f + k * minor));
                if minor * k < step {
                    p.vline(
                        xm,
                        (tick_y - 4.0)..=tick_y,
                        Stroke::new(1.0, color::TEXT_DISABLED),
                    );
                }
            }
            f += step;
        }
        // Green segments under the work area bar, one per cached run (AE-UI-064).
        for span in session
            .lock()
            .cached_frames(&info.path, cx.stage.revision())
        {
            p.hline(
                axis.x(span.start)..=axis.x(span.end),
                wa_top + 8.0 + 2.0,
                Stroke::new(2.0, egui::Color32::from_rgb(72, 196, 95)),
            );
        }
        // CTI handle.
        let x = axis.x(now);
        p.add(Shape::convex_polygon(
            vec![
                pos2(x - 5.0, ruler.bottom() - 14.0),
                pos2(x + 5.0, ruler.bottom() - 14.0),
                pos2(x + 5.0, ruler.bottom() - 6.0),
                pos2(x, ruler.bottom()),
                pos2(x - 5.0, ruler.bottom() - 6.0),
            ],
            color::ACCENT,
            Stroke::NONE,
        ));
        p.vline(
            x,
            ruler.bottom() - 6.0..=ruler.bottom(),
            Stroke::new(1.0, color::ACCENT),
        );

        // Interaction: work area handles, then the ruler (CTI).
        let handle_w = 6.0;
        let wa_resp = ui
            .interact(
                wa.expand2(vec2(handle_w, 2.0)),
                ui.id().with("work_area"),
                Sense::click_and_drag(),
            )
            .labeled(egui::WidgetType::Slider, tr("Work Area"));
        let ruler_resp = ui
            .interact(
                Rect::from_min_max(pos2(ruler.left(), wa.bottom() + 1.0), ruler.max),
                ui.id().with("ruler"),
                Sense::click_and_drag(),
            )
            .labeled(egui::WidgetType::Slider, tr("Time Ruler"));
        if wa_resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        if wa_resp.drag_started()
            && let Some(pos) = wa_resp.interact_pointer_pos()
        {
            let near_start = (pos.x - wa.left()).abs() <= handle_w;
            let near_end = (pos.x - wa.right()).abs() <= handle_w;
            self.drag = Some(Drag::WorkArea {
                origin: pos.x,
                start: near_start || !near_end,
                end: near_end || !near_start,
            });
        }
        if wa_resp.drag_stopped() && matches!(self.drag, Some(Drag::WorkArea { .. })) {
            self.drag = None;
            if (ws, we) != (info.work_area.start, info.work_area.end) {
                cx.invoke_with(
                    "comp.set_work_area",
                    json!({"comp": info.path, "start": ws, "end": we}),
                );
            }
        }
        if (ruler_resp.clicked() || ruler_resp.dragged())
            && let Some(pos) = ruler_resp.interact_pointer_pos()
        {
            let sound = ui.input(|i| i.modifiers.command);
            if sound {
                session.retry_audio();
            }
            let mut t = axis.t(pos.x);
            // Shift: snap to keyframes, markers, in / out points and the work
            // area (AE).
            if ui.input(|i| i.modifiers.shift) {
                let now = session.lock().time(&info.path);
                // The targets start with the current time: the CTI itself.
                let targets: Vec<Time> = super::layer_bar::bar_targets(cx.stage, info, now, &[])
                    .into_iter()
                    .skip(1)
                    .collect();
                let tol = Time((f64::from(super::layer_bar::SNAP_PX) * axis.per_point()) as i64);
                if let Some(snap) = crate::graph::snap_to(t, &targets, tol) {
                    t = snap;
                }
            }
            session.lock().scrub_to(cx, info, t, sound);
            self.drag = Some(Drag::Cti);
        }
        if ruler_resp.drag_stopped() {
            session.lock().stop();
            self.drag = None;
        }
        // Comp markers between the work area bar and the ruler numbers (on top of the
        // ruler, so they take the clicks).
        let strip = Rect::from_min_max(
            pos2(ruler.left(), wa.bottom() + 1.0),
            pos2(ruler.right(), head.bottom()),
        );
        self.comp_markers_ui(ui, cx, session, info, axis, strip);
    }

    pub(super) fn footer(
        &mut self,
        ui: &mut Ui,
        foot: Rect,
        info: &CompInfo,
        axis: &TimeAxis,
        now: Time,
        graph_left: f32,
    ) {
        ui.painter().rect_filled(foot, 0.0, color::PANEL);
        // AE's pane buttons at the bottom left: show or hide the Layer Switches,
        // Transfer Controls (Modes), In/Out/Duration/Stretch and Render Time
        // columns, highlighted while shown.
        const PANES: [(Icon, &str, &[Column]); 4] = [
            (
                Icon::PaneSwitches,
                "Expand or Collapse the Layer Switches pane",
                &[Column::Switches],
            ),
            (
                Icon::PaneModes,
                "Expand or Collapse the Transfer Controls pane",
                &[Column::Modes],
            ),
            (
                Icon::PaneInOut,
                "Expand or Collapse the In/Out/Duration/Stretch panes",
                &[Column::In, Column::Out, Column::Duration, Column::Stretch],
            ),
            (
                Icon::PaneRenderTime,
                "Expand or Collapse the Render Time pane",
                &[Column::RenderTime],
            ),
        ];
        let size = (foot.height() - 4.0).min(20.0);
        let mut x = foot.left() + 6.0;
        for (i, (icon, label, cols)) in PANES.into_iter().enumerate() {
            let r = Rect::from_min_size(pos2(x, foot.center().y - size / 2.0), vec2(size, size));
            x += size + 2.0;
            let shown = cols.iter().any(|c| self.columns.visible(*c));
            let resp = ui
                .interact(r, ui.id().with(("pane", i)), Sense::click())
                .labeled_toggle(egui::WidgetType::Button, tr(label), shown)
                .on_hover_text(tr(label));
            if shown {
                ui.painter().rect_filled(r, 2.0, color::BUTTON_ON);
            }
            icons::paint(
                ui.painter(),
                r.shrink(2.0),
                icon,
                if shown || resp.hovered() {
                    color::ICON
                } else {
                    color::ICON_DIM
                },
            );
            if resp.clicked() {
                self.columns.toggle_pane(cols);
            }
            #[cfg(test)]
            {
                self.last_panes[i] = Some(r);
            }
        }
        // Toggle Switches / Modes (AE's button under the outline; F4).
        let b = Rect::from_min_size(
            pos2(x + 8.0, foot.top() + 3.0),
            vec2(150.0, foot.height() - 6.0),
        );
        let resp = ui
            .interact(b, ui.id().with("toggle_switches_modes"), Sense::click())
            .labeled(egui::WidgetType::Button, tr("Toggle Switches / Modes"));
        ui.painter().text(
            b.left_center(),
            Align2::LEFT_CENTER,
            tr("Toggle Switches / Modes"),
            theme::small(),
            if resp.hovered() {
                color::TEXT_STRONG
            } else {
                color::TEXT_DIM
            },
        );
        if resp.clicked() {
            self.columns.toggle_switches_modes();
        }
        ui.painter().hline(
            foot.x_range(),
            foot.top() + 0.5,
            Stroke::new(1.0, color::SEPARATOR),
        );
        // Zoom slider (AE's mountain icons).
        let r = Rect::from_min_max(
            pos2(graph_left + 8.0, foot.top() + 2.0),
            pos2(
                (graph_left + 220.0).min(foot.right() - 8.0),
                foot.bottom() - 2.0,
            ),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(r)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        icons::show(&mut child, Icon::MountainSmall, 14.0, color::ICON_DIM);
        let mut z = self.zoom;
        child.spacing_mut().slider_width = (r.width() - 40.0).max(40.0);
        if child
            .add(egui::Slider::new(&mut z, 0.0..=1.0).show_value(false))
            .labeled_value(
                egui::WidgetType::Slider,
                tr("Zoom"),
                &format!("{:.0}%", z * 100.0),
            )
            .changed()
        {
            self.zoom = z;
            let a = self.axis(info, 0.0, 1000.0);
            self.scroll = now - Time(a.span.0 / 2);
        }
        icons::show(&mut child, Icon::MountainLarge, 14.0, color::ICON_DIM);
        // Horizontal scroll when zoomed: shift+wheel / trackpad.
        if self.zoom > 0.0
            && ui.rect_contains_pointer(Rect::from_min_max(pos2(graph_left, 0.0), foot.max))
        {
            let dx = ui.input(|i| i.smooth_scroll_delta.x);
            if dx != 0.0 {
                self.scroll = axis.start - Time((f64::from(dx) * axis.per_point()) as i64);
            }
        }
    }
}
