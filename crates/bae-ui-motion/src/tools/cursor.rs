//! Pointer shapes of the tools over the Composition viewer. egui's system cursors
//! cover the selection arrow, hand, zoom, crosshair and I-beam; the pen, rotation,
//! anchor and camera tools draw their own glyph (the system pointer hidden), with
//! the hot spot where AE puts it (the pen nib's tip, the glyph's center).

use bae_ui::icons::{self, Icon};
use egui::{Color32, CursorIcon, Painter, Pos2, Rect, Ui, Vec2, vec2};

use super::Tool;

/// How the pointer looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Cursor {
    System(CursorIcon),
    /// A drawn glyph; `hot` is the hot spot inside the 16×16 glyph box.
    Glyph {
        icon: Icon,
        hot: Vec2,
    },
}

/// What happens next with the pointer, for the cursor of tools whose look depends
/// on it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Hover {
    /// Alt/Option is held (Zoom tool zooms out).
    pub alt: bool,
    /// A drag is in progress (closed hand).
    pub dragging: bool,
}

/// The cursor of `tool` over the viewer.
pub(crate) fn for_tool(tool: Tool, hover: Hover) -> Cursor {
    use Tool::*;
    let glyph = |icon: Icon, hot: (f32, f32)| Cursor::Glyph {
        icon,
        hot: vec2(hot.0, hot.1),
    };
    match tool {
        Selection => Cursor::System(CursorIcon::Default),
        Hand if hover.dragging => Cursor::System(CursorIcon::Grabbing),
        Hand => Cursor::System(CursorIcon::Grab),
        Zoom if hover.alt => Cursor::System(CursorIcon::ZoomOut),
        Zoom => Cursor::System(CursorIcon::ZoomIn),
        Rotation => glyph(Icon::Rotate, (8.0, 8.0)),
        PanBehind => glyph(Icon::PanBehind, (8.0, 8.0)),
        Rectangle | RoundedRectangle | Ellipse | Polygon | Star => {
            Cursor::System(CursorIcon::Crosshair)
        }
        Pen => glyph(Icon::Pen, (8.0, 1.5)),
        AddVertex | DeleteVertex | ConvertVertex | MaskFeather => glyph(tool.icon(), (5.5, 2.0)),
        TypeHorizontal => Cursor::System(CursorIcon::Text),
        TypeVertical => Cursor::System(CursorIcon::VerticalText),
        // Not available in the Composition viewer in this version.
        _ => Cursor::System(CursorIcon::NotAllowed),
    }
}

/// Show `cursor` at `pos` (call while the pointer is over the viewer).
pub(crate) fn show(ui: &Ui, painter: &Painter, pos: Pos2, cursor: Cursor) {
    match cursor {
        Cursor::System(icon) => ui.ctx().set_cursor_icon(icon),
        Cursor::Glyph { icon, hot } => {
            ui.ctx().set_cursor_icon(CursorIcon::None);
            let rect = Rect::from_min_size(pos - hot, Vec2::splat(16.0));
            // A dark outline keeps the white glyph visible on light pixels.
            for d in [
                vec2(-1.0, 0.0),
                vec2(1.0, 0.0),
                vec2(0.0, -1.0),
                vec2(0.0, 1.0),
            ] {
                icons::paint(painter, rect.translate(d), icon, Color32::BLACK);
            }
            icons::paint(painter, rect, icon, Color32::WHITE);
        }
    }
}
