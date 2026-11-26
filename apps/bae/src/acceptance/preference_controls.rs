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

#[test]
fn transmit_stays_disabled_even_with_a_saved_enabled_value() {
    let mut h = gui();
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("video.enable_mercury_transmit", true);
    h.state().prefs.set(prefs).unwrap();
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::settings::ACTION,
        serde_json::json!({"page": "video"}),
    ));
    settle(&mut h);
    for label in [
        "Enable Mercury Transmit",
        "Disable Video Output When in the Background",
    ] {
        assert!(h.get_by_label(label).accesskit_node().is_disabled());
    }
    assert_eq!(
        h.get_by_label("Enable Mercury Transmit")
            .accesskit_node()
            .toggled(),
        Some(egui::accesskit::Toggled::False)
    );
    assert!(
        !h.get_by_label(
            "Enable Extended Display Range / High Dynamic Range Previews (When Available)"
        )
        .accesskit_node()
        .is_disabled()
    );
    h.get_by_label("Cancel").click();
    settle(&mut h);
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::settings::ACTION,
        serde_json::json!({"page": "export"}),
    ));
    settle(&mut h);
    assert!(
        h.get_by_label("Preview Output During Render Queue Output")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn panel_coachmarks_can_be_dismissed_and_reset() {
    let mut h = gui();
    h.state_mut().workspace.focus_panel("project");
    settle(&mut h);
    let tip = "Drop files here to import footage.";
    assert!(find_text(&h, tip).is_none());
    let open_appearance = |h: &mut Ui| {
        h.state_mut().workspace.show_settings(Some("appearance"));
        settle(h);
    };
    open_appearance(&mut h);
    h.get_by_label("Show Coachmarks").click();
    settle(&mut h);
    h.get_by_label("OK").click();
    settle(&mut h);
    let project = find_text(&h, "Project").unwrap().center();
    click_at(&mut h, project);
    settle(&mut h);
    assert!(find_text(&h, tip).is_some());
    h.get_by_label("Got it").click();
    settle(&mut h);
    assert!(find_text(&h, tip).is_none());
    assert!(
        h.state_mut()
            .workspace
            .state()
            .dismissed_coachmarks
            .contains("project")
    );
    open_appearance(&mut h);
    h.get_by_label("Reset Coachmarks").click();
    settle(&mut h);
    assert!(h.state_mut().workspace.take_settings_requests().is_empty());
    h.get_by_label("OK").click();
    settle(&mut h);
    let project = find_text(&h, "Project").unwrap().center();
    click_at(&mut h, project);
    settle(&mut h);
    assert!(find_text(&h, tip).is_some());
}
