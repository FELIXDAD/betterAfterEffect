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

// ---- Retest of #116 (dev d23a328): find controls by accessible name ----

/// Labels of every node in the accesskit tree.
fn labels(h: &Ui) -> Vec<String> {
    h.root()
        .children_recursive()
        .filter_map(|n| n.accesskit_node().label())
        .filter(|l| !l.is_empty())
        .collect()
}

/// A comp with a solid and a text layer, Position revealed, the Render Queue open.
fn busy_window() -> Ui {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid", "motion.layer.new.text"]);
    key(&mut h, Modifiers::NONE, Key::P);
    key(&mut h, Modifiers::COMMAND, Key::M);
    h
}

/// English labels that must be in the tree (with at least this many nodes).
const MAIN_CONTROLS: &[(&str, usize)] = &[
    ("Selection Tool", 1),
    ("Hand Tool", 1),
    ("Pen Tool", 1),
    ("Horizontal Type Tool", 1),
    ("Close Panel", 3),
    ("Panel Menu", 3),
    ("Time-Vary Stopwatch", 1),
    ("Current Time", 1),
    ("Work Area", 1),
    ("Render", 1),
];

#[test]
fn main_controls_are_found_by_accessible_name() {
    let mut h = busy_window();
    let all = labels(&h);
    let mut missing = Vec::new();
    for (label, min) in MAIN_CONTROLS {
        let n = all.iter().filter(|l| l == label).count();
        if n < *min {
            missing.push(format!("{label} ({n} < {min})"));
        }
    }
    // Names shown in the UI: layers, comp tab, workspaces.
    for label in ["Solid 1", "Text", "Comp 1", "Motion", "Review"] {
        if !all.iter().any(|l| l == label) {
            missing.push(label.to_owned());
        }
    }
    assert!(
        missing.is_empty(),
        "not found by name: {missing:?}\nall labels: {all:?}"
    );
    // The Project panel's search field (the Render Queue covers it here).
    let fresh = gui();
    assert!(
        labels(&fresh).iter().any(|l| l == "Search"),
        "search field has no name"
    );
    // Acting through accesskit works: the Hand Tool becomes the selected tool,
    // a click on the "Text" layer row selects that layer.
    // egui reports a selected toggle as accesskit `toggled`.
    let on = |h: &Ui, l: &str| {
        h.get_by_label(l).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
    };
    assert!(on(&h, "Selection Tool") && !on(&h, "Hand Tool"));
    h.get_by_label("Hand Tool").click_accesskit();
    settle(&mut h);
    assert!(
        on(&h, "Hand Tool") && !on(&h, "Selection Tool"),
        "an accesskit click did not switch to the Hand Tool"
    );
    let text_layer = h.state().layers()[0].clone();
    h.state().session().lock().selected_layers.clear();
    let rows: Vec<_> = h
        .root()
        .query_all_by_label("Text")
        .map(|n| n.rect())
        .collect();
    let head = find_text(&h, "Layer Name").unwrap();
    let row = rows
        .into_iter()
        .find(|r| r.top() > head.bottom())
        .expect("Text row in the timeline");
    click_at(&mut h, row.center());
    assert_eq!(h.state().selected_layers(), [text_layer]);
}

#[test]
fn same_kind_controls_can_be_told_apart() {
    // Several stopwatches / switches: a screen reader should be able to tell
    // which property or layer each belongs to (by label or by a labelled parent).
    let mut h = busy_window();
    key(&mut h, Modifiers::SHIFT, Key::S); // reveal Scale too
    let stopwatches: Vec<String> = h
        .root()
        .children_recursive()
        .filter(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.contains("Stopwatch"))
        })
        .map(|n| {
            let parent = n.parent().and_then(|p| p.accesskit_node().label());
            format!(
                "{:?} in {:?}",
                n.accesskit_node().label().unwrap_or_default(),
                parent
            )
        })
        .collect();
    println!("stopwatches: {stopwatches:?}");
    let mut unique = stopwatches.clone();
    unique.sort();
    unique.dedup();
    println!("distinct: {} of {}", unique.len(), stopwatches.len());
}

#[test]
fn accessible_names_are_chinese_in_the_chinese_ui() {
    if !in_child(
        "acceptance::startup::accessible_names_are_chinese_in_the_chinese_ui",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    let h = busy_window();
    let all = labels(&h);
    let mut wrong = Vec::new();
    if !labels(&gui()).iter().any(|l| l == "搜索") {
        wrong.push("Search → 搜索 not in the tree".to_owned());
    }
    for (en, min) in MAIN_CONTROLS {
        let zh = bae_ui::tr(en);
        if zh == *en {
            wrong.push(format!("{en}: not translated"));
            continue;
        }
        let n = all.iter().filter(|l| *l == zh).count();
        if n < *min {
            wrong.push(format!("{en} → {zh}: {n} < {min} in the tree"));
        }
        if all.iter().any(|l| l == en) {
            wrong.push(format!("{en}: English label still in the tree"));
        }
    }
    // Every fixed label of bae-ui has Chinese and no fixed English label
    // leaks into the Chinese tree.
    let leaked: Vec<&String> = all
        .iter()
        .filter(|l| bae_ui::a11y::LABELS.contains(&l.as_str()) && bae_ui::tr(l) != l.as_str())
        .collect();
    println!("zh labels: {all:?}");
    assert!(
        wrong.is_empty() && leaked.is_empty(),
        "{wrong:?} leaked {leaked:?}"
    );
}
