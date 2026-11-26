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
fn a_new_composition_viewer_can_present_previews_from_disk() {
    use bae_gpu::{WarmUp, testing::shared_context, viewport::ViewportTextureRegistry};
    use std::{
        sync::{Arc, atomic::Ordering},
        time::{Duration, Instant},
    };
    let Some(gpu) = shared_context() else { return };
    let mut h = H::new();
    let dir = tempfile::tempdir().unwrap();
    let mut p = bae_prefs::Preferences::default();
    p.set(
        "disk.disk_cache_folder",
        dir.path().to_string_lossy().as_ref(),
    );
    let prefs = Arc::new(bae_prefs::PrefsStore::new(p));
    let cache = crate::disk_frames::DiskFrames::new(dir.path().to_path_buf(), &prefs);
    cache.run(&gpu).unwrap();
    let registry = Arc::new(ViewportTextureRegistry::new());
    h.resources.insert(registry.clone());
    h.resources.insert(prefs);
    h.resources.insert(gpu);
    h.resources.insert(cache.clone());
    h.layer("solid");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !cache.enabled() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    for reopen in [false, true] {
        h.viewer = CompositionViewer::new(None);
        loop {
            for slot in registry.snapshot() {
                if slot.texture.is_some() && slot.display_handle.is_none() {
                    registry.set_display_handle(slot.id, Some(slot.id.raw()));
                }
            }
            h.frame(Vec::new(), Modifiers::NONE);
            let presented=h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Mesh(m) if matches!(m.texture_id,egui::TextureId::User(_))));
            let ready = if reopen {
                cache.restored.load(Ordering::Acquire) > 0
            } else {
                std::fs::read_dir(dir.path().join("BAE Preview Frames"))
                    .unwrap()
                    .flatten()
                    .any(|e| e.path().extension().is_some_and(|s| s == "bpf"))
            };
            if ready && presented {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "disk viewer reopen={reopen}, ready={ready}, presented={presented}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
