//! Drag-and-drop feedback shared by every panel (AE): what is being dragged
//! follows the pointer (its icon and name, or how many items); a target that
//! takes it marks where it lands (each target draws its own mark: the
//! Timeline's insertion line, the viewer's placement box, a highlighted
//! folder or layer) and the pointer shows Copy; anywhere else the pointer
//! shows Not Allowed and releasing does nothing. Esc cancels (egui's
//! DragAndDrop drops the payload).

use std::any::Any;

use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use egui::{Context, CursorIcon, Id, LayerId, Order, Rect, Response, Stroke, Ui, vec2};

/// What the pointer carries while dragging.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ghost {
    pub icon: Icon,
    /// The item's name, or "N items".
    pub label: String,
}

fn ghost_id() -> Id {
    Id::new("bae-ui-motion.drag-ghost")
}

fn accepted_id() -> Id {
    Id::new("bae-ui-motion.drag-accepted")
}

/// Starts dragging from `resp`: `payload` for the targets, `ghost` for the
/// pointer (both built only when the drag starts).
pub(crate) fn source<P: Any + Send + Sync>(
    resp: &Response,
    ghost: impl FnOnce() -> Ghost,
    payload: impl FnOnce() -> P,
) {
    if resp.drag_started() {
        resp.ctx.add_plugin(Feedback);
        egui::DragAndDrop::set_payload(&resp.ctx, payload());
        let ghost = ghost();
        resp.ctx.data_mut(|d| d.insert_temp(ghost_id(), ghost));
    }
}

/// A target under the pointer takes what is dragged (this frame).
pub(crate) fn accept(ctx: &Context) {
    ctx.data_mut(|d| d.insert_temp(accepted_id(), true));
}

/// Installs the feedback (sources do it when a drag starts; for tests).
#[cfg(test)]
pub(crate) fn install(ctx: &Context) {
    ctx.add_plugin(Feedback);
}

/// Whether a target took the drag this frame (for tests).
#[cfg(test)]
pub(crate) fn accepted(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(accepted_id())) == Some(true)
}

/// "N items" in the UI language.
pub(crate) fn items_label(n: usize) -> String {
    bae_ui::tr("{n} items").replace("{n}", &n.to_string())
}

/// Draws the ghost and sets the pointer after every panel has drawn.
struct Feedback;

impl egui::Plugin for Feedback {
    fn debug_name(&self) -> &'static str {
        "BAE drag feedback"
    }

    fn on_begin_pass(&mut self, ui: &mut Ui) {
        ui.ctx().data_mut(|d| d.remove::<bool>(accepted_id()));
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let Some(ghost) = ctx.data(|d| d.get_temp::<Ghost>(ghost_id())) else {
            return;
        };
        if !egui::DragAndDrop::has_any_payload(&ctx) {
            ctx.data_mut(|d| d.remove::<Ghost>(ghost_id()));
            return;
        }
        let accepted = ctx.data(|d| d.get_temp::<bool>(accepted_id())) == Some(true);
        ui.output_mut(|o| {
            o.cursor_icon = if accepted {
                CursorIcon::Copy
            } else {
                CursorIcon::NotAllowed
            };
        });
        let Some(pos) = ctx.pointer_latest_pos() else {
            return;
        };
        let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, ghost_id()));
        let galley = painter.layout_no_wrap(ghost.label.clone(), theme::body(), color::TEXT);
        let icon = 16.0;
        let size = vec2(icon + 6.0 + galley.size().x, icon.max(galley.size().y)) + vec2(12.0, 6.0);
        let rect = Rect::from_min_size(pos + vec2(14.0, 12.0), size);
        painter.rect_filled(rect, 3.0, color::BUTTON_ON.gamma_multiply(0.9));
        painter.rect_stroke(
            rect,
            3.0,
            Stroke::new(1.0, color::ACCENT),
            egui::StrokeKind::Inside,
        );
        let icon_rect = Rect::from_min_size(
            rect.min + vec2(6.0, (rect.height() - icon) / 2.0),
            vec2(icon, icon),
        );
        icons::paint(&painter, icon_rect, ghost.icon, color::TEXT);
        painter.galley(
            egui::pos2(
                icon_rect.right() + 6.0,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            color::TEXT,
        );
    }
}

/// Chinese strings.
pub(crate) const ZH: &[(&str, &str)] = &[("{n} items", "{n} 个项目")];
