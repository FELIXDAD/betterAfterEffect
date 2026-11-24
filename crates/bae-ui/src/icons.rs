//! UI icons.
//!
//! Every icon is fill-path data in `assets/icons/icons.bin`, built from the SVGs in
//! `assets/icons/` (Adobe's open-source Spectrum 1 icons plus BAE's own drawings
//! for AE-specific glyphs; see `assets/icons/README.md`) by
//! `tools/icons/build_icons.py`. Icons are rasterized at the exact pixel size and
//! tinted with the caller's color; see [`vector`].

use egui::epaint::{PathShape, PathStroke};
use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::theme::color;

mod vector;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    // Panel chrome
    Menu,
    Close,
    ChevronRight,
    ChevronDown,
    ChevronLeft,
    ChevronDoubleRight,
    Lock,
    Unlock,
    Search,
    SortUp,
    // Properties and keyframes
    Stopwatch,
    Keyframe,
    KeyframeOutline,
    TriangleLeft,
    TriangleRight,
    Link,
    Eyedropper,
    // Layer switches
    Eye,
    Speaker,
    Solo,
    Tag,
    Shy,
    /// The Shy switch of a shy layer (the figure ducked behind the bar).
    ShyOn,
    Collapse,
    Quality,
    Fx,
    FrameBlend,
    MotionBlur,
    Adjustment,
    Cube,
    Pickwhip,
    // Project items
    Folder,
    Composition,
    Footage,
    Star,
    TextLayer,
    Flowchart,
    InterpretFootage,
    Send,
    Trash,
    /// Save (a floppy disk), e.g. Save Preset in Composition Settings.
    Save,
    // Tools
    Home,
    Selection,
    Hand,
    Zoom,
    Orbit,
    PanCamera,
    Dolly,
    Rotate,
    PanBehind,
    Rectangle,
    Extrude,
    Pen,
    Type,
    Brush,
    CloneStamp,
    Eraser,
    RotoBrush,
    Puppet,
    // Members of tool groups (toolbar flyouts)
    RoundedRectangle,
    Ellipse,
    Polygon,
    StarTool,
    Sphere,
    Cylinder,
    Cone,
    Torus,
    Plane,
    PenAdd,
    PenDelete,
    PenConvert,
    MaskFeather,
    TypeVertical,
    RefineEdge,
    PuppetStarch,
    PuppetBend,
    PuppetAdvanced,
    PuppetOverlap,
    AxisLocal,
    AxisWorld,
    AxisView,
    SnapEdges,
    SnapFeatures,
    PlayCircle,
    AutoOpen,
    // Viewer
    FastPreview,
    TransparencyGrid,
    MaskVisibility,
    RegionOfInterest,
    Guides,
    Channels,
    Exposure,
    Snapshot,
    ShowSnapshot,
    // Timeline
    MiniFlowchart,
    GraphEditor,
    Marker,
    CompButton,
    MountainSmall,
    MountainLarge,
    PaneSwitches,
    PaneModes,
    PaneInOut,
    PaneRenderTime,
    // Transport
    FirstFrame,
    PrevFrame,
    Play,
    Pause,
    NextFrame,
    LastFrame,
}

impl Icon {
    pub const ALL: &[Icon] = &[
        Icon::Menu,
        Icon::Close,
        Icon::ChevronRight,
        Icon::ChevronDown,
        Icon::ChevronLeft,
        Icon::ChevronDoubleRight,
        Icon::Lock,
        Icon::Unlock,
        Icon::Search,
        Icon::SortUp,
        Icon::Stopwatch,
        Icon::Keyframe,
        Icon::KeyframeOutline,
        Icon::TriangleLeft,
        Icon::TriangleRight,
        Icon::Link,
        Icon::Eyedropper,
        Icon::Eye,
        Icon::Speaker,
        Icon::Solo,
        Icon::Tag,
        Icon::Shy,
        Icon::ShyOn,
        Icon::Collapse,
        Icon::Quality,
        Icon::Fx,
        Icon::FrameBlend,
        Icon::MotionBlur,
        Icon::Adjustment,
        Icon::Cube,
        Icon::Pickwhip,
        Icon::Folder,
        Icon::Composition,
        Icon::Footage,
        Icon::Star,
        Icon::TextLayer,
        Icon::Flowchart,
        Icon::InterpretFootage,
        Icon::Send,
        Icon::Trash,
        Icon::Save,
        Icon::Home,
        Icon::Selection,
        Icon::Hand,
        Icon::Zoom,
        Icon::Orbit,
        Icon::PanCamera,
        Icon::Dolly,
        Icon::Rotate,
        Icon::PanBehind,
        Icon::Rectangle,
        Icon::Extrude,
        Icon::Pen,
        Icon::Type,
        Icon::Brush,
        Icon::CloneStamp,
        Icon::Eraser,
        Icon::RotoBrush,
        Icon::Puppet,
        Icon::RoundedRectangle,
        Icon::Ellipse,
        Icon::Polygon,
        Icon::StarTool,
        Icon::Sphere,
        Icon::Cylinder,
        Icon::Cone,
        Icon::Torus,
        Icon::Plane,
        Icon::PenAdd,
        Icon::PenDelete,
        Icon::PenConvert,
        Icon::MaskFeather,
        Icon::TypeVertical,
        Icon::RefineEdge,
        Icon::PuppetStarch,
        Icon::PuppetBend,
        Icon::PuppetAdvanced,
        Icon::PuppetOverlap,
        Icon::AxisLocal,
        Icon::AxisWorld,
        Icon::AxisView,
        Icon::SnapEdges,
        Icon::SnapFeatures,
        Icon::PlayCircle,
        Icon::AutoOpen,
        Icon::FastPreview,
        Icon::TransparencyGrid,
        Icon::MaskVisibility,
        Icon::RegionOfInterest,
        Icon::Guides,
        Icon::Channels,
        Icon::Exposure,
        Icon::Snapshot,
        Icon::ShowSnapshot,
        Icon::MiniFlowchart,
        Icon::GraphEditor,
        Icon::Marker,
        Icon::CompButton,
        Icon::MountainSmall,
        Icon::MountainLarge,
        Icon::PaneSwitches,
        Icon::PaneModes,
        Icon::PaneInOut,
        Icon::PaneRenderTime,
        Icon::FirstFrame,
        Icon::PrevFrame,
        Icon::Play,
        Icon::Pause,
        Icon::NextFrame,
        Icon::LastFrame,
    ];
}

/// Paint `icon` centered in `rect`, scaled to fit.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    vector::paint(painter, rect, icon, color);
}

/// Paint an arbitrary path (used for custom glyphs outside the icon set).
pub fn path(painter: &Painter, points: Vec<Pos2>, closed: bool, stroke: Stroke) {
    let shape = if closed {
        PathShape::closed_line(points, PathStroke::from(stroke))
    } else {
        PathShape::line(points, PathStroke::from(stroke))
    };
    painter.add(shape);
}

/// Icon-only button: draws a hover/selected background and the icon.
///
/// `size` is the clickable square in points; the glyph is drawn at 16pt
/// or smaller to fit.
pub fn button(ui: &mut Ui, icon: Icon, size: f32, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let bg = if selected {
            Some(color::TOOL_SELECTED)
        } else if response.is_pointer_button_down_on() {
            Some(color::BUTTON_ON)
        } else if response.hovered() {
            Some(color::HOVER)
        } else {
            None
        };
        if let Some(bg) = bg {
            p.rect_filled(rect, 3.0, bg);
        }
        let fg = if selected {
            Color32::WHITE
        } else if response.hovered() {
            color::TEXT_STRONG
        } else {
            color::ICON
        };
        let glyph = size.min(16.0);
        paint(
            p,
            Rect::from_center_size(rect.center(), Vec2::splat(glyph)),
            icon,
            fg,
        );
    }
    response
}

/// Icon sized and colored for inline use (e.g. in a list row).
pub fn show(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        paint(ui.painter(), rect, icon, color);
    }
    response
}

/// Small solid label swatch (layer and project item labels).
pub fn swatch(painter: &Painter, center: Pos2, side: f32, fill: Color32) {
    painter.rect_filled(Rect::from_center_size(center, Vec2::splat(side)), 0.0, fill);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Shape;

    /// Every icon has fill-path data and draws something.
    #[test]
    fn every_icon_has_a_picture() {
        let ctx = crate::testing::ctx_with_fonts();
        for icon in Icon::ALL {
            let out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let rect = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::splat(16.0));
                paint(ui.painter(), rect, *icon, color::ICON);
            });
            let drawn = out
                .shapes
                .iter()
                .filter(|s| !matches!(s.shape, Shape::Noop))
                .count();
            out.drop_without_applying_deltas();
            assert!(drawn >= 1, "{icon:?} draws nothing");
        }
    }

    #[test]
    fn all_lists_every_variant_in_declaration_order() {
        for (i, icon) in Icon::ALL.iter().enumerate() {
            assert_eq!(*icon as usize, i, "{icon:?} is out of order in Icon::ALL");
        }
    }
}
