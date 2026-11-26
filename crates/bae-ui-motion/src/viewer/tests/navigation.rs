//! Mouse wheel zoom and trackpad pan.

use super::*;

#[test]
fn mouse_wheel_zooms_around_the_pointer_and_trackpads_pan() {
    let mut h = H::new();
    let at = h.screen(100.0, 100.0);
    let wheel = |unit, d: egui::Vec2, m: Modifiers| Event::MouseWheel {
        unit,
        delta: d,
        phase: egui::TouchPhase::Move,
        modifiers: m,
    };
    let before = h.viewer.last_view.unwrap().scale;
    h.move_to(
        at,
        Modifiers::NONE,
        vec![wheel(
            egui::MouseWheelUnit::Line,
            vec2(0.0, 1.0),
            Modifiers::NONE,
        )],
    );
    h.frame(Vec::new(), Modifiers::NONE);
    let v = h.viewer.last_view.unwrap();
    assert!(
        (v.scale / before - 1.125).abs() < 1e-3,
        "{} → {}",
        before,
        v.scale
    );
    // The comp point under the pointer stays there.
    let p = v.to_screen(DVec2::new(100.0, 100.0));
    assert!(p.distance(at) < 0.5, "{p:?} {at:?}");
    // Pixel-precise scrolling: a trackpad on macOS pans (AE 25.3+), a
    // high-resolution wheel elsewhere zooms (#571).
    let o = v.origin;
    h.move_to(
        at,
        Modifiers::NONE,
        vec![wheel(
            egui::MouseWheelUnit::Point,
            vec2(10.0, -20.0),
            Modifiers::NONE,
        )],
    );
    h.frame(Vec::new(), Modifiers::NONE);
    let v2 = h.viewer.last_view.unwrap();
    if cfg!(target_os = "macos") {
        assert!((v2.scale - v.scale).abs() < 1e-6);
        assert!(
            (v2.origin - o - vec2(10.0, -20.0)).length() < 0.5,
            "{:?}",
            v2.origin - o
        );
    } else {
        assert!(v2.scale < v.scale, "zoomed out: {} → {}", v.scale, v2.scale);
    }
}

/// Zoom In / Out (. and ,) step by powers of two like AE (no 33.3 %, #570),
/// and / sets 100 % (#569).
#[test]
fn zoom_keys_step_by_powers_of_two_and_slash_is_100() {
    let mut h = H::new();
    h.viewer.zoom = Some(1.0);
    h.frame(Vec::new(), Modifiers::NONE);
    let mag = |h: &H| h.viewer.last_view.unwrap().scale * h.ctx.pixels_per_point();
    let mut seen = Vec::new();
    for _ in 0..3 {
        assert!(h.action("viewer.zoom_out"));
        h.frame(Vec::new(), Modifiers::NONE);
        seen.push((mag(&h) * 1000.0).round() / 1000.0);
    }
    assert_eq!(seen, [0.5, 0.25, 0.125]);
    assert!(h.action(crate::viewer::ZOOM_100));
    h.frame(Vec::new(), Modifiers::NONE);
    assert!((mag(&h) - 1.0).abs() < 1e-4, "{}", mag(&h));
}
