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
    // A trackpad scroll pans, the magnification stays.
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
    assert!((v2.scale - v.scale).abs() < 1e-6);
    assert!(
        (v2.origin - o - vec2(10.0, -20.0)).length() < 0.5,
        "{:?}",
        v2.origin - o
    );
}
