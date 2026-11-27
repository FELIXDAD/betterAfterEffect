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
        sync::Arc,
        time::{Duration, Instant},
    };
    let Some(gpu) = shared_context() else { return };
    let mut h = H::new();
    // At 999 fps every frame renders slower than it lasts; with free reads it is stored.
    h.comp = path(
        &h.run(
            "comp.create",
            json!({"width": 640, "height": 360, "fps": 999.0}),
        )["path"],
    );
    h.session().lock().open_comp(&h.comp);
    let dir = tempfile::tempdir().unwrap();
    let mut p = bae_prefs::Preferences::default();
    p.set(
        "disk.disk_cache_folder",
        dir.path().to_string_lossy().as_ref(),
    );
    p.set(bae_prefs::keys::DISK_LIMIT_GB, 1);
    let prefs = Arc::new(bae_prefs::PrefsStore::new(p));
    let cache = crate::disk_frames::DiskFrames::new(dir.path().to_path_buf(), &prefs);
    cache.run(&gpu).unwrap();
    cache.assume_free_reads();
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
                cache.reads() > 0
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
