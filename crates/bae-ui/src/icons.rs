//! UI icons.
//!
//! Icons with fill-path data in `assets/icons/icons.bin` (built from the SVGs in
//! `assets/icons/` by `tools/icons/build_icons.py`) are rasterized at the exact
//! pixel size and tinted with the caller's color; see [`vector`]. The rest are
//! painted with the egui `Painter` on a 16×16 design grid scaled to the target rect.

use egui::epaint::{CubicBezierShape, PathShape, PathStroke};
use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, vec2};

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

/// Maps the 16×16 design grid into a screen rect.
struct Pen<'a> {
    painter: &'a Painter,
    origin: Pos2,
    scale: f32,
    color: Color32,
    /// Stroke width in grid units.
    width: f32,
}

impl Pen<'_> {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.origin + vec2(x, y) * self.scale
    }

    fn pts(&self, pts: &[(f32, f32)]) -> Vec<Pos2> {
        pts.iter().map(|&(x, y)| self.p(x, y)).collect()
    }

    fn stroke(&self) -> Stroke {
        Stroke::new(self.width * self.scale, self.color)
    }

    fn line(&self, pts: &[(f32, f32)]) {
        self.painter.add(Shape::line(self.pts(pts), self.stroke()));
    }

    fn closed(&self, pts: &[(f32, f32)]) {
        self.painter
            .add(Shape::closed_line(self.pts(pts), self.stroke()));
    }

    /// Filled convex polygon.
    fn fill(&self, pts: &[(f32, f32)]) {
        self.painter.add(Shape::convex_polygon(
            self.pts(pts),
            self.color,
            Stroke::NONE,
        ));
    }

    fn circle(&self, x: f32, y: f32, r: f32) {
        self.painter
            .circle_stroke(self.p(x, y), r * self.scale, self.stroke());
    }

    fn disc(&self, x: f32, y: f32, r: f32) {
        self.painter
            .circle_filled(self.p(x, y), r * self.scale, self.color);
    }

    fn disc_with(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.painter.circle_filled(self.p(x, y), r * self.scale, c);
    }

    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, radius: f32) {
        self.painter.rect_stroke(
            Rect::from_min_max(self.p(x0, y0), self.p(x1, y1)),
            radius * self.scale,
            self.stroke(),
            egui::StrokeKind::Middle,
        );
    }

    fn rect_fill(&self, x0: f32, y0: f32, x1: f32, y1: f32, radius: f32) {
        self.rect_fill_with(x0, y0, x1, y1, radius, self.color);
    }

    fn rect_fill_with(&self, x0: f32, y0: f32, x1: f32, y1: f32, radius: f32, c: Color32) {
        self.painter.rect_filled(
            Rect::from_min_max(self.p(x0, y0), self.p(x1, y1)),
            radius * self.scale,
            c,
        );
    }

    /// Arc around (cx, cy); angles in degrees, 0 = +x, clockwise on screen.
    fn arc(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) {
        let n = (((a1 - a0).abs() / 10.0).ceil() as usize).max(2);
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
                self.p(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        self.painter.add(Shape::line(pts, self.stroke()));
    }

    fn ellipse(&self, cx: f32, cy: f32, rx: f32, ry: f32) {
        let pts: Vec<Pos2> = (0..36)
            .map(|i| {
                let a = (i as f32 * 10.0).to_radians();
                self.p(cx + rx * a.cos(), cy + ry * a.sin())
            })
            .collect();
        self.painter.add(Shape::closed_line(pts, self.stroke()));
    }

    fn cubic(&self, p: [(f32, f32); 4]) {
        let pts = [
            self.p(p[0].0, p[0].1),
            self.p(p[1].0, p[1].1),
            self.p(p[2].0, p[2].1),
            self.p(p[3].0, p[3].1),
        ];
        self.painter.add(CubicBezierShape::from_points_stroke(
            pts,
            false,
            Color32::TRANSPARENT,
            self.stroke(),
        ));
    }

    /// Small arrow head at `tip` pointing along (dx, dy).
    fn arrow_head(&self, tip: (f32, f32), dir: (f32, f32), size: f32) {
        let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-3);
        let (ux, uy) = (dir.0 / len, dir.1 / len);
        let (px, py) = (-uy, ux);
        let b = (tip.0 - ux * size, tip.1 - uy * size);
        self.fill(&[
            tip,
            (b.0 + px * size * 0.6, b.1 + py * size * 0.6),
            (b.0 - px * size * 0.6, b.1 - py * size * 0.6),
        ]);
    }

    fn with_color(&self, c: Color32) -> Pen<'_> {
        Pen {
            painter: self.painter,
            origin: self.origin,
            scale: self.scale,
            color: c,
            width: self.width,
        }
    }
}

/// Paint `icon` centered in `rect`, scaled to fit.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    if vector::paint(painter, rect, icon, color) {
        return;
    }
    let side = rect.width().min(rect.height());
    let scale = side / 16.0;
    let origin = rect.center() - Vec2::splat(side / 2.0);
    let pen = Pen {
        painter,
        origin,
        scale,
        color,
        width: 1.0,
    };
    draw(&pen, icon);
}

fn draw(g: &Pen<'_>, icon: Icon) {
    let hole = color::PANEL;
    match icon {
        Icon::Menu => {
            for y in [4.5, 8.0, 11.5] {
                g.line(&[(4.25, y), (11.75, y)]);
            }
        }
        Icon::Close => {
            g.line(&[(6.0, 6.0), (10.0, 10.0)]);
            g.line(&[(10.0, 6.0), (6.0, 10.0)]);
        }
        Icon::ChevronRight => g.line(&[(6.5, 4.5), (10.0, 8.0), (6.5, 11.5)]),
        Icon::ChevronLeft => g.line(&[(9.5, 4.5), (6.0, 8.0), (9.5, 11.5)]),
        Icon::ChevronDown => g.line(&[(4.5, 6.0), (8.0, 9.5), (11.5, 6.0)]),
        Icon::ChevronDoubleRight => {
            let g = Pen { width: 1.4, ..*g };
            g.line(&[(3.5, 4.0), (7.5, 8.0), (3.5, 12.0)]);
            g.line(&[(8.5, 4.0), (12.5, 8.0), (8.5, 12.0)]);
        }
        Icon::Lock | Icon::Unlock => {
            g.rect_fill(4.0, 7.5, 12.0, 13.5, 1.0);
            let x1 = if icon == Icon::Lock { 10.5 } else { 13.0 };
            let x0 = x1 - 5.0;
            g.line(&[(x0, 7.5), (x0, 5.5)]);
            g.arc((x0 + x1) / 2.0, 5.5, 2.5, 180.0, 360.0);
            if icon == Icon::Lock {
                g.line(&[(x1, 5.5), (x1, 7.5)]);
            }
            g.disc_with(8.0, 10.3, 1.0, hole);
            g.rect_fill_with(7.6, 10.3, 8.4, 12.0, 0.0, hole);
        }
        Icon::Search => {
            g.circle(6.5, 6.5, 3.5);
            let g = Pen { width: 1.5, ..*g };
            g.line(&[(9.0, 9.0), (12.5, 12.5)]);
        }
        Icon::Stopwatch => {
            g.circle(8.0, 9.0, 4.8);
            g.line(&[(6.5, 2.8), (9.5, 2.8)]);
            g.line(&[(8.0, 2.8), (8.0, 4.2)]);
            g.line(&[(4.4, 4.6), (3.4, 3.6)]);
        }
        Icon::Link => {
            let g = Pen { width: 1.3, ..*g };
            g.rect(1.5, 5.8, 9.0, 10.2, 2.2);
            g.rect(7.0, 5.8, 14.5, 10.2, 2.2);
        }
        Icon::Eyedropper => {
            let g = Pen { width: 1.2, ..*g };
            g.line(&[(3.0, 13.0), (4.0, 12.0), (9.5, 6.5)]);
            g.line(&[(4.0, 12.0), (3.5, 10.5), (8.0, 6.0)]);
            g.fill(&[(9.0, 4.0), (11.0, 2.5), (13.5, 5.0), (12.0, 7.0)]);
            g.line(&[(7.5, 5.0), (11.0, 8.5)]);
        }
        Icon::Eye => {
            g.cubic([(1.5, 8.0), (4.0, 3.5), (12.0, 3.5), (14.5, 8.0)]);
            g.cubic([(1.5, 8.0), (4.0, 12.5), (12.0, 12.5), (14.5, 8.0)]);
            g.disc(8.0, 8.0, 2.4);
        }
        Icon::Speaker => {
            g.rect_fill(2.5, 6.0, 5.5, 10.0, 0.0);
            g.fill(&[(5.0, 6.0), (9.0, 3.0), (9.0, 13.0), (5.0, 10.0)]);
            g.arc(9.5, 8.0, 2.5, -50.0, 50.0);
            g.arc(9.5, 8.0, 4.8, -50.0, 50.0);
        }
        Icon::Tag => {
            g.fill(&[(2.0, 2.5), (8.5, 2.5), (14.0, 8.0), (8.0, 14.0), (2.0, 8.5)]);
            g.disc_with(5.2, 5.7, 1.3, hole);
        }
        Icon::Fx => {
            let g = Pen { width: 1.2, ..*g };
            g.cubic([(9.5, 2.5), (7.0, 1.8), (6.6, 4.0), (6.0, 8.0)]);
            g.cubic([(6.0, 8.0), (5.4, 12.0), (5.0, 14.0), (2.5, 13.5)]);
            g.line(&[(4.0, 6.0), (8.5, 6.0)]);
            g.line(&[(8.5, 7.0), (13.5, 13.0)]);
            g.line(&[(13.5, 7.0), (8.5, 13.0)]);
        }
        Icon::Folder => {
            g.fill(&[(1.5, 3.5), (6.0, 3.5), (7.5, 5.0), (1.5, 5.0)]);
            g.rect_fill(1.5, 5.0, 14.5, 13.0, 0.5);
            g.line(&[(1.5, 6.2), (14.5, 6.2)]);
        }
        Icon::Footage => {
            g.rect(2.0, 3.0, 14.0, 13.0, 0.5);
            g.line(&[(4.5, 3.0), (4.5, 13.0)]);
            g.line(&[(11.5, 3.0), (11.5, 13.0)]);
            for y in [5.0, 8.0, 11.0] {
                g.rect_fill(2.6, y - 0.7, 3.9, y + 0.7, 0.0);
                g.rect_fill(12.1, y - 0.7, 13.4, y + 0.7, 0.0);
            }
        }
        Icon::Star => {
            let outer = 6.0;
            let inner = 2.5;
            let pt = |i: usize, r: f32| {
                let a = (-90.0 + i as f32 * 36.0f32).to_radians();
                (8.0 + r * a.cos(), 8.4 + r * a.sin())
            };
            let core: Vec<(f32, f32)> = (0..5).map(|i| pt(i * 2 + 1, inner)).collect();
            g.fill(&core);
            for i in 0..5 {
                g.fill(&[pt(i * 2 + 9, inner), pt(i * 2, outer), pt(i * 2 + 1, inner)]);
            }
        }
        Icon::TextLayer | Icon::Type => {
            let w = if icon == Icon::Type { 1.6 } else { 1.3 };
            let g = Pen { width: w, ..*g };
            g.line(&[(3.0, 5.0), (3.0, 3.0), (13.0, 3.0), (13.0, 5.0)]);
            g.line(&[(8.0, 3.0), (8.0, 13.0)]);
            g.line(&[(6.0, 13.0), (10.0, 13.0)]);
        }
        Icon::Flowchart | Icon::MiniFlowchart => {
            g.rect_fill(6.0, 2.0, 10.0, 5.5, 0.5);
            g.rect_fill(1.5, 10.5, 5.5, 14.0, 0.5);
            g.rect_fill(10.5, 10.5, 14.5, 14.0, 0.5);
            g.line(&[(8.0, 5.5), (8.0, 8.0)]);
            g.line(&[(3.5, 10.5), (3.5, 8.0), (12.5, 8.0), (12.5, 10.5)]);
        }
        Icon::Send => {
            g.fill(&[(1.5, 7.5), (14.5, 2.0), (8.0, 9.5)]);
            g.fill(&[(8.0, 9.5), (14.5, 2.0), (10.5, 14.0)]);
            g.fill(&[(6.0, 9.0), (8.0, 9.5), (6.5, 13.5)]);
        }
        Icon::Trash => {
            g.line(&[(2.5, 4.0), (13.5, 4.0)]);
            g.line(&[(6.0, 4.0), (6.0, 2.5), (10.0, 2.5), (10.0, 4.0)]);
            g.closed(&[(4.0, 4.0), (12.0, 4.0), (11.2, 14.0), (4.8, 14.0)]);
            for x in [6.5, 9.5] {
                g.line(&[(x, 6.5), (x, 11.5)]);
            }
        }
        Icon::Save => {
            g.closed(&[
                (2.5, 2.5),
                (11.5, 2.5),
                (13.5, 4.5),
                (13.5, 13.5),
                (2.5, 13.5),
            ]);
            g.rect_fill(5.0, 2.5, 10.5, 6.0, 0.0);
            g.rect(4.5, 8.5, 11.5, 13.5, 0.0);
        }
        Icon::Home => {
            g.fill(&[(1.5, 8.0), (8.0, 1.8), (14.5, 8.0)]);
            g.rect_fill(3.5, 7.5, 12.5, 14.0, 0.0);
            g.rect_fill_with(6.8, 9.5, 9.2, 14.0, 0.0, hole);
        }
        Icon::Selection => {
            g.fill(&[(4.5, 1.5), (4.5, 12.6), (12.4, 9.8)]);
            g.fill(&[(7.4, 10.4), (9.6, 9.6), (11.4, 13.6), (9.4, 14.5)]);
        }
        Icon::Hand => {
            let g2 = Pen { width: 1.7, ..*g };
            for (x, top) in [(5.5, 4.0), (7.8, 2.8), (10.1, 3.4), (12.2, 5.0)] {
                g2.line(&[(x, top), (x, 9.0)]);
            }
            g2.line(&[(2.6, 8.2), (4.8, 11.0)]);
            g.rect_fill(4.6, 7.5, 13.1, 14.0, 3.0);
        }
        Icon::Zoom => {
            let g = Pen { width: 1.4, ..*g };
            g.circle(6.5, 6.5, 4.2);
            let g = Pen { width: 2.0, ..g };
            g.line(&[(9.6, 9.6), (13.5, 13.5)]);
        }
        Icon::Orbit => {
            g.circle(8.0, 8.0, 3.0);
            g.ellipse(8.0, 8.0, 6.5, 2.6);
            g.arrow_head((14.0, 9.6), (0.3, 1.0), 2.0);
        }
        Icon::PanCamera => {
            g.line(&[(8.0, 2.5), (8.0, 13.5)]);
            g.line(&[(2.5, 8.0), (13.5, 8.0)]);
            g.arrow_head((8.0, 1.5), (0.0, -1.0), 2.4);
            g.arrow_head((8.0, 14.5), (0.0, 1.0), 2.4);
            g.arrow_head((1.5, 8.0), (-1.0, 0.0), 2.4);
            g.arrow_head((14.5, 8.0), (1.0, 0.0), 2.4);
        }
        Icon::Dolly => {
            g.line(&[(8.0, 2.5), (8.0, 13.5)]);
            g.arrow_head((8.0, 1.5), (0.0, -1.0), 2.6);
            g.arrow_head((8.0, 14.5), (0.0, 1.0), 2.6);
            g.line(&[(4.5, 13.5), (11.5, 13.5)]);
        }
        Icon::Rotate => {
            let g = Pen { width: 1.4, ..*g };
            g.arc(8.0, 8.5, 5.0, -60.0, 220.0);
            g.arrow_head((10.8, 3.6), (1.0, -0.3), 2.6);
        }
        Icon::PanBehind => {
            g.circle(8.0, 8.0, 2.4);
            g.line(&[(8.0, 1.5), (8.0, 5.0)]);
            g.line(&[(8.0, 11.0), (8.0, 14.5)]);
            g.line(&[(1.5, 8.0), (5.0, 8.0)]);
            g.line(&[(11.0, 8.0), (14.5, 8.0)]);
            g.rect(2.5, 2.5, 13.5, 13.5, 0.0);
        }
        Icon::Rectangle => {
            let g = Pen { width: 1.4, ..*g };
            g.rect(2.0, 3.0, 14.0, 13.0, 0.0);
        }
        Icon::Pen => {
            g.closed(&[
                (8.0, 1.5),
                (12.5, 7.0),
                (10.5, 13.0),
                (5.5, 13.0),
                (3.5, 7.0),
            ]);
            g.line(&[(8.0, 1.5), (8.0, 7.5)]);
            g.disc(8.0, 8.5, 1.1);
            g.rect_fill(5.5, 13.0, 10.5, 14.5, 0.0);
        }
        Icon::Brush => {
            let g2 = Pen { width: 1.8, ..*g };
            g2.line(&[(14.0, 2.0), (8.0, 8.5)]);
            g.fill(&[(7.0, 7.4), (8.8, 9.2), (6.5, 12.6), (3.6, 10.6)]);
            g.cubic([(3.6, 10.6), (2.5, 12.0), (2.5, 13.5), (1.5, 14.5)]);
            g.cubic([(1.5, 14.5), (4.5, 14.5), (6.0, 13.6), (6.5, 12.6)]);
        }
        Icon::CloneStamp => {
            g.disc(8.0, 3.5, 2.2);
            g.rect_fill(7.0, 5.0, 9.0, 9.5, 0.0);
            g.rect_fill(3.0, 9.5, 13.0, 12.0, 1.0);
            g.line(&[(2.0, 14.0), (14.0, 14.0)]);
        }
        Icon::Eraser => {
            g.fill(&[(9.0, 2.0), (14.0, 7.0), (9.0, 12.0), (4.0, 7.0)]);
            g.closed(&[(4.0, 7.0), (9.0, 12.0), (7.0, 14.0), (2.0, 9.0)]);
        }
        Icon::Puppet => {
            g.disc(8.0, 5.5, 3.6);
            g.line(&[(8.0, 9.0), (8.0, 14.5)]);
            g.disc_with(6.8, 4.4, 1.0, hole);
        }
        Icon::PlayCircle => {
            g.disc(8.0, 8.0, 6.5);
            g.with_color(hole)
                .fill(&[(6.3, 4.8), (11.2, 8.0), (6.3, 11.2)]);
        }
        Icon::FastPreview => {
            g.rect(1.5, 2.0, 14.5, 14.0, 1.0);
            g.fill(&[(9.5, 3.5), (5.0, 9.0), (8.0, 9.0)]);
            g.fill(&[(8.0, 7.0), (11.0, 7.0), (6.5, 12.5)]);
        }
        Icon::TransparencyGrid => {
            for i in 0..4 {
                for j in 0..4 {
                    if (i + j) % 2 == 0 {
                        let (x, y) = (2.0 + i as f32 * 3.0, 2.0 + j as f32 * 3.0);
                        g.rect_fill(x, y, x + 3.0, y + 3.0, 0.0);
                    }
                }
            }
            g.rect(2.0, 2.0, 14.0, 14.0, 0.0);
        }
        Icon::RegionOfInterest => {
            g.rect(1.5, 3.0, 14.5, 13.0, 0.0);
            g.rect_fill(5.0, 6.0, 11.0, 10.0, 0.0);
        }
        Icon::Exposure => {
            g.circle(8.0, 8.0, 5.8);
            for i in 0..6 {
                let a = (i as f32 * 60.0).to_radians();
                let b = a + 1.2;
                g.line(&[
                    (8.0 + 2.4 * a.cos(), 8.0 + 2.4 * a.sin()),
                    (8.0 + 5.8 * b.cos(), 8.0 + 5.8 * b.sin()),
                ]);
            }
        }
        Icon::Snapshot => {
            g.rect_fill(1.5, 5.0, 14.5, 13.5, 1.5);
            g.rect_fill(5.5, 3.0, 10.5, 5.5, 0.5);
            g.disc_with(8.0, 9.2, 3.0, hole);
            g.disc(8.0, 9.2, 1.8);
        }
        Icon::Marker => {
            g.fill(&[
                (3.0, 2.5),
                (13.0, 2.5),
                (13.0, 10.0),
                (8.0, 14.0),
                (3.0, 10.0),
            ]);
        }
        // Drawn from fill-path data (see `vector`); nothing to paint here.
        _ => {}
    }
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

    /// Every icon draws something, whether from fill-path data or the painter.
    #[test]
    fn every_icon_has_a_picture() {
        let ctx = egui::Context::default();
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
