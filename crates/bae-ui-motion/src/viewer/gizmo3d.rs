//! The axis gizmo of a selected 3D layer, at its anchor point (AE's 3D Gizmo
//! modes): red X, green Y and blue Z arrows move the layer along one axis,
//! arrows ending in squares scale it along one of its axes, and three rings
//! rotate it about one axis (Orientation or X / Y / Z Rotation, the Rotation
//! tool's Set option). Moving and rotating use the toolbar's axis mode (the
//! layer's axes, the comp's, or the view's); scaling always the layer's.

use bae_anim::EvaluatedLayer;
use bae_anim::glam::{DVec2, DVec3};
use bae_render::camera::{Camera, Projection};
use egui::{Color32, Painter, Pos2, Shape, Stroke};

use crate::render::{self, View};
use crate::tools::AxisMode;

/// What the gizmo shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Style {
    Arrows,
    Scale,
    Rings,
}

/// Arrow length and ring radius on screen (points).
const SIZE: f32 = 70.0;
/// How close to an arrow or a ring a press picks it (points).
const PICK: f32 = 6.0;
const COLORS: [Color32; 3] = [
    Color32::from_rgb(0xf0, 0x3c, 0x3c),
    Color32::from_rgb(0x3c, 0xd2, 0x3c),
    Color32::from_rgb(0x3c, 0x78, 0xff),
];
const RING_SEGMENTS: usize = 48;

pub(super) struct Gizmo {
    /// The anchor point (comp space).
    pub origin: DVec3,
    /// The layer's unit axes (comp space).
    pub axes: [DVec3; 3],
    /// Comp-space length shown [`SIZE`] points long at the anchor.
    length: f64,
}

impl Gizmo {
    /// The gizmo of `l` in `view`: 3D layers in front of the camera.
    pub fn of(l: &EvaluatedLayer, view: &View, mode: AxisMode) -> Option<Gizmo> {
        if !render::is_3d(l) {
            return None;
        }
        let (camera, size) = view.camera?;
        let origin = l.world_matrix.transform_point3(l.transform.anchor_point);
        let m = match mode {
            AxisMode::Local => l.world_matrix,
            AxisMode::World => bae_anim::glam::DMat4::IDENTITY,
            AxisMode::View => camera.world,
        };
        let axes = [m.x_axis, m.y_axis, m.z_axis].map(|a| a.truncate().normalize_or_zero());
        let at = camera.project(origin, size)?;
        let depth = camera.view().transform_point3(origin).z;
        let per_pixel = (camera.unproject(at + DVec2::X, depth, size)
            - camera.unproject(at, depth, size))
        .length();
        let length = f64::from(SIZE / view.scale.max(1e-6)) * per_pixel;
        Some(Gizmo {
            origin,
            axes,
            length,
        })
    }

    fn arrow(&self, view: &View, i: usize) -> Option<[Pos2; 2]> {
        Some([
            view.project(self.origin, true)?,
            view.project(self.origin + self.axes[i] * self.length, true)?,
        ])
    }

    fn ring(&self, view: &View, i: usize) -> Vec<Pos2> {
        let (u, v) = (self.axes[(i + 1) % 3], self.axes[(i + 2) % 3]);
        (0..=RING_SEGMENTS)
            .filter_map(|k| {
                let t = std::f64::consts::TAU * k as f64 / RING_SEGMENTS as f64;
                view.project(
                    self.origin + (u * t.cos() + v * t.sin()) * self.length,
                    true,
                )
            })
            .collect()
    }

    /// `active`: the axis hovered or dragged.
    pub fn paint(&self, painter: &Painter, view: &View, style: Style, active: Option<usize>) {
        for (i, color) in COLORS.into_iter().enumerate() {
            let width = if active == Some(i) { 3.0 } else { 1.5 };
            let stroke = Stroke::new(width, color);
            if style == Style::Rings {
                painter.add(Shape::line(self.ring(view, i), stroke));
                continue;
            }
            let Some([a, b]) = self.arrow(view, i) else {
                continue;
            };
            painter.line_segment([a, b], stroke);
            let d = (b - a).normalized();
            if style == Style::Scale {
                painter.rect_filled(
                    egui::Rect::from_center_size(b, egui::vec2(7.0, 7.0)),
                    0.0,
                    color,
                );
            } else if d.length() > 0.5 {
                let n = egui::vec2(-d.y, d.x);
                painter.add(Shape::convex_polygon(
                    vec![b + d * 8.0, b + n * 4.0, b - n * 4.0],
                    color,
                    Stroke::NONE,
                ));
            }
        }
    }

    /// The axis whose arrow (or ring) is under `pos`.
    pub fn hit(&self, view: &View, pos: Pos2, style: Style) -> Option<usize> {
        (0..3)
            .filter_map(|i| {
                let d = if style == Style::Rings {
                    let r = self.ring(view, i);
                    r.windows(2)
                        .map(|w| segment_distance(pos, w[0], w[1]))
                        .fold(f32::INFINITY, f32::min)
                } else {
                    let [a, b] = self.arrow(view, i)?;
                    segment_distance(pos, a, b)
                };
                (d <= PICK).then_some((i, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }
}

fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// The pointer's ray through comp pixel `p`: origin and unit direction.
pub(super) fn ray(camera: &Camera, size: DVec2, p: DVec2) -> (DVec3, DVec3) {
    match camera.projection {
        Projection::Perspective => {
            let eye = camera.position();
            (eye, (camera.unproject(p, 1.0, size) - eye).normalize())
        }
        Projection::Orthographic { .. } => (camera.unproject(p, 0.0, size), camera.direction()),
    }
}

/// Where along the line `origin + dir · t` the pointer at comp pixel `p` is:
/// the point closest to its ray (`None` when the line points along the ray).
pub(super) fn along(
    camera: &Camera,
    size: DVec2,
    p: DVec2,
    origin: DVec3,
    dir: DVec3,
) -> Option<f64> {
    let (o, r) = ray(camera, size, p);
    let w = origin - o;
    let (a, b, c) = (dir.dot(dir), dir.dot(r), r.dot(r));
    let den = a * c - b * b;
    (den.abs() > 1e-9 * a * c).then(|| (b * r.dot(w) - c * dir.dot(w)) / den)
}

/// The sign that turns a clockwise drag on screen into the rotation about
/// `axis` (positive when the axis points away from the viewer).
pub(super) fn spin_sign(camera: &Camera, origin: DVec3, axis: DVec3) -> f64 {
    let view_dir = match camera.projection {
        Projection::Perspective => origin - camera.position(),
        Projection::Orthographic { .. } => camera.direction(),
    };
    if axis.dot(view_dir) >= 0.0 { 1.0 } else { -1.0 }
}
