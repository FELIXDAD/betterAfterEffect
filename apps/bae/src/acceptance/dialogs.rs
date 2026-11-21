//! Motion dialogs driven like a user: Composition Settings for New Composition
//! (Cmd/Ctrl+N) and for the open comp (Cmd/Ctrl+K). Controls are found by
//! their accesskit labels; values are typed.

use egui_kittest::kittest::Queryable as _;

use super::*;

/// Click a hot number / field by its label, select its text and type `text`;
/// Tab commits it (the dialog stays open).
fn type_into(h: &mut Ui, label: &str, text: &str) {
    let rect = h.get_by_label(label).rect();
    click_at(h, rect.center());
    key(h, Modifiers::COMMAND, Key::A);
    type_text(h, text);
    key(h, Modifiers::NONE, Key::Tab);
}

fn click_label(h: &mut Ui, label: &str) {
    let rect = h.get_by_label(label).rect();
    click_at(h, rect.center());
}

#[test]
fn new_composition_asks_for_settings_first() {
    let mut h = gui();
    key(&mut h, Modifiers::COMMAND, Key::N);
    assert!(dialog_open(&h), "Cmd+N opened no dialog");
    assert!(find_text(&h, "Composition Settings").is_some());
    assert!(h.state().active_comp().is_none(), "created before OK");
    // The name is selected: typing replaces it.
    type_text(&mut h, "Main");
    type_into(&mut h, "Width", "1280");
    type_into(&mut h, "Height", "720");
    // OK and Cancel sit where the platform puts them.
    let ok = h.get_by_label("OK").rect();
    let cancel = h.get_by_label("Cancel").rect();
    if cfg!(target_os = "macos") {
        assert!(ok.center().x > cancel.center().x, "macOS: Cancel, OK");
    } else {
        assert!(ok.center().x < cancel.center().x, "Windows: OK, Cancel");
    }
    click_label(&mut h, "OK");
    assert!(!dialog_open(&h));
    let comp = h.state().active_comp().expect("OK opened the comp");
    let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
    assert_eq!(info.name, "Main");
    assert_eq!((info.width, info.height), (1280, 720));
    // One undo step removes it again.
    let (undo, _) = h.state().doc.undo_state();
    assert_eq!(undo.as_deref(), Some("New Composition"));
}

#[test]
fn cancel_escape_and_blocked_shortcuts() {
    let mut h = gui();
    key(&mut h, Modifiers::COMMAND, Key::N);
    click_label(&mut h, "Cancel");
    assert!(!dialog_open(&h));
    assert!(bae_motion::query::comps(h.state().stage()).is_empty());
    // While it is open, shortcuts and menu commands do not reach the project.
    key(&mut h, Modifiers::COMMAND, Key::N);
    assert!(h.state().workspace.modal_open(h.state().app.resources()));
    key(&mut h, Modifiers::COMMAND, Key::Y);
    key(&mut h, Modifiers::COMMAND | Modifiers::ALT, Key::N);
    assert!(dialog_open(&h));
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(!dialog_open(&h));
    assert!(bae_motion::query::comps(h.state().stage()).is_empty());
    assert!(!h.state().doc.is_dirty());
    // Shortcuts work again afterwards.
    new_comp(&mut h);
    assert!(h.state().active_comp().is_some());
}

#[test]
fn composition_settings_edits_the_open_comp_in_one_undo_step() {
    let mut h = gui();
    // Composition Settings needs a comp.
    assert!(
        !h.state()
            .menu_item("motion.comp.composition_settings")
            .enabled
    );
    key(&mut h, Modifiers::COMMAND, Key::K);
    assert!(!dialog_open(&h));
    comp_with(&mut h, &["motion.layer.new.null"]);
    let comp = h.state().active_comp().unwrap();
    let null = h.state().layers()[0].clone();
    let pos = |h: &Ui| {
        bae_motion::query::static_value(
            &h.state().stage().prim(&null).unwrap(),
            "transform:position",
        )
    };
    let before = pos(&h);
    assert!(
        h.state()
            .menu_item("motion.comp.composition_settings")
            .enabled
    );
    key(&mut h, Modifiers::COMMAND, Key::K);
    assert!(dialog_open(&h));
    type_into(&mut h, "Width", "1000");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(!dialog_open(&h));
    let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
    assert_eq!((info.width, info.height), (1000, 1080));
    // The layer kept its place in the frame: centered anchor, half the change.
    assert_eq!(pos(&h), Some(bae_model::Value::Vec3([500.0, 540.0, 0.0])));
    let (undo, _) = h.state().doc.undo_state();
    assert_eq!(undo.as_deref(), Some("Composition Settings"));
    key(&mut h, Modifiers::COMMAND, Key::Z);
    let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
    assert_eq!((info.width, info.height), (1920, 1080));
    assert_eq!(pos(&h), before);
    // The menu item opens the same dialog.
    assert!(h.state_mut().menu("motion.comp.composition_settings"));
    settle(&mut h);
    assert!(dialog_open(&h));
    key(&mut h, Modifiers::NONE, Key::Escape);
}

#[test]
fn composition_settings_in_chinese() {
    if !in_child(
        "acceptance::dialogs::composition_settings_in_chinese",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let mut h = gui();
    key(&mut h, Modifiers::COMMAND, Key::N);
    for text in [
        "合成设置",
        "合成名称:",
        "基本",
        "高级",
        "3D 渲染器",
        "宽度:",
        "帧速率:",
        "背景颜色:",
    ] {
        assert!(find_text(&h, text).is_some(), "{text} missing");
    }
    assert!(h.query_by_label("确定").is_some());
    assert!(h.query_by_label("取消").is_some());
    click_label(&mut h, "高级");
    for text in ["锚点:", "运动模糊", "快门角度:"] {
        assert!(find_text(&h, text).is_some(), "{text} missing");
    }
    key(&mut h, Modifiers::NONE, Key::Enter);
    let comp = h.state().active_comp().expect("comp created");
    assert_eq!(h.state().name(&comp), "合成 1");
}

#[test]
fn color_picker_from_the_background_swatch() {
    let mut h = gui();
    key(&mut h, Modifiers::COMMAND, Key::N);
    click_label(&mut h, "Background Color");
    // The picker is a second modal dialog over Composition Settings.
    assert!(h.query_by_label("Hex Color").is_some(), "picker not open");
    assert!(h.query_by_label("Hue").is_some());
    type_into(&mut h, "Hex Color", "FF8000");
    // Enter closes only the picker (OK); Composition Settings stays open.
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(h.query_by_label("Hex Color").is_none(), "picker still open");
    assert!(dialog_open(&h));
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(!dialog_open(&h));
    let comp = h.state().active_comp().expect("comp created");
    let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
    assert_eq!(info.bg_color, [1.0, 128.0 / 255.0, 0.0, 1.0]);
    // Esc in the picker cancels it and keeps the dialog's color.
    key(&mut h, Modifiers::COMMAND, Key::K);
    click_label(&mut h, "Background Color");
    type_into(&mut h, "Hex Color", "00FF00");
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(h.query_by_label("Hex Color").is_none());
    assert!(dialog_open(&h));
    key(&mut h, Modifiers::NONE, Key::Enter);
    let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
    assert_eq!(info.bg_color, [1.0, 128.0 / 255.0, 0.0, 1.0]);
}
