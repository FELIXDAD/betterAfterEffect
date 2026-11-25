//! Pointer shapes of the tools over the Composition viewer. egui's system cursors
//! cover the selection arrow, hand, zoom, crosshair and I-beam; the pen, rotation,
//! anchor and camera tools draw their own glyph (the system pointer hidden), with
//! the hot spot where AE puts it: the pen nib's tip (where a click lands), the
//! glyph's center for the others. Hot spots are in points inside the 16-point
//! glyph box, so they hold at every display scale.

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
        // The nib points to the lower left (Spectrum VectorDraw; measured on its
        // outline). The vertex tools' pen is smaller, its tip higher.
        Pen => glyph(Icon::Pen, (2.0, 14.8)),
        AddVertex | DeleteVertex | ConvertVertex | MaskFeather => glyph(tool.icon(), (2.0, 11.6)),
        OrbitCursor | OrbitScene | OrbitPoi | PanCursor | PanPoi | DollyCursor | DollyTo
        | DollyPoi => glyph(tool.icon(), (8.0, 8.0)),
        TypeHorizontal => Cursor::System(CursorIcon::Text),
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

/// The small star AE adds to the shape tools' crosshair when the drag will make
/// a shape (new shape layer or new group) rather than a mask.
pub(crate) fn shape_star(painter: &Painter, pos: Pos2) {
    let c = pos + vec2(10.0, 10.0);
    let spokes: Vec<[Pos2; 2]> = (0..3)
        .map(|i| {
            let a = std::f32::consts::PI * (i as f32 / 3.0 + 0.5);
            let d = vec2(a.cos(), a.sin()) * 3.5;
            [c - d, c + d]
        })
        .collect();
    for (width, color) in [(2.5, Color32::BLACK), (1.0, Color32::WHITE)] {
        for s in &spokes {
            painter.line_segment(*s, egui::Stroke::new(width, color));
        }
    }
}

/// The small circle by the Pen when a click closes the path.
pub(crate) fn close_mark(painter: &Painter, pos: Pos2) {
    let c = pos + vec2(9.0, 12.0);
    painter.circle_stroke(c, 3.0, egui::Stroke::new(2.5, Color32::BLACK));
    painter.circle_stroke(c, 3.0, egui::Stroke::new(1.0, Color32::WHITE));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A click lands on the pen nib's tip, at the glyph's lower left.
    #[test]
    fn pen_hot_spots_are_the_nib_tips() {
        for tool in [
            Tool::Pen,
            Tool::AddVertex,
            Tool::DeleteVertex,
            Tool::ConvertVertex,
        ] {
            let Cursor::Glyph { hot, .. } = for_tool(tool, Hover::default()) else {
                panic!("{tool:?} draws a glyph");
            };
            assert!(hot.x < 4.0 && hot.y > 10.0, "{tool:?}: {hot:?}");
        }
        let Cursor::Glyph { hot, .. } = for_tool(Tool::OrbitScene, Hover::default()) else {
            panic!("camera tools draw a glyph");
        };
        assert_eq!(hot, vec2(8.0, 8.0));
    }
}
