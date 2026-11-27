//! Window ▸ Flowchart: AE's Flowchart panel for the active composition — the
//! comp, then (left to right) the items its layers use: nested comps with
//! their own sources, footage and solids, joined by lines. Double-click a comp
//! node to open it.

use crate::project::{ItemKind, item_kind};
use bae_model::{PrimPath, Stage};
use bae_motion::query;
use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use bae_ui::{Panel, PanelContext, TabInfo, tr};
use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::common;

const NODE: egui::Vec2 = vec2(150.0, 24.0);
const GAP: egui::Vec2 = vec2(60.0, 10.0);
/// Nested comps deeper than this are not expanded.
const MAX_DEPTH: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Comp,
    Footage,
    Solid,
    Layer,
    Effect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IconMenu {
    Composition,
    Layer,
    Solid,
    Labels,
}

fn icon_menu_for(stage: &Stage, node: &Node) -> Option<IconMenu> {
    match node.kind {
        Kind::Comp => Some(IconMenu::Composition),
        Kind::Layer => Some(IconMenu::Layer),
        Kind::Solid => Some(IconMenu::Solid),
        Kind::Effect => Some(IconMenu::Labels),
        Kind::Footage => stage
            .prim(&node.item)
            .and_then(|v| bae_model::asset::AssetFile::from_prim(&v))
            .map(|_| IconMenu::Labels),
    }
}

fn node_label(stage: &Stage, node: &Node) -> usize {
    if let Some(label) = stage
        .prim(&node.item)
        .and_then(|v| v.value("label").and_then(|label| label.as_f64()))
    {
        return label.clamp(0.0, 16.0) as usize;
    }
    match node.kind {
        Kind::Solid => 1,
        Kind::Footage => 3,
        Kind::Effect => query::owning_layer(stage, &node.item)
            .map_or(0, |l| crate::actions::label_of(stage, &l) as usize),
        _ => 0,
    }
}

/// The generated solid has no source file. AE exposes the Project operations but
/// disables commands that need a file or a composition.
const SOLID_ICON: &[crate::context_menu::Entry] = {
    use crate::context_menu::Entry::*;
    &[
        Off("Composition Settings..."),
        Off("Open in Essential Graphics"),
        Off("New Comp from Selection..."),
        Own("Replace with 0 Precomps"),
        Menu("File/Create Proxy"),
        Menu("File/Set Proxy"),
        Menu("File/Import"),
        Menu("File/Import Recent Footage"),
        Menu("File/Interpret Footage"),
        Menu("File/Replace Footage"),
        Own("Reload Footage"),
        Off("License..."),
        Off(if cfg!(target_os = "macos") {
            "Reveal in Finder"
        } else {
            "Reveal in Explorer"
        }),
        Off("Reveal in Bridge"),
        Sub("Reveal in Composition", &[]),
        Off("Solids Folder"),
        Own("Rename"),
    ]
};

struct Node {
    item: PrimPath,
    kind: Kind,
    depth: usize,
    row: usize,
    parent: Option<usize>,
    expandable: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Direction {
    Right,
    Left,
    #[default]
    Down,
    Up,
}

impl Direction {
    fn vertical(self) -> bool {
        matches!(self, Self::Down | Self::Up)
    }
    fn reverse(self) -> bool {
        matches!(self, Self::Left | Self::Up)
    }
}

const DIRECTIONS: [(Direction, &str); 4] = [
    (Direction::Down, "Top to Bottom"),
    (Direction::Up, "Bottom to Top"),
    (Direction::Right, "Left to Right"),
    (Direction::Left, "Right to Left"),
];
const JUSTIFY: [&str; 5] = [
    "Left Justify",
    "Left Center Justify",
    "Center Justify",
    "Right Center Justify",
    "Right Justify",
];

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Options {
    footage: bool,
    solids: bool,
    layers: bool,
    effects: bool,
    direction: Direction,
    curved: bool,
    justify: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            footage: true,
            solids: true,
            layers: true,
            effects: true,
            direction: Direction::Down,
            curved: true,
            justify: 1,
        }
    }
}

fn push(nodes: &mut Vec<Node>, item: &PrimPath, kind: Kind, parent: Option<usize>) -> usize {
    let depth = parent.map_or(0, |i| nodes[i].depth + 1);
    let index = nodes.len();
    nodes.push(Node {
        item: item.clone(),
        kind,
        depth,
        row: 0,
        parent,
        expandable: false,
    });
    index
}

fn chart(
    stage: &Stage,
    root: &PrimPath,
    options: &Options,
    expanded: &dyn Fn(&PrimPath) -> bool,
) -> Vec<Node> {
    fn add_sources(
        stage: &Stage,
        comp: &PrimPath,
        index: usize,
        options: &Options,
        expanded: &dyn Fn(&PrimPath) -> bool,
        ancestors: &mut Vec<PrimPath>,
        nodes: &mut Vec<Node>,
    ) {
        if ancestors.len() >= MAX_DEPTH || ancestors.contains(comp) {
            return;
        }
        let layers = query::comp_layers(stage, comp);
        nodes[index].expandable = !layers.is_empty();
        if !expanded(comp) {
            return;
        }
        ancestors.push(comp.clone());
        let mut seen = std::collections::HashSet::new();
        for layer in layers {
            let parent = if options.layers {
                push(nodes, &layer, Kind::Layer, Some(index))
            } else {
                index
            };
            if options.effects {
                for effect in bae_motion::effect::effects_of(stage, &layer) {
                    push(nodes, &effect, Kind::Effect, Some(parent));
                }
            }
            let source = stage
                .prim(&layer)
                .and_then(|p| p.value("source").and_then(|v| v.as_prim_ref()).cloned());
            let Some(source) = source else { continue };
            if !options.layers && !seen.insert(source.clone()) {
                continue;
            }
            let kind = match item_kind(stage, &source) {
                Some(ItemKind::Comp) => Kind::Comp,
                Some(ItemKind::Footage) if options.footage => Kind::Footage,
                Some(ItemKind::Solid) if options.solids => Kind::Solid,
                _ => continue,
            };
            let child = push(nodes, &source, kind, Some(parent));
            if kind == Kind::Comp {
                add_sources(stage, &source, child, options, expanded, ancestors, nodes);
            }
        }
        ancestors.pop();
    }
    let mut out = Vec::new();
    let first = push(&mut out, root, Kind::Comp, None);
    add_sources(
        stage,
        root,
        first,
        options,
        expanded,
        &mut Vec::new(),
        &mut out,
    );
    // Rows: a parent sits on the row of its first child (AE's compact layout).
    let mut row = 0;
    for (i, node) in out.iter_mut().enumerate() {
        let first_child = node.parent.is_some_and(|p| p + 1 == i);
        if i > 0 && !first_child {
            row += 1;
        }
        node.row = row;
    }
    out
}

/// Registers the panel (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    crate::register_panel(
        app,
        "flowchart",
        bae_ui::PanelKind::new("Flowchart", |spawn| {
            Box::new(FlowchartPanel {
                options: spawn
                    .state
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default(),
                ..Default::default()
            })
        }),
    );
}

/// The Flowchart panel.
#[derive(Default)]
pub struct FlowchartPanel {
    offset: egui::Vec2,
    options: Options,
    expanded: std::collections::HashSet<PrimPath>,
    positions: HashMap<(PrimPath, Option<PrimPath>), egui::Vec2>,
    last_active: Option<PrimPath>,
    selected: Option<PrimPath>,
    rename: Option<(PrimPath, String)>,
}

const ZH: &[(&str, &str)] = &[
    ("Replace with 0 Precomps", "替换为 0 个预合成"),
    ("Show Footage", "显示素材"),
    ("Show Solids", "显示纯色"),
    ("Show Layers", "显示图层"),
    ("Show Effects", "显示效果"),
    ("Flow Direction", "流动方向"),
    ("Top to Bottom", "自上而下"),
    ("Bottom to Top", "自下而上"),
    ("Left to Right", "从左到右"),
    ("Right to Left", "从右到左"),
    ("Justification", "对齐方式"),
    ("Left Justify", "左对齐"),
    ("Left Center Justify", "左中对齐"),
    ("Center Justify", "居中对齐"),
    ("Right Center Justify", "右中对齐"),
    ("Right Justify", "右对齐"),
    ("Straight Lines", "直线"),
    ("Curved Lines", "曲线"),
    ("Clean Up", "清理"),
];
impl FlowchartPanel {
    fn options_menu(&mut self, ui: &mut Ui, has_chart: bool) {
        if ui
            .add_enabled(has_chart, egui::Button::new(tr("Clean Up")))
            .clicked()
        {
            self.positions.clear();
            self.offset = egui::Vec2::ZERO;
            ui.close();
        }
        ui.separator();
        for (shown, label) in [
            (&mut self.options.layers, "Show Layers"),
            (&mut self.options.effects, "Show Effects"),
            (&mut self.options.footage, "Show Footage"),
            (&mut self.options.solids, "Show Solids"),
        ] {
            if ui.checkbox(shown, tr(label)).clicked() {
                ui.close();
            }
        }
        ui.separator();
        for (value, label) in [(false, "Straight Lines"), (true, "Curved Lines")] {
            if ui
                .radio_value(&mut self.options.curved, value, tr(label))
                .clicked()
            {
                ui.close();
            }
        }
        ui.separator();
        for (value, label) in DIRECTIONS {
            if ui
                .radio_value(&mut self.options.direction, value, tr(label))
                .clicked()
            {
                ui.close();
            }
        }
        ui.separator();
        for (value, label) in JUSTIFY.iter().enumerate() {
            if ui
                .radio_value(&mut self.options.justify, value, tr(label))
                .clicked()
            {
                ui.close();
            }
        }
    }

    fn label_menu(&self, ui: &mut Ui, cx: &mut PanelContext<'_>, node: &Node, nodes: &[Node]) {
        let can_select = matches!(node.kind, Kind::Comp | Kind::Layer);
        let label = |n: &Node| node_label(cx.stage, n);
        let current = label(node);
        if ui
            .add_enabled(can_select, egui::Button::new(tr("Select Label Group")))
            .clicked()
        {
            if node.kind == Kind::Layer {
                cx.invoke(crate::actions::ids::SELECT_LABEL_GROUP);
            } else {
                let mut items = Vec::new();
                for n in nodes.iter().filter(|n| {
                    matches!(n.kind, Kind::Comp | Kind::Footage | Kind::Solid)
                        && label(n) == current
                }) {
                    if !items.contains(&n.item) {
                        items.push(n.item.clone());
                    }
                }
                common::session(cx.resources).lock().selected_items = items;
            }
            ui.close();
        }
        ui.separator();
        for (index, (name, _)) in theme::LABELS.iter().enumerate() {
            if index == 1 {
                ui.separator();
            }
            if ui.radio(current == index, tr(name)).clicked() {
                cx.invoke_with("attr.set", serde_json::json!({"path":node.item,"attr":"label","value":{"type":"int","value":index}}));
                ui.close();
            }
        }
        ui.separator();
        if ui.button(tr("Edit Label Colors...")).clicked() {
            cx.invoke_with("app.settings", serde_json::json!({"page":"labels"}));
            ui.close();
        }
    }

    fn select(&mut self, cx: &PanelContext<'_>, n: &Node) {
        let session = common::session(cx.resources);
        let mut s = session.lock();
        self.selected = Some(n.item.clone());
        match n.kind {
            Kind::Comp => {
                s.open_comp(&n.item);
                s.selected_items = vec![n.item.clone()];
            }
            Kind::Footage | Kind::Solid => s.selected_items = vec![n.item.clone()],
            Kind::Layer | Kind::Effect => {
                if let Some(c) = query::owning_comp(cx.stage, &n.item) {
                    s.open_comp(&c);
                }
                let layer = if n.kind == Kind::Layer {
                    Some(n.item.clone())
                } else {
                    query::owning_layer(cx.stage, &n.item)
                };
                s.selected_layers = layer.into_iter().collect();
                s.selected_effects = if n.kind == Kind::Effect {
                    vec![n.item.clone()]
                } else {
                    vec![]
                };
            }
        }
        self.last_active.clone_from(&s.active_comp);
    }

    fn icon_menu(
        &mut self,
        ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        n: &Node,
        menu: IconMenu,
        nodes: &[Node],
    ) {
        let session = common::session(cx.resources);
        let rename = match menu {
            IconMenu::Composition => {
                use crate::context_menu as m;
                for entry in m::VIEWER_BLANK {
                    match entry {
                        m::Entry::Menu("Composition/Composition Mini-Flowchart") => {
                            ui.add_enabled(
                                false,
                                egui::Button::new(tr("Composition Mini-Flowchart")),
                            );
                        }
                        m::Entry::Menu("Composition/Open in Essential Graphics") => {
                            if ui.button(tr("Open in Essential Graphics")).clicked() {
                                cx.open_panel("essential_graphics");
                                ui.close();
                            }
                        }
                        _ => {
                            m::show(ui, cx, std::slice::from_ref(entry));
                        }
                    }
                }
                false
            }
            IconMenu::Layer => crate::timeline::layer_menu(ui, cx, &session, &n.item).is_some(),
            IconMenu::Labels => {
                self.label_menu(ui, cx, n, nodes);
                false
            }
            IconMenu::Solid => match crate::context_menu::show(ui, cx, SOLID_ICON) {
                Some("Rename") => true,
                Some("Replace with 0 Precomps") => {
                    cx.invoke_with(
                        "item.replace_with_precomp",
                        serde_json::json!({"item":n.item}),
                    );
                    false
                }
                Some("Reload Footage") => {
                    // Solids are generated from the stage. Re-evaluate their current source
                    // on the next composition redraw, without an import or an undo step.
                    let mut s = session.lock();
                    let generation = s.solid_reload.as_ref().map_or(1, |(n, _)| n + 1);
                    s.solid_reload = Some((generation, n.item.clone()));
                    ui.ctx().request_repaint();
                    false
                }
                _ => false,
            },
        };
        if rename {
            self.rename = Some((n.item.clone(), query::display_name_of(cx.stage, &n.item)));
        }
    }
}

impl Panel for FlowchartPanel {
    fn tab(&self, cx: &PanelContext<'_>) -> TabInfo {
        // AE: "Flowchart <comp>".
        let session = common::session(cx.resources);
        let s = session.lock();
        let comp = s.flowchart_root.clone().or_else(|| s.active_comp.clone());
        let name = comp.map_or_else(
            || tr("(none)").to_owned(),
            |c| bae_motion::query::display_name_of(cx.stage, &c),
        );
        TabInfo::new(format!("{} {name}", tr("Flowchart")))
    }

    fn on_action(&mut self, action: &bae_ui::Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        if let Some(item) = self
            .selected
            .as_ref()
            .filter(|p| item_kind(cx.stage, p).is_some())
        {
            if action.id == crate::actions::ids::DELETE {
                cx.invoke_with("item.delete", serde_json::json!({"paths":[item]}));
                return true;
            }
            if action.id == crate::actions::ids::LAYER_SETTINGS {
                if let Some(target) =
                    crate::actions::layer_settings_target(cx.stage, &session, true)
                {
                    crate::dialogs::layer_settings(cx, &session, &target);
                }
                return true;
            }
        }
        crate::actions::handle(action, cx, &session)
    }

    fn menu(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        self.options_menu(
            ui,
            common::session(cx.resources).lock().active_comp.is_some(),
        );
    }
    fn save_state(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.options).ok()
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        if let Some(progress) = crate::render_queue::panel_progress(cx.resources, cx.stage) {
            ui.label(progress);
        }
        let full = ui.available_rect_before_wrap();
        let canvas = ui.allocate_rect(full, Sense::click_and_drag());
        let p = ui.painter_at(full);
        p.rect_filled(full, 0.0, Color32::from_gray(35));
        let session = common::session(cx.resources);
        let (active, mut root) = {
            let s = session.lock();
            (s.active_comp.clone(), s.flowchart_root.clone())
        };
        if active != self.last_active || root.as_ref().is_some_and(|r| !cx.stage.has_prim(r)) {
            root.clone_from(&active);
            session.lock().flowchart_root.clone_from(&root);
            self.last_active = active;
            self.expanded.clear();
            self.offset = egui::Vec2::ZERO;
            self.positions.clear();
            self.rename = None;
        }
        let Some(root) = root else {
            return;
        };
        let nodes = chart(cx.stage, &root, &self.options, &|p| {
            self.expanded.contains(p)
        });
        let max_depth = nodes.iter().map(|n| n.depth).max().unwrap_or(0);
        let dir = self.options.direction;
        let sibling = if dir.vertical() {
            NODE.x + GAP.x
        } else {
            NODE.y + GAP.y
        };
        let mut last_row: Vec<usize> = nodes.iter().map(|n| n.row).collect();
        for (i, n) in nodes.iter().enumerate().rev() {
            if let Some(parent) = n.parent {
                last_row[parent] = last_row[parent].max(last_row[i]);
            }
        }
        let key_of = |n: &Node| (n.item.clone(), n.parent.map(|i| nodes[i].item.clone()));
        let origin = full.min + vec2(16.0, 16.0) + self.offset;
        let rects: Vec<_> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let depth = if dir.reverse() {
                    max_depth - n.depth
                } else {
                    n.depth
                };
                let across = (n.row as f32
                    + (last_row[i] - n.row) as f32 * self.options.justify.min(4) as f32 / 4.0)
                    * sibling;
                let grid = if dir.vertical() {
                    vec2(across, depth as f32 * (NODE.y + GAP.y))
                } else {
                    vec2(depth as f32 * (NODE.x + GAP.x), across)
                };
                Rect::from_min_size(
                    origin + grid + self.positions.get(&key_of(n)).copied().unwrap_or_default(),
                    NODE,
                )
            })
            .collect();
        let line = Stroke::new(1.0, color::TEXT_DIM);
        for (i, n) in nodes.iter().enumerate() {
            if let Some(parent) = n.parent {
                let (a, b) = (rects[parent], rects[i]);
                let (start, end) = match dir {
                    Direction::Right => (a.right_center(), b.left_center()),
                    Direction::Left => (a.left_center(), b.right_center()),
                    Direction::Down => (a.center_bottom(), b.center_top()),
                    Direction::Up => (a.center_top(), b.center_bottom()),
                };
                let half = (end - start) * 0.5;
                let delta = if dir.vertical() {
                    vec2(0.0, half.y)
                } else {
                    vec2(half.x, 0.0)
                };
                let pts = [start, start + delta, end - delta, end];
                if self.options.curved {
                    p.add(egui::epaint::CubicBezierShape::from_points_stroke(
                        pts,
                        false,
                        Color32::TRANSPARENT,
                        line,
                    ));
                } else {
                    p.add(egui::Shape::line(pts.to_vec(), line));
                }
            }
        }
        if canvas.dragged() {
            self.offset += canvas.drag_delta();
        }
        common::context_menu(&canvas, &mut |ui| self.options_menu(ui, true));
        for (i, n) in nodes.iter().enumerate() {
            let r = rects[i];
            if !full.intersects(r) {
                continue;
            }
            let icon_rect = Rect::from_min_max(r.min, pos2(r.left() + 25.0, r.bottom()));
            let toggle = Rect::from_min_max(pos2(r.right() - 18.0, r.top()), r.max);
            let title_rect = Rect::from_min_max(
                pos2(icon_rect.right(), r.top()),
                pos2(
                    if n.expandable {
                        toggle.left()
                    } else {
                        r.right()
                    },
                    r.bottom(),
                ),
            );
            let icon_resp = ui.interact(icon_rect, ui.id().with(("flow-icon", i)), Sense::click());
            let title = ui.interact(
                title_rect,
                ui.id().with(("flow-title", i)),
                Sense::click_and_drag(),
            );
            let hovered = icon_resp.hovered() || title.hovered();
            p.rect_filled(
                r,
                3.0,
                if hovered {
                    color::FIELD_HOVER
                } else {
                    color::FIELD
                },
            );
            if self.selected.as_ref().unwrap_or(&root) == &n.item {
                p.rect_stroke(
                    r,
                    3.0,
                    Stroke::new(1.0, color::ACCENT),
                    egui::StrokeKind::Inside,
                );
            }
            let icon = match n.kind {
                Kind::Comp => Icon::Composition,
                Kind::Effect => Icon::Fx,
                Kind::Layer => Icon::TextLayer,
                _ => Icon::Footage,
            };
            icons::paint(
                &p,
                Rect::from_center_size(icon_rect.center(), vec2(14.0, 14.0)),
                icon,
                color::ICON,
            );
            if n.expandable {
                let open = self.expanded.contains(&n.item);
                p.text(
                    toggle.center(),
                    Align2::CENTER_CENTER,
                    if open { "−" } else { "+" },
                    theme::body(),
                    color::TEXT,
                );
                if ui
                    .interact(
                        toggle,
                        ui.id().with(("flow-expand", key_of(n))),
                        Sense::click(),
                    )
                    .clicked()
                {
                    if open {
                        self.expanded.remove(&n.item);
                    } else {
                        self.expanded.insert(n.item.clone());
                    }
                }
            }
            let name = query::display_name_of(cx.stage, &n.item);
            if let Some((_, text)) = self.rename.as_mut().filter(|(path, _)| path == &n.item) {
                if let Some(result) =
                    common::rename_field(ui, title_rect, text, ui.id().with(("rename", i)))
                {
                    if let Some(value) = result.filter(|v| !v.trim().is_empty() && v != &name) {
                        cx.invoke_with("attr.set",serde_json::json!({"path":n.item,"attr":"name","value":{"type":"string","value":value}}));
                    }
                    self.rename = None;
                }
            } else {
                p.text(
                    title_rect.left_center(),
                    Align2::LEFT_CENTER,
                    name,
                    theme::body(),
                    color::TEXT,
                );
            }
            if title.dragged() {
                *self.positions.entry(key_of(n)).or_default() += title.drag_delta();
            }
            if icon_resp.clicked()
                || icon_resp.secondary_clicked()
                || title.clicked()
                || title.secondary_clicked()
            {
                self.select(cx, n);
            }
            if (title.double_clicked() || icon_resp.double_clicked()) && n.kind == Kind::Comp {
                session.lock().flowchart_root = Some(n.item.clone());
                self.offset = egui::Vec2::ZERO;
                self.positions.clear();
            }
            if let Some(menu) = icon_menu_for(cx.stage, n) {
                common::context_menu(&icon_resp, &mut |ui| {
                    self.icon_menu(ui, cx, n, menu, &nodes)
                });
            }
            common::context_menu(&title, &mut |ui| self.label_menu(ui, cx, n, &nodes));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_comps_chart_left_to_right() {
        let mut stage = Stage::new();
        let mut reg = bae_model::CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        let author = bae_model::AuthorId::new("user:test").unwrap();
        let run = |stage: &mut Stage, id: &str, p: serde_json::Value| {
            reg.execute(stage, &author, id, p).unwrap().output
        };
        let path = |v: &serde_json::Value| PrimPath::new(v["path"].as_str().unwrap()).unwrap();
        let outer = path(&run(
            &mut stage,
            "comp.create",
            serde_json::json!({"width": 64, "height": 64}),
        ));
        let inner = path(&run(
            &mut stage,
            "comp.create",
            serde_json::json!({"width": 32, "height": 32}),
        ));
        run(
            &mut stage,
            "layer.create",
            serde_json::json!({"comp": outer, "kind": "pre_comp", "source": inner}),
        );
        run(
            &mut stage,
            "layer.create",
            serde_json::json!({"comp": inner, "kind": "solid"}),
        );
        let compact = Options {
            layers: false,
            ..Default::default()
        };
        let collapsed = chart(&stage, &outer, &compact, &|_| false);
        assert!(collapsed.len() == 1 && collapsed[0].expandable);
        let nodes = chart(&stage, &outer, &compact, &|_| true);
        assert_eq!(nodes[0].item, outer);
        assert_eq!(nodes[1].item, inner);
        assert_eq!((nodes[1].depth, nodes[1].parent), (1, Some(0)));
        // The solid inside the nested comp, one column further.
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[2].depth, 2);
        let hidden = chart(
            &stage,
            &outer,
            &Options {
                solids: false,
                ..compact
            },
            &|_| true,
        );
        assert_eq!(hidden.len(), 2);
        let layered = chart(
            &stage,
            &outer,
            &Options {
                layers: true,
                ..Default::default()
            },
            &|_| true,
        );
        assert_eq!(
            layered.iter().map(|n| n.kind).collect::<Vec<_>>(),
            [
                Kind::Comp,
                Kind::Layer,
                Kind::Comp,
                Kind::Layer,
                Kind::Solid
            ]
        );
        // Compact rows: each first child shares its parent's row.
        assert_eq!(nodes.iter().map(|n| n.row).collect::<Vec<_>>(), [0, 0, 0]);
    }
    #[test]
    fn canvas_menu_localization_and_cleanup_availability() {
        use bae_ui::i18n;
        i18n::register(ZH);
        for active in [false, true] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut panel = FlowchartPanel::default();
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                panel.options_menu(ui, active)
            });
            out.textures_delta.clear();
            let update = out.platform_output.accesskit_update.unwrap();
            for (en, zh, enabled) in [
                ("Show Layers", "显示图层", true),
                ("Curved Lines", "曲线", true),
                ("Top to Bottom", "自上而下", true),
                ("Left Center Justify", "左中对齐", true),
                ("Clean Up", "清理", active),
            ] {
                assert_eq!(i18n::translation(en), Some(zh));
                assert!(
                    update.nodes.iter().any(|(_, n)| n.label() == Some(tr(en))
                        && n.is_disabled() != enabled
                        && (en == "Clean Up"
                            || n.toggled() == Some(egui::accesskit::Toggled::True))),
                    "{en}"
                );
            }
        }
    }
    #[test]
    fn node_names_show_label_colors_with_the_recorded_context_states() {
        let mut app = bae_app::AppBuilder::new();
        app.add_module(crate::MotionUiModule);
        let _app = app.build();
        let mut stage = Stage::new();
        let author = bae_model::AuthorId::new("user:test").unwrap();
        let mut reg = bae_model::CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        let mut run = |command, args| {
            reg.execute(&mut stage, &author, command, args)
                .unwrap()
                .output
        };
        let comp = run("comp.create", serde_json::json!({"width":64,"height":64}))["path"].clone();
        let layer = run(
            "layer.create",
            serde_json::json!({"comp":comp,"kind":"solid"}),
        )["path"]
            .clone();
        run(
            "attr.set",
            serde_json::json!({"path":comp,"attr":"label","value":{"type":"int","value":1}}),
        );
        run(
            "attr.set",
            serde_json::json!({"path":layer,"attr":"label","value":{"type":"int","value":8}}),
        );
        let resources = bae_app::Resources::new();
        let id = bae_ui::PanelId::new("flowchart");
        let mut requests = bae_ui::PanelRequests::default();
        let mut cx =
            PanelContext::new(&id, "Flowchart", &stage, &author, &resources, &mut requests);
        let solid = stage
            .prim(&PrimPath::new(layer.as_str().unwrap()).unwrap())
            .unwrap()
            .value("source")
            .and_then(|v| v.as_prim_ref())
            .unwrap()
            .to_string();
        for (kind, path) in [
            (Kind::Comp, comp.clone()),
            (Kind::Layer, layer.clone()),
            (Kind::Solid, serde_json::json!(solid)),
            (Kind::Footage, serde_json::json!("/Unresolved")),
            (Kind::Effect, serde_json::json!("/Effect")),
        ] {
            let mut nodes = Vec::new();
            push(
                &mut nodes,
                &PrimPath::new(path.as_str().unwrap()).unwrap(),
                kind,
                None,
            );
            assert_eq!(
                icon_menu_for(&stage, &nodes[0]),
                match kind {
                    Kind::Comp => Some(IconMenu::Composition),
                    Kind::Layer => Some(IconMenu::Layer),
                    Kind::Solid => Some(IconMenu::Solid),
                    Kind::Footage => None,
                    Kind::Effect => Some(IconMenu::Labels),
                }
            );
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut out = ctx.run_ui(Default::default(), |ui| {
                FlowchartPanel::default().label_menu(ui, &mut cx, &nodes[0], &nodes)
            });
            out.textures_delta.clear();
            let update = out.platform_output.accesskit_update.unwrap();
            for (key, zh, enabled, checked) in [
                (
                    "Select Label Group",
                    "选择标签组",
                    matches!(kind, Kind::Comp | Kind::Layer),
                    false,
                ),
                (
                    "Red",
                    "红色",
                    true,
                    matches!(kind, Kind::Comp | Kind::Solid),
                ),
                ("Blue", "蓝色", true, kind == Kind::Layer),
                ("Edit Label Colors...", "编辑标签颜色...", true, false),
            ] {
                assert_eq!(bae_ui::i18n::translation(key), Some(zh));
                let node = update
                    .nodes
                    .iter()
                    .find(|(_, n)| n.label() == Some(tr(key)))
                    .unwrap();
                assert_eq!(
                    (
                        !node.1.is_disabled(),
                        node.1.toggled() == Some(egui::accesskit::Toggled::True)
                    ),
                    (enabled, checked)
                );
            }
        }
    }
    #[test]
    fn solid_icon_menu_matches_the_windows_inventory() {
        use crate::context_menu::Entry::*;
        let mut app = bae_app::AppBuilder::new();
        app.add_module(crate::MotionUiModule);
        let _app = app.build();
        let expected = [
            ("合成设置...", false),
            ("在基本图形中打开", false),
            ("基于所选项新建合成...", false),
            ("替换为 0 个预合成", true),
            ("创建代理", true),
            ("设置代理", true),
            ("导入", true),
            ("导入最近的素材", true),
            ("解释素材", true),
            ("替换素材", true),
            ("重新加载素材", true),
            ("许可...", false),
            (
                if cfg!(target_os = "macos") {
                    "在 Finder 中显示"
                } else {
                    "在资源管理器中显示"
                },
                false,
            ),
            ("在 Bridge 中显示", false),
            ("在合成中显示", false),
            ("纯色文件夹", false),
            ("重命名", true),
        ];
        let actual: Vec<_> = SOLID_ICON
            .iter()
            .map(|entry| {
                let (title, enabled) = match *entry {
                    Own(title) => (title, true),
                    Off(title) | Sub(title, []) => (title, false),
                    Menu(path) => match crate::context_menu::find(path) {
                        Some(crate::catalog::Node::Submenu(menu)) => {
                            (menu.title, !menu.children.is_empty())
                        }
                        other => panic!("missing solid action {path}: {other:?}"),
                    },
                    _ => panic!("unexpected solid menu row"),
                };
                (bae_ui::i18n::translation(title).unwrap_or(title), enabled)
            })
            .collect();
        assert_eq!(actual, expected);
    }
}
