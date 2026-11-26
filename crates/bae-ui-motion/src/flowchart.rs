//! Window ▸ Flowchart: AE's Flowchart panel for the active composition — the
//! comp, then (left to right) the items its layers use: nested comps with
//! their own sources, footage and solids, joined by lines. Double-click a comp
//! node to open it.

use bae_model::{PrimPath, PrimType, Stage};
use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use bae_ui::{Panel, PanelContext, TabInfo, tr};
use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::common;

const NODE: egui::Vec2 = vec2(150.0, 24.0);
const GAP: egui::Vec2 = vec2(60.0, 10.0);
/// Nested comps deeper than this are not expanded.
const MAX_DEPTH: usize = 8;

/// A node of the chart.
struct Node {
    item: PrimPath,
    comp: bool,
    depth: usize,
    /// Row (top to bottom).
    row: usize,
    parent: Option<usize>,
}

/// The items `comp`'s layers use, in layer order, without repeats.
fn sources(stage: &Stage, comp: &PrimPath) -> Vec<PrimPath> {
    let mut out: Vec<PrimPath> = Vec::new();
    for layer in bae_motion::query::comp_layers(stage, comp) {
        let source = stage
            .prim(&layer)
            .and_then(|v| v.value("source").and_then(|s| s.as_prim_ref()).cloned());
        if let Some(s) = source.filter(|s| stage.has_prim(s))
            && !out.contains(&s)
        {
            out.push(s);
        }
    }
    out
}

/// The tree under `comp`, depth first (rows in order).
fn chart(stage: &Stage, comp: &PrimPath) -> Vec<Node> {
    fn add(
        stage: &Stage,
        item: &PrimPath,
        depth: usize,
        parent: Option<usize>,
        path: &mut Vec<PrimPath>,
        out: &mut Vec<Node>,
    ) {
        let comp = bae_motion::query::comp_info(stage, item).is_some();
        let index = out.len();
        out.push(Node {
            item: item.clone(),
            comp,
            depth,
            row: index,
            parent,
        });
        if comp && depth < MAX_DEPTH && !path.contains(item) {
            path.push(item.clone());
            for s in sources(stage, item) {
                add(stage, &s, depth + 1, Some(index), path, out);
            }
            path.pop();
        }
    }
    let mut out = Vec::new();
    add(stage, comp, 0, None, &mut Vec::new(), &mut out);
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
    crate::register_panel(
        app,
        "flowchart",
        bae_ui::PanelKind::new("Flowchart", |_| Box::new(FlowchartPanel::default())),
    );
}

/// The Flowchart panel.
#[derive(Default)]
pub struct FlowchartPanel {
    offset: egui::Vec2,
}

impl Panel for FlowchartPanel {
    fn tab(&self, cx: &PanelContext<'_>) -> TabInfo {
        // AE: "Flowchart <comp>".
        let comp = common::session(cx.resources).lock().active_comp.clone();
        let name = comp.map_or_else(
            || tr("(none)").to_owned(),
            |c| bae_motion::query::display_name_of(cx.stage, &c),
        );
        TabInfo::new(format!("{} {name}", tr("Flowchart")))
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let full = ui.max_rect();
        let resp = ui.allocate_rect(full, Sense::click_and_drag());
        if resp.dragged() {
            self.offset += resp.drag_delta();
        }
        let p = ui.painter_at(full);
        p.rect_filled(full, 0.0, Color32::from_gray(35));
        let session = common::session(cx.resources);
        let Some(comp) = session.lock().active_comp.clone() else {
            return;
        };
        let nodes = chart(cx.stage, &comp);
        let origin = full.min + vec2(16.0, 16.0) + self.offset;
        let rect_of = |n: &Node| {
            Rect::from_min_size(
                origin
                    + vec2(
                        n.depth as f32 * (NODE.x + GAP.x),
                        n.row as f32 * (NODE.y + GAP.y),
                    ),
                NODE,
            )
        };
        let line = Stroke::new(1.0, color::TEXT_DIM);
        for n in &nodes {
            if let Some(parent) = n.parent {
                let (a, b) = (rect_of(&nodes[parent]), rect_of(n));
                let from = pos2(a.right(), a.center().y);
                let to = pos2(b.left(), b.center().y);
                let mid = (from.x + to.x) / 2.0;
                p.line_segment([from, pos2(mid, from.y)], line);
                p.line_segment([pos2(mid, from.y), pos2(mid, to.y)], line);
                p.line_segment([pos2(mid, to.y), to], line);
            }
        }
        let font = theme::body();
        let mut open = None;
        let pointer = ui.input(|i| i.pointer.hover_pos());
        for n in &nodes {
            let r = rect_of(n);
            if !full.intersects(r) {
                continue;
            }
            let hovered = pointer.is_some_and(|q| r.contains(q));
            p.rect_filled(
                r,
                3.0,
                if hovered {
                    color::FIELD_HOVER
                } else {
                    color::FIELD
                },
            );
            if n.depth == 0 {
                p.rect_stroke(
                    r,
                    3.0,
                    Stroke::new(1.0, color::ACCENT),
                    egui::StrokeKind::Inside,
                );
            }
            let icon = if n.comp {
                Icon::Composition
            } else if cx
                .stage
                .prim(&n.item)
                .is_some_and(|v| v.is_a(&PrimType::FOLDER))
            {
                Icon::Folder
            } else {
                Icon::Footage
            };
            icons::paint(
                &p,
                Rect::from_center_size(pos2(r.left() + 13.0, r.center().y), vec2(14.0, 14.0)),
                icon,
                color::ICON,
            );
            let name = bae_motion::query::display_name_of(cx.stage, &n.item);
            p.text(
                pos2(r.left() + 26.0, r.center().y),
                Align2::LEFT_CENTER,
                name,
                font.clone(),
                color::TEXT,
            );
            if hovered && resp.double_clicked() && n.comp {
                open = Some(n.item.clone());
            }
        }
        if let Some(c) = open {
            session.lock().open_comp(&c);
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
        let nodes = chart(&stage, &outer);
        assert_eq!(nodes[0].item, outer);
        assert_eq!(nodes[1].item, inner);
        assert_eq!((nodes[1].depth, nodes[1].parent), (1, Some(0)));
        // The solid inside the nested comp, one column further.
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[2].depth, 2);
        // Compact rows: each first child shares its parent's row.
        assert_eq!(nodes.iter().map(|n| n.row).collect::<Vec<_>>(), [0, 0, 0]);
    }
}
