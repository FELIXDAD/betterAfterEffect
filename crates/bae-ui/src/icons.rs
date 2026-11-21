//! Vector icons painted with the egui `Painter`.
//!
//! Every icon is drawn on a 16×16 design grid and scaled to the target rect, so
//! icons stay sharp at any size and no image assets are shipped. The shapes are
//! our own drawings of common metaphors; no Adobe artwork is used.

use egui::epaint::{CubicBezierShape, PathShape, PathStroke};
use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, vec2};

use crate::theme::color;

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
        Icon::SortUp => g.fill(&[(4.0, 10.0), (12.0, 10.0), (8.0, 6.0)]),
        Icon::Stopwatch => {
            g.circle(8.0, 9.0, 4.8);
            g.line(&[(6.5, 2.8), (9.5, 2.8)]);
            g.line(&[(8.0, 2.8), (8.0, 4.2)]);
            g.line(&[(4.4, 4.6), (3.4, 3.6)]);
        }
        Icon::Keyframe => g.fill(&[(8.0, 3.5), (12.5, 8.0), (8.0, 12.5), (3.5, 8.0)]),
        Icon::KeyframeOutline => g.closed(&[(8.0, 4.0), (12.0, 8.0), (8.0, 12.0), (4.0, 8.0)]),
        Icon::TriangleLeft => g.fill(&[(10.0, 4.5), (10.0, 11.5), (5.0, 8.0)]),
        Icon::TriangleRight => g.fill(&[(6.0, 4.5), (11.0, 8.0), (6.0, 11.5)]),
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
        Icon::Solo => g.disc(8.0, 8.0, 3.2),
        Icon::Tag => {
            g.fill(&[(2.0, 2.5), (8.5, 2.5), (14.0, 8.0), (8.0, 14.0), (2.0, 8.5)]);
            g.disc_with(5.2, 5.7, 1.3, hole);
        }
        Icon::Shy => {
            // A small face peeking over a wall.
            g.rect_fill(2.5, 3.5, 13.5, 9.0, 5.5);
            g.disc_with(6.2, 6.8, 0.9, hole);
            g.disc_with(9.8, 6.8, 0.9, hole);
            g.rect_fill(1.5, 9.0, 14.5, 12.0, 0.5);
            g.rect_fill(5.0, 12.0, 6.5, 14.0, 0.0);
            g.rect_fill(9.5, 12.0, 11.0, 14.0, 0.0);
        }
        Icon::Collapse => {
            g.circle(8.0, 8.0, 2.5);
            for i in 0..8 {
                let a = (i as f32 * 45.0).to_radians();
                let (c, s) = (a.cos(), a.sin());
                g.line(&[
                    (8.0 + 4.0 * c, 8.0 + 4.0 * s),
                    (8.0 + 6.0 * c, 8.0 + 6.0 * s),
                ]);
            }
        }
        Icon::Quality => {
            let g = Pen { width: 1.3, ..*g };
            g.line(&[(4.0, 13.0), (12.0, 3.0)]);
        }
        Icon::Fx => {
            let g = Pen { width: 1.2, ..*g };
            g.cubic([(9.5, 2.5), (7.0, 1.8), (6.6, 4.0), (6.0, 8.0)]);
            g.cubic([(6.0, 8.0), (5.4, 12.0), (5.0, 14.0), (2.5, 13.5)]);
            g.line(&[(4.0, 6.0), (8.5, 6.0)]);
            g.line(&[(8.5, 7.0), (13.5, 13.0)]);
            g.line(&[(13.5, 7.0), (8.5, 13.0)]);
        }
        Icon::FrameBlend => {
            g.rect(3.0, 2.5, 13.0, 13.5, 0.5);
            g.line(&[(5.5, 2.5), (5.5, 13.5)]);
            g.line(&[(10.5, 2.5), (10.5, 13.5)]);
            for y in [4.5, 8.0, 11.5] {
                g.rect_fill(3.0, y - 0.6, 5.5, y + 0.6, 0.0);
                g.rect_fill(10.5, y - 0.6, 13.0, y + 0.6, 0.0);
            }
        }
        Icon::MotionBlur => {
            g.circle(5.0, 8.0, 3.0);
            g.circle(8.0, 8.0, 3.0);
            g.disc(11.0, 8.0, 3.2);
        }
        Icon::Adjustment => {
            g.circle(8.0, 8.0, 5.0);
            let pts: Vec<(f32, f32)> = (0..=18)
                .map(|i| {
                    let a = (-90.0 + i as f32 * 10.0f32).to_radians();
                    (8.0 + 5.0 * a.cos(), 8.0 + 5.0 * a.sin())
                })
                .collect();
            g.fill(&pts);
        }
        Icon::Cube | Icon::Extrude => {
            g.closed(&[
                (8.0, 2.5),
                (13.0, 5.0),
                (13.0, 11.0),
                (8.0, 13.5),
                (3.0, 11.0),
                (3.0, 5.0),
            ]);
            g.line(&[(3.0, 5.0), (8.0, 7.5), (13.0, 5.0)]);
            g.line(&[(8.0, 7.5), (8.0, 13.5)]);
            if icon == Icon::Extrude {
                g.fill(&[(8.0, 7.5), (13.0, 5.0), (13.0, 11.0), (8.0, 13.5)]);
            }
        }
        Icon::Pickwhip => {
            // Spiral.
            let pts: Vec<(f32, f32)> = (0..=60)
                .map(|i| {
                    let t = i as f32 / 60.0;
                    let a = (t * 900.0f32 - 90.0).to_radians();
                    let r = 0.6 + 5.2 * t;
                    (8.0 + r * a.cos(), 8.0 + r * a.sin())
                })
                .collect();
            g.line(&pts);
        }
        Icon::Folder => {
            g.fill(&[(1.5, 3.5), (6.0, 3.5), (7.5, 5.0), (1.5, 5.0)]);
            g.rect_fill(1.5, 5.0, 14.5, 13.0, 0.5);
            g.line(&[(1.5, 6.2), (14.5, 6.2)]);
        }
        Icon::Composition => {
            g.rect(1.5, 2.5, 14.5, 13.5, 1.0);
            let red = Color32::from_rgb(0xe0, 0x4b, 0x3c);
            let blue = Color32::from_rgb(0x48, 0x8c, 0xe8);
            let yellow = Color32::from_rgb(0xe8, 0xc5, 0x4a);
            g.with_color(red)
                .fill(&[(3.5, 11.5), (6.8, 5.0), (10.0, 11.5)]);
            g.disc_with(10.5, 6.6, 2.2, blue);
            g.rect_fill_with(9.0, 9.0, 12.5, 12.0, 0.0, yellow);
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
        Icon::InterpretFootage => {
            g.rect(1.5, 3.5, 11.0, 12.5, 0.5);
            g.line(&[(4.0, 3.5), (4.0, 12.5)]);
            for y in [6.0, 8.5, 11.0] {
                g.line(&[(9.0, y), (14.5, y)]);
            }
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
        Icon::RotoBrush => {
            g.disc(5.0, 3.5, 1.8);
            let g2 = Pen { width: 1.6, ..*g };
            g2.line(&[(5.0, 6.0), (4.5, 10.0), (2.5, 14.0)]);
            g2.line(&[(4.5, 10.0), (7.0, 14.0)]);
            g2.line(&[(2.0, 7.5), (8.0, 7.5)]);
            g2.line(&[(14.0, 2.0), (10.0, 7.0)]);
            g.fill(&[(9.2, 6.2), (10.8, 7.8), (9.0, 10.5), (7.5, 9.0)]);
        }
        Icon::RoundedRectangle => {
            let g = Pen { width: 1.4, ..*g };
            g.rect(2.0, 3.0, 14.0, 13.0, 3.0);
        }
        Icon::Ellipse => {
            let g = Pen { width: 1.4, ..*g };
            g.ellipse(8.0, 8.0, 6.0, 5.0);
        }
        Icon::Polygon | Icon::StarTool => {
            let g = Pen { width: 1.3, ..*g };
            let star = icon == Icon::StarTool;
            let n = if star { 10 } else { 5 };
            let pts: Vec<(f32, f32)> = (0..n)
                .map(|i| {
                    let r = if star && i % 2 == 1 { 2.8 } else { 6.2 };
                    let a = (-90.0 + i as f32 * 360.0 / n as f32).to_radians();
                    (8.0 + r * a.cos(), 8.6 + r * a.sin())
                })
                .collect();
            g.closed(&pts);
        }
        Icon::Sphere => {
            g.circle(8.0, 8.0, 5.8);
            g.ellipse(8.0, 8.0, 5.8, 2.0);
        }
        Icon::Cylinder => {
            g.ellipse(8.0, 3.8, 5.0, 1.8);
            g.line(&[(3.0, 3.8), (3.0, 12.2)]);
            g.line(&[(13.0, 3.8), (13.0, 12.2)]);
            g.arc(8.0, 12.2, 5.0, 0.0, 180.0);
        }
        Icon::Cone => {
            g.line(&[(8.0, 2.0), (3.0, 12.0)]);
            g.line(&[(8.0, 2.0), (13.0, 12.0)]);
            g.ellipse(8.0, 12.0, 5.0, 1.8);
        }
        Icon::Torus => {
            g.ellipse(8.0, 8.0, 6.5, 4.0);
            g.ellipse(8.0, 8.0, 2.6, 1.3);
        }
        Icon::Plane => {
            g.closed(&[(4.5, 4.0), (14.5, 4.0), (11.5, 12.0), (1.5, 12.0)]);
        }
        Icon::PenAdd | Icon::PenDelete | Icon::PenConvert | Icon::MaskFeather => {
            // A smaller nib on the left, the modifier on the right.
            g.closed(&[(5.5, 2.0), (9.0, 6.5), (7.5, 11.5), (3.5, 11.5), (2.0, 6.5)]);
            g.line(&[(5.5, 2.0), (5.5, 7.0)]);
            g.rect_fill(3.5, 11.5, 7.5, 13.0, 0.0);
            let g2 = Pen { width: 1.5, ..*g };
            match icon {
                Icon::PenAdd => {
                    g2.line(&[(10.5, 11.0), (15.0, 11.0)]);
                    g2.line(&[(12.75, 8.75), (12.75, 13.25)]);
                }
                Icon::PenDelete => g2.line(&[(10.5, 11.0), (15.0, 11.0)]),
                Icon::PenConvert => g2.line(&[(10.0, 13.5), (12.5, 8.5), (15.0, 13.5)]),
                _ => {
                    g.circle(12.5, 11.0, 2.4);
                    g.circle(12.5, 11.0, 0.8);
                }
            }
        }
        Icon::TypeVertical => {
            let g = Pen { width: 1.6, ..*g };
            g.line(&[(5.0, 3.0), (3.0, 3.0), (3.0, 13.0), (5.0, 13.0)]);
            g.line(&[(3.0, 8.0), (13.0, 8.0)]);
            g.line(&[(13.0, 6.0), (13.0, 10.0)]);
        }
        Icon::RefineEdge => {
            g.arc(8.0, 8.0, 5.5, 0.0, 60.0);
            g.arc(8.0, 8.0, 5.5, 90.0, 150.0);
            g.arc(8.0, 8.0, 5.5, 180.0, 240.0);
            g.arc(8.0, 8.0, 5.5, 270.0, 330.0);
            g.disc(8.0, 8.0, 1.6);
        }
        Icon::PuppetStarch | Icon::PuppetBend | Icon::PuppetAdvanced | Icon::PuppetOverlap => {
            g.disc(8.0, 5.5, 3.0);
            g.line(&[(8.0, 8.5), (8.0, 14.5)]);
            match icon {
                Icon::PuppetStarch => g.rect(3.0, 1.0, 13.0, 10.0, 1.0),
                Icon::PuppetBend => g.arc(8.0, 5.5, 5.5, 200.0, 340.0),
                Icon::PuppetAdvanced => {
                    g.circle(8.0, 5.5, 5.5);
                    g.rect_fill(12.0, 1.0, 14.5, 3.5, 0.0);
                }
                _ => {
                    g.line(&[(2.0, 12.0), (14.0, 12.0)]);
                    g.line(&[(2.0, 14.5), (14.0, 14.5)]);
                }
            }
        }
        Icon::Puppet => {
            g.disc(8.0, 5.5, 3.6);
            g.line(&[(8.0, 9.0), (8.0, 14.5)]);
            g.disc_with(6.8, 4.4, 1.0, hole);
        }
        Icon::AxisLocal | Icon::AxisWorld | Icon::AxisView => {
            let o = (8.0, 9.5);
            g.line(&[o, (8.0, 2.5)]);
            g.line(&[o, (2.5, 13.0)]);
            g.line(&[o, (13.5, 13.0)]);
            g.disc(8.0, 2.5, 1.2);
            g.disc(2.5, 13.0, 1.2);
            g.disc(13.5, 13.0, 1.2);
            match icon {
                Icon::AxisWorld => g.ellipse(8.0, 9.5, 4.0, 1.8),
                Icon::AxisView => g.rect(5.5, 7.0, 10.5, 12.0, 0.0),
                _ => {}
            }
        }
        Icon::SnapEdges => {
            g.line(&[(3.0, 13.0), (12.0, 4.0)]);
            g.arrow_head((13.0, 3.0), (1.0, -1.0), 2.6);
            g.line(&[(10.0, 13.0), (13.0, 13.0), (13.0, 10.0)]);
        }
        Icon::SnapFeatures => {
            for (x, y) in [(3.0, 3.0), (13.0, 3.0), (3.0, 13.0), (13.0, 13.0)] {
                let dx = if x < 8.0 { 2.0 } else { -2.0 };
                let dy = if y < 8.0 { 2.0 } else { -2.0 };
                g.line(&[(x + dx, y), (x, y), (x, y + dy)]);
            }
            g.rect_fill(6.5, 6.5, 9.5, 9.5, 0.0);
        }
        Icon::PlayCircle => {
            g.disc(8.0, 8.0, 6.5);
            g.with_color(hole)
                .fill(&[(6.3, 4.8), (11.2, 8.0), (6.3, 11.2)]);
        }
        Icon::AutoOpen => {
            g.rect(1.5, 2.5, 14.5, 13.5, 0.5);
            g.line(&[(6.0, 2.5), (6.0, 13.5)]);
            g.line(&[(8.0, 8.0), (13.0, 8.0)]);
            g.arrow_head((8.0, 8.0), (-1.0, 0.0), 2.4);
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
        Icon::MaskVisibility => {
            g.closed(&[(3.0, 4.0), (12.0, 2.5), (13.0, 12.0), (4.0, 13.0)]);
            for (x, y) in [(3.0, 4.0), (12.0, 2.5), (13.0, 12.0), (4.0, 13.0)] {
                g.rect_fill(x - 1.2, y - 1.2, x + 1.2, y + 1.2, 0.0);
            }
        }
        Icon::RegionOfInterest => {
            g.rect(1.5, 3.0, 14.5, 13.0, 0.0);
            g.rect_fill(5.0, 6.0, 11.0, 10.0, 0.0);
        }
        Icon::Guides => {
            g.rect(1.5, 1.5, 12.5, 12.5, 0.0);
            g.line(&[(7.0, 1.5), (7.0, 12.5)]);
            g.line(&[(1.5, 7.0), (12.5, 7.0)]);
            g.fill(&[(11.5, 13.5), (15.5, 13.5), (13.5, 15.5)]);
        }
        Icon::Channels => {
            let r = Color32::from_rgb(0xe0, 0x40, 0x3a);
            let gr = Color32::from_rgb(0x4c, 0xb0, 0x4c);
            let b = Color32::from_rgb(0x46, 0x7c, 0xe6);
            g.disc_with(8.0, 4.8, 3.2, r);
            g.disc_with(5.0, 9.6, 3.2, gr);
            g.disc_with(11.0, 9.6, 3.2, b);
            g.fill(&[(11.5, 14.0), (15.5, 14.0), (13.5, 16.0)]);
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
        Icon::ShowSnapshot => {
            g.cubic([(1.0, 9.0), (3.5, 4.5), (10.5, 4.5), (13.0, 9.0)]);
            g.cubic([(1.0, 9.0), (3.5, 13.5), (10.5, 13.5), (13.0, 9.0)]);
            g.disc(7.0, 9.0, 2.0);
            g.rect_fill(11.0, 1.5, 15.0, 5.0, 0.5);
        }
        Icon::GraphEditor => {
            g.rect(1.5, 2.5, 14.5, 13.5, 1.0);
            g.cubic([(3.0, 12.0), (8.0, 12.0), (8.0, 4.0), (13.0, 4.0)]);
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
        Icon::CompButton => {
            g.rect_fill(2.0, 2.0, 7.0, 6.0, 0.5);
            g.rect_fill(9.0, 10.0, 14.0, 14.0, 0.5);
            g.line(&[(4.5, 6.0), (4.5, 12.0), (9.0, 12.0)]);
            g.arrow_head((12.5, 4.0), (1.0, 0.0), 2.2);
            g.line(&[(7.0, 4.0), (11.0, 4.0)]);
        }
        Icon::MountainSmall => {
            g.fill(&[(4.0, 12.0), (8.0, 7.0), (12.0, 12.0)]);
        }
        Icon::MountainLarge => {
            g.fill(&[(1.5, 13.0), (6.5, 5.0), (11.5, 13.0)]);
            g.fill(&[(7.0, 13.0), (10.5, 8.0), (14.5, 13.0)]);
        }
        Icon::PaneSwitches => {
            g.rect(1.5, 3.0, 14.5, 13.0, 1.0);
            g.rect_fill(3.5, 5.0, 6.0, 7.5, 0.0);
            g.rect_fill(3.5, 8.5, 6.0, 11.0, 0.0);
            g.line(&[(7.5, 6.2), (12.5, 6.2)]);
            g.line(&[(7.5, 9.8), (12.5, 9.8)]);
        }
        Icon::PaneModes => {
            g.rect(1.5, 3.0, 14.5, 13.0, 1.0);
            g.line(&[(8.0, 3.0), (8.0, 13.0)]);
            g.rect_fill(1.5, 3.0, 8.0, 13.0, 1.0);
        }
        Icon::PaneInOut => {
            g.line(&[(5.5, 3.0), (3.0, 3.0), (3.0, 13.0), (5.5, 13.0)]);
            g.line(&[(10.5, 3.0), (13.0, 3.0), (13.0, 13.0), (10.5, 13.0)]);
            g.line(&[(5.5, 8.0), (10.5, 8.0)]);
        }
        Icon::PaneRenderTime => {
            g.circle(8.0, 8.5, 5.5);
            g.line(&[(8.0, 8.5), (8.0, 5.0)]);
            g.line(&[(8.0, 8.5), (10.5, 10.0)]);
        }
        Icon::FirstFrame => {
            g.rect_fill(3.0, 3.5, 4.6, 12.5, 0.0);
            g.fill(&[(12.5, 3.5), (12.5, 12.5), (5.5, 8.0)]);
        }
        Icon::PrevFrame => {
            g.fill(&[(10.0, 3.5), (10.0, 12.5), (3.5, 8.0)]);
            g.rect_fill(11.4, 3.5, 13.0, 12.5, 0.0);
        }
        Icon::Play => g.fill(&[(4.5, 2.5), (13.0, 8.0), (4.5, 13.5)]),
        Icon::Pause => {
            g.rect_fill(4.0, 3.0, 6.8, 13.0, 0.0);
            g.rect_fill(9.2, 3.0, 12.0, 13.0, 0.0);
        }
        Icon::NextFrame => {
            g.rect_fill(3.0, 3.5, 4.6, 12.5, 0.0);
            g.fill(&[(6.0, 3.5), (12.5, 8.0), (6.0, 12.5)]);
        }
        Icon::LastFrame => {
            g.fill(&[(3.5, 3.5), (10.5, 8.0), (3.5, 12.5)]);
            g.rect_fill(11.4, 3.5, 13.0, 12.5, 0.0);
        }
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
