//! The Layer panel's time ruler: dragging an end trims the layer.

use super::Harness;
use crate::layer_panel::LayerPanel;
use bae_model::Time;
use bae_motion::query;
use egui::{Event, PointerButton, pos2};

#[test]
fn dragging_an_end_of_the_layer_ruler_trims_the_layer() {
    let mut h = Harness::new();
    let (comp, solid, _shape) = h.comp_with_layers();
    {
        let s = h.session();
        let mut s = s.lock();
        s.open_comp(&comp);
        s.layer_panel = Some(solid.clone());
    }
    let mut panel = LayerPanel::new(None);
    h.frame("layer", &mut panel, Vec::new());
    let info = query::comp_info(&h.stage, &comp).unwrap();
    // The ruler is the 22 points above the 28-point bar (window 1280 × 800);
    // its track starts 10 points in.
    let y = 800.0 - 28.0 - 11.0;
    let (left, width) = (10.0, 1260.0);
    let at = |t: f64| left + (t / info.duration.as_seconds()) as f32 * width;
    let from = pos2(at(0.0), y);
    let to = pos2(at(info.duration.as_seconds() / 4.0), y);
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.frame("layer", &mut panel, vec![Event::PointerMoved(from)]);
    h.frame("layer", &mut panel, vec![button(from, true)]);
    for i in 1..=4 {
        let p = from + (to - from) * (i as f32 / 4.0);
        h.frame("layer", &mut panel, vec![Event::PointerMoved(p)]);
    }
    let r = h.frame("layer", &mut panel, vec![button(to, false)]);
    assert_eq!(r.invocations.len(), 1, "one undo step: {:?}", r.invocations);
    h.apply(r);
    let t = query::layer_timing(&h.stage, &solid);
    let quarter = Time::from_seconds(info.duration.as_seconds() / 4.0);
    let frame = info.frame_duration().as_seconds();
    assert!(
        (t.in_point.as_seconds() - quarter.as_seconds()).abs() <= frame,
        "{:?}",
        t.in_point
    );
}

/// A mouse-wheel notch over the picture zooms in by AE's step (×9/8).
#[test]
fn the_wheel_zooms_the_layer_picture() {
    let mut h = Harness::new();
    let (comp, solid, _shape) = h.comp_with_layers();
    {
        let s = h.session();
        let mut s = s.lock();
        s.open_comp(&comp);
        s.layer_panel = Some(solid.clone());
    }
    let mut panel = LayerPanel::new(None);
    h.frame("layer", &mut panel, Vec::new());
    assert_eq!(panel.zoom, None, "Fit");
    let at = pos2(640.0, 300.0);
    h.frame("layer", &mut panel, vec![Event::PointerMoved(at)]);
    h.frame(
        "layer",
        &mut panel,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 1.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        }],
    );
    // The 640 × 360 solid fits at 100 % (the window is larger).
    let zoom = panel.zoom.expect("zoomed");
    assert!((zoom - 9.0 / 8.0).abs() < 1e-3, "{zoom}");
}
