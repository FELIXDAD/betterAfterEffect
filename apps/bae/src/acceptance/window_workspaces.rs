use super::*;

#[test]
fn window_workspaces_switch_assign_and_save() {
    let mut h = gui();
    for (menu, id) in [
        ("all_panels", "ae_all_panels"),
        ("animation", "ae_animation"),
        ("essential_graphics", "ae_essential_graphics"),
        ("effects", "ae_effects"),
        ("small_screen", "ae_small_screen"),
        ("standard", "ae_standard"),
        ("undocked_panels", "ae_undocked_panels"),
        ("learn", "ae_learn"),
        ("libraries", "ae_libraries"),
        ("minimal", "ae_minimal"),
        ("paint", "ae_paint"),
        ("text", "ae_text"),
        ("motion_tracking", "ae_motion_tracking"),
        ("review", "review"),
        ("color", "ae_color"),
        ("default", "motion"),
    ] {
        assert!(h.state_mut().menu(&format!("menu.window.workspace.{menu}")));
        settle(&mut h);
        assert_eq!(h.state().workspace.active_workspace(), id);
    }
    for (keycode, id) in [
        (Key::F11, "review"),
        (Key::F12, "ae_learn"),
        (Key::F10, "motion"),
    ] {
        key(&mut h, Modifiers::SHIFT, keycode);
        settle(&mut h);
        assert_eq!(h.state().workspace.active_workspace(), id);
    }
    assert!(
        h.state_mut()
            .menu("menu.window.workspace.save_as_new_workspace")
    );
    settle(&mut h);
    type_text(&mut h, "My layout");
    key(&mut h, Modifiers::NONE, Key::Enter);
    settle(&mut h);
    assert_eq!(h.state().workspace.active_workspace_title(), "My layout");
    let saved = h.state().workspace.active_workspace().to_owned();
    assert!(
        h.state_mut()
            .menu("menu.window.assign_shortcut_to_workspace.shift_f11")
    );
    settle(&mut h);
    key(&mut h, Modifiers::SHIFT, Key::F10);
    settle(&mut h);
    assert_eq!(h.state().workspace.active_workspace(), "motion");
    key(&mut h, Modifiers::SHIFT, Key::F11);
    settle(&mut h);
    assert_eq!(h.state().workspace.active_workspace(), saved);
    assert!(
        h.state_mut()
            .menu(&format!("menu.window.workspace.{saved}"))
    );
}

#[test]
fn window_frame_io_opens_closes_and_restores_the_signed_out_panel() {
    let mut h = gui();
    let id = "menu.window.review_with_frame_io";
    for showing in [true, false, true] {
        assert!(h.state_mut().menu(id));
        settle(&mut h);
        assert_eq!(
            h.state()
                .workspace
                .open_panels()
                .iter()
                .any(|p| p.0 == "frame_io"),
            showing
        );
        let notice = bae_ui::tr("Sign in with your Adobe ID to review and share your work.");
        assert_eq!(
            painted_texts(&h).iter().any(|text| text == notice),
            showing,
            "visible text: {:?}",
            painted_texts(&h)
        );
    }
}
