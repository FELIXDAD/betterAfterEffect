//! Layer transform math (AE-ANI-013 … AE-ANI-022).
//!
//! AE coordinates: +x right, +y down, +z away from the viewer; column vectors; angles
//! in degrees; scale and opacity in percent. Everything is `f64` (comps can be large);
//! the renderer converts to `f32` when uploading.

use glam::{DMat4, DVec3};
use serde::Serialize;

/// Evaluated transform group of a layer, in AE units.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Transform {
    pub anchor_point: DVec3,
    /// In the parent's layer space (comp space without a parent).
    pub position: DVec3,
    /// Percent.
    pub scale: DVec3,
    /// 3D only, degrees.
    pub orientation: DVec3,
    /// X, Y and Z rotation in degrees. 2D layers only use `z`.
    pub rotation: DVec3,
    /// Percent, 0–100.
    pub opacity: f64,
    /// Extra rotation from auto-orient, applied outside Orientation/Rotation (identity
    /// unless the layer's Auto-Orient option is on).
    pub auto_orient: AutoOrientRotation,
}

/// The automatic rotation `L` of AE-ANI-018 … AE-ANI-022.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub enum AutoOrientRotation {
    #[default]
    None,
    /// 2D Orient Along Path: degrees added to Z rotation.
    Angle(f64),
    /// 3D: `Ry(yaw) * Rx(pitch)` in degrees, turning local +z towards a direction
    /// without roll (along path, towards the point of interest or the camera).
    YawPitch { yaw: f64, pitch: f64 },
}

impl Default for Transform {
    fn default() -> Self {
        Transform {
            anchor_point: DVec3::ZERO,
            position: DVec3::ZERO,
            scale: DVec3::splat(100.0),
            orientation: DVec3::ZERO,
            rotation: DVec3::ZERO,
            opacity: 100.0,
            auto_orient: AutoOrientRotation::None,
        }
    }
}

impl AutoOrientRotation {
    fn matrix(self) -> DMat4 {
        match self {
            AutoOrientRotation::None | AutoOrientRotation::Angle(_) => DMat4::IDENTITY,
            AutoOrientRotation::YawPitch { yaw, pitch } => {
                DMat4::from_rotation_y(yaw.to_radians())
                    * DMat4::from_rotation_x(pitch.to_radians())
            }
        }
    }
}

impl Transform {
    /// Layer space → parent space.
    ///
    /// 2D: `T(p) · Rz(rotation + auto) · S(s) · T(-a)` with z untouched (AE-ANI-013).
    /// 3D: `T(p) · L · Rx·Ry·Rz(orientation) · Rx·Ry·Rz(rotation) · S(s) · T(-a)`
    /// (AE-ANI-014).
    pub fn matrix(&self, three_d: bool) -> DMat4 {
        if !three_d {
            let extra = match self.auto_orient {
                AutoOrientRotation::Angle(a) => a,
                _ => 0.0,
            };
            let p = self.position;
            let a = self.anchor_point;
            return DMat4::from_translation(DVec3::new(p.x, p.y, 0.0))
                * DMat4::from_rotation_z((self.rotation.z + extra).to_radians())
                * DMat4::from_scale(DVec3::new(self.scale.x / 100.0, self.scale.y / 100.0, 1.0))
                * DMat4::from_translation(DVec3::new(-a.x, -a.y, 0.0));
        }
        DMat4::from_translation(self.position)
            * self.auto_orient.matrix()
            * xyz_rotation(self.orientation)
            * xyz_rotation(self.rotation)
            * DMat4::from_scale(self.scale / 100.0)
            * DMat4::from_translation(-self.anchor_point)
    }

    /// Cameras and lights: `T(p) · L · Or · Rot`, no scale or anchor (AE-ANI-014).
    pub fn camera_matrix(&self) -> DMat4 {
        DMat4::from_translation(self.position)
            * self.auto_orient.matrix()
            * xyz_rotation(self.orientation)
            * xyz_rotation(self.rotation)
    }
}

/// `Rx(x) · Ry(y) · Rz(z)`: Z is applied first, then Y, then X.
fn xyz_rotation(r: DVec3) -> DMat4 {
    DMat4::from_rotation_x(r.x.to_radians())
        * DMat4::from_rotation_y(r.y.to_radians())
        * DMat4::from_rotation_z(r.z.to_radians())
}

/// Yaw/pitch that turn local +z towards `d` without roll (AE-ANI-020/022).
pub(crate) fn look_along(d: DVec3) -> AutoOrientRotation {
    if d.length_squared() < 1e-24 {
        return AutoOrientRotation::None;
    }
    let horiz = d.x.hypot(d.z);
    let yaw = if horiz < 1e-12 { 0.0 } else { d.x.atan2(d.z) };
    let pitch = (-d.y).atan2(horiz);
    AutoOrientRotation::YawPitch {
        yaw: yaw.to_degrees(),
        pitch: pitch.to_degrees(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: DVec3, b: DVec3) {
        assert!((a - b).length() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn spec_2d_vectors() {
        // AE-ANI-013 test vectors.
        let t = Transform {
            anchor_point: DVec3::new(50.0, 50.0, 0.0),
            position: DVec3::new(960.0, 540.0, 0.0),
            rotation: DVec3::new(0.0, 0.0, 90.0),
            ..Default::default()
        };
        close(
            t.matrix(false)
                .transform_point3(DVec3::new(100.0, 50.0, 0.0)),
            DVec3::new(960.0, 590.0, 0.0),
        );
        let t = Transform {
            scale: DVec3::new(200.0, 50.0, 100.0),
            rotation: DVec3::ZERO,
            ..t
        };
        close(
            t.matrix(false).transform_point3(DVec3::ZERO),
            DVec3::new(860.0, 515.0, 0.0),
        );
    }

    #[test]
    fn spec_3d_signs() {
        // AE-ANI-014 sign checks.
        let rx = Transform {
            rotation: DVec3::new(90.0, 0.0, 0.0),
            ..Default::default()
        };
        close(
            rx.matrix(true)
                .transform_point3(DVec3::new(0.0, -50.0, 0.0)),
            DVec3::new(0.0, 0.0, -50.0),
        );
        let ry = Transform {
            rotation: DVec3::new(0.0, 90.0, 0.0),
            ..Default::default()
        };
        close(
            ry.matrix(true).transform_point3(DVec3::new(50.0, 0.0, 0.0)),
            DVec3::new(0.0, 0.0, -50.0),
        );
        let rz = Transform {
            rotation: DVec3::new(0.0, 0.0, 90.0),
            ..Default::default()
        };
        close(
            rz.matrix(true).transform_point3(DVec3::new(50.0, 0.0, 0.0)),
            DVec3::new(0.0, 50.0, 0.0),
        );
    }

    #[test]
    fn flat_3d_equals_2d() {
        let t = Transform {
            anchor_point: DVec3::new(10.0, 20.0, 0.0),
            position: DVec3::new(300.0, 200.0, 0.0),
            scale: DVec3::new(150.0, 80.0, 100.0),
            rotation: DVec3::new(0.0, 0.0, 33.0),
            ..Default::default()
        };
        assert!(t.matrix(true).abs_diff_eq(t.matrix(false), 1e-12));
    }

    #[test]
    fn default_camera_looks_down_z() {
        assert_eq!(
            look_along(DVec3::new(0.0, 0.0, 1.0)),
            AutoOrientRotation::YawPitch {
                yaw: 0.0,
                pitch: 0.0
            }
        );
        // Looking right: +z turns to +x.
        let AutoOrientRotation::YawPitch { yaw, pitch } = look_along(DVec3::X) else {
            panic!()
        };
        assert!((yaw - 90.0).abs() < 1e-12 && pitch.abs() < 1e-12);
    }
}
