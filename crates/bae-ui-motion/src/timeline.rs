//! Timeline panel: AE's layer outline (A/V switches, label, #, name, switches,
//! Parent & Link with pick whip), property rows with stopwatches, keyframe
//! navigators, values and expressions, and the time graph (comp tabs, current time,
//! ruler, work area, CTI, layer duration bars, keyframes).
//!
//! Every edit is a command: switches → `attr.set`, values → `attr.set_at`, stopwatch
//! → `attr.set_animated`, dragging bars → `layer.shift_time` / `layer.trim`, dragging
//! keyframes → `key.move`, work area → `comp.set_work_area`, reordering →
//! `layer.reorder`, parenting → `layer.set_parent`. Drags preview locally and run
//! one command when released.

use std::collections::HashSet;

use bae_model::{Interpolation, Keyframe, LayerKind, PrimPath, Stage, Time};
use bae_motion::KeyRef;
use bae_motion::query::{self, CompInfo};
use bae_ui::Accessible as _;
use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use bae_ui::widgets::{self, KeyNav};
use bae_ui::{Invocation, Panel, PanelContext, TabInfo, tr};
use egui::{Align2, Color32, CursorIcon, Id, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use serde_json::json;

use crate::actions::{self, ids};
use crate::common::{self, EvalCache};
use crate::project::ItemDrag;
use crate::props::{self, Node};
use crate::session::{Session, SharedSession};

const ROW_H: f32 = 19.0;
const TABS_H: f32 = 22.0;
const HEAD_H: f32 = 34.0;
const COLS_H: f32 = 20.0;
const FOOT_H: f32 = 22.0;
const SW: f32 = 18.0;
/// Left padding of the time graph so frame 0 is not on the edge.
const PAD: f32 = 8.0;

/// Outline column x offsets (relative to the panel's left edge).
struct Cols {
    av: f32,
    label: f32,
    index: f32,
    name: f32,
    switches: f32,
    parent: f32,
    end: f32,
}

impl Cols {
    fn new(width: f32) -> Cols {
        let av = 4.0;
        let label = av + 4.0 * SW + 4.0;
        let index = label + 18.0;
        let name = index + 26.0;
        let name_w = 150.0_f32.max(width * 0.0);
        let switches = name + name_w;
        let parent = switches + 7.0 * SW + 6.0;
        let end = parent + 128.0;
        Cols {
            av,
            label,
            index,
            name,
            switches,
            parent,
            end,
        }
    }
}

#[derive(Clone, Debug)]
enum RowKind {
    Layer,
    Group {
        id: String,
        label: String,
        open: bool,
    },
    Prop {
        path: PrimPath,
        attr: String,
        label: String,
    },
    Expression {
        path: PrimPath,
        attr: String,
    },
}

#[derive(Clone, Debug)]
struct Row {
    kind: RowKind,
    layer: PrimPath,
    depth: usize,
}

#[derive(Clone, Debug)]
enum Drag {
    Cti,
    Keys { origin: f32, keys: Vec<KeyRef> },
    Move { origin: f32, layers: Vec<PrimPath> },
    TrimIn { origin: f32, layer: PrimPath },
    TrimOut { origin: f32, layer: PrimPath },
    WorkArea { origin: f32, start: bool, end: bool },
    Marquee { origin: Pos2, additive: bool },
    PickWhip { layers: Vec<PrimPath>, origin: Pos2 },
    Reorder { layers: Vec<PrimPath> },
}

#[derive(Default)]
pub struct TimelinePanel {
    eval: EvalCache,
    expanded: HashSet<String>,
    /// 0 = whole comp, 1 = a few frames.
    zoom: f32,
    /// Start of the visible range when zoomed (comp time).
    scroll: Time,
    drag: Option<Drag>,
    rename: Option<(PrimPath, String)>,
    title: String,
    /// Expression sources being edited, by `path.attr`.
    expr_edit: Option<(PrimPath, String, String)>,
}

/// Comp time ↔ x in the graph.
#[derive(Clone, Copy)]
struct TimeAxis {
    left: f32,
    width: f32,
    start: Time,
    span: Time,
}

impl TimeAxis {
    fn x(&self, t: Time) -> f32 {
        self.left
            + PAD
            + (t - self.start).0 as f32 / self.span.0.max(1) as f32 * (self.width - 2.0 * PAD)
    }

    fn t(&self, x: f32) -> Time {
        let f = (x - self.left - PAD) / (self.width - 2.0 * PAD).max(1.0);
        self.start + Time((f64::from(f) * self.span.0 as f64).round() as i64)
    }

    /// Time per point.
    fn per_point(&self) -> f64 {
        self.span.0 as f64 / f64::from((self.width - 2.0 * PAD).max(1.0))
    }
}

fn flatten_tree(
    nodes: &[Node],
    layer: &PrimPath,
    depth: usize,
    expanded: &HashSet<String>,
    stage: &Stage,
    out: &mut Vec<Row>,
) {
    for n in nodes {
        match n {
            Node::Group {
                id,
                label,
                children,
            } => {
                let open = expanded.contains(id);
                out.push(Row {
                    kind: RowKind::Group {
                        id: id.clone(),
                        label: label.clone(),
                        open,
                    },
                    layer: layer.clone(),
                    depth,
                });
                if open {
                    flatten_tree(children, layer, depth + 1, expanded, stage, out);
                }
            }
            Node::Prop { path, attr, label } => {
                push_prop(path, attr, label, layer, depth, stage, out);
            }
        }
    }
}

fn push_prop(
    path: &PrimPath,
    attr: &str,
    label: &str,
    layer: &PrimPath,
    depth: usize,
    stage: &Stage,
    out: &mut Vec<Row>,
) {
    out.push(Row {
        kind: RowKind::Prop {
            path: path.clone(),
            attr: attr.to_owned(),
            label: label.to_owned(),
        },
        layer: layer.clone(),
        depth,
    });
    if query::attr_spec(stage, path, attr).is_some_and(|s| s.expression.is_some()) {
        out.push(Row {
            kind: RowKind::Expression {
                path: path.clone(),
                attr: attr.to_owned(),
            },
            layer: layer.clone(),
            depth: depth + 1,
        });
    }
}

fn bool_attr(stage: &Stage, path: &PrimPath, attr: &str) -> bool {
    stage
        .prim(path)
        .and_then(|v| query::static_value(&v, attr))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn label_of(stage: &Stage, path: &PrimPath) -> usize {
    stage
        .prim(path)
        .and_then(|v| query::static_value(&v, "label"))
        .and_then(|v| v.as_f64())
        .map_or(0, |v| v as usize)
}

/// AE keyframe glyph: each half shows its side's interpolation.
fn paint_key(p: &egui::Painter, c: Pos2, k: &Keyframe, selected: bool) {
    let fill = if selected {
        color::ACCENT
    } else {
        color::KEYFRAME
    };
    let stroke = Stroke::new(1.0, Color32::from_gray(20));
    let r = 5.0;
    let half = |left: bool, interp: Interpolation, auto: bool| -> Vec<Pos2> {
        let s = if left { -1.0 } else { 1.0 };
        match interp {
            Interpolation::Hold => vec![
                c + vec2(0.0, -r),
                c + vec2(s * r * 0.8, -r),
                c + vec2(s * r * 0.8, r),
                c + vec2(0.0, r),
            ],
            Interpolation::Bezier if auto => (0..=8)
                .map(|i| {
                    let a = std::f32::consts::PI * (i as f32 / 8.0) - std::f32::consts::FRAC_PI_2;
                    c + vec2(s * a.cos() * r, a.sin() * r)
                })
                .collect(),
            Interpolation::Bezier => vec![
                c + vec2(0.0, -r),
                c + vec2(s * r * 0.75, -r),
                c + vec2(s * r * 0.2, 0.0),
                c + vec2(s * r * 0.75, r),
                c + vec2(0.0, r),
            ],
            Interpolation::Linear => {
                vec![c + vec2(0.0, -r), c + vec2(s * r, 0.0), c + vec2(0.0, r)]
            }
        }
    };
    let auto = k.flags.temporal_auto_bezier;
    for (left, interp) in [(true, k.interp_in), (false, k.interp_out)] {
        let pts = half(left, interp, auto);
        p.add(Shape::convex_polygon(pts.clone(), fill, Stroke::NONE));
        p.add(Shape::line(pts, stroke));
    }
}

/// Ruler step in frames so labels are at least `min_px` apart.
fn ruler_step(axis: &TimeAxis, info: &CompInfo, min_px: f32) -> i64 {
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

fn ruler_label(frame: i64, info: &CompInfo) -> String {
    let fps = info.frame_rate.as_f64().round().max(1.0) as i64;
    if frame % fps == 0 {
        format!("{}s", frame / fps)
    } else {
        format!("{}f", frame % fps)
    }
}

impl TimelinePanel {
    fn axis(&self, info: &CompInfo, left: f32, width: f32) -> TimeAxis {
        let full = info.duration;
        let min = Time(info.frame_duration().0 * 10).min(full);
        let f = (1.0 - self.zoom).clamp(0.0, 1.0) as f64;
        let span = Time((min.0 as f64 + (full.0 - min.0) as f64 * f * f * f) as i64).max(Time(1));
        let start = self.scroll.clamp(Time::ZERO, (full - span).max(Time::ZERO));
        TimeAxis {
            left,
            width,
            start,
            span,
        }
    }

    fn rows(&self, stage: &Stage, comp: &PrimPath, s: &Session) -> Vec<Row> {
        let mut out = Vec::new();
        for layer in query::comp_layers(stage, comp) {
            out.push(Row {
                kind: RowKind::Layer,
                layer: layer.clone(),
                depth: 0,
            });
            if let Some(r) = s.reveal.get(&layer) {
                for n in props::revealed(stage, &layer, r) {
                    if let Node::Prop { path, attr, label } = n {
                        push_prop(&path, &attr, &label, &layer, 1, stage, &mut out);
                    }
                }
            } else if self.expanded.contains(layer.as_str()) {
                flatten_tree(
                    &props::layer_tree(stage, &layer),
                    &layer,
                    1,
                    &self.expanded,
                    stage,
                    &mut out,
                );
            }
        }
        out
    }

    fn toggle(&mut self, id: &str) {
        if !self.expanded.remove(id) {
            self.expanded.insert(id.to_owned());
        }
    }

    /// Zoom keeping the current time in view.
    fn zoom_step(&mut self, by: f32, info: &CompInfo, now: Time) {
        self.zoom = (self.zoom + by).clamp(0.0, 1.0);
        let axis = self.axis(info, 0.0, 1000.0);
        self.scroll = now - Time(axis.span.0 / 2);
    }
}

/// AE's name of a layer switch, for screen readers.
fn switch_name(icon: Icon) -> &'static str {
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

fn switch(ui: &Ui, rect: Rect, on: bool, icon: Icon, id: Id) -> bool {
    let resp = ui.interact(rect, id, Sense::click()).labeled_toggle(
        egui::WidgetType::Checkbox,
        tr(switch_name(icon)),
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
    #[cfg(test)]
    pub(crate) fn start_rename(&mut self, layer: PrimPath, text: &str) {
        self.rename = Some((layer, text.to_owned()));
    }
}

impl Panel for TimelinePanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        if self.title.is_empty() {
            TabInfo::new(tr("Timeline"))
        } else {
            TabInfo::new(self.title.clone())
        }
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        Some(json!({"zoom": self.zoom}))
    }

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        match action.id.as_str() {
            "timeline.zoom_in" | "timeline.zoom_out" => {
                if let Some(info) = common::active_comp(cx.stage, &session) {
                    let now = session.lock().time(&info.path);
                    let by = if action.id == "timeline.zoom_in" {
                        0.1
                    } else {
                        -0.1
                    };
                    self.zoom_step(by, &info, now);
                }
                true
            }
            _ => actions::handle(action, cx, &session),
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        session.lock().sync(cx.stage);
        let stage = cx.stage;
        let full = ui.available_rect_before_wrap();
        ui.painter().rect_filled(full, 0.0, color::PANEL);

        // Comp tabs.
        let tabs = Rect::from_min_size(full.min, vec2(full.width(), TABS_H));
        comp_tabs(ui, tabs, stage, &session);

        let Some(info) = common::active_comp(stage, &session) else {
            self.title.clear();
            let rest = Rect::from_min_max(pos2(full.left(), tabs.bottom()), full.max);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rest));
            common::empty_hint(
                &mut child,
                tr("No composition"),
                tr("Open a composition from the Project panel, or create one."),
            );
            return;
        };
        self.title = format!("{} {}", tr("Timeline"), info.name);
        if session.lock().tick(&info) {
            ui.ctx().request_repaint();
        }
        let now = session.lock().time(&info.path);
        let cols = Cols::new(full.width());
        let graph_left = full.left() + cols.end + 2.0;
        let graph_w = (full.right() - graph_left).max(40.0);
        let axis = self.axis(&info, graph_left, graph_w);

        let head = Rect::from_min_max(
            pos2(full.left(), tabs.bottom()),
            pos2(full.right(), tabs.bottom() + HEAD_H),
        );
        let colrow = Rect::from_min_max(
            pos2(full.left(), head.bottom()),
            pos2(full.right(), head.bottom() + COLS_H),
        );
        let foot = Rect::from_min_max(pos2(full.left(), full.bottom() - FOOT_H), full.max);
        let body = Rect::from_min_max(
            pos2(full.left(), colrow.bottom()),
            pos2(full.right(), foot.top()),
        );
        let graph_body = Rect::from_min_max(
            pos2(graph_left, body.top()),
            pos2(full.right(), body.bottom()),
        );

        self.header(ui, cx, &session, &info, head, colrow, &cols, &axis, now);

        // Rows.
        let rows = {
            let s = session.lock();
            self.rows(stage, &info.path, &s)
        };
        let comp_layers = query::comp_layers(stage, &info.path);

        // Background of the graph body: marquee selection and deselect.
        let bg = ui
            .interact(
                graph_body,
                ui.id().with("graph_bg"),
                Sense::click_and_drag(),
            )
            .labeled(egui::WidgetType::Panel, tr("Timeline"));
        if bg.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = bg.interact_pointer_pos()
        {
            self.drag = Some(Drag::Marquee {
                origin: p,
                additive: ui.input(|i| i.modifiers.shift),
            });
        }
        if bg.clicked() {
            let mut s = session.lock();
            s.selected_keys.clear();
            s.selected_layers.clear();
        }
        bg.context_menu(|ui| new_layer_menu(ui, cx, &session));
        // Dropped project items.
        if let Some(item) = bg.dnd_release_payload::<ItemDrag>() {
            crate::viewer::add_item_as_layer(cx, &session, &item.0, None);
        }

        let mut key_rects: Vec<(Rect, KeyRef)> = Vec::new();
        let mut layer_rows: Vec<(Rect, PrimPath)> = Vec::new();
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        egui::ScrollArea::vertical()
            .id_salt("timeline_rows")
            .auto_shrink([false, false])
            .show_rows(&mut child, ROW_H, rows.len(), |ui, range| {
                for i in range {
                    let row = &rows[i];
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(full.width(), ROW_H), Sense::hover());
                    self.row_ui(
                        ui,
                        cx,
                        &session,
                        &info,
                        &cols,
                        &axis,
                        row,
                        rect,
                        &comp_layers,
                        &mut key_rects,
                    );
                    if matches!(row.kind, RowKind::Layer) {
                        layer_rows.push((rect, row.layer.clone()));
                    }
                }
            });

        // Drags that span rows.
        self.finish_drags(
            ui,
            cx,
            &session,
            &info,
            &axis,
            graph_body,
            &key_rects,
            &layer_rows,
            &comp_layers,
        );

        // CTI through the body.
        let p = ui.painter().with_clip_rect(graph_body);
        let x = axis.x(now);
        p.vline(x, graph_body.y_range(), Stroke::new(1.0, color::ACCENT));

        self.footer(ui, foot, &info, &axis, now, graph_left);
    }
}

fn comp_tabs(ui: &Ui, rect: Rect, stage: &Stage, session: &SharedSession) {
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
            name,
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
            .labeled(egui::WidgetType::Button, tr("Close"));
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

fn new_layer_menu(ui: &mut Ui, cx: &mut PanelContext<'_>, session: &SharedSession) {
    ui.menu_button(tr("New"), |ui| {
        let items: [(&str, LayerKind, Option<&str>); 9] = [
            ("Text", LayerKind::Text, None),
            ("Solid", LayerKind::Solid, None),
            ("Light", LayerKind::Light, None),
            ("Camera", LayerKind::Camera, None),
            ("Null Object", LayerKind::Null, None),
            ("Shape Layer", LayerKind::Shape, None),
            ("Rectangle", LayerKind::Shape, Some("rect")),
            ("Ellipse", LayerKind::Shape, Some("ellipse")),
            ("Adjustment Layer", LayerKind::Adjustment, None),
        ];
        for (title, kind, shape) in items {
            if ui.button(tr(title)).clicked() {
                let mut params = json!({"kind": kind});
                if let Some(s) = shape {
                    params["shape"] = json!(s);
                }
                actions::handle(
                    &Invocation::with_params(ids::NEW_LAYER, params),
                    cx,
                    session,
                );
                ui.close();
            }
        }
    });
}

fn keyframe_menu(ui: &mut Ui, cx: &mut PanelContext<'_>, session: &SharedSession) {
    ui.menu_button(tr("Keyframe Assistant"), |ui| {
        for (title, id, params) in [
            ("Easy Ease", ids::EASE, json!({"mode": "easy_ease"})),
            ("Easy Ease In", ids::EASE, json!({"mode": "ease_in"})),
            ("Easy Ease Out", ids::EASE, json!({"mode": "ease_out"})),
            ("Time-Reverse Keyframes", ids::REVERSE_KEYS, json!({})),
        ] {
            if ui.button(tr(title)).clicked() {
                actions::handle(&Invocation::with_params(id, params), cx, session);
                ui.close();
            }
        }
    });
    if ui.button(tr("Toggle Hold Keyframe")).clicked() {
        actions::handle(&Invocation::new(ids::TOGGLE_HOLD), cx, session);
        ui.close();
    }
}

impl TimelinePanel {
    #[allow(clippy::too_many_arguments)]
    fn header(
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
            pos2(l + cols.name + 16.0, cy),
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
            pos2(l + cols.parent + 4.0, cy),
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
    }

    #[allow(clippy::too_many_arguments)]
    fn row_ui(
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
            let bg = match row.kind {
                RowKind::Layer if layer_selected => color::ROW_SELECTED,
                RowKind::Layer => color::ROW,
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
                    if switch(ui, av(i), on, icon, rid(attr)) {
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
                        tr(if open { "Collapse" } else { "Expand" }),
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
                        .labeled_toggle(egui::WidgetType::SelectableLabel, &name, layer_selected);
                    if resp.clicked() {
                        select_layer(session, layer, ui.input(|i| i.modifiers));
                    }
                    if resp.double_clicked() {
                        self.rename = Some((layer.clone(), name.clone()));
                    }
                    if resp.drag_started() {
                        if !layer_selected {
                            select_layer(session, layer, egui::Modifiers::NONE);
                        }
                        self.drag = Some(Drag::Reorder {
                            layers: session.lock().selected_layers.clone(),
                        });
                    }
                    resp.context_menu(|ui| {
                        if !session.lock().selected_layers.contains(layer) {
                            select_layer(session, layer, egui::Modifiers::NONE);
                        }
                        new_layer_menu(ui, cx, session);
                        ui.separator();
                        for (title, id) in [
                            ("Duplicate", ids::DUPLICATE),
                            ("Split Layer", ids::SPLIT),
                            ("Delete", ids::DELETE),
                        ] {
                            if ui.button(tr(title)).clicked() {
                                actions::handle(&Invocation::new(id), cx, session);
                                ui.close();
                            }
                        }
                        ui.separator();
                        keyframe_menu(ui, cx, session);
                    });
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
                    if switch(ui, sw(i), on, icon, rid(attr)) {
                        cx.invoke_with("attr.set", json!({"path": layer, "attr": attr, "value": {"type": "bool", "value": !on}}));
                    }
                }
                let draft = stage
                    .prim(layer)
                    .and_then(|v| query::static_value(&v, "quality"))
                    .is_some_and(|v| v.as_str() == Some("draft"));
                if switch(ui, sw(2), !draft, Icon::Quality, rid("quality")) {
                    let to = if draft { "best" } else { "draft" };
                    cx.invoke_with("attr.set", json!({"path": layer, "attr": "quality", "value": {"type": "enum", "value": to}}));
                }
                let three_d = bool_attr(stage, layer, "three_d");
                let cube = Rect::from_min_size(
                    pos2(l + cols.switches + SW * 7.0, rect.top()),
                    vec2(SW, ROW_H),
                );
                if switch(
                    ui,
                    cube.translate(vec2(-SW * 0.0, 0.0)),
                    three_d,
                    Icon::Cube,
                    rid("three_d"),
                ) {
                    cx.invoke_with("attr.set", json!({"path": layer, "attr": "three_d", "value": {"type": "bool", "value": !three_d}}));
                }
                // Parent & Link: pick whip and dropdown.
                let whip =
                    Rect::from_center_size(pos2(l + cols.parent + 8.0, cy), vec2(14.0, 14.0));
                icons::paint(ui.painter(), whip, Icon::Pickwhip, color::ICON);
                let wresp = ui
                    .interact(whip, rid("whip"), Sense::drag())
                    .labeled(egui::WidgetType::Button, tr("Parent Pick Whip"));
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
                    pos2(l + cols.parent + 18.0, rect.top() + 1.0),
                    pos2(l + cols.end - 4.0, rect.bottom() - 1.0),
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(drop_rect)
                        .id_salt(rid("parent")),
                );
                widgets::dropdown(
                    &mut child,
                    rid("parent_dd"),
                    &mut sel,
                    &items,
                    drop_rect.width(),
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
                self.layer_bar(ui, cx, session, info, axis, layer, rect, layer_selected);
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
                    .labeled_toggle(egui::WidgetType::CollapsingHeader, tr(label), *open);
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
    fn layer_bar(
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
                &query::display_name_of(stage, layer),
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
            select_layer(session, layer, ui.input(|i| i.modifiers));
        }
        if resp.drag_started()
            && let Some(p) = resp.interact_pointer_pos()
        {
            if !selected {
                select_layer(session, layer, egui::Modifiers::NONE);
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

    #[allow(clippy::too_many_arguments)]
    fn prop_row(
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
            match widgets::keyframe_nav(&mut child, has_prev, on_key, has_next) {
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
        if animatable && widgets::stopwatch(&mut child, &mut on).clicked() {
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
        ui.painter().text(
            pos2(x + 18.0, cy),
            Align2::LEFT_CENTER,
            label,
            theme::body(),
            name_color,
        );
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
            let edit = common::value_editor(&mut child, id.with("ed"), attr, &value, dims);
            common::apply_edit(cx, session, path, attr, now, edit);
        }
        // Keyframes.
        let graph = Rect::from_min_max(pos2(axis.left, rect.top()), rect.max);
        let p = ui.painter().with_clip_rect(graph);
        let selected_keys = session.lock().selected_keys.clone();
        let pointer_x = ui.input(|i| i.pointer.interact_pos()).map(|p| p.x);
        let frame = info.frame_duration();
        for (t, k) in &spec.keyframes {
            let kr = KeyRef::new(path.clone(), attr, *t);
            let selected = selected_keys.contains(&kr);
            let mut ct = query::to_comp_time(stage, path, *t);
            if let (Some(Drag::Keys { origin, keys }), Some(x)) = (&self.drag, pointer_x)
                && keys.contains(&kr)
            {
                let d = Time((f64::from(x - origin) * axis.per_point()) as i64);
                ct += Time((d.0 as f64 / frame.0 as f64).round() as i64 * frame.0);
            }
            let c = pos2(axis.x(ct), cy);
            if !graph.expand(6.0).contains(c) {
                continue;
            }
            paint_key(&p, c, k, selected);
            let hit = Rect::from_center_size(c, vec2(11.0, 13.0));
            key_rects.push((hit, kr.clone()));
            let resp = ui
                .interact(
                    hit,
                    ui.id().with(("key", path.as_str(), attr, t.0)),
                    Sense::click_and_drag(),
                )
                .labeled_toggle(egui::WidgetType::Button, tr("Keyframe"), selected);
            if resp.clicked() {
                let mods = ui.input(|i| i.modifiers);
                let mut s = session.lock();
                if mods.shift || mods.command {
                    if let Some(i) = s.selected_keys.iter().position(|x| *x == kr) {
                        s.selected_keys.remove(i);
                    } else {
                        s.selected_keys.push(kr.clone());
                    }
                } else {
                    s.selected_keys = vec![kr.clone()];
                }
            }
            if resp.double_clicked() {
                session.lock().set_time(info, ct);
            }
            if resp.drag_started()
                && let Some(pos) = resp.interact_pointer_pos()
            {
                let mut s = session.lock();
                if !s.selected_keys.contains(&kr) {
                    s.selected_keys = vec![kr.clone()];
                }
                self.drag = Some(Drag::Keys {
                    origin: pos.x,
                    keys: s.selected_keys.clone(),
                });
            }
            resp.context_menu(|ui| {
                if !session.lock().selected_keys.contains(&kr) {
                    session.lock().selected_keys = vec![kr.clone()];
                }
                keyframe_menu(ui, cx, session);
                if ui.button(tr("Delete")).clicked() {
                    actions::handle(&Invocation::new(ids::DELETE), cx, session);
                    ui.close();
                }
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn expression_row(
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
            .labeled(egui::WidgetType::Checkbox, tr("Enable Expression"));
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
        let r = ui.put(
            field,
            egui::TextEdit::singleline(&mut buf)
                .font(egui::TextStyle::Monospace)
                .text_color(color::TEXT_STRONG),
        );
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

    #[allow(clippy::too_many_arguments)]
    fn finish_drags(
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

    fn footer(
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

fn row_key(row: &Row) -> String {
    match &row.kind {
        RowKind::Layer => "layer".into(),
        RowKind::Group { id, .. } => id.clone(),
        RowKind::Prop { path, attr, .. } => format!("{path}.{attr}"),
        RowKind::Expression { path, attr } => format!("{path}.{attr}=expr"),
    }
}

fn select_layer(session: &SharedSession, layer: &PrimPath, mods: egui::Modifiers) {
    let mut s = session.lock();
    if mods.command || mods.shift {
        if let Some(i) = s.selected_layers.iter().position(|p| p == layer) {
            s.selected_layers.remove(i);
        } else {
            s.selected_layers.push(layer.clone());
        }
    } else {
        s.selected_layers = vec![layer.clone()];
        s.selected_keys.clear();
    }
}

/// AE's default expression text: the property's script name.
fn default_expression(attr: &str) -> String {
    match attr {
        "transform:anchor_point" => "transform.anchorPoint".into(),
        "transform:position" => "transform.position".into(),
        "transform:scale" => "transform.scale".into(),
        "transform:rotation" => "transform.rotation".into(),
        "transform:opacity" => "transform.opacity".into(),
        other => format!("value // {other}"),
    }
}
