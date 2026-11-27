//! Rulers, guides, grids and safe areas (View menu).

use super::*;

/// View ▸ Show Rulers; a guide dragged out of the top ruler; Snap to Guides;
/// a guide dragged back onto the ruler goes away (each one undo step).
#[test]
fn rulers_guides_and_snapping_to_guides() {
    let mut h = H::new();
    let solid = h.solid([100.0, 100.0], [320.0, 180.0]);
    tool_action(&h, "motion.view.show_rulers", json!({}));
    assert!(h.session().lock().view_options.rulers);
    h.frame(Vec::new(), Modifiers::NONE);
    // Out of the top ruler (16 points under the Composition Navigator) to
    // comp y ≈ 100.
    let ruler = crate::viewer::navigator::HEIGHT + 8.0;
    let to = h.screen(320.0, 100.0);
    let from = egui::pos2(to.x, ruler);
    h.move_to(from, Modifiers::NONE, Vec::new());
    h.pointer_button(from, true, Modifiers::NONE);
    for i in 1..=4 {
        h.move_to(
            from + (to - from) * (i as f32 / 4.0),
            Modifiers::NONE,
            Vec::new(),
        );
    }
    let r = h.pointer_button(to, false, Modifiers::NONE);
    h.apply(&r);
    let g = crate::viewer::guides::guides_of(&h.stage, &h.comp);
    assert_eq!(g.len(), 1);
    assert!(!g[0].vertical && (g[0].at - 100.0).abs() <= 1.0, "{g:?}");
    // Snap to Guides: grabbed near its top edge (y 130 is the nearest feature),
    // dragged up 26 px to 104, the solid's top snaps to the guide.
    tool_action(&h, "motion.view.snap_to_guides", json!({}));
    h.select(&[&solid]);
    h.frame(Vec::new(), Modifiers::NONE);
    let r = h.drag((320.0, 140.0), (320.0, 114.0), Modifiers::NONE);
    h.apply(&r);
    let p = h.v3(&solid, "transform:position");
    assert!((p[1] - (g[0].at + 50.0)).abs() < 1e-6, "{p:?}");
    // Back onto the ruler: removed.
    h.session().lock().selected_layers.clear();
    h.frame(Vec::new(), Modifiers::NONE);
    let on = h.screen(100.0, g[0].at);
    h.move_to(on, Modifiers::NONE, Vec::new());
    h.pointer_button(on, true, Modifiers::NONE);
    for i in 1..=4 {
        h.move_to(
            on + (egui::pos2(on.x, ruler) - on) * (i as f32 / 4.0),
            Modifiers::NONE,
            Vec::new(),
        );
    }
    let r = h.pointer_button(egui::pos2(on.x, ruler), false, Modifiers::NONE);
    h.apply(&r);
    assert!(crate::viewer::guides::guides_of(&h.stage, &h.comp).is_empty());
}

/// View ▸ Export / Import Guides use AE's guide file: orientationType 0
/// horizontal / 1 vertical, positionType 0 pixels / 1 percent.
#[test]
fn guide_files_round_trip_like_ae() {
    use crate::viewer::guides::{Guide, guides_json, parse_guides};
    let h = H::new();
    let comp = h.session().lock().active_comp.clone().unwrap();
    let info = bae_motion::query::comp_info(&h.stage, &comp).unwrap();
    let guides = vec![
        Guide {
            vertical: true,
            at: 100.0,
        },
        Guide {
            vertical: false,
            at: 50.5,
        },
    ];
    let text = guides_json(&guides).to_string();
    assert_eq!(parse_guides(&text, &info), Some(guides));
    let percent = r#"{"guides":[{"orientationType":1,"positionType":1,"position":50}]}"#;
    let half = f64::from(info.width) / 2.0;
    assert_eq!(
        parse_guides(percent, &info),
        Some(vec![Guide {
            vertical: true,
            at: half
        }])
    );
    assert_eq!(parse_guides("not json", &info), None);
}
