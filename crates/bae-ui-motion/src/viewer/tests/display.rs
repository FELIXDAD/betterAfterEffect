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

#[test]
fn zoom_quality_changes_the_displayed_gpu_texture_in_playback_and_at_rest() {
    use bae_gpu::{testing::shared_context, viewport::ViewportTextureRegistry, wgpu::FilterMode};
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    let Some(gpu) = shared_context() else {
        return;
    };
    let mut h = H::new();
    let registry = Arc::new(ViewportTextureRegistry::new());
    let store = Arc::new(bae_prefs::PrefsStore::default());
    h.resources.insert(gpu);
    h.resources.insert(registry.clone());
    h.resources.insert(store.clone());
    h.viewer = CompositionViewer::new(None);
    h.layer("solid");
    let show = |h: &mut H, setting: &str, expected| {
        let mut p = (*store.current()).clone();
        p.set("previews.zoom_quality", setting);
        store.set(p).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            for slot in registry.snapshot() {
                if slot.texture.is_some() && slot.display_handle.is_none() {
                    registry.set_display_handle(slot.id, Some(slot.id.raw()));
                }
            }
            h.frame(Vec::new(), Modifiers::NONE);
            if h.shapes.iter().any(|s| {
                let egui::Shape::Mesh(m) = &s.shape else {
                    return false;
                };
                let egui::TextureId::User(handle) = m.texture_id else {
                    return false;
                };
                registry.snapshot().iter().any(|s| {
                    s.display_handle == Some(handle)
                        && s.texture.as_ref().is_some_and(|t| t.filter == expected)
                })
            }) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "viewer did not display {expected:?} for {setting}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    show(&mut h, "faster", FilterMode::Nearest);
    show(
        &mut h,
        "more_accurate_except_ram_preview",
        FilterMode::Linear,
    );
    let info = query::comp_info(&h.stage, &h.comp).unwrap();
    h.session().lock().play(&info);
    show(
        &mut h,
        "more_accurate_except_ram_preview",
        FilterMode::Nearest,
    );
    show(&mut h, "more_accurate", FilterMode::Linear);
}
