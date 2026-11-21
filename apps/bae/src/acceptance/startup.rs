//! Startup: the first-launch welcome dialog closes with its close button, Esc
//! and a click outside, and the app works afterwards; choosing a role switches
//! the workspace.

use egui::{Key, Modifiers, pos2};
use egui_kittest::kittest::{NodeT as _, Queryable as _};

use super::*;

const WELCOME: &str = "Welcome to BAE";

fn first_launch() -> Ui {
    let mut g = Gui::new();
    g.workspace.show_role_picker();
    let mut h = gui_with(g);
    settle(&mut h);
    assert!(
        welcome_open(&h),
        "welcome dialog not shown: {:?}",
        painted_texts(&h)
    );
    h
}

fn welcome_open(h: &Ui) -> bool {
    find_text(h, WELCOME).is_some()
}

/// The dialog still works as an app afterwards: Cmd+N makes a comp.
fn app_works(h: &mut Ui) {
    key(h, Modifiers::COMMAND, Key::N);
    assert!(
        h.state().active_comp().is_some(),
        "Cmd+N after closing the welcome dialog"
    );
}

#[test]
fn welcome_dialog_close_button() {
    let mut h = first_launch();
    let title = find_text(&h, WELCOME).unwrap();
    // The close icon: the small widget on the title's row, at the dialog's right.
    let close = h
        .root()
        .children_recursive()
        .filter_map(|n| {
            let b = n.accesskit_node().bounding_box()?;
            Some(Rect::from_min_max(
                pos2(b.x0 as f32, b.y0 as f32),
                pos2(b.x1 as f32, b.y1 as f32),
            ))
        })
        .filter(|r| {
            (r.center().y - title.center().y).abs() < 6.0
                && r.left() > title.right() + 100.0
                && r.width() <= 24.0
                && r.height() <= 24.0
        })
        .max_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("no close button next to the title");
    click_at(&mut h, close.center());
    assert!(!welcome_open(&h), "close button did not close the dialog");
    assert_eq!(h.state().workspace.active_workspace(), "motion");
    app_works(&mut h);
}

#[test]
fn welcome_dialog_escape() {
    let mut h = first_launch();
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(!welcome_open(&h), "Esc did not close the dialog");
    assert_eq!(h.state().workspace.active_workspace(), "motion");
    app_works(&mut h);
}

#[test]
fn welcome_dialog_click_outside() {
    let mut h = first_launch();
    let title = find_text(&h, WELCOME).unwrap();
    // Far left of the window, well outside the centred dialog.
    click_at(&mut h, pos2(20.0, title.center().y + 200.0));
    assert!(!welcome_open(&h), "click outside did not close the dialog");
    app_works(&mut h);
}

#[test]
fn welcome_dialog_role_choice_switches_workspace() {
    let mut h = first_launch();
    let review = h
        .root()
        .query_all_by_role(egui::accesskit::Role::Button)
        .find(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with("Review"))
        })
        .map(|n| n.rect());
    let Some(review) = review else {
        panic!("no Review role button: {:?}", painted_texts(&h));
    };
    click_at(&mut h, review.center());
    assert!(!welcome_open(&h));
    assert_eq!(h.state().workspace.active_workspace(), "review");
}

#[test]
fn shortcuts_behind_the_welcome_dialog() {
    // The dialog is modal: AE-style shortcuts should not change the project
    // behind it while it is open.
    let mut h = first_launch();
    key(&mut h, Modifiers::COMMAND, Key::N);
    let leaked = h.state().active_comp().is_some();
    assert!(
        !leaked,
        "Cmd+N behind the open welcome dialog created a comp"
    );
}

#[test]
fn version_line_and_about() {
    let line = bae_platform::version_line();
    println!("version line: {line}");
    assert!(line.contains(bae_platform::VERSION));
    let mut h = gui();
    h.state_mut().invoke(Invocation::new(ids::ABOUT));
    settle(&mut h);
    assert!(find_text(&h, "About BAE").is_some());
    assert!(
        find_text(&h, &line).is_some(),
        "About does not show {line:?}"
    );
}

#[test]
fn controls_have_accessibility_labels() {
    // VoiceOver reads accesskit labels; icon buttons (toolbar tools, panel
    // close / menu buttons, stopwatches), tabs and timeline rows have none.
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let mut labelled = 0;
    let mut unlabelled = 0;
    for n in h.root().children_recursive() {
        let a = n.accesskit_node();
        if a.bounding_box().is_none() || a.role() == egui::accesskit::Role::GenericContainer {
            continue;
        }
        if a.label().is_some_and(|l| !l.is_empty()) || a.value().is_some_and(|v| !v.is_empty()) {
            labelled += 1;
        } else {
            unlabelled += 1;
            let b = a.bounding_box().unwrap();
            println!(
                "unlabelled {:?} at ({:.0}, {:.0}) {:.0}×{:.0}",
                a.role(),
                b.x0,
                b.y0,
                b.width(),
                b.height()
            );
        }
    }
    println!("accessibility: {labelled} labelled, {unlabelled} unlabelled widgets");
    assert!(
        unlabelled * 10 <= labelled + unlabelled,
        "{unlabelled} of {} widgets have no label",
        labelled + unlabelled
    );
}
