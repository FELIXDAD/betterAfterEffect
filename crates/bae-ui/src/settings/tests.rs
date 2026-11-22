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
                if crate::testing::first_pass_installs_fonts(ui.ctx()) {
                    return;
                }
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
            if p.label.is_empty() || p.quiet || p.kind == Kind::Path {
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
        schema::visit_here(page.items, &mut check);
    }
    // Platform-only controls: "Use System Shortcut Keys" is macOS only.
    let h = harness("composition", &prefs);
    assert_eq!(
        h.query_all_by_label("Use System Shortcut Keys")
            .next()
            .is_some(),
        cfg!(target_os = "macos")
    );
}

#[test]
fn ok_returns_the_edits_cancel_drops_them() {
    let prefs = Preferences::default();
    let mut h = harness("composition", &prefs);
    h.get_by_label("Default Spatial Interpolation to Linear")
        .click();
    h.run();
    assert!(!toggled(&h, "Default Spatial Interpolation to Linear"));
    h.get_by_label("OK").click();
    h.run();
    let [Outcome::Ok(saved)] = h.state().outcomes.as_slice() else {
        panic!("{:?}", h.state().outcomes);
    };
    assert!(
        !saved.bool(keys::DEFAULT_SPATIAL_LINEAR),
        "AE 26's default is on; one click turned it off"
    );
    assert!(!h.state().dialog.is_open());

    let mut h = harness("appearance", &prefs);
    h.get_by_label("Show Tool Tips").click();
    h.run();
    h.get_by_label("Cancel").click();
    h.run();
    assert_eq!(h.state().outcomes, [Outcome::Cancel]);
}

#[test]
fn escape_cancels() {
    let mut h = harness("startup_repair", &Preferences::default());
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(h.state().outcomes, [Outcome::Cancel]);
}

/// AE 26: Previous / Next wrap around and are never disabled.
#[test]
fn previous_next_wrap_and_page_list() {
    let mut h = harness("startup_repair", &Preferences::default());
    assert!(!h.get_by_label("Previous").accesskit_node().is_disabled());
    h.get_by_label("Previous").click();
    h.run();
    assert_eq!(h.state().dialog.page(), "scripting_expressions");
    h.get_by_label("Next").click();
    h.run();
    assert_eq!(h.state().dialog.page(), "startup_repair");
    h.get_by_label("Next").click();
    h.run();
    assert_eq!(h.state().dialog.page(), "project");
    h.get_by_label("Grids & Guides").click();
    h.run();
    assert_eq!(h.state().dialog.page(), "grids_guides");
}

#[test]
fn dependent_controls_gray_out() {
    let mut h = harness("project", &Preferences::default());
    let enabled = |h: &Harness<'_, State>| {
        h.query_all_by_label("Save Every Interval")
            .any(|n| !n.accesskit_node().is_disabled())
    };
    assert!(enabled(&h));
    h.get_by_label("Save Every:").click();
    h.run();
    assert!(!enabled(&h), "the interval follows Save Every");
    // Choose Folder... follows Auto-Save Location (a radio value).
    let choose = |h: &Harness<'_, State>| {
        h.query_all_by_label("Choose Folder...")
            .all(|n| n.accesskit_node().is_disabled())
    };
    assert!(choose(&h));
}

#[test]
fn color_theme_cards() {
    let mut h = harness("appearance", &Preferences::default());
    h.get_by_label("Light").click();
    h.run();
    assert_eq!(h.state().dialog.draft().str(keys::COLOR_THEME), "light");
}

#[test]
fn unknown_page_opens_the_first_page() {
    let h = harness("nope", &Preferences::default());
    assert_eq!(h.state().dialog.page(), "startup_repair");
}

#[test]
fn color_theme_changes_grays_only() {
    use crate::theme::{self, Appearance, color};
    assert_eq!(
        theme::adjust(color::PANEL, Appearance::Darkest),
        color::PANEL
    );
    let dark = theme::adjust(color::PANEL, Appearance::Dark);
    assert!(dark.r() > color::PANEL.r() && dark.r() == dark.g());
    let light = theme::adjust(color::PANEL, Appearance::Light);
    assert!(light.r() > 0xc0, "{light:?}");
    assert!(
        theme::adjust(color::TEXT, Appearance::Light).r() < 0x80,
        "dark text"
    );
    let red = theme::LABELS[1].1;
    assert_eq!(theme::adjust(red, Appearance::Light), red);
    assert_eq!(
        theme::adjust(color::ACCENT, Appearance::Light),
        color::ACCENT
    );
    assert_eq!(
        theme::adjust(Color32::TRANSPARENT, Appearance::Light),
        Color32::TRANSPARENT
    );
}

#[test]
fn grid_and_safe_margins_follow_preferences() {
    let mut prefs = Preferences::default();
    let g = GridGuides::from_prefs(&prefs);
    assert_eq!(g.spacing, 80.0);
    assert_eq!(g.subdivisions, 4);
    let comp = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(1920.0, 1080.0));
    let ([action, title], [cc_action, _]) = g.safe_rects(comp);
    assert_eq!(action.width(), 1920.0 * 0.9);
    assert_eq!(title.height(), 1080.0 * 0.8);
    // Center-cut action-safe: the 4:3 action-safe width (1440 x 0.9 = 1296).
    assert!((cc_action[1] - cc_action[0] - 1296.0).abs() < 0.5);
    prefs.set(bae_prefs::keys::ACTION_SAFE, 20.0);
    prefs.set(bae_prefs::keys::GRID_COLOR, "#ff0000");
    prefs.set(bae_prefs::keys::GRID_STYLE, "dots");
    let g = GridGuides::from_prefs(&prefs);
    assert_eq!(g.safe_rects(comp).0[0].width(), 1920.0 * 0.8);
    assert_eq!(g.grid_color, Color32::from_rgb(255, 0, 0));
    assert_eq!(g.grid_style, "dots");
}
