//! AE's camera model for the Classic 3D renderer (2.5D): where a camera is, how it
//! projects 3D layers onto the comp frame, and the lens arithmetic of the Camera
//! Settings dialog.
//!
//! Coordinates follow AE (`docs/spec/ae/transform-keyframes-expressions.md`,
//! AE-ANI-014): +x right, +y down, +z away from the viewer, comp pixels, column
//! vectors. A camera looks along its local +z with local +y pointing down the
//! screen. A point at camera-space `(xc, yc, zc)` lands at
//!
//! ```text
//! x_comp = W/2 + zoom · xc / zc        y_comp = H/2 + zoom · yc / zc
//! ```
//!
//! (AE-CMP-402, principal point at the comp centre, `W × H` comp pixels). A point
//! at depth `zc == zoom` maps 1:1, so AE's default camera (AE-CMP-403) at
//! `(W/2, H/2, −zoom)` shows a 3D layer at z = 0 exactly like the same layer in 2D.
//!
//! Interface for the viewer (camera views, 3D axis handles) and the camera
//! commands: [`Camera`] (a camera at one time, with [`Camera::project`],
//! [`Camera::unproject`] and [`Camera::plane_homography`]), the lens functions
//! ([`zoom_from_focal_length`], [`angle_of_view`] …, AE-CMP-418) and
//! [`look_at`] (two-node cameras, AE-ANI-022). Everything here is plain `f64`
//! math without GPU types.

use glam::{DMat3, DMat4, DVec2, DVec3, DVec4};

/// Cameras do not draw geometry closer than this (comp pixels in front of the
/// lens). AE's near plane is undocumented; it is very close (AE-CMP-402).
pub const NEAR: f64 = 1.0;

/// Film width of AE's lens presets (35 mm film, measured horizontally).
pub const PRESET_FILM_SIZE: f64 = 36.0;

/// AE's lens presets (Camera Settings > Preset): name and focal length in mm on
/// 36 mm film. The default preset (and the default comp camera) is `50mm`.
pub const PRESETS: [(&str, f64); 9] = [
    ("15mm", 15.0),
    ("20mm", 20.0),
    ("24mm", 24.0),
    ("28mm", 28.0),
    ("35mm", 35.0),
    ("50mm", 50.0),
    ("80mm", 80.0),
    ("135mm", 135.0),
    ("200mm", 200.0),
];

/// A camera at one time: placement and lens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Camera space → comp space (the camera layer's world matrix, parents and
    /// auto-orient included). The camera sits at the translation and looks along
    /// local +z; local +y is screen-down.
    pub world: DMat4,
    /// Comp pixels: the distance at which a plane facing the camera appears at
    /// 100 % (AE's Zoom property).
    pub zoom: f64,
    /// Depth of field switch (data only: depth-of-field blur is not rendered yet).
    pub depth_of_field: bool,
    /// Pixels from the camera to the plane in focus.
    pub focus_distance: f64,
    /// Aperture in pixels.
    pub aperture: f64,
    /// Blur level in percent.
    pub blur_level: f64,
}

impl Camera {
    /// A camera at `world` with `zoom`, depth of field off, focus at the zoom
    /// distance (AE's "Lock to Zoom").
    pub fn new(world: DMat4, zoom: f64) -> Self {
        Self {
            world,
            zoom,
            depth_of_field: false,
            focus_distance: zoom,
            aperture: 0.0,
            blur_level: 100.0,
        }
    }

    /// AE's default comp camera for a `width × height` comp (AE-CMP-403): a 50 mm
    /// lens on 36 mm film (`zoom = W · 50 / 36`) at `(W/2, H/2, −zoom)` looking
    /// along +z, so z = 0 maps 1:1 onto the comp.
    pub fn default_for(width: u32, height: u32) -> Self {
        let zoom = default_zoom(width);
        let position = DVec3::new(f64::from(width) / 2.0, f64::from(height) / 2.0, -zoom);
        Self::new(DMat4::from_translation(position), zoom)
    }

    /// A camera built from its transform properties (AE-ANI-009/022): Position,
    /// Point of Interest (`Some` for two-node cameras, which aim at it before
    /// Orientation and X/Y/Z Rotation apply; `None` for one-node cameras),
    /// Orientation and Rotation in degrees, all in the camera's parent space
    /// (`parent` = parent world matrix, identity without a parent).
    pub fn from_transform(
        parent: DMat4,
        position: DVec3,
        point_of_interest: Option<DVec3>,
        orientation: DVec3,
        rotation: DVec3,
        zoom: f64,
    ) -> Self {
        let aim = match point_of_interest {
            Some(poi) => look_at(poi - position),
            None => DMat4::IDENTITY,
        };
        let world = parent
            * DMat4::from_translation(position)
            * aim
            * xyz_rotation(orientation)
            * xyz_rotation(rotation);
        Self::new(world, zoom)
    }

    /// Where the camera is (comp space).
    pub fn position(&self) -> DVec3 {
        self.world.w_axis.truncate()
    }

    /// Unit viewing direction (comp space).
    pub fn direction(&self) -> DVec3 {
        self.world.z_axis.truncate().normalize_or(DVec3::Z)
    }

    /// Comp space → camera space.
    pub fn view(&self) -> DMat4 {
        self.world.inverse()
    }

    /// The 3×3 matrix taking camera space to homogeneous comp pixels:
    /// `(xc, yc, zc) ↦ (zoom·xc + W/2·zc, zoom·yc + H/2·zc, zc)`.
    pub fn intrinsics(&self, comp_size: DVec2) -> DMat3 {
        DMat3::from_cols(
            DVec3::new(self.zoom, 0.0, 0.0),
            DVec3::new(0.0, self.zoom, 0.0),
            DVec3::new(comp_size.x / 2.0, comp_size.y / 2.0, 1.0),
        )
    }

    /// Comp pixel position of a comp-space point, `None` when it is not in front
    /// of the near plane.
    pub fn project(&self, point: DVec3, comp_size: DVec2) -> Option<DVec2> {
        let c = self.view().transform_point3(point);
        if c.z < NEAR {
            return None;
        }
        Some(DVec2::new(
            comp_size.x / 2.0 + self.zoom * c.x / c.z,
            comp_size.y / 2.0 + self.zoom * c.y / c.z,
        ))
    }

    /// The comp-space point at comp pixel `pixel` and camera depth `depth` (the
    /// inverse of [`Camera::project`]; viewer tools use it to drag in 3D).
    pub fn unproject(&self, pixel: DVec2, depth: f64, comp_size: DVec2) -> DVec3 {
        let xc = (pixel.x - comp_size.x / 2.0) * depth / self.zoom;
        let yc = (pixel.y - comp_size.y / 2.0) * depth / self.zoom;
        self.world.transform_point3(DVec3::new(xc, yc, depth))
    }

    /// The projective map of a planar layer: layer pixels `(x, y)` on the layer's
    /// z = 0 plane, placed by `layer_world` (layer space → comp space), to
    /// homogeneous comp pixels `(X, Y, w)` with comp position `(X/w, Y/w)` and
    /// `w` = the point's camera-space depth.
    pub fn plane_homography(&self, layer_world: &DMat4, comp_size: DVec2) -> DMat3 {
        let m = self.view() * *layer_world;
        let col = |v: DVec4| v.truncate();
        self.intrinsics(comp_size) * DMat3::from_cols(col(m.x_axis), col(m.y_axis), col(m.w_axis))
    }
}

/// The default camera zoom of a comp `width` pixels wide: `W · 50 / 36`
/// (e.g. 2666.67 for 1920, AE-CMP-403).
pub fn default_zoom(width: u32) -> f64 {
    zoom_from_focal_length(50.0, PRESET_FILM_SIZE, f64::from(width))
}

/// How the film size is measured (Camera Settings > Measure Film Size).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FilmMeasure {
    /// Across the comp width (AE's default).
    #[default]
    Horizontal,
    /// Across the comp height.
    Vertical,
    /// Across the comp diagonal.
    Diagonal,
}

impl FilmMeasure {
    /// The comp dimension in pixels the film size spans (AE-CMP-418).
    pub fn dimension(self, width: f64, height: f64) -> f64 {
        match self {
            FilmMeasure::Horizontal => width,
            FilmMeasure::Vertical => height,
            FilmMeasure::Diagonal => width.hypot(height),
        }
    }
}

/// Zoom in pixels of a lens: `focal_length · dim / film_size` (focal length and
/// film size in the same unit, `dim` from [`FilmMeasure::dimension`]).
pub fn zoom_from_focal_length(focal_length: f64, film_size: f64, dim: f64) -> f64 {
    focal_length * dim / film_size
}

/// Focal length (in the film size's unit) of a zoom in pixels.
pub fn focal_length_from_zoom(zoom: f64, film_size: f64, dim: f64) -> f64 {
    zoom * film_size / dim
}

/// Angle of view in degrees across `dim` pixels: `2·atan(dim / (2·zoom))`.
pub fn angle_of_view(zoom: f64, dim: f64) -> f64 {
    2.0 * (dim / (2.0 * zoom)).atan().to_degrees()
}

/// Zoom in pixels for an angle of view in degrees across `dim` pixels.
pub fn zoom_from_angle_of_view(degrees: f64, dim: f64) -> f64 {
    dim / (2.0 * (degrees.to_radians() / 2.0).tan())
}

/// The aim rotation of a two-node camera or light (AE-ANI-022): turns local +z
/// towards `direction` without roll (yaw about y, then pitch about x), so the
/// camera's up stays towards −y. Identity for a zero direction; straight up or
/// down uses yaw 0.
pub fn look_at(direction: DVec3) -> DMat4 {
    if direction.length_squared() < 1e-24 {
        return DMat4::IDENTITY;
    }
    let horizontal = direction.x.hypot(direction.z);
    let yaw = if horizontal < 1e-12 {
        0.0
    } else {
        direction.x.atan2(direction.z)
    };
    let pitch = (-direction.y).atan2(horizontal);
    DMat4::from_rotation_y(yaw) * DMat4::from_rotation_x(pitch)
}

/// `Rx(x) · Ry(y) · Rz(z)` in degrees: Z applies first (AE-ANI-014).
pub fn xyz_rotation(degrees: DVec3) -> DMat4 {
    DMat4::from_rotation_x(degrees.x.to_radians())
        * DMat4::from_rotation_y(degrees.y.to_radians())
        * DMat4::from_rotation_z(degrees.z.to_radians())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: DVec2 = DVec2::new(1920.0, 1080.0);

    fn close(a: f64, b: f64, eps: f64) {
        assert!((a - b).abs() <= eps, "{a} != {b}");
    }

    fn map(h: &DMat3, p: DVec2) -> DVec2 {
        let v = *h * p.extend(1.0);
        v.truncate() / v.z
    }

    #[test]
    fn default_camera_is_identity_at_z0() {
        let cam = Camera::default_for(1920, 1080);
        close(cam.zoom, 2666.666_666_666_667, 1e-9);
        // A layer at z = 0 with an arbitrary 2D placement projects 1:1.
        let layer = DMat4::from_translation(DVec3::new(100.0, 50.0, 0.0))
            * DMat4::from_rotation_z(0.3)
            * DMat4::from_scale(DVec3::new(1.5, 0.5, 1.0));
        let h = cam.plane_homography(&layer, SIZE);
        for p in [
            DVec2::ZERO,
            DVec2::new(37.0, -12.0),
            DVec2::new(1000.0, 800.0),
        ] {
            let expect = layer.transform_point3(p.extend(0.0)).truncate();
            let got = map(&h, p);
            assert!((got - expect).length() < 1e-9, "{got} != {expect}");
        }
    }

    #[test]
    fn depth_scales_about_the_comp_centre() {
        let cam = Camera::default_for(1920, 1080);
        // At z = zoom the distance to the camera doubles: half size.
        let p = cam
            .project(DVec3::new(1960.0, 540.0, cam.zoom), SIZE)
            .unwrap();
        close(p.x, 960.0 + 500.0, 1e-9);
        close(p.y, 540.0, 1e-9);
        // Behind the camera: not visible.
        assert!(
            cam.project(DVec3::new(0.0, 0.0, -cam.zoom - 1.0), SIZE)
                .is_none()
        );
        // Round trip.
        let q = cam.unproject(p, 2.0 * cam.zoom, SIZE);
        assert!((q - DVec3::new(1960.0, 540.0, cam.zoom)).length() < 1e-9);
    }

    #[test]
    fn lens_presets_match_ae() {
        // AE-CMP-418 preset table at 1920×1080 (zoom, horizontal / vertical /
        // diagonal angle of view).
        let rows = [
            (15.0, 800.00, 100.39, 68.04, 108.02),
            (50.0, 2666.67, 39.60, 22.90, 44.89),
            (200.0, 10666.67, 10.29, 5.80, 11.79),
        ];
        for (focal, zoom, h, v, d) in rows {
            let z = zoom_from_focal_length(focal, PRESET_FILM_SIZE, 1920.0);
            close(z, zoom, 0.005);
            close(angle_of_view(z, 1920.0), h, 0.005);
            close(angle_of_view(z, 1080.0), v, 0.005);
            close(
                angle_of_view(z, FilmMeasure::Diagonal.dimension(1920.0, 1080.0)),
                d,
                0.005,
            );
            close(
                zoom_from_angle_of_view(angle_of_view(z, 1920.0), 1920.0),
                z,
                1e-6,
            );
            close(
                focal_length_from_zoom(z, PRESET_FILM_SIZE, 1920.0),
                focal,
                1e-9,
            );
        }
    }

    #[test]
    fn two_node_camera_aims_at_its_point_of_interest() {
        // Default placement: the aim is the identity.
        let cam = Camera::from_transform(
            DMat4::IDENTITY,
            DVec3::new(960.0, 540.0, -2666.67),
            Some(DVec3::new(960.0, 540.0, 0.0)),
            DVec3::ZERO,
            DVec3::ZERO,
            2666.67,
        );
        assert!(cam.world.abs_diff_eq(
            DMat4::from_translation(DVec3::new(960.0, 540.0, -2666.67)),
            1e-9
        ));
        // From the left, the point of interest lands in the comp centre.
        let poi = DVec3::new(500.0, 300.0, 200.0);
        let cam = Camera::from_transform(
            DMat4::IDENTITY,
            DVec3::new(-1500.0, -200.0, -900.0),
            Some(poi),
            DVec3::ZERO,
            DVec3::ZERO,
            1000.0,
        );
        let p = cam.project(poi, SIZE).unwrap();
        assert!((p - SIZE / 2.0).length() < 1e-9, "{p}");
        // No roll: the camera's x axis stays horizontal.
        close(cam.world.x_axis.y, 0.0, 1e-12);
    }

    #[test]
    fn camera_rotation_signs() {
        // A one-node camera turned +90° about y looks along +x: a point to its
        // right in comp space is straight ahead.
        let cam = Camera::from_transform(
            DMat4::IDENTITY,
            DVec3::new(960.0, 540.0, 0.0),
            None,
            DVec3::ZERO,
            DVec3::new(0.0, 90.0, 0.0),
            1000.0,
        );
        let p = cam.project(DVec3::new(2960.0, 540.0, 0.0), SIZE).unwrap();
        assert!((p - SIZE / 2.0).length() < 1e-9, "{p}");
        // Orientation and rotation compose as Or · Rot.
        let a = Camera::from_transform(
            DMat4::IDENTITY,
            DVec3::ZERO,
            None,
            DVec3::new(10.0, 20.0, 30.0),
            DVec3::new(40.0, 50.0, 60.0),
            1.0,
        );
        let expect =
            xyz_rotation(DVec3::new(10.0, 20.0, 30.0)) * xyz_rotation(DVec3::new(40.0, 50.0, 60.0));
        assert!(a.world.abs_diff_eq(expect, 1e-12));
    }

    #[test]
    fn matches_bae_anim_look_along() {
        // The aim is the same rotation bae-anim uses for its camera matrices.
        let d = DVec3::new(-3.0, 2.0, 5.0);
        let tf = bae_anim::Transform {
            position: DVec3::new(1.0, 2.0, 3.0),
            auto_orient: bae_anim::AutoOrientRotation::YawPitch {
                yaw: d.x.atan2(d.z).to_degrees(),
                pitch: (-d.y).atan2(d.x.hypot(d.z)).to_degrees(),
            },
            ..Default::default()
        };
        let ours = DMat4::from_translation(tf.position) * look_at(d);
        assert!(ours.abs_diff_eq(tf.camera_matrix(), 1e-12));
    }
}
