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
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        head: Rect,
        colrow: Rect,
        cols: &Cols,
        axis: &TimeAxis,
        now: Time,
    ) {
        let p = ui.painter();
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

        // Graph Editor switch (AE's comp button; Shift+F3).
        let ge = Rect::from_center_size(
            pos2(head.left() + cols.end - 14.0, head.center().y),
            vec2(20.0, 20.0),
        );
        let on = session.lock().graph_editor;
        let gresp = ui
            .interact(ge, ui.id().with("graph_editor_switch"), Sense::click())
            .labeled_toggle(egui::WidgetType::Button, tr("Graph Editor"), on);
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
        if gresp.clicked() {
            session.lock().graph_editor = !on;
        }
        gresp.on_hover_text(tr("Graph Editor"));

        // Column headers.
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
        {
            icons::paint(
                p,
                Rect::from_center_size(
                    pos2(l + cols.av + SW * (i as f32 + 0.5), cy),
                    vec2(12.0, 12.0),
                ),
                icon,
                color::ICON_DIM,
            );
        }
        icons::paint(
            p,
            Rect::from_center_size(pos2(l + cols.label + 7.0, cy), vec2(12.0, 12.0)),
            Icon::Tag,
            color::ICON_DIM,
        );
        p.text(
            pos2(l + cols.index + 4.0, cy),
            Align2::LEFT_CENTER,
            "#",
            theme::small(),
            color::TEXT,
        );
        p.text(
            pos2(l + cols.name + 2.0, cy),
            Align2::LEFT_CENTER,
            tr("Layer Name"),
            theme::small(),
            color::TEXT,
        );
        for (i, icon) in [
            Icon::Shy,
            Icon::Collapse,
            Icon::Quality,
            Icon::Fx,
            Icon::FrameBlend,
            Icon::MotionBlur,
            Icon::Adjustment,
            Icon::Cube,
        ]
        .into_iter()
        .enumerate()
        {
            icons::paint(
                p,
                Rect::from_center_size(
                    pos2(l + cols.switches + SW * (i as f32 + 0.5), cy),
                    vec2(12.0, 12.0),
                ),
                icon,
                color::ICON_DIM,
            );
        }
        p.text(
            pos2(l + cols.parent + PARENT_PAD, cy),
            Align2::LEFT_CENTER,
            tr("Parent & Link"),
            theme::small(),
            color::TEXT,
        );

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
            let mut s = session.lock();
            s.stop();
            s.set_time(info, axis.t(pos.x));
            self.drag = Some(Drag::Cti);
        }
        if ruler_resp.drag_stopped() {
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
