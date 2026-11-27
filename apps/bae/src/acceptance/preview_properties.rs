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
    assert!(find_text(&h, "Full Screen").is_none());
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(!session.lock().is_playing());
    assert!(h.query_by_label("Full Screen").is_some());
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

#[test]
fn properties_font_size_previews_before_committing_the_drag() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.text"]);
    let session = h.state().session();
    let layer = session.lock().selected_layers[0].clone();
    h.state_mut().dispatch(Invocation::with_params(
        "text.set_text",
        serde_json::json!({"layer":layer,"text":"Title","time":0}),
    ));
    to_front(&mut h, "properties");
    h.state_mut().dispatch(Invocation::new(ids::MAXIMIZE_FRAME));
    settle(&mut h);
    let at = h
        .query_by(|n| {
            n.role() == egui::accesskit::Role::SpinButton
                && n.label().as_deref() == Some("Font Size")
        })
        .unwrap()
        .rect()
        .center();
    let stored = |h: &Ui| {
        let view = h.state().stage().prim(&layer).unwrap();
        let Some(bae_model::Value::TextDocument(doc)) =
            bae_motion::query::static_value(&view, "text:source")
        else {
            panic!("missing text document");
        };
        doc.style_at(0).font_size
    };
    let before = stored(&h);
    let button = |pos, pressed| Event::PointerButton {
        pos,
        pressed,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
    };
    let end = at + egui::vec2(28.0, 0.0);
    send(&mut h, Event::PointerMoved(at));
    send(&mut h, button(at, true));
    send(&mut h, Event::PointerMoved(end));
    settle(&mut h);
    let live = {
        let s = session.lock();
        let Some(bae_model::Value::TextDocument(doc)) = s
            .scrub
            .as_ref()
            .and_then(|v| v.value_for(&layer, "text:source"))
        else {
            panic!("the viewer did not receive the live edit");
        };
        doc.style_at(0).font_size
    };
    assert!(live > before);
    assert_eq!(stored(&h), before);
    send(&mut h, button(end, false));
    settle(&mut h);
    assert_eq!(stored(&h), live);
}

/// #745: Space plays and stops without moving the focus off the Timeline, so
/// its own keys ([ ] Delete, markers) keep working (AE).
#[test]
fn space_playback_keeps_the_timeline_focused() {
    let mut h = gui();
    new_comp(&mut h);
    to_front(&mut h, "timeline");
    settle(&mut h);
    assert_eq!(
        h.state().workspace.focused_kind().as_deref(),
        Some("timeline")
    );
    key(&mut h, Modifiers::NONE, Key::Space);
    assert!(h.state().session().lock().is_playing());
    key(&mut h, Modifiers::NONE, Key::Space);
    assert_eq!(
        h.state().workspace.focused_kind().as_deref(),
        Some("timeline")
    );
}
