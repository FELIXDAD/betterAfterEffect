//! Show Channel, Exposure, Region of Interest and snapshots.

use super::*;

/// Show Channel shortcuts toggle back to RGB (Alt+Shift+4 is RGB Straight);
/// with Region of Interest on and no region, a drag draws one and the tools
/// leave it alone.
#[test]
fn show_channel_toggles_and_a_drag_draws_the_region_of_interest() {
    use crate::viewer::display::SHOW_CHANNEL;
    let mut h = H::new();
    h.action_with(SHOW_CHANNEL, json!({"channel": "red", "colorize": false}));
    assert_eq!(h.viewer.display.channel, 1);
    h.action_with(SHOW_CHANNEL, json!({"channel": "red", "colorize": false}));
    assert_eq!(h.viewer.display.channel, 0, "again: RGB");
    h.action_with(SHOW_CHANNEL, json!({"channel": "alpha", "colorize": true}));
    assert_eq!(h.viewer.display.channel, 5, "RGB Straight");
    h.viewer.display.roi_on = true;
    h.frame(Vec::new(), Modifiers::NONE);
    let r = h.drag((100.0, 50.0), (300.0, 200.0), Modifiers::NONE);
    assert!(r.invocations.is_empty(), "{:?}", r.invocations);
    assert_eq!(
        h.viewer.display.roi(),
        Some(bae_render::IRect::new(100, 50, 300, 200))
    );
}

/// Fast Previews (Cmd/Ctrl+Alt+1, 2, 5) and View ▸ Show Layer Controls.
#[test]
fn fast_previews_modes_and_show_layer_controls() {
    use crate::viewer::display::{FAST_PREVIEWS, Fast};
    let mut h = H::new();
    h.action_with(FAST_PREVIEWS, json!({"mode": "wireframe"}));
    assert_eq!(h.viewer.display.fast, Fast::Wireframe);
    h.action_with(FAST_PREVIEWS, json!({"mode": "draft_3d"}));
    assert_eq!(
        h.viewer.display.fast,
        Fast::Wireframe,
        "Draft 3D is not a mode here"
    );
    h.action_with(FAST_PREVIEWS, json!({"mode": "adaptive_resolution"}));
    assert_eq!(h.viewer.display.fast, Fast::Adaptive);
    assert!(h.session().lock().view_options.layer_controls);
    tool_action(&h, "motion.view.show_layer_controls", json!({}));
    assert!(!h.session().lock().view_options.layer_controls);
    // The tools still work with the controls hidden.
    let a = h.solid([100.0, 100.0], [320.0, 180.0]);
    h.select(&[&a]);
    h.frame(Vec::new(), Modifiers::NONE);
    let r = h.drag((320.0, 180.0), (340.0, 180.0), Modifiers::NONE);
    h.apply(&r);
    assert!(close3(h.v3(&a, "transform:position"), [340.0, 180.0, 0.0]));
}
