//! Timeline panel: AE's layer outline (A/V switches, label, #, name, switches,
//! Parent & Link with pick whip), property rows with stopwatches, keyframe
//! navigators, values and expressions, and the time graph (comp tabs, current time,
//! ruler, work area, CTI, layer duration bars, keyframes).
//!
//! Every edit is a command: switches → `layer.set_attr`, values → `attr.set_at`, stopwatch
//! → `attr.set_animated`, dragging bars → `layer.shift_time` / `layer.trim`, dragging
//! keyframes → `key.move`, work area → `comp.set_work_area`, reordering →
//! `layer.reorder`, parenting → `layer.set_parent`. Drags preview locally and run
//! one command when released.

mod columns;
mod dialogs;
mod drags;
mod header;
mod keys;
mod layer_bar;
mod markers;
mod menus;
mod modes;
pub(crate) mod navigation;
mod rows;
mod search;
mod switches;
#[cfg(test)]
mod tests;
mod waveform;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bae_model::{Interpolation, Keyframe, PrimPath, Stage, Time};
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
use crate::project::{DropPlacement, ItemDrag};
use crate::props::{self, Node};
use crate::session::{Reveal, Session, SharedSession};

pub(crate) use columns::Column;
use columns::{Cols, Columns};
pub(crate) use dialogs::{InterpolationDialog, KeyDialog, SequenceDialog, VelocityDialog};
use header::comp_tabs;
pub(crate) use keys::{click_conversion, clicked_set};
pub(crate) use markers::MarkerDialog;
#[cfg(test)]
pub(crate) use markers::parse_timecode;
pub(crate) use menus::keys_with_labels;
#[cfg(test)]
pub(crate) use menus::{Related, catalog_entry, rove_state, select_label_group, select_related};
pub(crate) use menus::{
    catalog_node, catalog_nodes, invert_layer_selection, key_menu, select_children,
};
#[cfg(test)]
pub(crate) use modes::flip_matte;

// Registration contract of the footage optical-flow backend.
const FRAME_INTERPOLATOR_ID: &str = "bae.render/frame-interpolator";

/// Row height and switch cell width: AE 26's (17 and about 15 points).
const ROW_H: f32 = 17.0;
const TABS_H: f32 = 22.0;
const HEAD_H: f32 = 34.0;
const COLS_H: f32 = 20.0;
const FOOT_H: f32 = 22.0;
const SW: f32 = 15.0;
/// Rows an expression field grows to.
const MAX_EXPR_LINES: usize = 30;
/// Left inset of the Parent & Link title and pick whip.
const PARENT_PAD: f32 = 6.0;
/// Left padding of the time graph so frame 0 is not on the edge.
const PAD: f32 = 8.0;

#[derive(Clone, Debug)]
enum RowKind {
    Layer,
    Waveform,
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
    /// One row per line of the expression (`line` of `lines`): the first has the
    /// name and switches, the last draws the field over all of them.
    Expression {
        path: PrimPath,
        attr: String,
        line: usize,
        lines: usize,
    },
}

#[derive(Clone, Debug)]
struct Row {
    kind: RowKind,
    layer_index: usize,
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
    /// Expression pick whip: drop on a property or a layer.
    ExprWhip {
        whip: crate::expr::Whip,
        origin: Pos2,
    },
    Reorder {
        layers: Vec<PrimPath>,
    },
    /// The Track Matte pick whip of a layer.
    MatteWhip {
        layer: PrimPath,
        origin: Pos2,
    },
    /// A comp or layer marker.
    Marker {
        origin: f32,
        marker: PrimPath,
    },
    /// A new comp marker dragged out of the marker bin.
    MarkerFromBin,
    /// Slip edit of a layer with a time-based source.
    Slip {
        origin: f32,
        layer: PrimPath,
    },
}

/// The layer order and parent-menu labels only change when the project changes.
struct TimelineLayers {
    revision: u64,
    comp: PrimPath,
    paths: Vec<PrimPath>,
    parent_numbers: Vec<usize>,
    parent_labels: Vec<String>,
}

#[derive(Default)]
pub struct TimelinePanel {
    eval: EvalCache,
    pub(crate) expanded: HashSet<String>,
    waveforms: waveform::Waveforms,
    /// 0 = whole comp, 1 = a few frames.
    zoom: f32,
    /// Start of the visible range when zoomed (comp time).
    scroll: Time,
    drag: Option<Drag>,
    switch_drag: Option<switches::SwitchDrag>,
    rename: Option<(PrimPath, String)>,
    /// The search field's text: filters the outline (search.rs).
    pub(crate) search: String,
    /// Edit ▸ Find: focus the search field next frame.
    focus_search: bool,
    comment_edit: Option<(PrimPath, String)>,
    /// Expression sources being edited, by `path.attr`.
    /// The expression field being edited: its text, and the stored source it
    /// started from (a pick whip or the language menu changing the stored
    /// expression meanwhile replaces the text).
    expr_edit: Option<(PrimPath, String, String, String)>,
    /// The field being edited: its text and selection (chars), for the pick whip
    /// and the language menu, which take its focus.
    expr_caret: Option<(PrimPath, String, String, std::ops::Range<usize>)>,
    /// [`Self::expr_caret`] as it was when the pick whip was pressed.
    expr_press: Option<crate::expr::Selection>,
    /// The property whose language menu is open and its field's selection then.
    lang_press: Option<(PrimPath, String, Option<crate::expr::Selection>)>,
    layers: Option<Arc<TimelineLayers>>,
    /// Graph Editor mode state (shown when the session's `graph_editor` is on).
    graph: GraphEditor,
    /// This frame shows the Graph Editor.
    graph_mode: bool,
    /// Switches / Modes / Parent & Link columns shown (F4, Shift+F4).
    columns: Columns,
    /// Where a keyframe drag puts the keyframes this frame.
    key_plan: Option<keys::KeyPlan>,
    /// Modifiers held when the primary button last went down (AE decides drag
    /// modes such as Alt time-scaling at mouse-down).
    press_mods: egui::Modifiers,
    /// The marker being dragged and its comp-time offset this frame.
    marker_plan: Option<(PrimPath, Time)>,
    /// The bar drag's offset this frame.
    bar_plan: Option<layer_bar::BarPlan>,
    /// Last frame's time axis, for tests.
    #[cfg(test)]
    pub(crate) last_axis: Option<TimeAxis>,
    /// Where the last frame drew the layer-bar keyframes, for tests.
    #[cfg(test)]
    pub(crate) last_keys: Vec<(KeyRef, Pos2)>,
    /// Each layer row's T box and Track Matte pick whip, for tests.
    #[cfg(test)]
    /// Per layer: T switch, pick whip, Alpha / Luma toggle (Invert is 17 px right).
    pub(crate) last_modes: Vec<(PrimPath, Rect, Pos2, Pos2)>,
    /// Where the last frame drew the markers (glyph centres), for tests.
    #[cfg(test)]
    pub(crate) last_markers: Vec<(PrimPath, Pos2)>,
    /// Expression rows: property, Enable switch, pick whip.
    #[cfg(test)]
    pub(crate) last_expr: Vec<(PrimPath, String, Pos2, Pos2)>,
    /// Expression rows: property, language menu button.
    #[cfg(test)]
    pub(crate) last_lang: Vec<(PrimPath, String, Pos2)>,
    /// The comp marker bin, for tests.
    #[cfg(test)]
    pub(crate) last_marker_bin: Option<Rect>,
    /// Where the last frame drew the layer bars, for tests.
    #[cfg(test)]
    pub(crate) last_bars: Vec<(PrimPath, Rect)>,
    /// The bottom-left pane buttons (Layer Switches, Transfer Controls,
    /// In/Out/Duration/Stretch, Render Time), for tests.
    #[cfg(test)]
    pub(crate) last_panes: [Option<Rect>; 4],
    /// Group rows: id, twirl arrow, name.
    #[cfg(test)]
    pub(crate) last_groups: Vec<(String, Pos2, Pos2)>,
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

/// The Timeline's Rename on empty space: the comp's name in the Project panel,
/// selected for typing (AE 26).
fn rename_comp(cx: &mut PanelContext<'_>, session: &SharedSession, comp: &PrimPath) {
    {
        let mut s = session.lock();
        s.selected_items = vec![comp.clone()];
        s.rename_item = Some(comp.clone());
    }
    cx.open_panel("project");
}

fn flatten_tree(
    nodes: &[Node],
    layer_index: usize,
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
                    layer_index,
                    depth,
                });
                if open {
                    flatten_tree(children, layer_index, depth + 1, expanded, stage, out);
                }
            }
            Node::Prop { path, attr, label } => {
                push_prop(path, attr, label, layer_index, depth, stage, out);
            }
        }
    }
}

fn push_prop(
    path: &PrimPath,
    attr: &str,
    label: &str,
    layer_index: usize,
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
        layer_index,
        depth,
    });
    if query::attr_spec(stage, path, attr).is_some_and(|s| s.expression.is_some()) {
        out.push(Row {
            kind: RowKind::Expression {
                path: path.clone(),
                attr: attr.to_owned(),
                line: 0,
                lines: 1,
            },
            layer_index,
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
    /// F4 (Switches / Modes columns) or Shift+F4 (Parent & Link column).
    fn toggle_columns(&mut self, id: &str) {
        match id {
            actions::ids::TOGGLE_SWITCHES_MODES => self.columns.toggle_switches_modes(),
            actions::ids::TOGGLE_PARENT_COLUMN => self.columns.toggle(Column::Parent),
            _ => {}
        }
    }
}

impl TimelinePanel {
    /// The field selection the language menu will replace (tests).
    #[cfg(test)]
    pub(crate) fn lang_insert(&self) -> Option<crate::expr::Selection> {
        self.lang_press.as_ref().and_then(|(.., i)| i.clone())
    }

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

    /// The group rows (ids) the outline shows for `comp`, for tests.
    #[cfg(test)]
    pub(crate) fn group_rows(
        &mut self,
        stage: &Stage,
        comp: &PrimPath,
        s: &Session,
    ) -> Vec<String> {
        let layers = self.layer_list(stage, comp);
        self.rows(stage, &layers, s)
            .into_iter()
            .filter_map(|r| match r.kind {
                RowKind::Group { id, .. } => Some(id),
                _ => None,
            })
            .collect()
    }

    fn layer_list(&mut self, stage: &Stage, comp: &PrimPath) -> Arc<TimelineLayers> {
        if let Some(layers) = &self.layers
            && layers.revision == stage.revision()
            && &layers.comp == comp
        {
            return layers.clone();
        }
        let paths = query::comp_layers(stage, comp);
        let numbers: HashMap<_, _> = paths.iter().enumerate().map(|(i, p)| (p, i + 1)).collect();
        let parent_numbers = paths
            .iter()
            .map(|path| {
                stage
                    .prim(path)
                    .and_then(|prim| {
                        prim.value("parent")
                            .and_then(|v| v.as_prim_ref())
                            .and_then(|p| numbers.get(p).copied())
                    })
                    .unwrap_or(0)
            })
            .collect();
        let parent_labels = paths
            .iter()
            .enumerate()
            .map(|(i, p)| format!("{}. {}", i + 1, query::display_name_of(stage, p)))
            .collect();
        let layers = Arc::new(TimelineLayers {
            revision: stage.revision(),
            comp: comp.clone(),
            paths,
            parent_numbers,
            parent_labels,
        });
        self.layers = Some(layers.clone());
        layers
    }

    fn rows(&self, stage: &Stage, layers: &TimelineLayers, s: &Session) -> Vec<Row> {
        let mut out = Vec::new();
        let hide_shy = bool_attr(stage, &layers.comp, "hide_shy_layers");
        let needle = self.search.trim().to_lowercase();
        for (layer_index, layer) in layers.paths.iter().enumerate() {
            if hide_shy && bool_attr(stage, layer, "shy") {
                continue;
            }
            // Searching: matching layers, and the matching properties opened.
            let found = (!needle.is_empty()).then(|| {
                let name = query::display_name_of(stage, layer).to_lowercase();
                let props = search::matching(&props::layer_tree(stage, layer), &needle);
                (name.contains(&needle), props)
            });
            if found
                .as_ref()
                .is_some_and(|(name, props)| !name && props.is_empty())
            {
                continue;
            }
            out.push(Row {
                kind: RowKind::Layer,
                layer_index,
                depth: 0,
            });
            if let Some((_, nodes)) = found.filter(|(_, props)| !props.is_empty()) {
                let mut open = HashSet::new();
                search::group_ids(&nodes, &mut open);
                flatten_tree(&nodes, layer_index, 1, &open, stage, &mut out);
                continue;
            }
            if let Some(r) = s.reveal.get(layer) {
                // E: the Effects group, open, its effects as twirled (AE shows
                // them above the revealed transform properties).
                if let Reveal::Attrs(v) = r
                    && v.iter().any(|a| a == "effects")
                    && let Some(Node::Group {
                        id,
                        label,
                        children,
                    }) = props::layer_tree(stage, layer)
                        .into_iter()
                        .find(|n| matches!(n, Node::Group { id, .. } if id.ends_with("#effects")))
                {
                    out.push(Row {
                        kind: RowKind::Group {
                            id,
                            label,
                            open: true,
                        },
                        layer_index,
                        depth: 1,
                    });
                    flatten_tree(&children, layer_index, 2, &self.expanded, stage, &mut out);
                }
                if matches!(r, Reveal::Attrs(attrs) if attrs.iter().any(|a| a == "audio:waveform"))
                    && bae_media::audio::layer_file(stage, layer).is_some()
                {
                    out.push(Row {
                        kind: RowKind::Waveform,
                        layer_index,
                        depth: 1,
                    });
                }
                for n in props::revealed(stage, layer, r) {
                    if let Node::Prop { path, attr, label } = n {
                        push_prop(&path, &attr, &label, layer_index, 1, stage, &mut out);
                    }
                }
            } else if self.expanded.contains(layer.as_str()) {
                flatten_tree(
                    &props::layer_tree(stage, layer),
                    layer_index,
                    1,
                    &self.expanded,
                    stage,
                    &mut out,
                );
            }
        }
        self.expression_lines(stage, out)
    }

    /// Multi-line expressions take one row per line (the text being edited, else
    /// the expression; at most [`MAX_EXPR_LINES`]).
    fn expression_lines(&self, stage: &Stage, rows: Vec<Row>) -> Vec<Row> {
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let RowKind::Expression { path, attr, .. } = &row.kind else {
                out.push(row);
                continue;
            };
            let stored = query::attr_spec(stage, path, attr).and_then(|s| s.expression.as_ref());
            let lines = match &self.expr_edit {
                Some((p, a, buf, base))
                    if p == path && a == attr && stored.is_some_and(|e| e.source == *base) =>
                {
                    buf.split('\n').count()
                }
                _ => stored.map_or(1, |e| e.source.split('\n').count()),
            }
            .clamp(1, MAX_EXPR_LINES);
            for line in 0..lines {
                out.push(Row {
                    kind: RowKind::Expression {
                        path: path.clone(),
                        attr: attr.clone(),
                        line,
                        lines,
                    },
                    layer_index: row.layer_index,
                    depth: row.depth,
                });
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
    #[cfg(test)]
    pub(crate) fn expanded_layer(&self, layer: &PrimPath) -> bool {
        self.expanded.contains(layer.as_str())
    }

    #[cfg(test)]
    pub(crate) fn start_rename(&mut self, layer: PrimPath, text: &str) {
        self.rename = Some((layer, text.to_owned()));
    }

    #[cfg(test)]
    pub(crate) fn graph_editor(&mut self) -> &mut GraphEditor {
        &mut self.graph
    }

    pub(crate) fn from_state(state: Option<&serde_json::Value>) -> Self {
        let mut panel = Self::default();
        if let Some(state) = state {
            panel.columns = Columns::load(state);
            panel.zoom = state["zoom"].as_f64().unwrap_or_default().clamp(0.0, 1.0) as f32;
        }
        panel
    }
    #[cfg(test)]
    pub(crate) fn column_visible(&self, c: Column) -> bool {
        self.columns.visible(c)
    }
}

#[derive(Clone, Copy)]
enum OtherTimelines {
    All,
    Left,
    Right,
}
impl OtherTimelines {
    fn matches(self, index: usize, active: usize) -> bool {
        match self {
            Self::All => index != active,
            Self::Left => index < active,
            Self::Right => index > active,
        }
    }
}
const OTHER_TIMELINES: [(OtherTimelines, &str); 3] = [
    (OtherTimelines::All, "Close Other Timeline Panels"),
    (OtherTimelines::Right, "Close Timeline Panels to the Right"),
    (OtherTimelines::Left, "Close Timeline Panels to the Left"),
];

impl Panel for TimelinePanel {
    fn tab(&self, cx: &PanelContext<'_>) -> TabInfo {
        // From the session every frame (issue #215), like the viewer's.
        match common::active_comp(cx.stage, &common::session(cx.resources)) {
            Some(info) => TabInfo::new(format!("{} {}", tr("Timeline"), info.name)),
            None => TabInfo::new(tr("Timeline")),
        }
        .close_submenu()
    }

    fn related_tabs_menu(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        let mut session = session.lock();
        let active = session
            .active_comp
            .as_ref()
            .and_then(|c| session.open_comps.iter().position(|p| p == c));
        for (action, label) in OTHER_TIMELINES {
            if matches!(action, OtherTimelines::Right) {
                ui.separator();
            }
            let targets: Vec<_> = session
                .open_comps
                .iter()
                .enumerate()
                .filter(|(i, _)| active.is_some_and(|at| action.matches(*i, at)))
                .map(|(_, c)| c.clone())
                .collect();
            if ui
                .add_enabled(!targets.is_empty(), egui::Button::new(tr(label)))
                .clicked()
            {
                for target in targets {
                    session.close_comp(&target);
                }
                ui.close();
            }
        }
        true
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        Some(json!({"zoom": self.zoom, "columns":self.columns}))
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
            id @ (actions::ids::TOGGLE_SWITCHES_MODES | actions::ids::TOGGLE_PARENT_COLUMN) => {
                self.toggle_columns(id);
                true
            }
            // Edit ▸ Find (Ctrl+F): the search field.
            "motion.file.find" => {
                self.focus_search = true;
                true
            }
            _ => actions::handle(action, cx, &session),
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        // F4 / Shift+F4 pressed with another panel in front.
        let toggles = std::mem::take(&mut common::session(cx.resources).lock().timeline_toggles);
        for id in toggles {
            self.toggle_columns(id);
        }
        let full = ui.available_rect_before_wrap();
        crate::import::drop_on_panel(ui, cx);
        let cols = self.columns.layout(ui, full);
        self.contents(ui, cx, &cols);
    }
}
impl TimelinePanel {
    fn contents(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>, cols: &Cols) {
        let session = common::session(cx.resources);
        session.lock().sync(cx.stage);
        let stage = cx.stage;
        if let Some(track) = session.lock().reveal_tracker.take()
            && let Some(layer) = query::owning_layer(stage, &track)
        {
            self.expanded.extend([
                layer.to_string(),
                format!("{layer}#trackers"),
                track.to_string(),
            ]);
        }
        let renaming_before = self.rename.is_some();
        let full = ui.available_rect_before_wrap();
        ui.painter().rect_filled(full, 0.0, color::PANEL);

        // Comp tabs.
        let tabs = Rect::from_min_size(full.min, vec2(full.width(), TABS_H));
        comp_tabs(ui, tabs, stage, &session);

        let Some(info) = common::active_comp(stage, &session) else {
            let rest = Rect::from_min_max(pos2(full.left(), tabs.bottom()), full.max);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rest));
            common::empty_hint(
                &mut child,
                tr("No composition"),
                tr("Open a composition from the Project panel, or create one."),
            );
            return;
        };
        session.lock().update_preview(cx, &info);
        let now = session.lock().time(&info.path);
        self.graph_mode = session.lock().graph_editor;
        let graph_left = full.left() + cols.viewport + 2.0;
        let graph_w = (full.right() - graph_left).max(40.0);
        let mut axis = self.axis(&info, graph_left, graph_w);
        if cfg!(target_os = "macos")
            && ui.rect_contains_pointer(Rect::from_min_max(
                pos2(graph_left, tabs.bottom()),
                full.max,
            ))
            && let Some(pointer) = ui.input(|i| i.pointer.hover_pos())
        {
            let smart = crate::viewer::smart_zoom(ui.ctx());
            let factor = ui.input(|i| i.zoom_delta());
            if smart || factor != 1.0 {
                let time = axis.t(pointer.x);
                let full_span = info.duration.0 as f64;
                let min_span = (info.frame_duration().0 * 10).min(info.duration.0) as f64;
                let span = if smart {
                    if self.zoom > 0.0 {
                        full_span
                    } else {
                        full_span / 2.0
                    }
                } else {
                    axis.span.0 as f64 / f64::from(factor)
                };
                if full_span > min_span {
                    self.zoom = 1.0
                        - ((span - min_span) / (full_span - min_span))
                            .clamp(0.0, 1.0)
                            .cbrt() as f32;
                    let new_axis = self.axis(&info, graph_left, graph_w);
                    self.scroll = time - (new_axis.t(pointer.x) - new_axis.start);
                    axis = self.axis(&info, graph_left, graph_w);
                    ui.ctx().request_repaint();
                }
            }
        }
        #[cfg(test)]
        {
            self.last_axis = Some(axis);
            self.last_keys.clear();
            self.last_modes.clear();
            self.last_markers.clear();
            self.last_expr.clear();
            self.last_lang.clear();
            self.last_bars.clear();
            self.last_groups.clear();
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

        let (pointer_x, shift) =
            ui.input(|i| (i.pointer.interact_pos().map(|p| p.x), i.modifiers.shift));
        self.marker_plan = self.plan_marker_drag(stage, &info, &axis, now, pointer_x, shift);
        self.bar_plan = self.plan_bar_drag(stage, &info, &axis, now, pointer_x, shift);
        self.header(ui, cx, &session, &info, head, colrow, cols, &axis, now);

        // Share one parent menu across the visible rows.
        let layers = self.layer_list(stage, &info.path);
        let comp_layers = &layers.paths;
        let parent_items: Vec<_> = std::iter::once(tr("None"))
            .chain(layers.parent_labels.iter().map(String::as_str))
            .collect();
        let rows = {
            let frame = ui.ctx().cumulative_frame_nr();
            let mut s = session.lock();
            if self.columns.visible(Column::RenderTime) && cols.on_screen(Column::RenderTime) {
                s.timing_column_frame = Some(frame);
            } else if s.timing_column_frame != Some(frame) {
                s.timing_column_frame = None;
            }
            self.rows(stage, &layers, &s)
        };
        if let Some(m) = ui.input(|i| i.pointer.primary_pressed().then_some(i.modifiers)) {
            self.press_mods = m;
        }
        self.key_plan = self.plan_key_drag(
            stage,
            &info,
            &axis,
            &rows,
            &layers.paths,
            now,
            pointer_x,
            shift,
        );
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
            // From where the button went down, not where the drag was recognised
            // (a few points later), or a tight marquee misses keys there (#551).
            if bg.drag_started_by(egui::PointerButton::Primary)
                && let Some(p) = ui
                    .input(|i| i.pointer.press_origin())
                    .or_else(|| bg.interact_pointer_pos())
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
                s.selected_groups.clear();
            }
            // Right-click on empty space: AE 26's comp menu (New ▸, Composition
            // Settings, Preview ▸ …).
            let mut chosen = None;
            crate::common::context_menu(&bg, &mut |ui| {
                chosen = crate::context_menu::show(ui, cx, crate::context_menu::TIMELINE_BLANK);
            });
            if chosen == Some("Rename") {
                rename_comp(cx, &session, &info.path);
            }
        }

        // Row widgets accept other drag types. Capture the project item before a
        // row consumes the released payload; the stack position comes from row layout.
        let project_drop = ui
            .ctx()
            .pointer_interact_pos()
            .filter(|_| ui.rect_contains_pointer(body))
            .and_then(|pos| {
                egui::DragAndDrop::payload::<ItemDrag>(ui.ctx()).map(|item| (item, pos))
            });
        let mut drop_slot = None;
        let mut last_row_bottom = body.top();
        let mut key_rects: Vec<(Rect, KeyRef)> = Vec::new();
        let mut layer_rows: Vec<(Rect, PrimPath)> = Vec::new();
        let mut prop_rows: Vec<(Rect, PrimPath, String)> = Vec::new();
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        let pixel_motion = cx
            .resources
            .get::<crate::gpu_preview::PreviewEffects>()
            .is_some_and(|effects| effects.0.contains(FRAME_INTERPOLATOR_ID));
        let switch_input = ui.input(|input| switches::SwitchInput::read(input, pixel_motion));
        let scrolled = egui::ScrollArea::vertical()
            .id_salt("timeline_rows")
            .auto_shrink([false, false])
            .show_rows(&mut child, ROW_H, rows.len(), |ui, range| {
                // Rows touch, as in AE (a 1-point line between them).
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut outline = ui.new_child(egui::UiBuilder::new().id_salt("outline_rows"));
                let mut clip = ui.clip_rect();
                clip.max.x = clip.max.x.min(graph_left - 2.0);
                outline.set_clip_rect(clip);
                for i in range {
                    let row = &rows[i];
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(full.width(), ROW_H), Sense::hover());
                    if let Some((_, pointer)) = &project_drop {
                        last_row_bottom = rect.bottom();
                        if drop_slot.is_none() && pointer.y <= rect.bottom() {
                            let number = row.layer_index + 1;
                            let before =
                                matches!(row.kind, RowKind::Layer) && pointer.y < rect.center().y;
                            drop_slot = Some(if before {
                                (number - 1, rect.top())
                            } else {
                                (number, rect.bottom())
                            });
                        }
                    }
                    self.row_ui(
                        &mut outline,
                        ui,
                        cx,
                        &session,
                        &info,
                        cols,
                        &axis,
                        row,
                        rect,
                        switch_input,
                        &layers,
                        &parent_items,
                        &mut key_rects,
                    );
                    match &row.kind {
                        RowKind::Layer => {
                            layer_rows.push((rect, layers.paths[row.layer_index].clone()))
                        }
                        RowKind::Prop { path, attr, .. } => {
                            prop_rows.push((rect, path.clone(), attr.clone()))
                        }
                        // An effect or a mask: the expression pick whip refers to
                        // it as a whole (empty attribute).
                        RowKind::Group { id, .. } => {
                            if let Some(prim) = id
                                .split_once('#')
                                .and_then(|(_, p)| PrimPath::new(p).ok())
                                .filter(|p| {
                                    stage.prim(p).is_some_and(|v| {
                                        v.is_a(&bae_model::PrimType::EFFECT)
                                            || v.is_a(&bae_model::PrimType::MASK)
                                    })
                                })
                            {
                                prop_rows.push((rect, prim, String::new()));
                            }
                        }
                        _ => {}
                    }
                }
            });
        // Empty outline space below the layers (both modes): a click deselects
        // everything, a right-click is AE's New menu, as on the empty graph.
        let rows_bottom =
            scrolled.inner_rect.top() + rows.len() as f32 * ROW_H - scrolled.state.offset.y;
        let outline_bg = Rect::from_min_max(
            pos2(body.left(), rows_bottom.max(body.top())),
            // The outline only, kept clear of egui's pointer slop (about 5 points)
            // around the time graph, whose background owns presses near its edge.
            pos2(body.left() + cols.end - 8.0, body.bottom()),
        );
        if outline_bg.height() > 0.0 {
            let bg = ui.interact(outline_bg, ui.id().with("outline_bg"), Sense::click());
            if bg.clicked() {
                let mut s = session.lock();
                s.selected_keys.clear();
                s.selected_layers.clear();
                s.selected_props.clear();
                s.selected_groups.clear();
            }
            let mut chosen = None;
            crate::common::context_menu(&bg, &mut |ui| {
                chosen = crate::context_menu::show(ui, cx, crate::context_menu::TIMELINE_BLANK);
            });
            if chosen == Some("Rename") {
                rename_comp(cx, &session, &info.path);
            }
        }

        if let Some((item, pointer)) = project_drop {
            let (index, y) = drop_slot.unwrap_or((comp_layers.len(), last_row_bottom));
            ui.painter().with_clip_rect(body).hline(
                body.x_range(),
                y,
                Stroke::new(2.0, color::ACCENT),
            );
            if ui.input(|i| i.pointer.primary_released()) {
                let start = graph_body
                    .contains(pointer)
                    .then(|| info.snap_visible_time(axis.t(pointer.x)));
                item.add_as_layer(cx, &session, DropPlacement::Timeline { index, start });
            }
        }

        self.finish_switch_drag(switch_input, cx);

        // Drags that span rows.
        self.finish_drags(
            ui,
            cx,
            &session,
            &info,
            graph_body,
            &key_rects,
            &layer_rows,
            &prop_rows,
            comp_layers,
        );

        let mut cti_body = graph_body;
        if self.graph_mode {
            let waveforms = &mut self.waveforms;
            let stage = cx.stage;
            let out = self.graph.ui(
                ui,
                cx,
                &session,
                &info,
                &axis,
                graph_body,
                now,
                &mut |ui, layer, area| {
                    waveforms.draw(
                        ui,
                        stage,
                        layer,
                        &axis,
                        area,
                        color::TEXT_DIM.gamma_multiply(0.5),
                    );
                },
            );
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
        navigation::show(ui, stage, &session);
        self.columns.scrollbar(ui, foot, cols);

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
        RowKind::Waveform => "waveform".into(),
        RowKind::Group { id, .. } => id.clone(),
        RowKind::Prop { path, attr, .. } => format!("{path}.{attr}"),
        RowKind::Expression {
            path, attr, line, ..
        } => format!("{path}.{attr}=expr{line}"),
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
        s.selected_groups.clear();
    }
}

#[cfg(test)]
mod layer_list_tests {
    use super::*;
    use bae_model::{AuthorId, CommandRegistry, Value};
    use serde_json::json;

    #[test]
    fn parent_choices_follow_edits_and_project_replacement() {
        let mut registry = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut registry);
        let author = AuthorId::new("user:test").unwrap();
        let run = |stage: &mut Stage, id, params| {
            registry.execute(stage, &author, id, params).unwrap().output
        };
        let create_comp = |stage: &mut Stage| {
            let result = run(stage, "comp.create", json!({"name":"Playback"}));
            PrimPath::new(result["path"].as_str().unwrap()).unwrap()
        };
        let mut stage = Stage::new();
        let comp = create_comp(&mut stage);
        let mut panel = TimelinePanel::default();
        for name in ["A", "B"] {
            run(
                &mut stage,
                "layer.create",
                json!({"comp":comp, "kind":"solid", "name":name}),
            );
        }
        let initial = panel.layer_list(&stage, &comp);
        assert_eq!(initial.parent_labels, ["1. B", "2. A"]);
        let mut session = Session::default();
        let info = query::comp_info(&stage, &comp).unwrap();
        session.play(&info);
        session.set_time(&info, info.frame_rate.frame_to_time(3));
        assert!(Arc::ptr_eq(&initial, &panel.layer_list(&stage, &comp)));

        let a = &initial.paths[1];
        let b = &initial.paths[0];
        run(
            &mut stage,
            "layer.set_parent",
            json!({"layers":[b], "parent":a, "time":Time::ZERO}),
        );
        assert_eq!(panel.layer_list(&stage, &comp).parent_numbers, [2, 0]);
        run(
            &mut stage,
            "attr.set",
            json!({"path":a, "attr":"name", "value":Value::String("Renamed".into())}),
        );
        assert_eq!(
            panel.layer_list(&stage, &comp).parent_labels,
            ["1. B", "2. Renamed"]
        );
        run(
            &mut stage,
            "layer.reorder",
            json!({"layers":[a], "index":0}),
        );
        let reordered = panel.layer_list(&stage, &comp);
        let rows = panel.rows(&stage, &reordered, &session);
        assert_eq!(rows[0].layer_index, 0);
        assert_eq!(&reordered.paths[rows[0].layer_index], a);
        assert_eq!(reordered.parent_labels, ["1. Renamed", "2. B"]);
        assert_eq!(reordered.parent_numbers, [0, 1]);
        stage.undo(&author).unwrap();
        assert_eq!(
            panel.layer_list(&stage, &comp).parent_labels,
            ["1. B", "2. Renamed"]
        );

        let mut other = Stage::new();
        assert_eq!(create_comp(&mut other), comp);
        assert!(panel.layer_list(&other, &comp).paths.is_empty());
    }
}

mod zh;
pub(crate) use zh::ZH;

/// Registers this module's Chinese strings and panel (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    crate::register_panel(
        app,
        "timeline",
        bae_ui::PanelKind::new("Timeline", |spawn| {
            Box::new(TimelinePanel::from_state(spawn.state))
        }),
    );
}

#[cfg(test)]
mod close_menu_tests {
    use super::*;
    #[test]
    fn close_other_timeline_choices_keep_the_active_comp() {
        let zh = [
            "关闭其他时间轴面板",
            "关闭右侧的时间轴面板",
            "关闭左侧的时间轴面板",
        ];
        for ((which, name), label) in OTHER_TIMELINES.into_iter().zip(zh) {
            assert!(ZH.contains(&(name, label)));
            assert!(!which.matches(0, 0));
        }
        assert!(!OtherTimelines::Left.matches(1, 0));
        assert!(!OtherTimelines::Right.matches(0, 1));
        assert!(OtherTimelines::Left.matches(0, 1));
        assert!(OtherTimelines::Right.matches(1, 0));
    }
}
