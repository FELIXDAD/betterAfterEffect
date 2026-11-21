//! Preferences (macOS: Settings): opening, OK / Cancel, and the settings that
//! change behavior (Levels of Undo, label colors, appearance).

use bae_prefs::keys;
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable as _;

use super::*;

fn open_general(h: &mut Ui) {
    key(h, Modifiers::COMMAND | Modifiers::ALT, Key::Semicolon);
    settle(h);
    assert!(h.state().workspace.modal_open(), "Cmd+Alt+; opened nothing");
}

#[test]
fn shortcut_opens_startup_repair_ok_applies_levels_of_undo() {
    let mut h = gui();
    open_general(&mut h);
    assert_eq!(
        h.state_mut().workspace.settings_dialog().page(),
        "startup_repair"
    );
    // Shortcuts wait while the dialog is open (N would otherwise do nothing
    // harmful, but Cmd+N would create a comp).
    let (m, k) = cmd(Key::N);
    key(&mut h, m, k);
    assert!(
        h.state().active_comp().is_none(),
        "Cmd+N ran behind the dialog"
    );
    h.state_mut()
        .workspace
        .settings_dialog()
        .set(keys::LEVELS_OF_UNDO, 2);
    h.get_by_label("OK").click();
    settle(&mut h);
    assert!(!h.state().workspace.modal_open());
    assert_eq!(h.state().prefs.current().levels_of_undo(), 2);
    assert_eq!(h.state().doc.stage().undo_limit(), 2);
    // Three steps, two levels: only two undo.
    comp_with(&mut h, &["motion.layer.new.null", "motion.layer.new.null"]);
    let author = h.state().doc.author().clone();
    let mut undos = 0;
    while h.state().doc.stage().can_undo(&author) {
        h.state_mut().dispatch(Invocation::new(ids::UNDO));
        undos += 1;
    }
    assert_eq!(undos, 2);
}

#[test]
fn cancel_keeps_the_stored_values() {
    let mut h = gui();
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::settings::ACTION,
        serde_json::json!({"page": "project"}),
    ));
    settle(&mut h);
    assert_eq!(
        h.state_mut().workspace.settings_dialog().page(),
        "project"
    );
    h.get_by_label("Automatically Save Projects").click();
    settle(&mut h);
    h.get_by_label("Cancel").click();
    settle(&mut h);
    assert!(!h.state().workspace.modal_open());
    assert_eq!(h.state().prefs.generation(), 0);
    assert_eq!(h.state().prefs.current().auto_save_minutes(), Some(5));
}

/// Label colors and the brightness reach the theme (process-wide state: runs in
/// a child process).
#[test]
fn label_colors_and_brightness_apply() {
    if !in_child(
        "acceptance::settings::label_colors_and_brightness_apply",
        &[],
    ) {
        return;
    }
    let mut h = gui();
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::settings::ACTION,
        serde_json::json!({"page": "labels"}),
    ));
    settle(&mut h);
    let dialog = h.state_mut().workspace.settings_dialog();
    dialog.set("labels.color_1", "#00ff00");
    dialog.set("labels.name_1", "Hot");
    dialog.set(keys::BRIGHTNESS, 1.0);
    h.get_by_label("OK").click();
    settle(&mut h);
    assert_eq!(
        bae_ui::theme::label_color(1),
        egui::Color32::from_rgb(0, 255, 0)
    );
    assert_eq!(bae_ui::theme::label_name(1), "Hot");
    assert_eq!(bae_ui::theme::appearance().brightness, 1.0);
}
