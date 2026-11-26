//! Preferences pages through the real workbench, including disabled controls.

use egui_kittest::kittest::{NodeT as _, Queryable as _};

use super::*;

#[test]
fn grid_drag_previews_the_spacing_before_release_and_cancel_restores_it() {
    use bae_prefs::keys::GRID_SPACING;
    let mut h = gui();
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::settings::ACTION,
        serde_json::json!({"page": "grids_guides"}),
    ));
    settle(&mut h);
    let at = h
        .query_by(|n| {
            n.role() == egui::accesskit::Role::SpinButton
                && n.label().as_deref() == Some("Gridline Every:")
        })
        .unwrap()
        .rect()
        .center();
    let button = |pos, pressed| Event::PointerButton {
        pos,
        pressed,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
    };
    let end = at + egui::vec2(15.0, 0.0);
    send(&mut h, Event::PointerMoved(at));
    send(&mut h, button(at, true));
    send(&mut h, Event::PointerMoved(end));
    settle(&mut h);
    let spacing = |h: &Ui| bae_ui::settings::GridGuides::current(h.state().app.resources()).spacing;
    assert!(spacing(&h) > 80.0);
    assert_eq!(h.state().prefs.current().int(GRID_SPACING), 80);
    send(&mut h, button(end, false));
    h.get_by_label("Cancel").click();
    settle(&mut h);
    assert_eq!(spacing(&h), 80.0);
}

#[test]
fn reset_all_requires_confirmation_then_dialog_commit() {
    let mut h = gui();
    let open = |h: &mut Ui, page: &str| {
        h.state_mut().dispatch(Invocation::with_params(
            bae_ui::settings::ACTION,
            serde_json::json!({"page": page}),
        ));
        settle(h);
    };
    let anchor = bae_prefs::keys::CENTER_SHAPE_ANCHOR;
    let mut p = (*h.state().prefs.current()).clone();
    p.set(anchor, true);
    h.state().prefs.set(p).unwrap();
    for commit in [false, true] {
        open(&mut h, "startup_repair");
        h.get_by_label("Reset All Preferences to Default...")
            .click();
        settle(&mut h);
        key(&mut h, Modifiers::NONE, Key::Enter);
        assert!(h.state().prefs.current().bool(anchor));
        h.get_by_label(if commit { "OK" } else { "Cancel" }).click();
        settle(&mut h);
        assert_eq!(h.state().prefs.current().bool(anchor), !commit);
    }
}
