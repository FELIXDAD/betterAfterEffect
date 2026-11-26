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
    pub cycle_mask_colors: bool,
    pub center_shape_anchor: bool,
    pub show_precomp_bounds: bool,
    pub checker_size: f32,
    pub checker_colors: [Color32; 2],
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
            cycle_mask_colors: p.bool("appearance.cycle_mask_colors"),
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
