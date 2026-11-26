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
