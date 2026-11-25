//! The Composition Navigator bar along the top of the Composition panel (AE):
//! the active comp and the comps upstream of it in the flow it was opened
//! through (double-clicking a pre-comp layer adds it to the path). Clicking a
//! name opens that comp; the path stays, so the comps further down remain a
//! click away. The flow runs right to left like AE's default: the comp that
//! contains the others is on the left.

use bae_motion::query;
use bae_ui::a11y::Accessible as _;
use bae_ui::theme::{self, color};
use bae_ui::{PanelContext, tr};
use egui::{Align2, Rect, Sense, Ui, vec2};

use crate::session::SharedSession;

/// Height of the bar in points.
pub(super) const HEIGHT: f32 = 20.0;

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
