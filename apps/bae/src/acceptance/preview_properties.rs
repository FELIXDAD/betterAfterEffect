//! Preview shortcuts and contextual properties through the application's workbench.

use super::*;
use egui_kittest::kittest::Queryable as _;

#[test]
fn preview_shortcuts_use_their_profiles_and_restore_the_workspace() {
    let mut h = gui();
    new_comp(&mut h);
    to_front(&mut h, "preview");
    h.state_mut().dispatch(Invocation::new(ids::MAXIMIZE_FRAME));
    settle(&mut h);
    let toggle = h.get_by_label("Full Screen").rect();
    click_at(&mut h, toggle.center());
    key(&mut h, Modifiers::NONE, Key::Space);
    let session = h.state().session();
    assert!(session.lock().fullscreen_preview());
    assert!(find_text(&h, "Effects & Presets").is_none());
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(!session.lock().is_playing());
    let pad = bae_ui::keymap::NUMPAD
        .iter()
        .find(|(name, ..)| *name == "Pad0")
        .unwrap()
        .1;
    key(&mut h, Modifiers::SHIFT, pad);
    {
        let s = session.lock();
        assert!(s.is_playing());
        assert!(s.preview_options().cache_before);
        assert_eq!(s.preview_options().skip, 1);
        assert!(!s.fullscreen_preview());
    }
    key(&mut h, Modifiers::SHIFT, pad);
    assert!(!session.lock().is_playing());
    key(&mut h, Modifiers::NONE, pad);
    assert_eq!(session.lock().preview_options().skip, 0);
    assert!(session.lock().is_playing());
}

#[test]
fn shape_properties_offer_geometry_and_the_add_menu() {
    let mut h = gui();
    comp_with(&mut h, &["shape:rect"]);
    to_front(&mut h, "properties");
    assert!(find_text(&h, "Quick Actions").is_some());
    assert!(!find_texts(&h, |t| t.contains("Roundness")).is_empty());
    let add = h.get_by_label("Add").rect();
    click_at(&mut h, add.center());
    let ellipse = h.get_by_label("Ellipse").rect();
    click_at(&mut h, ellipse.center());
    assert!(h.state().stage().traverse().into_iter().any(|p| {
        h.state()
            .stage()
            .prim(&p)
            .and_then(|v| v.prim_type().cloned())
            == Some(bae_model::PrimType::ELLIPSE)
    }));
}
