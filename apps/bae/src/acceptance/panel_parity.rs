//! Panel and dialog interactions through the real workbench.

use super::*;
use egui_kittest::kittest::{NodeT as _, Queryable as _};

/// Enter commits the active number. Tab commits and selects the next number.
#[test]
fn enter_commits_the_typed_number_and_tab_moves_on() {
    let mut h = gui();
    let size = |h: &Ui| {
        let comp = h.state().active_comp().expect("OK made a comp");
        let info = bae_motion::query::comp_info(h.state().stage(), &comp).unwrap();
        (info.width, info.height)
    };
    key(&mut h, Modifiers::COMMAND, Key::N);
    let width = h.get_by_label("Width").rect();
    click_at(&mut h, width.center());
    type_text(&mut h, "1000+200");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(dialog_open(&h));
    assert!(h.state().active_comp().is_none());
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(size(&h).0, 1200);
    key(&mut h, Modifiers::COMMAND, Key::N);
    let width = h.get_by_label("Width").rect();
    click_at(&mut h, width.center());
    type_text(&mut h, "1280");
    key(&mut h, Modifiers::NONE, Key::Tab);
    assert!(dialog_open(&h), "Tab closed the dialog");
    type_text(&mut h, "720");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(dialog_open(&h));
    let ok = h.get_by_label("OK").rect();
    click_at(&mut h, ok.center());
    assert!(!dialog_open(&h));
    assert_eq!(size(&h), (1280, 720));
}

#[test]
fn builtin_preset_applies_keys_and_effect_controls_edit_the_instance() {
    let mut h = gui_with(Gui::with_app(crate::app_builder().build()));
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let layer = h.state().layers()[0].clone();
    h.state_mut().invoke(Invocation::with_params(
        bae_ui::action::ids::OPEN_PANEL,
        serde_json::json!({"id":"effects"}),
    ));
    settle(&mut h);
    for label in ["* Animation Presets", "Adobe Express", "In"] {
        let rect = h.get_by_label(label).rect();
        click_at(&mut h, rect.center());
    }
    let preset = h.get_by_label("Soft Focus Entrance").rect();
    double_click_at(&mut h, preset.center());
    let effects = |h: &Ui| bae_motion::effect::effects_of(h.state().stage(), &layer);
    let effect = effects(&h)[0].clone();
    let stage = h.state().stage();
    assert_eq!(
        stage
            .prim(&effect)
            .unwrap()
            .attr("param:0001")
            .unwrap()
            .keyframes
            .len(),
        2
    );
    let title = find_text(&h, "Gaussian Blur").expect("effect header");
    click_at(&mut h, title.center());
    key(&mut h, Modifiers::NONE, Key::Enter);
    type_text(&mut h, "Entrance");
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().name(&effect), "Entrance");
    key(&mut h, Modifiers::COMMAND, Key::D);
    assert_eq!(effects(&h).len(), 2);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(effects(&h), vec![effect]);
}

/// Effects & Presets: a click selects an effect (highlighted), Down moves the
/// selection, a double-click applies it to the selected layer in one undo
/// step (AE).
#[test]
fn effects_panel_selects_and_applies_on_double_click() {
    let mut h = gui_with(Gui::with_app(crate::app_builder().build()));
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let layer = h.state().layers()[0].clone();
    h.state_mut().invoke(Invocation::with_params(
        bae_ui::action::ids::OPEN_PANEL,
        serde_json::json!({"id":"effects"}),
    ));
    settle(&mut h);
    let category = find_text(&h, "Blur & Sharpen").expect("category");
    click_at(
        &mut h,
        egui::pos2(category.left() - 9.0, category.center().y),
    );
    let toggled = |h: &Ui| -> Vec<String> {
        h.query_all_by(|n| {
            n.role() == egui::accesskit::Role::Button
                && n.toggled() == Some(egui::accesskit::Toggled::True)
        })
        .filter_map(|n| n.accesskit_node().label())
        .collect()
    };
    // The highlighted rows: what is on now and was not before a row was picked.
    let before = toggled(&h);
    let selected = |h: &Ui| -> Vec<String> {
        toggled(h)
            .into_iter()
            .filter(|l| !before.contains(l))
            .collect()
    };
    let blur = h.get_by_label("Gaussian Blur").rect();
    click_at(&mut h, blur.center());
    assert_eq!(selected(&h), ["Gaussian Blur"]);
    let position = |h: &Ui| {
        h.state()
            .stage()
            .prim(&layer)
            .and_then(|p| p.attr("transform:position").cloned())
    };
    let at = position(&h);
    key(&mut h, Modifiers::NONE, Key::ArrowDown);
    let next = selected(&h);
    assert!(next.len() == 1 && next[0] != "Gaussian Blur", "{next:?}");
    assert_eq!(position(&h), at, "Down moved the selection, not the layer");
    double_click_at(&mut h, blur.center());
    let effects = |h: &Ui| bae_motion::effect::effects_of(h.state().stage(), &layer);
    assert_eq!(effects(&h).len(), 1);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert!(effects(&h).is_empty(), "one undo step");
}
