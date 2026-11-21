use super::*;
use bae_prefs::keys;
use egui::accesskit::Toggled;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};

struct State {
    dialog: SettingsDialog,
    outcomes: Vec<Outcome>,
}

fn harness(page: &str, prefs: &Preferences) -> Harness<'static, State> {
    let mut dialog = SettingsDialog::default();
    dialog.open(Some(page), prefs);
    let mut h = Harness::builder()
        .with_size(vec2(1000.0, 700.0))
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, s: &mut State| {
                let o = s.dialog.show(ui.ctx());
                if o != Outcome::None {
                    s.outcomes.push(o);
                }
            },
            State {
                dialog,
                outcomes: Vec::new(),
            },
        );
    h.run();
    h
}

fn toggled(h: &Harness<'_, State>, label: &str) -> bool {
    h.query_all_by_label(label)
        .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::CheckBox)
        .find_map(|n| n.accesskit_node().toggled())
        .unwrap_or_else(|| panic!("no checkbox {label:?}"))
        == Toggled::True
}

/// Every control of every page is in the dialog (by its label) and checkboxes
/// show their AE default.
#[test]
fn every_page_has_its_controls_with_defaults() {
    let prefs = Preferences::default();
    for page in PAGES {
        let h = harness(page.id, &prefs);
        assert!(
            h.query_all_by_label(page.title).next().is_some(),
            "page title {}",
            page.title
        );
        let mut check = |p: &'static Pref| {
            if !p.os.current() || p.label.is_empty() {
                return;
            }
            assert!(
                h.query_all_by_label(p.label).next().is_some(),
                "{}: no control labeled {:?}",
                page.id,
                p.label
            );
            if p.kind == Kind::Bool {
                assert_eq!(
                    toggled(&h, p.label),
                    prefs.bool(p.key),
                    "{}: default of {:?}",
                    page.id,
                    p.label
                );
            }
        };
        schema::visit(page.items, &mut check);
    }
}

#[test]
fn ok_returns_the_edits_cancel_drops_them() {
    let prefs = Preferences::default();
    let mut h = harness("general", &prefs);
    h.get_by_label("Default Spatial Interpolation to Linear")
        .click();
    h.run();
    assert!(toggled(&h, "Default Spatial Interpolation to Linear"));
    h.get_by_label("OK").click();
    h.run();
    let [Outcome::Ok(saved)] = h.state().outcomes.as_slice() else {
        panic!("{:?}", h.state().outcomes);
    };
    assert!(saved.bool(keys::DEFAULT_SPATIAL_LINEAR));
    assert!(!h.state().dialog.is_open());

    let mut h = harness("general", &prefs);
    h.get_by_label("Show Tool Tips").click();
    h.run();
    h.get_by_label("Cancel").click();
    h.run();
    assert_eq!(h.state().outcomes, [Outcome::Cancel]);
}

#[test]
fn escape_cancels() {
    let mut h = harness("general", &Preferences::default());
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(h.state().outcomes, [Outcome::Cancel]);
}

#[test]
fn previous_next_and_page_list() {
    let mut h = harness("general", &Preferences::default());
    assert!(h.get_by_label("Previous").accesskit_node().is_disabled());
    h.get_by_label("Next").click();
    h.run();
    assert_eq!(h.state().dialog.page(), "previews");
    h.get_by_label("Previous").click();
    h.run();
    assert_eq!(h.state().dialog.page(), "general");
    h.get_by_label("Grids & Guides").click();
    h.run();
    assert_eq!(h.state().dialog.page(), "grids_guides");
    let mut h = harness("notifications", &Preferences::default());
    assert!(h.get_by_label("Next").accesskit_node().is_disabled());
    h.run();
}

#[test]
fn dependent_controls_gray_out() {
    let mut h = harness("auto_save", &Preferences::default());
    let enabled = |h: &Harness<'_, State>| {
        h.query_all_by_label("Save Every:")
            .any(|n| !n.accesskit_node().is_disabled())
    };
    assert!(enabled(&h));
    h.get_by_label("Automatically Save Projects").click();
    h.run();
    assert!(
        !enabled(&h),
        "Save Every follows Automatically Save Projects"
    );
}

#[test]
fn default_buttons_reset_sliders() {
    let mut prefs = Preferences::default();
    prefs.set(keys::BRIGHTNESS, 0.8);
    let mut h = harness("appearance", &prefs);
    h.query_all_by_label("Default").next().unwrap().click();
    h.run();
    assert_eq!(h.state().dialog.draft().float(keys::BRIGHTNESS), 0.0);
}

#[test]
fn unknown_page_opens_general() {
    let h = harness("nope", &Preferences::default());
    assert_eq!(h.state().dialog.page(), "general");
}

#[test]
fn appearance_brightness_lightens_grays_only() {
    use crate::theme::{self, Appearance, color};
    let labels: [Color32; 17] = std::array::from_fn(theme::label_color);
    let def = Appearance::default();
    assert_eq!(theme::adjust(color::PANEL, &def, &labels), color::PANEL);
    let light = Appearance {
        brightness: 1.0,
        ..def
    };
    let panel = theme::adjust(color::PANEL, &light, &labels);
    assert!(panel.r() > color::PANEL.r() + 40, "{panel:?}");
    assert_eq!(panel.r(), panel.g());
    // Label colors stay unless "Affect Label Colors" is on.
    let red = labels[1];
    assert_eq!(theme::adjust(red, &light, &labels), red);
    let affect = Appearance {
        affect_labels: true,
        ..light
    };
    assert_ne!(theme::adjust(red, &affect, &labels), red);
    // Interactive controls: the accent blue follows its own slider.
    let darker = Appearance {
        interactive: 0.0,
        ..def
    };
    let accent = theme::adjust(color::ACCENT, &darker, &labels);
    assert!(accent.b() < color::ACCENT.b());
    assert_eq!(theme::adjust(color::FOCUS, &darker, &labels), color::FOCUS);
    // Transparent stays transparent.
    assert_eq!(
        theme::adjust(Color32::TRANSPARENT, &light, &labels),
        Color32::TRANSPARENT
    );
}
