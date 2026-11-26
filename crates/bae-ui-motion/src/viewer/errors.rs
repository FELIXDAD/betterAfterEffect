//! AE's expression error banner along the bottom of the Composition panel:
//! "This project contains expression errors: error 1 of 2", the message, arrows
//! through the errors, expand and close.

use bae_anim::AttrError;
use bae_model::Stage;
use bae_motion::query;
use bae_ui::Accessible as _;
use bae_ui::icons::{self, Icon};
use bae_ui::theme;
use bae_ui::{PanelContext, tr};
use egui::{Color32, Rect, Sense, Ui, pos2, vec2};

use crate::session::SharedSession;

const ROW: f32 = 24.0;
/// The banner and the timeline's warning icons.
pub(crate) const YELLOW: Color32 = Color32::from_rgb(0xe8, 0xb6, 0x2c);
const INK: Color32 = Color32::from_rgb(0x1d, 0x1d, 0x1d);

#[derive(Default)]
pub(super) struct Banner {
    index: usize,
    expanded: bool,
    /// Closed for these errors: shown again when they change.
    closed: Option<Vec<AttrError>>,
}

/// AE's sentence for an error: property, layer and comp, then the message.
pub(crate) fn describe(stage: &Stage, e: &AttrError) -> String {
    let layer = query::owning_layer(stage, &e.prim);
    let comp = layer.as_ref().and_then(|l| query::owning_comp(stage, l));
    let index = match (&layer, &comp) {
        (Some(l), Some(c)) => query::comp_layers(stage, c)
            .iter()
            .position(|x| x == l)
            .map_or(0, |i| i + 1),
        _ => 0,
    };
    let three_d = layer
        .as_ref()
        .is_some_and(|l| crate::common::is_3d(stage, l));
    let name = |p: Option<&bae_model::PrimPath>| {
        p.map(|p| query::display_name_of(stage, p))
            .unwrap_or_default()
    };
    let head = tr("Error in property '{prop}' of layer {index} ('{layer}') in comp '{comp}'.")
        .replace(
            "{prop}",
            tr(&crate::common::attr_label(None, &e.attr, three_d)),
        )
        .replace("{index}", &index.to_string())
        .replace("{layer}", &name(layer.as_ref()))
        .replace("{comp}", &name(comp.as_ref()));
    format!("{head}\n{}", e.message)
}

impl Banner {
    /// Draw the banner at the bottom of `area` when the comp has errors.
    pub(super) fn ui(
        &mut self,
        ui: &mut Ui,
        cx: &PanelContext<'_>,
        session: &SharedSession,
        area: Rect,
    ) {
        let errors = session.lock().expr_errors.clone();
        if errors.is_empty()
            || self.closed.as_ref() == Some(&errors)
            || !crate::prefs::current(cx.resources)
                .bool("scripting_expressions.show_expression_error_warning_banner")
        {
            return;
        }
        self.closed = None;
        self.index = self.index.min(errors.len() - 1);
        let text = describe(cx.stage, &errors[self.index]);
        let lines = if self.expanded {
            1 + text.lines().count()
        } else {
            1
        };
        let rect = Rect::from_min_max(
            pos2(area.left(), area.bottom() - ROW * lines as f32),
            area.max,
        );
        ui.painter().rect_filled(rect, 0.0, YELLOW);
        let cy = rect.top() + ROW / 2.0;
        let mut x = rect.left() + 6.0;
        icons::paint(
            ui.painter(),
            Rect::from_center_size(pos2(x + 8.0, cy), vec2(16.0, 16.0)),
            Icon::Alert,
            INK,
        );
        x += 22.0;
        let button = |ui: &mut Ui, x: f32, icon: Icon, label: &str, id: &str| {
            let r = Rect::from_center_size(pos2(x, cy), vec2(18.0, 18.0));
            let resp = ui
                .interact(r, ui.id().with(("expr_banner", id)), Sense::click())
                .labeled(egui::WidgetType::Button, tr(label));
            icons::paint(ui.painter(), r.shrink(2.0), icon, INK);
            resp.on_hover_text(tr(label)).clicked()
        };
        if errors.len() > 1 {
            if button(ui, x + 9.0, Icon::ChevronLeft, "Previous Error", "prev") {
                self.index = (self.index + errors.len() - 1) % errors.len();
            }
            if button(ui, x + 29.0, Icon::ChevronRight, "Next Error", "next") {
                self.index = (self.index + 1) % errors.len();
            }
            x += 42.0;
        }
        let count = tr("This project contains expression errors: error {n} of {count}")
            .replace("{n}", &(self.index + 1).to_string())
            .replace("{count}", &errors.len().to_string());
        let right = rect.right() - 50.0;
        let galley = ui.painter().layout(
            if self.expanded {
                format!("{count}\n{text}")
            } else {
                format!("{count}  {}", text.replace('\n', "  "))
            },
            theme::body(),
            INK,
            (right - x).max(0.0),
        );
        let clip = Rect::from_min_max(pos2(x, rect.top()), pos2(right, rect.bottom()));
        ui.painter()
            .with_clip_rect(clip)
            .galley(pos2(x, rect.top() + 4.0), galley, INK);
        ui.interact(clip, ui.id().with("expr_banner_text"), Sense::hover())
            .labeled(egui::WidgetType::Label, &format!("{count} {text}"));
        let expand = if self.expanded {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        if button(ui, right + 14.0, expand, "Expand or Collapse", "expand") {
            self.expanded = !self.expanded;
        }
        if button(ui, right + 36.0, Icon::Close, "Close", "close") {
            self.closed = Some(errors);
        }
    }
}
