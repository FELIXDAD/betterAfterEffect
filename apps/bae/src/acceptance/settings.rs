//! Preferences (macOS: Settings): opening, OK / Cancel, and the settings that
//! change behavior (Levels of Undo, label colors, appearance).

use bae_prefs::keys;
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable as _;

use super::*;

fn open_general(h: &mut Ui) {
    key(h, Modifiers::COMMAND | Modifiers::ALT, Key::Semicolon);
    settle(h);
    assert!(
        h.state().workspace.modal_open(h.state().app.resources()),
        "Cmd+Alt+; opened nothing"
    );
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
    assert!(!h.state().workspace.modal_open(h.state().app.resources()));
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
    assert_eq!(h.state_mut().workspace.settings_dialog().page(), "project");
    h.get_by_label("Save Every:").click();
    settle(&mut h);
    h.get_by_label("Cancel").click();
    settle(&mut h);
    assert!(!h.state().workspace.modal_open(h.state().app.resources()));
    assert_eq!(h.state().prefs.generation(), 0);
    assert_eq!(h.state().prefs.current().auto_save_minutes(), Some(5));
}

/// Label colors and the color theme reach the theme (process-wide state: runs in
/// a child process).
#[test]
fn label_colors_and_color_theme_apply() {
    if !in_child(
        "acceptance::settings::label_colors_and_color_theme_apply",
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
    dialog.set("labels.label_color_1", "#00ff00");
    dialog.set("labels.label_name_1", "Hot");
    dialog.set(keys::COLOR_THEME, "light");
    h.get_by_label("OK").click();
    settle(&mut h);
    assert_eq!(
        bae_ui::theme::label_color(1),
        egui::Color32::from_rgb(0, 255, 0)
    );
    assert_eq!(bae_ui::theme::label_name(1), "Hot");
    assert_eq!(
        bae_ui::theme::appearance(),
        bae_ui::theme::Appearance::Light
    );
}

/// Issue #206: in Chinese, Preferences ▸ Project shows the new project solids
/// folder as "纯色", the name the folder gets when a solid is made.
#[test]
fn solids_folder_default_matches_the_folder_bae_makes_in_chinese() {
    if !in_child(
        "acceptance::settings::solids_folder_default_matches_the_folder_bae_makes_in_chinese",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let mut h = gui();
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::settings::ACTION,
        serde_json::json!({"page": "project"}),
    ));
    settle(&mut h);
    assert!(
        find_text(&h, "新建项目纯色文件夹").is_some(),
        "Project page"
    );
    assert!(
        find_text(&h, "纯色").is_some(),
        "the default shows in Chinese"
    );
    assert!(find_text(&h, "Solids").is_none(), "English default shown");
    // Cancel: the stored value stays the default.
    h.get_by_label("取消").click();
    settle(&mut h);
    // A solid makes the solids folder with the same name.
    new_comp(&mut h);
    new_solid(&mut h);
    let stage = h.state().stage();
    let solids = bae_model::PrimPath::new(bae_motion::query::SOLIDS_FOLDER).unwrap();
    assert_eq!(bae_motion::query::display_name_of(stage, &solids), "纯色");
}

/// Appearance > Language switches the UI at once, no restart: the panels and
/// menus show the chosen language while the window is still open, OK keeps it,
/// Cancel goes back.
#[test]
fn language_switches_without_restart() {
    if !in_child(
        "acceptance::settings::language_switches_without_restart",
        &[("BAE_LANG", "en")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let mut h = gui();
    let open = |h: &mut Ui| {
        h.state_mut().dispatch(Invocation::with_params(
            bae_ui::settings::ACTION,
            serde_json::json!({"page": "appearance"}),
        ));
        settle(h);
    };
    open(&mut h);
    assert!(
        find_text(&h, "Language / 语言").is_some(),
        "first on Appearance"
    );
    h.state_mut()
        .workspace
        .settings_dialog()
        .set(keys::LANGUAGE, "zh-CN");
    settle(&mut h);
    assert_eq!(bae_ui::i18n::lang(), bae_ui::Lang::Zh);
    let texts = painted_texts(&h);
    for zh in ["项目", "时间轴", "语言 / Language"] {
        assert!(texts.iter().any(|t| t.contains(zh)), "{zh} not shown");
    }
    // The menu bar is built from these titles.
    let menus = h.state().workspace.contributed_menus(&h.state().app);
    assert!(
        menus.menus.iter().any(|(t, _)| bae_ui::tr(t) == "文件"),
        "File menu not in Chinese"
    );
    h.get_by_label("确定").click();
    settle(&mut h);
    assert_eq!(h.state().prefs.current().str(keys::LANGUAGE), "zh-CN");
    assert_eq!(bae_ui::i18n::lang(), bae_ui::Lang::Zh);
    // English, then Cancel: back to Chinese.
    open(&mut h);
    h.state_mut()
        .workspace
        .settings_dialog()
        .set(keys::LANGUAGE, "en");
    settle(&mut h);
    assert!(
        find_text(&h, "Timeline").is_some(),
        "English while choosing"
    );
    h.get_by_label("Cancel").click();
    settle(&mut h);
    assert_eq!(bae_ui::i18n::lang(), bae_ui::Lang::Zh);
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
    h.state_mut().workspace.take_settings_requests();
    h.get_by_label("OK").click();
    settle(&mut h);
    assert!(find_text(&h, tip).is_some());
}
