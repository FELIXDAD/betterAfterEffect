//! The Composition Navigator bar along the top of the Composition panel (AE):
//! the active comp and the comps upstream of it in the flow it was opened
//! through (double-clicking a pre-comp layer adds it to the path). Clicking a
//! name opens that comp; the path stays, so the comps further down remain a
//! click away. The flow runs right to left like AE's default: the comp that
//! contains the others is on the left.
//!
//! Composition ▸ Composition Mini-Flowchart (Tab in the Composition panel, or
//! an arrow of the bar) pops up the comps around the active one: the comps
//! that use it on the left, the pre-comps in it on the right. Clicking one
//! opens it.

use bae_model::{PrimPath, Stage, Value};
use bae_motion::query;
use bae_ui::a11y::Accessible as _;
use bae_ui::theme::{self, color};
use bae_ui::{Invocation, PanelContext, tr};
use egui::{Align2, Rect, Sense, Ui, vec2};

use crate::session::SharedSession;

/// Height of the bar in points.
pub(super) const HEIGHT: f32 = 20.0;

/// Composition ▸ Composition Mini-Flowchart (Tab).
pub(crate) const MINI_FLOWCHART: &str = "motion.comp.composition_mini_flowchart";

/// `(action id, English title)` for the action registry.
pub(crate) const ACTIONS: &[(&str, &str)] = &[(MINI_FLOWCHART, "Composition Mini-Flowchart")];

/// Opens or closes the Mini-Flowchart of the active comp.
pub(crate) fn handle(inv: &Invocation, cx: &PanelContext<'_>) -> bool {
    if inv.id != MINI_FLOWCHART {
        return false;
    }
    let session = crate::common::session(cx.resources);
    let mut s = session.lock();
    s.mini_flowchart = s.active_comp.is_some() && !s.mini_flowchart;
    true
}

/// The comps whose layers show `comp` (upstream) and the comps its own
/// layers show (downstream), each once, in project / layer order.
fn neighbours(stage: &Stage, comp: &PrimPath) -> (Vec<PrimPath>, Vec<PrimPath>) {
    let source = |layer: &PrimPath| match stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, "source"))
    {
        Some(Value::PrimRef(p)) if query::comp_info(stage, &p).is_some() => Some(p),
        _ => None,
    };
    let up = query::comps(stage)
        .into_iter()
        .filter(|c| c != comp)
        .filter(|c| {
            query::comp_layers(stage, c)
                .iter()
                .any(|l| source(l).as_ref() == Some(comp))
        })
        .collect();
    let mut down: Vec<PrimPath> = Vec::new();
    for l in query::comp_layers(stage, comp) {
        if let Some(p) = source(&l)
            && !down.contains(&p)
        {
            down.push(p);
        }
    }
    (up, down)
}

/// Draws the open Mini-Flowchart under the bar (`below`: the bar's rect).
pub(super) fn mini_flowchart(ui: &Ui, cx: &PanelContext<'_>, session: &SharedSession, below: Rect) {
    let (open, active) = {
        let s = session.lock();
        (s.mini_flowchart, s.active_comp.clone())
    };
    let Some(active) = active.filter(|_| open) else {
        return;
    };
    let (up, down) = neighbours(cx.stage, &active);
    let name = |c: &PrimPath| query::display_name_of(cx.stage, c);
    let mut picked: Option<(PrimPath, bool)> = None;
    let area = egui::Area::new(ui.id().with("mini-flowchart"))
        .order(egui::Order::Foreground)
        .fixed_pos(below.left_bottom() + vec2(8.0, 2.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let column =
                        |ui: &mut Ui,
                         comps: &[PrimPath],
                         upstream: bool,
                         picked: &mut Option<(PrimPath, bool)>| {
                            ui.vertical(|ui| {
                                for c in comps {
                                    if ui.button(name(c)).clicked() {
                                        *picked = Some((c.clone(), upstream));
                                    }
                                }
                            });
                        };
                    if !up.is_empty() {
                        column(ui, &up, true, &mut picked);
                        ui.label(egui::RichText::new("◂").color(color::TEXT_DIM));
                    }
                    ui.label(
                        egui::RichText::new(name(&active))
                            .strong()
                            .color(color::TEXT_STRONG),
                    );
                    if !down.is_empty() {
                        ui.label(egui::RichText::new("◂").color(color::TEXT_DIM));
                        column(ui, &down, false, &mut picked);
                    }
                });
            });
        });
    let outside = ui.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !area.response.rect.contains(p))
    });
    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let mut s = session.lock();
    if let Some((comp, upstream)) = picked {
        // Upstream: the comp that uses this one, with this one after it on
        // the path; downstream: one step further along the path.
        if upstream {
            s.comp_flow = vec![comp.clone(), active.clone()];
            s.open_comp(&comp);
        } else {
            s.open_from(&active, &comp);
        }
        s.mini_flowchart = false;
    } else if outside || esc {
        s.mini_flowchart = false;
    }
}

/// Draws the bar in `rect`.
pub(super) fn bar(ui: &Ui, cx: &PanelContext<'_>, session: &SharedSession, rect: Rect) {
    let (flow, active) = {
        let s = session.lock();
        (s.comp_flow.clone(), s.active_comp.clone())
    };
    // A comp made active some other way (a closed tab) stands alone.
    let flow = match &active {
        Some(a) if !flow.contains(a) => vec![a.clone()],
        _ => flow,
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, color::PANEL);
    let font = theme::small();
    let mut x = rect.left() + 8.0;
    let y = rect.center().y;
    for (i, comp) in flow.iter().enumerate() {
        if query::comp_info(cx.stage, comp).is_none() {
            continue;
        }
        if i > 0 {
            // The flow arrow: this comp feeds the one to its left.
            let arrow = painter.text(
                egui::pos2(x, y),
                Align2::LEFT_CENTER,
                "◂",
                font.clone(),
                color::TEXT_DIM,
            );
            let hit = arrow.expand2(vec2(3.0, 2.0));
            if ui
                .interact(hit, ui.id().with(("comp-nav-arrow", i)), Sense::click())
                .labeled(egui::WidgetType::Button, tr("Composition Mini-Flowchart"))
                .clicked()
            {
                session.lock().mini_flowchart = true;
            }
            x = arrow.right() + 6.0;
        }
        let name = query::display_name_of(cx.stage, comp);
        let current = active.as_ref() == Some(comp);
        let galley = painter.layout_no_wrap(name.clone(), font.clone(), color::TEXT);
        let r = Rect::from_min_size(
            egui::pos2(x - 3.0, rect.top() + 2.0),
            vec2(galley.size().x + 6.0, rect.height() - 4.0),
        );
        let resp = ui
            .interact(r, ui.id().with(("comp-nav", i)), Sense::click())
            .labeled(egui::WidgetType::Button, &name)
            .on_hover_text(tr(
                "Click a name to open the composition; click an arrow to open the Mini-Flowchart (Tab)",
            ));
        let text = if current {
            color::TEXT_STRONG
        } else if resp.hovered() {
            color::TEXT
        } else {
            color::TEXT_DIM
        };
        if current {
            painter.rect_filled(r, 2.0, color::FIELD_HOVER);
        }
        painter.text(
            egui::pos2(x, y),
            Align2::LEFT_CENTER,
            name,
            font.clone(),
            text,
        );
        if resp.clicked() && !current {
            session.lock().open_comp(comp);
        }
        x = r.right() + 6.0;
    }
}

/// Chinese (AE's zh_CN dictionary, Breadcrumb).
pub(crate) const ZH: &[(&str, &str)] = &[(
    "Click a name to open the composition; click an arrow to open the Mini-Flowchart (Tab)",
    "单击名称可打开合成；单击箭头可打开微型流程图（Tab 键）",
)];
