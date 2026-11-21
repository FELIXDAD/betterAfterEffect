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

/// Whether accessible name `label` is the control `name`, alone or with its
/// context in front (issue #128: "Project · Close Panel").
fn names(label: &str, name: &str) -> bool {
    label == name || label.ends_with(&format!(" · {name}"))
}

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
        let n = all.iter().filter(|l| names(l, label)).count();
        if n < *min {
            missing.push(format!("{label} ({n} < {min})"));
        }
    }
    // Names shown in the UI: layers (timeline rows carry the layer number,
    // "Layer 1 “Text”", #128), comp tab, workspaces.
    for label in ["Solid 1", "Text", "Comp 1", "Motion", "Review"] {
        let quoted = format!("“{label}”");
        if !all.iter().any(|l| l == label || l.ends_with(&quoted)) {
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
        .query_all_by_label("Layer 1 “Text”")
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
        let n = all.iter().filter(|l| names(l, zh)).count();
        if n < *min {
            wrong.push(format!("{en} → {zh}: {n} < {min} in the tree"));
        }
        if all.iter().any(|l| names(l, en)) {
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

/// Issue #128: within one panel (an accesskit Pane named after it) or the
/// window chrome outside panels, no two interactive widgets of the same role
/// have the same name — screen readers and UI automation must tell them apart
/// (seven stopwatches all called "Time-Vary Stopwatch", layer switches without
/// the layer).
#[test]
fn interactive_widgets_have_unique_names_per_panel() {
    unique_names_per_panel();
}

/// The same in Chinese: two English labels must not translate to one name.
#[test]
fn interactive_widgets_have_unique_names_in_chinese() {
    if !in_child(
        "acceptance::startup::interactive_widgets_have_unique_names_in_chinese",
        &[("BAE_LANG", "zh")],
    ) {
        return;
    }
    crate::i18n::init_from_env();
    assert_eq!(bae_ui::i18n::lang(), bae_ui::Lang::Zh);
    unique_names_per_panel();
}

fn unique_names_per_panel() {
    use egui::accesskit::{Action, Role};
    let mut h = gui();
    comp_with(
        &mut h,
        &["motion.layer.new.solid", "motion.layer.new.rectangle"],
    );
    // Both layers selected, Position revealed on each: two Position rows with
    // stopwatches, keyframe navigators and values, two sets of layer switches.
    key(&mut h, Modifiers::COMMAND, Key::A);
    key(&mut h, Modifiers::NONE, Key::P);
    settle(&mut h);
    let mut seen: std::collections::HashMap<(String, String, String), usize> =
        std::collections::HashMap::new();
    let mut panes = std::collections::BTreeSet::new();
    let mut checked = 0;
    for n in h.root().children_recursive() {
        let a = n.accesskit_node();
        if a.role() == Role::Pane {
            panes.insert(a.label().unwrap_or_default());
        }
        let data = a.data();
        let interactive =
            data.supports_action(Action::Click) || data.supports_action(Action::Focus);
        if !interactive || a.bounding_box().is_none() || a.is_hidden() {
            continue;
        }
        let Some(label) = a.label().filter(|l| !l.is_empty()) else {
            continue; // unlabelled widgets are controls_have_accessibility_labels' job
        };
        let mut group = "window".to_owned();
        let mut up = a.parent();
        while let Some(p) = up {
            if p.role() == Role::Pane {
                group = p.label().unwrap_or_default();
                break;
            }
            up = p.parent();
        }
        checked += 1;
        *seen
            .entry((group, format!("{:?}", a.role()), label))
            .or_default() += 1;
    }
    let dupes: Vec<_> = seen.iter().filter(|(_, n)| **n > 1).collect();
    for ((group, role, label), n) in &dupes {
        println!("duplicate ×{n} in {group}: {role} “{label}”");
    }
    println!("checked {checked} interactive widgets in panes {panes:?}");
    // Panes are named like the panel tabs ("Timeline Comp 1").
    for panel in [
        "Project",
        "Timeline",
        "Properties",
        "Composition",
        "Preview",
    ] {
        let panel = bae_ui::tr(panel);
        assert!(
            panes.iter().any(|p| p.starts_with(panel)),
            "{panel} is not marked as a pane: {panes:?}"
        );
    }
    assert!(dupes.is_empty(), "{} duplicate names", dupes.len());
}

// ---- Retest of #128 (dev 277f11a): edge cases ----

/// Interactive widgets whose (pane, role, name) is not unique.
fn duplicate_names(h: &Ui) -> Vec<String> {
    use egui::accesskit::{Action, Role};
    let mut seen: std::collections::HashMap<(String, String, String), usize> =
        std::collections::HashMap::new();
    for n in h.root().children_recursive() {
        let a = n.accesskit_node();
        let data = a.data();
        let interactive =
            data.supports_action(Action::Click) || data.supports_action(Action::Focus);
        if !interactive || a.bounding_box().is_none() || a.is_hidden() {
            continue;
        }
        let Some(label) = a.label().filter(|l| !l.is_empty()) else {
            continue;
        };
        let mut group = "window".to_owned();
        let mut up = a.parent();
        while let Some(p) = up {
            if p.role() == Role::Pane {
                group = p.label().unwrap_or_default();
                break;
            }
            up = p.parent();
        }
        *seen
            .entry((group, format!("{:?}", a.role()), label))
            .or_default() += 1;
    }
    seen.into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|((g, r, l), n)| format!("×{n} in {g}: {r} “{l}”"))
        .collect()
}

fn rename(h: &mut Ui, layer: &PrimPath, name: &str) {
    h.state_mut().invoke(Invocation::with_params(
        "attr.set",
        serde_json::json!({"path": layer, "attr": "name", "value": {"type": "string", "value": name}}),
    ));
    settle(h);
}

#[test]
#[ignore = "issue #128"]
fn layers_with_the_same_name_still_have_distinct_control_names() {
    // AE allows two layers with one name; the layer number tells them apart.
    let mut h = gui();
    comp_with(
        &mut h,
        &["motion.layer.new.solid", "motion.layer.new.solid"],
    );
    for l in h.state().layers() {
        rename(&mut h, &l, "背景");
    }
    key(&mut h, Modifiers::COMMAND, Key::A);
    key(&mut h, Modifiers::NONE, Key::P);
    settle(&mut h);
    let sample: Vec<String> = labels(&h)
        .into_iter()
        .filter(|l| l.contains("背景") && l.contains("Lock"))
        .collect();
    println!("lock switches: {sample:?}");
    let dupes = duplicate_names(&h);
    assert!(
        dupes.is_empty(),
        "same-named layers give duplicate control names: {dupes:#?}"
    );
}

#[test]
fn control_names_follow_a_rename() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let l = h.state().layers()[0].clone();
    key(&mut h, Modifiers::NONE, Key::P);
    rename(&mut h, &l, "新的 名字");
    let all = labels(&h);
    assert!(
        all.iter().any(|x| x.contains("新的 名字")),
        "no label with the new name: {all:?}"
    );
    // The layer's controls follow the rename; the solid's source item in the
    // Project panel keeps its own name (as in AE).
    let stale: Vec<String> = h
        .root()
        .children_recursive()
        .filter(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.contains("Solid 1"))
        })
        .map(|n| {
            let mut pane = String::from("window");
            let mut up = n.accesskit_node().parent();
            while let Some(p) = up {
                if p.role() == egui::accesskit::Role::Pane {
                    pane = p.label().unwrap_or_default();
                    break;
                }
                up = p.parent();
            }
            format!("{pane}: {}", n.accesskit_node().label().unwrap_or_default())
        })
        .filter(|s| !s.starts_with("Project"))
        .collect();
    assert!(
        stale.is_empty(),
        "labels still carry the old name: {stale:?}"
    );
}

#[test]
#[ignore = "issue #128"]
fn inputs_have_names_not_only_values() {
    // A field that only exposes its value ("PNG", a path, "0") is read without
    // saying what it is: the Render Queue's Format / Output To / Frames and the
    // Properties panel's text field (found on Windows too, #128).
    use egui::accesskit::Role;
    let h = busy_window();
    let unnamed: Vec<String> = h
        .root()
        .children_recursive()
        .filter_map(|n| {
            let a = n.accesskit_node();
            let input = matches!(
                a.role(),
                Role::ComboBox | Role::TextInput | Role::MultilineTextInput | Role::SpinButton
            );
            (input && a.bounding_box().is_some() && a.label().is_none_or(|l| l.is_empty()))
                .then(|| format!("{:?} value={:?}", a.role(), a.value()))
        })
        .collect();
    assert!(unnamed.is_empty(), "inputs without a name: {unnamed:#?}");
}
