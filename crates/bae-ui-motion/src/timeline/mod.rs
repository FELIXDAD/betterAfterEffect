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

mod dialogs;
mod drags;
mod header;
mod keys;
mod layer_bar;
mod menus;
mod rows;

use std::collections::{HashMap, HashSet};

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
use crate::graph::GraphEditor;
use crate::project::ItemDrag;
use crate::props::{self, Node};
use crate::session::{Session, SharedSession};

pub(crate) use dialogs::{InterpolationDialog, KeyDialog, VelocityDialog};
use header::comp_tabs;
use menus::new_layer_menu;
#[cfg(test)]
pub(crate) use menus::{Related, rove_state, select_related};
pub(crate) use menus::{key_menu, keyframe_menu};

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
    /// Selected keyframes; `grabbed` is the one under the pointer; `scale`: Alt was
    /// held at the start (scale the timing instead of moving).
    Keys {
        origin: f32,
        keys: Vec<KeyRef>,
        grabbed: KeyRef,
        scale: bool,
    },
    Move {
        origin: f32,
        layers: Vec<PrimPath>,
    },
    TrimIn {
        origin: f32,
        layer: PrimPath,
    },
    TrimOut {
        origin: f32,
        layer: PrimPath,
    },
    WorkArea {
        origin: f32,
        start: bool,
        end: bool,
    },
    Marquee {
        origin: Pos2,
        additive: bool,
    },
    PickWhip {
        layers: Vec<PrimPath>,
        origin: Pos2,
    },
    Reorder {
        layers: Vec<PrimPath>,
    },
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
    /// AE layer numbers (# column) of the shown comp, for accessibility names:
    /// two layers may share a name (issue #128).
    layer_numbers: HashMap<PrimPath, usize>,
    /// Graph Editor mode state (shown when the session's `graph_editor` is on).
    graph: GraphEditor,
    /// This frame shows the Graph Editor.
    graph_mode: bool,
    /// Where a keyframe drag puts the keyframes this frame.
    key_plan: Option<keys::KeyPlan>,
    /// Last frame's time axis, for tests.
    #[cfg(test)]
    pub(crate) last_axis: Option<TimeAxis>,
    /// Where the last frame drew the layer-bar keyframes, for tests.
    #[cfg(test)]
    pub(crate) last_keys: Vec<(KeyRef, Pos2)>,
}

/// Comp time ↔ x in the graph.
#[derive(Clone, Copy)]
pub(crate) struct TimeAxis {
    pub(crate) left: f32,
    pub(crate) width: f32,
    pub(crate) start: Time,
    pub(crate) span: Time,
}

impl TimeAxis {
    pub(crate) fn x(&self, t: Time) -> f32 {
        self.left
            + PAD
            + (t - self.start).0 as f32 / self.span.0.max(1) as f32 * (self.width - 2.0 * PAD)
    }

    pub(crate) fn t(&self, x: f32) -> Time {
        let f = (x - self.left - PAD) / (self.width - 2.0 * PAD).max(1.0);
        self.start + Time((f64::from(f) * self.span.0 as f64).round() as i64)
    }

    /// Time per point.
    pub(crate) fn per_point(&self) -> f64 {
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

    /// Zoom and scroll so `[a, b]` (comp time) fills the time graph with a margin.
    fn fit_time(&mut self, info: &CompInfo, a: Time, b: Time) {
        let full = info.duration;
        let min = Time(info.frame_duration().0 * 10).min(full);
        let want = Time((((b - a).0 as f64) * 1.2) as i64).max(min).min(full);
        let f = if full.0 > min.0 {
            ((want.0 - min.0) as f64 / (full.0 - min.0) as f64).cbrt()
        } else {
            1.0
        };
        self.zoom = (1.0 - f as f32).clamp(0.0, 1.0);
        let span = self.axis(info, 0.0, 1000.0).span;
        self.scroll = Time((a.0 + b.0) / 2 - span.0 / 2);
    }
}

impl TimelinePanel {
    /// "Layer 2 “Solid”": the accessibility context of a layer's controls.
    fn layer_ctx(&self, stage: &bae_model::Stage, layer: &PrimPath) -> String {
        bae_ui::a11y::layer(
            self.layer_numbers.get(layer).copied(),
            &query::display_name_of(stage, layer),
        )
    }

    #[cfg(test)]
    pub(crate) fn start_rename(&mut self, layer: PrimPath, text: &str) {
        self.rename = Some((layer, text.to_owned()));
    }

    #[cfg(test)]
    pub(crate) fn graph_editor(&mut self) -> &mut GraphEditor {
        &mut self.graph
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
        let renaming_before = self.rename.is_some();
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
        self.graph_mode = session.lock().graph_editor;
        let cols = Cols::new(full.width());
        let graph_left = full.left() + cols.end + 2.0;
        let graph_w = (full.right() - graph_left).max(40.0);
        let axis = self.axis(&info, graph_left, graph_w);
        #[cfg(test)]
        {
            self.last_axis = Some(axis);
            self.last_keys.clear();
        }

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
        let (pointer_x, shift) =
            ui.input(|i| (i.pointer.interact_pos().map(|p| p.x), i.modifiers.shift));
        self.key_plan = self.plan_key_drag(stage, &info, &axis, &rows, now, pointer_x, shift);
        let comp_layers = query::comp_layers(stage, &info.path);
        self.layer_numbers = comp_layers
            .iter()
            .enumerate()
            .map(|(i, l)| (l.clone(), i + 1))
            .collect();

        // Background of the graph body: marquee selection and deselect (the Graph
        // Editor has its own).
        if !self.graph_mode {
            let bg = ui
                .interact(
                    graph_body,
                    ui.id().with("graph_bg"),
                    Sense::click_and_drag(),
                )
                .labeled(egui::WidgetType::Other, tr("Time Graph"));
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
                s.selected_props.clear();
            }
            bg.context_menu(|ui| new_layer_menu(ui, cx, &session));
            // Dropped project items.
            if let Some(item) = bg.dnd_release_payload::<ItemDrag>() {
                crate::viewer::add_item_as_layer(cx, &session, &item.0, None);
            }
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
            graph_body,
            &key_rects,
            &layer_rows,
            &comp_layers,
        );

        let mut cti_body = graph_body;
        if self.graph_mode {
            let out = self
                .graph
                .ui(ui, cx, &session, &info, &axis, graph_body, now);
            if let Some((a, b)) = out.fit_time {
                self.fit_time(&info, a, b);
            }
            if out.zoom_by != 0.0 {
                self.zoom_step(out.zoom_by, &info, now);
            }
            cti_body.max.y -= crate::graph::BAR_H;
        }

        // CTI through the body.
        let p = ui.painter().with_clip_rect(cti_body);
        let x = axis.x(now);
        p.vline(x, cti_body.y_range(), Stroke::new(1.0, color::ACCENT));

        self.footer(ui, foot, &info, &axis, now, graph_left);

        // Keyframe dialogs (modal; Enter and Esc are theirs).
        let dialog_open = session.lock().key_dialog.is_some();
        dialogs::show(ui.ctx(), cx, &session);

        // AE: Enter (Return) renames the selected layer (the topmost one when
        // several are selected); not the Enter that just committed a rename.
        if cx.focused
            && !dialog_open
            && !renaming_before
            && self.rename.is_none()
            && bae_ui::panel_key_pressed(ui, egui::Key::Enter)
        {
            let selected = session.lock().selected_layers.clone();
            if let Some(layer) = comp_layers
                .iter()
                .find(|l| selected.contains(l) && !query::is_locked(stage, l))
            {
                self.rename = Some((layer.clone(), query::display_name_of(stage, layer)));
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

/// Click selection (AE): a click selects the layer; Cmd+click adds or removes
/// it; Shift+click selects the contiguous range from the anchor (the layer
/// clicked last without Shift) to it, in timeline order.
fn select_layer(session: &SharedSession, stage: &Stage, layer: &PrimPath, mods: egui::Modifiers) {
    // AE: a locked layer cannot be selected (unlock it first).
    if query::is_locked(stage, layer) {
        return;
    }
    let mut s = session.lock();
    if mods.shift {
        let order = query::owning_comp(stage, layer)
            .map(|c| query::comp_layers(stage, &c))
            .unwrap_or_default();
        let anchor = s
            .selected_layers
            .first()
            .filter(|a| order.contains(a))
            .cloned()
            .unwrap_or_else(|| layer.clone());
        let (Some(a), Some(b)) = (
            order.iter().position(|l| *l == anchor),
            order.iter().position(|l| l == layer),
        ) else {
            s.selected_layers = vec![layer.clone()];
            return;
        };
        let (lo, hi) = (a.min(b), a.max(b));
        // The anchor stays first so the next Shift+click extends from it.
        let mut range = vec![anchor.clone()];
        range.extend(
            order[lo..=hi]
                .iter()
                .filter(|l| **l != anchor && !query::is_locked(stage, l))
                .cloned(),
        );
        s.selected_layers = range;
    } else if mods.command {
        if let Some(i) = s.selected_layers.iter().position(|p| p == layer) {
            s.selected_layers.remove(i);
        } else {
            s.selected_layers.push(layer.clone());
        }
    } else {
        s.selected_layers = vec![layer.clone()];
        s.selected_keys.clear();
        s.selected_props.clear();
    }
}
