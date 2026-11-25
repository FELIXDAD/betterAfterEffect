//! The Align panel (AE: Window > Align): "Align Layers to:" Selection or
//! Composition, six align buttons and six distribute buttons. Each click moves
//! the selected 2D layers by their bounds in one undo step (keyframed positions
//! get a key at the current time, as dragging in the viewer).

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2};
use bae_model::PrimPath;
use bae_ui::icons::{self, Icon};
use bae_ui::{Panel, PanelContext, tr};
use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::common::{self, EvalCache};
use crate::render::{Affine, layer_bounds};
use crate::viewer::transform::{Kind, TransformDrag};

/// What layers are aligned to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignTo {
    /// The selected layers' bounds (needs two layers).
    #[default]
    Selection,
    Composition,
}

/// An edge or center of a box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    HCenter,
    Right,
    Top,
    VCenter,
    Bottom,
}

impl Edge {
    fn of(self, b: &Bounds) -> f64 {
        match self {
            Edge::Left => b.min.x,
            Edge::HCenter => (b.min.x + b.max.x) / 2.0,
            Edge::Right => b.max.x,
            Edge::Top => b.min.y,
            Edge::VCenter => (b.min.y + b.max.y) / 2.0,
            Edge::Bottom => b.max.y,
        }
    }

    fn horizontal(self) -> bool {
        matches!(self, Edge::Left | Edge::HCenter | Edge::Right)
    }
}

/// A layer's bounds in comp space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: DVec2,
    pub max: DVec2,
}

/// AE's buttons: (edge, icon, tooltip).
const ALIGN: [(Edge, Icon, &str); 6] = [
    (Edge::Left, Icon::AlignLeft, "Align Left"),
    (Edge::HCenter, Icon::AlignCenter, "Align Horizontally"),
    (Edge::Right, Icon::AlignRight, "Align Right"),
    (Edge::Top, Icon::AlignTop, "Align Top"),
    (Edge::VCenter, Icon::AlignMiddle, "Align Vertically"),
    (Edge::Bottom, Icon::AlignBottom, "Align Bottom"),
];
const DISTRIBUTE: [(Edge, Icon, &str); 6] = [
    (Edge::Top, Icon::DistributeTopEdge, "Distribute Top"),
    (
        Edge::VCenter,
        Icon::DistributeVerticalCenter,
        "Distribute Vertically",
    ),
    (
        Edge::Bottom,
        Icon::DistributeBottomEdge,
        "Distribute Bottom",
    ),
    (Edge::Left, Icon::DistributeLeftEdge, "Distribute Left"),
    (
        Edge::HCenter,
        Icon::DistributeHorizontalCenter,
        "Distribute Horizontally",
    ),
    (Edge::Right, Icon::DistributeRightEdge, "Distribute Right"),
];

/// How far each layer moves to align `edge` with the selection's (or the
/// comp's, `comp = Some((width, height))`) same edge.
pub fn align_offsets(layers: &[Bounds], edge: Edge, comp: Option<(f64, f64)>) -> Vec<DVec2> {
    let target = match comp {
        Some((w, h)) => edge.of(&Bounds {
            min: DVec2::ZERO,
            max: DVec2::new(w, h),
        }),
        None => {
            let all = layers.iter().fold(None::<Bounds>, |acc, b| {
                Some(acc.map_or(*b, |a| Bounds {
                    min: a.min.min(b.min),
                    max: a.max.max(b.max),
                }))
            });
            match all {
                Some(a) => edge.of(&a),
                None => return Vec::new(),
            }
        }
    };
    layers
        .iter()
        .map(|b| {
            let d = target - edge.of(b);
            if edge.horizontal() {
                DVec2::new(d, 0.0)
            } else {
                DVec2::new(0.0, d)
            }
        })
        .collect()
}

/// How far each layer moves so that `edge` is evenly spaced between the
/// outermost two layers' (which stay), in the layers' order along that axis.
pub fn distribute_offsets(layers: &[Bounds], edge: Edge) -> Vec<DVec2> {
    let mut out = vec![DVec2::ZERO; layers.len()];
    if layers.len() < 3 {
        return out;
    }
    let mut order: Vec<usize> = (0..layers.len()).collect();
    order.sort_by(|&a, &b| edge.of(&layers[a]).total_cmp(&edge.of(&layers[b])));
    let first = edge.of(&layers[order[0]]);
    let last = edge.of(&layers[order[order.len() - 1]]);
    let step = (last - first) / (order.len() - 1) as f64;
    for (k, &i) in order.iter().enumerate() {
        let d = first + step * k as f64 - edge.of(&layers[i]);
        out[i] = if edge.horizontal() {
            DVec2::new(d, 0.0)
        } else {
            DVec2::new(0.0, d)
        };
    }
    out
}

/// The Align panel.
#[derive(Default)]
pub struct AlignPanel {
    to: AlignTo,
    eval: EvalCache,
}

impl AlignPanel {
    pub fn from_state(state: Option<&serde_json::Value>) -> Self {
        AlignPanel {
            to: state
                .and_then(|s| s.get("to"))
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            eval: EvalCache::default(),
        }
    }
}

/// What a click asks for.
enum Op {
    Align(Edge),
    Distribute(Edge),
}

impl Panel for AlignPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        let info = common::active_comp(cx.stage, &session);
        let selected: Vec<PrimPath> = info.as_ref().map_or(Vec::new(), |i| {
            let order = bae_motion::query::comp_layers(cx.stage, &i.path);
            session
                .lock()
                .selected_layers
                .iter()
                .filter(|l| order.contains(l))
                .cloned()
                .collect()
        });
        // AE: Selection needs two layers; with one, layers align to the comp.
        let to = if selected.len() < 2 {
            AlignTo::Composition
        } else {
            self.to
        };
        let font = bae_ui::theme::body();
        let text = bae_ui::theme::color::TEXT;
        let mut op = None;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tr("Align Layers to:")).color(text));
            let current = match to {
                AlignTo::Selection => tr("Selection"),
                AlignTo::Composition => tr("Composition"),
            };
            let resp = combo(ui, current, 106.0);
            let resp = resp.on_hover_text(tr("Align Options"));
            egui::Popup::menu(&resp)
                .id(ui.id().with("align_to"))
                .show(|ui| {
                    ui.add_enabled_ui(selected.len() >= 2, |ui| {
                        if ui
                            .selectable_label(to == AlignTo::Selection, tr("Selection"))
                            .clicked()
                        {
                            self.to = AlignTo::Selection;
                        }
                    });
                    if ui
                        .selectable_label(to == AlignTo::Composition, tr("Composition"))
                        .clicked()
                    {
                        self.to = AlignTo::Composition;
                    }
                });
        });
        ui.add_space(6.0);
        let can_align = !selected.is_empty() && (to == AlignTo::Composition || selected.len() >= 2);
        ui.add_enabled_ui(can_align, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                for (edge, icon, tip) in ALIGN {
                    if icon_button(ui, icon, tr(tip)).clicked() {
                        op = Some(Op::Align(edge));
                    }
                }
            });
        });
        ui.add_space(4.0);
        let r = ui.available_rect_before_wrap();
        ui.painter().hline(
            r.left() + 6.0..=r.left() + 200.0,
            r.top(),
            Stroke::new(1.0, bae_ui::theme::color::DIVIDER),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tr("Distribute Layers:")).color(text));
        });
        ui.add_space(2.0);
        ui.add_enabled_ui(selected.len() >= 3, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                for (edge, icon, tip) in DISTRIBUTE {
                    if icon_button(ui, icon, tr(tip)).clicked() {
                        op = Some(Op::Distribute(edge));
                    }
                }
            });
        });
        let _ = font;
        if let (Some(op), Some(info)) = (op, info) {
            let time = session.lock().time(&info.path);
            let Ok(comp) = self
                .eval
                .get(cx.resources)
                .evaluate_comp(cx.stage, &info.path, time)
            else {
                return;
            };
            let comp_size = (f64::from(info.width), f64::from(info.height));
            let edits = move_edits(cx.stage, &comp, &selected, time, |bounds| match op {
                Op::Align(edge) => align_offsets(
                    bounds,
                    edge,
                    (to == AlignTo::Composition).then_some(comp_size),
                ),
                Op::Distribute(edge) => distribute_offsets(bounds, edge),
            });
            if !edits.is_empty() {
                cx.invoke_with("attr.set_many", json!({ "edits": edits }));
            }
        }
    }

    fn save_state(&self) -> Option<serde_json::Value> {
        Some(json!({ "to": self.to }))
    }
}

/// The value edits moving `layers` (2D, unlocked, with bounds) by the offsets
/// `offsets` computes from their bounds, as one `attr.set_many`.
fn move_edits(
    stage: &bae_model::Stage,
    comp: &EvaluatedComp,
    layers: &[PrimPath],
    time: bae_model::Time,
    offsets: impl FnOnce(&[Bounds]) -> Vec<DVec2>,
) -> Vec<serde_json::Value> {
    let outer = Affine::new(DMat4::IDENTITY);
    let mut paths = Vec::new();
    let mut bounds = Vec::new();
    for l in comp.layers.iter().filter(|l| layers.contains(&l.path)) {
        // AE cannot align 3D layers; locked layers do not move.
        if l.switches.three_d || bae_motion::query::is_locked(stage, &l.path) {
            continue;
        }
        let Some((min, max)) = layer_bounds(l) else {
            continue;
        };
        let xf = outer.then(&l.world_matrix);
        let corners =
            [min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)].map(|p| xf.apply(p));
        let lo = corners
            .iter()
            .fold(DVec2::splat(f64::INFINITY), |a, p| a.min(*p));
        let hi = corners
            .iter()
            .fold(DVec2::splat(f64::NEG_INFINITY), |a, p| a.max(*p));
        paths.push(l.path.clone());
        bounds.push(Bounds { min: lo, max: hi });
    }
    let mut edits = Vec::new();
    for (path, d) in paths.iter().zip(offsets(&bounds)) {
        if d.length_squared() < 1e-12 {
            continue;
        }
        let mut drag = TransformDrag::new(
            stage,
            comp,
            std::slice::from_ref(path),
            Kind::Move { start: DVec2::ZERO },
        );
        drag.update(d, false, None);
        edits.extend(drag.commit(stage, time));
    }
    edits
}

/// A dark menu field (AE's dropdown).
fn combo(ui: &mut Ui, current: &str, width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click());
    use bae_ui::a11y::Accessible as _;
    let resp = resp.labeled_value(egui::WidgetType::ComboBox, tr("Align Layers to:"), current);
    let p = ui.painter();
    let bg = if resp.hovered() {
        bae_ui::theme::color::FIELD_HOVER
    } else {
        bae_ui::theme::color::DROPDOWN
    };
    p.rect_filled(rect, 3.0, bg);
    p.text(
        pos2(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        current,
        bae_ui::theme::body(),
        bae_ui::theme::color::TEXT_STRONG,
    );
    icons::paint(
        p,
        Rect::from_center_size(pos2(rect.right() - 12.0, rect.center().y), vec2(14.0, 14.0)),
        Icon::ChevronDown,
        bae_ui::theme::color::TEXT_STRONG,
    );
    resp
}

/// One of the panel's icon buttons with its AE tooltip.
fn icon_button(ui: &mut Ui, icon: Icon, tip: &str) -> egui::Response {
    let enabled = ui.is_enabled();
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 24.0), Sense::click());
    use bae_ui::a11y::Accessible as _;
    let resp = resp
        .labeled(egui::WidgetType::Button, tip)
        .on_hover_text(tip);
    let p = ui.painter();
    if enabled && resp.hovered() {
        p.rect_filled(rect, 3.0, bae_ui::theme::color::HOVER);
    }
    let c = if enabled {
        bae_ui::theme::color::ICON
    } else {
        bae_ui::theme::color::TEXT_DISABLED
    };
    icons::paint(
        p,
        Rect::from_center_size(rect.center(), vec2(18.0, 18.0)),
        icon,
        c,
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x0: f64, y0: f64, x1: f64, y1: f64) -> Bounds {
        Bounds {
            min: DVec2::new(x0, y0),
            max: DVec2::new(x1, y1),
        }
    }

    #[test]
    fn aligns_to_the_selection_or_the_comp() {
        let layers = [b(100.0, 100.0, 200.0, 150.0), b(300.0, 50.0, 500.0, 90.0)];
        // Left to the selection: the leftmost edge.
        let d = align_offsets(&layers, Edge::Left, None);
        assert_eq!(d, [DVec2::ZERO, DVec2::new(-200.0, 0.0)]);
        // Vertical centers to the comp's center.
        let d = align_offsets(&layers, Edge::VCenter, Some((1920.0, 1080.0)));
        assert_eq!(d, [DVec2::new(0.0, 415.0), DVec2::new(0.0, 470.0)]);
        // Right to the selection: the rightmost edge.
        let d = align_offsets(&layers, Edge::Right, None);
        assert_eq!(d[0], DVec2::new(300.0, 0.0));
    }

    #[test]
    fn distributes_between_the_outermost_layers() {
        let layers = [
            b(0.0, 0.0, 10.0, 10.0),
            b(100.0, 0.0, 120.0, 10.0),
            b(30.0, 0.0, 40.0, 10.0),
        ];
        let d = distribute_offsets(&layers, Edge::Left);
        // Lefts 0, 30, 100 → 0, 50, 100: the middle one (index 2) moves 20.
        assert_eq!(d, [DVec2::ZERO, DVec2::ZERO, DVec2::new(20.0, 0.0)]);
        assert!(
            distribute_offsets(&layers[..2], Edge::Left)
                .iter()
                .all(|d| *d == DVec2::ZERO)
        );
    }
}
