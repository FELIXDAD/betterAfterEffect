//! Comp markers (in the time ruler, with the marker bin at its right end) and layer
//! markers (on the layer bars): drag to move (Shift snaps), drag a new comp marker
//! out of the bin, double-click for the Marker dialog, right-click for the marker
//! menu. Every change is one `marker.*` command.

use bae_motion::markers::{marker_time, markers_of};

use super::*;

/// Half width of a marker glyph.
const MARK_W: f32 = 5.0;

/// A marker as drawn: its prim, its comp time, comment, duration (comp time) and
/// label.
struct Shown {
    path: PrimPath,
    time: Time,
    end: Time,
    comment: String,
    label: usize,
}

fn shown(stage: &Stage, owner: &PrimPath) -> Vec<Shown> {
    markers_of(stage, owner)
        .into_iter()
        .map(|(path, t)| {
            let view = stage.prim(&path);
            let get = |a: &str| view.as_ref().and_then(|v| query::static_value(v, a));
            let duration = get("duration")
                .and_then(|v| v.as_time())
                .unwrap_or_default();
            let time = query::to_comp_time(stage, &path, t);
            Shown {
                end: query::to_comp_time(stage, &path, t + duration),
                time,
                comment: get("comment")
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default(),
                label: get("label").and_then(|v| v.as_f64()).unwrap_or(0.0) as usize,
                path,
            }
        })
        .collect()
}

/// A marker glyph: a small flag with a point at the bottom, at `x` with its point
/// on `bottom`.
fn paint_marker(p: &egui::Painter, x: f32, bottom: f32, fill: Color32) {
    p.add(Shape::convex_polygon(
        vec![
            pos2(x - MARK_W, bottom - 10.0),
            pos2(x + MARK_W, bottom - 10.0),
            pos2(x + MARK_W, bottom - 4.0),
            pos2(x, bottom),
            pos2(x - MARK_W, bottom - 4.0),
        ],
        fill,
        Stroke::new(1.0, Color32::from_gray(20)),
    ));
}

fn marker_fill(label: usize) -> Color32 {
    if label == 0 {
        color::TEXT
    } else {
        theme::label_color(label)
    }
}

impl TimelinePanel {
    /// Comp time of a marker being dragged (the drag preview), else its own.
    fn marker_preview(&self, path: &PrimPath, t: Time) -> Time {
        match &self.marker_plan {
            Some((m, d)) if m == path => t + *d,
            _ => t,
        }
    }

    /// The markers of `owner` along `strip` (bottom = the glyphs' points), with their
    /// interactions.
    #[allow(clippy::too_many_arguments)]
    fn markers_ui(
        &mut self,
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        axis: &TimeAxis,
        owner: &PrimPath,
        strip: Rect,
    ) {
        let stage = cx.stage;
        let p = ui.painter().with_clip_rect(strip);
        let locked = query::locked_layer_of(stage, owner).is_some();
        for m in shown(stage, owner) {
            let t = self.marker_preview(&m.path, m.time);
            let x = axis.x(t);
            if x < strip.left() - 200.0 || x > strip.right() + MARK_W {
                continue;
            }
            let fill = marker_fill(m.label);
            if m.end > m.time {
                let xe = axis.x(self.marker_preview(&m.path, m.end));
                p.rect_filled(
                    Rect::from_min_max(
                        pos2(x, strip.bottom() - 9.0),
                        pos2(xe, strip.bottom() - 5.0),
                    ),
                    1.0,
                    fill.gamma_multiply(0.5),
                );
            }
            paint_marker(&p, x, strip.bottom(), fill);
            if !m.comment.is_empty() {
                p.text(
                    pos2(x + MARK_W + 3.0, strip.bottom() - 6.0),
                    Align2::LEFT_CENTER,
                    &m.comment,
                    theme::small(),
                    color::TEXT,
                );
            }
            let hit = Rect::from_center_size(
                pos2(x, strip.bottom() - 5.0),
                vec2(2.0 * MARK_W + 2.0, 12.0),
            );
            #[cfg(test)]
            self.last_markers.push((m.path.clone(), hit.center()));
            let sense = if locked {
                Sense::hover()
            } else {
                Sense::click_and_drag()
            };
            let resp = ui
                .interact(hit, ui.id().with(("marker", m.path.as_str())), sense)
                .labeled(
                    egui::WidgetType::Button,
                    &bae_ui::a11y::context(&[
                        tr("Marker"),
                        &m.comment,
                        &common::timecode(info, m.time),
                    ]),
                );
            if resp.drag_started()
                && let Some(pos) = ui
                    .input(|i| i.pointer.press_origin())
                    .or(resp.interact_pointer_pos())
            {
                self.drag = Some(Drag::Marker {
                    origin: pos.x,
                    marker: m.path.clone(),
                });
            }
            if resp.double_clicked() {
                session.lock().key_dialog = MarkerDialog::edit(stage, info, &m.path)
                    .map(|d| KeyDialog::Marker(Box::new(d)));
            }
            let path = m.path.clone();
            let owner = owner.clone();
            crate::common::context_menu(&resp, &mut |ui| {
                marker_menu(ui, cx, session, info, &owner, &path)
            });
        }
    }

    /// Comp markers in the ruler strip and the marker bin at its right end (drag a
    /// new comp marker out of it).
    pub(super) fn comp_markers_ui(
        &mut self,
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        axis: &TimeAxis,
        strip: Rect,
    ) {
        let bin = Rect::from_min_max(pos2(strip.right() - 18.0, strip.top()), strip.max);
        #[cfg(test)]
        {
            self.last_marker_bin = Some(bin);
        }
        let marks = Rect::from_min_max(strip.min, pos2(bin.left() - 2.0, strip.bottom()));
        self.markers_ui(ui, cx, session, info, axis, &info.path, marks);
        let p = ui.painter();
        p.rect_filled(bin, 2.0, color::PANEL);
        paint_marker(p, bin.center().x, bin.bottom() - 1.0, color::TEXT_DIM);
        let resp = ui
            .interact(bin, ui.id().with("marker_bin"), Sense::drag())
            .labeled(egui::WidgetType::Button, tr("Comp Marker Bin"))
            .on_hover_text(tr("Drag to create a comp marker"));
        if resp.drag_started() {
            self.drag = Some(Drag::MarkerFromBin);
        }
        if matches!(self.drag, Some(Drag::MarkerFromBin))
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            paint_marker(
                &ui.painter().with_clip_rect(marks),
                pos.x,
                strip.bottom(),
                color::TEXT,
            );
            if ui.input(|i| i.pointer.any_released()) {
                self.drag = None;
                if marks.x_range().contains(pos.x) {
                    let t = info
                        .frame_rate
                        .snap(axis.t(pos.x) + Time(info.frame_duration().0 / 2));
                    cx.invoke_with(
                        "marker.add",
                        json!({"owner": info.path, "time": t.clamp(Time::ZERO, info.duration)}),
                    );
                }
            }
        }
    }

    /// Layer markers on a layer's bar row.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn layer_markers_ui(
        &mut self,
        ui: &Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        info: &CompInfo,
        axis: &TimeAxis,
        layer: &PrimPath,
        rect: Rect,
    ) {
        let strip = Rect::from_min_max(pos2(axis.left, rect.top()), rect.max);
        self.markers_ui(
            ui,
            cx,
            session,
            info,
            axis,
            layer,
            strip.shrink2(vec2(0.0, 1.0)),
        );
    }

    /// Where the marker drag puts the marker this frame: (marker, comp-time
    /// offset), whole frames or (Shift) snapped to the CTI, keyframes, In / Out
    /// points and other markers.
    pub(super) fn plan_marker_drag(
        &self,
        stage: &Stage,
        info: &CompInfo,
        axis: &TimeAxis,
        now: Time,
        pointer_x: Option<f32>,
        shift: bool,
    ) -> Option<(PrimPath, Time)> {
        let Some(Drag::Marker { origin, marker }) = &self.drag else {
            return None;
        };
        let frame = info.frame_duration();
        let t0 = query::to_comp_time(stage, marker, marker_time(stage, marker));
        let raw =
            Time((f64::from(pointer_x.unwrap_or(*origin) - origin) * axis.per_point()) as i64);
        let mut delta = Time((raw.0 as f64 / frame.0 as f64).round() as i64 * frame.0);
        if shift {
            let mut targets = vec![
                now,
                Time::ZERO,
                info.duration,
                info.work_area.start,
                info.work_area.end,
            ];
            for l in query::comp_layers(stage, &info.path) {
                let t = query::layer_timing(stage, &l);
                targets.push(t.in_point);
                targets.push(t.out_point);
                targets.extend(crate::actions::layer_key_times(stage, &l));
                for (m, t) in markers_of(stage, &l) {
                    if m != *marker {
                        targets.push(query::to_comp_time(stage, &m, t));
                    }
                }
            }
            for (m, t) in markers_of(stage, &info.path) {
                if m != *marker {
                    targets.push(t);
                }
            }
            let tol = Time((8.0 * axis.per_point()) as i64);
            if let Some(t) = crate::graph::snap_to(t0 + raw, &targets, tol) {
                delta = t - t0;
            }
        }
        Some((marker.clone(), delta))
    }

    /// Release of a marker drag: one `marker.move` in the marker's own time base.
    pub(super) fn commit_marker_drag(
        &self,
        cx: &mut PanelContext<'_>,
        marker: &PrimPath,
        delta: Time,
    ) {
        let stage = cx.stage;
        let mt = marker_time(stage, marker);
        let ct = query::to_comp_time(stage, marker, mt);
        let d = query::to_key_time(stage, marker, ct + delta) - mt;
        if d != Time::ZERO {
            cx.invoke_with("marker.move", json!({"markers": [marker], "delta": d}));
        }
    }
}

/// Right-click on a marker: AE 26's marker menu.
fn marker_menu(
    ui: &mut Ui,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    info: &CompInfo,
    owner: &PrimPath,
    marker: &PrimPath,
) {
    let stage = cx.stage;
    if ui.button(tr("Settings...")).clicked() {
        session.lock().key_dialog =
            MarkerDialog::edit(stage, info, marker).map(|d| KeyDialog::Marker(Box::new(d)));
        ui.close();
    }
    ui.separator();
    ui.add_enabled(false, egui::Button::new(tr("Update Markers From Source")));
    if ui.button(tr("Go To Marker Time")).clicked() {
        let t = query::to_comp_time(stage, marker, marker_time(stage, marker));
        session.lock().set_time(info, t);
        ui.close();
    }
    if ui.button(tr("Delete This Marker")).clicked() {
        cx.invoke_with("marker.delete", json!({"markers": [marker]}));
        ui.close();
    }
    if ui.button(tr("Delete All Markers")).clicked() {
        let all: Vec<PrimPath> = markers_of(stage, owner)
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        cx.invoke_with("marker.delete", json!({"markers": all}));
        ui.close();
    }
    ui.separator();
    ui.add_enabled(false, egui::Button::new(tr("Lock Markers")));
    if *owner == info.path {
        ui.separator();
        let mut on = stage
            .prim(marker)
            .and_then(|v| query::static_value(&v, "protected_region"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if widgets::checkbox(ui, &mut on, tr("Enable Protected Region")).clicked() {
            cx.invoke_with(
                "marker.edit",
                json!({"path": marker, "protected_region": on}),
            );
            ui.close();
        }
    }
}

// ---------------------------------------------------------------------------
// Marker dialog.

/// The Marker dialog (double-click a marker, Settings…, Alt+* for a new one).
#[derive(Clone, Debug)]
pub struct MarkerDialog {
    /// The marker edited, or None for a new marker on `owner`.
    pub marker: Option<PrimPath>,
    pub owner: PrimPath,
    pub info: CompInfo,
    /// Comp time and duration.
    pub time: Time,
    pub duration: Time,
    pub comment: String,
    pub label: usize,
    pub protected_region: bool,
}

impl MarkerDialog {
    /// The dialog for an existing marker.
    pub fn edit(stage: &Stage, info: &CompInfo, marker: &PrimPath) -> Option<Self> {
        let view = stage.prim(marker)?;
        let get = |a: &str| query::static_value(&view, a);
        let s = |a: &str| {
            get(a)
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default()
        };
        let owner = marker.parent()?.parent()?;
        let mt = marker_time(stage, marker);
        let d = get("duration")
            .and_then(|v| v.as_time())
            .unwrap_or_default();
        let time = query::to_comp_time(stage, marker, mt);
        Some(Self {
            marker: Some(marker.clone()),
            owner,
            info: info.clone(),
            time,
            duration: query::to_comp_time(stage, marker, mt + d) - time,
            comment: s("comment"),
            label: get("label").and_then(|v| v.as_f64()).unwrap_or(0.0) as usize,
            protected_region: get("protected_region")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }

    /// The dialog for a new marker on `owner` at comp time `time`.
    pub fn new_marker(info: &CompInfo, owner: PrimPath, time: Time) -> Self {
        Self {
            marker: None,
            owner,
            info: info.clone(),
            time,
            duration: Time::ZERO,
            comment: String::new(),
            label: 0,
            protected_region: false,
        }
    }

    /// The command OK runs.
    pub(super) fn command(&self, stage: &Stage) -> (&'static str, serde_json::Value) {
        // Marker time base: comp markers comp time, layer markers layer time.
        let reference = self.marker.clone().unwrap_or_else(|| self.owner.clone());
        let t = query::to_key_time(stage, &reference, self.time);
        let end = query::to_key_time(stage, &reference, self.time + self.duration);
        let mut fields = json!({
            "time": t,
            "duration": (end - t).max(Time::ZERO),
            "comment": self.comment,
            "label": self.label,
        });
        if self.owner == self.info.path {
            fields["protected_region"] = json!(self.protected_region);
        }
        match &self.marker {
            Some(m) => {
                fields["path"] = json!(m);
                ("marker.edit", fields)
            }
            None => {
                fields["owner"] = json!(self.owner);
                ("marker.add", fields)
            }
        }
    }

    pub(super) fn title(&self) -> &'static str {
        if self.owner == self.info.path {
            "Composition Marker"
        } else {
            "Layer Marker"
        }
    }
}

/// A time field in frames, shown and typed as timecode (or a frame number).
fn time_field(ui: &mut Ui, info: &CompInfo, t: &mut Time, label: &str) {
    let frame = info.frame_duration().0.max(1);
    let mut frames = (t.0 as f64 / frame as f64).round() as i64;
    let fmt_info = info.clone();
    let parse_info = info.clone();
    let resp = ui
        .add(
            egui::DragValue::new(&mut frames)
                .range(0..=i64::MAX / frame)
                .custom_formatter(move |v, _| {
                    common::timecode(&fmt_info, Time(v as i64 * fmt_info.frame_duration().0))
                })
                .custom_parser(move |s| parse_timecode(&parse_info, s).map(|f| f as f64)),
        )
        .labeled(egui::WidgetType::DragValue, tr(label));
    if resp.changed() {
        *t = Time(frames * frame);
    }
}

/// `h:mm:ss:ff` / `h;mm;ss;ff` (any number of fields, from the right) or a plain
/// frame count → frames.
pub(crate) fn parse_timecode(info: &CompInfo, s: &str) -> Option<i64> {
    let fps = info.frame_rate.as_f64().round().max(1.0) as i64;
    let parts: Vec<&str> = s.trim().split([':', ';', '.']).collect();
    if parts.len() == 1 {
        return parts[0].trim().parse::<i64>().ok();
    }
    let nums: Vec<i64> = parts
        .iter()
        .map(|p| p.trim().parse::<i64>().ok())
        .collect::<Option<Vec<_>>>()?;
    let mut it = nums.iter().rev();
    let f = *it.next()?;
    let sec = *it.next().unwrap_or(&0);
    let min = *it.next().unwrap_or(&0);
    let hour = *it.next().unwrap_or(&0);
    Some(((hour * 60 + min) * 60 + sec) * fps + f)
}

pub(super) fn marker_body(ui: &mut Ui, d: &mut MarkerDialog) {
    let info = d.info.clone();
    let label_w = vec2(90.0, 18.0);
    ui.horizontal(|ui| {
        ui.add_sized(label_w, egui::Label::new(tr("Time:")));
        time_field(ui, &info, &mut d.time, "Time:");
        ui.add_space(12.0);
        ui.label(tr("Duration:"));
        time_field(ui, &info, &mut d.duration, "Duration:");
    });
    ui.add_space(4.0);
    ui.label(tr("Comment:"));
    ui.add(
        egui::TextEdit::multiline(&mut d.comment)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    )
    .labeled(egui::WidgetType::TextEdit, tr("Comment:"));
    ui.horizontal(|ui| {
        ui.add_sized(label_w, egui::Label::new(tr("Label:")));
        let names: Vec<&str> = theme::LABELS.iter().map(|(n, _)| tr(n)).collect();
        widgets::dropdown(ui, "marker_label", &mut d.label, &names, 140.0);
    });
    if d.owner == d.info.path {
        widgets::checkbox(ui, &mut d.protected_region, tr("Protected Region"));
    } else {
        // Layer markers: Comment Marker / Segment Marker (AE 26). The model has no
        // marker type yet, so only the default is shown.
        ui.add_enabled_ui(false, |ui| {
            ui.horizontal(|ui| {
                let _ = ui.radio(true, tr("Comment Marker"));
                let _ = ui.radio(false, tr("Segment Marker"));
            });
        });
    }
}
