//! Preferences the viewer tools read (D's settings, `Arc<PrefsStore>`):
//! Appearance > Path Point and Handle Size, Use Label Color for Layer Handles
//! and Paths, Cycle Mask Colors; Composition > Center Anchor Point in New Shape
//! Layers. AE defaults without a store (tests).

use bae_prefs::{Preferences, PrefsStore, keys};
use egui::Color32;

/// Handle and path color when label colors are off (AE's 待核).
const PLAIN_HANDLES: Color32 = Color32::from_rgb(0x4d, 0x9c, 0xff);

/// The viewer's settings for one frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ViewerPrefs {
    /// Path point and handle size in points.
    pub point_size: f32,
    pub label_handles: bool,
    pub contrasting_masks: bool,
    pub center_shape_anchor: bool,
    pub show_precomp_bounds: bool,
    pub checker_size: f32,
    pub checker_colors: [Color32; 2],
    /// Signed dolly steps per wheel notch. Zero leaves the wheel on composition zoom.
    pub dolly_wheel: f32,
    pub reverse_dolly_drag: bool,
    pub alt_camera_navigation: bool,
    pub camera_point: CameraPoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CameraPoint {
    Hidden,
    Indicator,
    Directional,
}

impl Default for ViewerPrefs {
    fn default() -> Self {
        Self::from(&Preferences::default())
    }
}

impl ViewerPrefs {
    pub fn from(p: &Preferences) -> Self {
        ViewerPrefs {
            point_size: p.int(keys::PATH_POINT_SIZE).clamp(4, 15) as f32,
            label_handles: p.bool(keys::LABEL_HANDLES),
            contrasting_masks: p.bool("appearance.cycle_mask_colors")
                && p.bool("appearance.use_contrasting_color_for_mask_path"),
            center_shape_anchor: p.bool(keys::CENTER_SHAPE_ANCHOR),
            show_precomp_bounds: p.bool("composition.show_bounding_box_precomp_layers"),
            checker_size: match p.str("grids_guides.transparency_grid_size") {
                "small" => 4.0,
                "large" => 16.0,
                _ => 8.0,
            },
            checker_colors: [keys::TRANSPARENCY_FIRST, keys::TRANSPARENCY_SECOND].map(|key| {
                let [r, g, b] = p.color(key);
                Color32::from_rgb(r, g, b)
            }),
            dolly_wheel: match p.str("3d.mouse_wheel_behavior") {
                "dolly_camera" => 1.0,
                "reverse_dolly" => -1.0,
                _ => 0.0,
            },
            reverse_dolly_drag: p.str("3d.drag_direction") == "inverted",
            alt_camera_navigation: p.bool("3d.override_alt_for_camera_navigation"),
            camera_point: match p.str("3d.camera_point_of_interest") {
                "none" => CameraPoint::Hidden,
                "directional_indicator" => CameraPoint::Directional,
                _ => CameraPoint::Indicator,
            },
        }
    }

    pub fn current(resources: &bae_app::Resources) -> Self {
        match resources.get::<std::sync::Arc<PrefsStore>>() {
            Some(store) => Self::from(&store.display()),
            None => Self::from(&Preferences::default()),
        }
    }

    /// The color of a layer's handles and paths: its label color, or one color
    /// for all when the preference is off.
    pub fn handle_color(&self, label: Color32) -> Color32 {
        if self.label_handles {
            label
        } else {
            PLAIN_HANDLES
        }
    }
}
