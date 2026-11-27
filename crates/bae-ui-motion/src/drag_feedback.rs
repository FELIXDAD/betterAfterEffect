//! Drag-and-drop feedback shared by every panel, as AE draws it: the dragged
//! row's outline (its icon square and name bar, one per item) follows the
//! pointer where it was grabbed; over a target that takes it the pointer is
//! the copy pointer (with AE's dotted box and plus beside the arrow where the
//! system has no such pointer) and the target marks where it lands (the
//! Timeline's insertion line and time, the viewer's placement box, a lit
//! folder or layer); anywhere else the pointer is Not Allowed and releasing
//! does nothing. Esc cancels (egui's DragAndDrop drops the payload).

use std::any::Any;

use bae_ui::theme::color;
use egui::{
    Color32, Context, CursorIcon, Id, LayerId, Order, Painter, Pos2, Rect, Response, Shape, Stroke,
    Ui, pos2, vec2,
};

/// The outline that follows the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ghost {
    /// The dragged row's icon square and name bar where they were drawn.
    pub icon: Rect,
    pub name: Rect,
    /// Where the press was.
    pub grab: Pos2,
    /// Items dragged (one outline each, up to four).
    pub count: usize,
}

impl Ghost {
    /// For a widget without its own icon: the name bar is the widget, the
    /// icon square sits to its left.
    pub(crate) fn of(resp: &Response, count: usize) -> Ghost {
        let r = resp.rect;
        let s = r.height();
        Ghost::row(
            Rect::from_min_size(r.min - vec2(s + 4.0, 0.0), vec2(s, s)),
            r,
            resp,
            count,
        )
    }

    /// A row with its icon and name boxes.
    pub(crate) fn row(icon: Rect, name: Rect, resp: &Response, count: usize) -> Ghost {
        let grab = resp
            .ctx
            .input(|i| i.pointer.press_origin())
            .unwrap_or_else(|| name.left_center());
        Ghost {
            icon,
            name,
            grab,
            count: count.max(1),
        }
    }
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

/// Draws the outline and sets the pointer after every panel has drawn.
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
        paint_outline(&painter, &ghost, pos);
        // macOS has a copy pointer; elsewhere the system arrow gets AE's
        // dotted box and plus.
        if accepted && !cfg!(target_os = "macos") {
            paint_copy_badge(&painter, pos);
        }
    }
}

fn paint_outline(painter: &Painter, ghost: &Ghost, pos: Pos2) {
    let stroke = Stroke::new(1.0, color::TEXT);
    let step = ghost.name.height() + 2.0;
    for k in 0..ghost.count.min(4) {
        let d = pos - ghost.grab + vec2(0.0, k as f32 * step);
        for r in [ghost.icon, ghost.name] {
            painter.rect_stroke(r.translate(d), 0.0, stroke, egui::StrokeKind::Inside);
        }
    }
}

/// The Windows copy pointer's badge: a dotted box with a plus at its corner.
fn paint_copy_badge(painter: &Painter, pos: Pos2) {
    let b = Rect::from_min_size(pos + vec2(6.0, 16.0), vec2(11.0, 11.0));
    let edge = [
        b.left_top(),
        b.right_top(),
        b.right_bottom(),
        b.left_bottom(),
        b.left_top(),
    ];
    painter.extend(Shape::dashed_line(
        &edge,
        Stroke::new(2.0, Color32::BLACK),
        2.0,
        2.0,
    ));
    painter.extend(Shape::dashed_line(
        &edge,
        Stroke::new(1.0, Color32::WHITE),
        2.0,
        2.0,
    ));
    let plus = Rect::from_min_size(b.right_bottom() - vec2(4.0, 4.0), vec2(9.0, 9.0));
    painter.rect_filled(plus, 0.0, Color32::WHITE);
    painter.rect_stroke(
        plus,
        0.0,
        Stroke::new(1.0, Color32::BLACK),
        egui::StrokeKind::Inside,
    );
    let m = plus.center();
    let s = Stroke::new(1.0, Color32::BLACK);
    painter.line_segment([pos2(m.x - 2.5, m.y), pos2(m.x + 2.5, m.y)], s);
    painter.line_segment([pos2(m.x, m.y - 2.5), pos2(m.x, m.y + 2.5)], s);
}
