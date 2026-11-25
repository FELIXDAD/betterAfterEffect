//! Project and timeline navigation through the actual workbench input routes.

use super::*;
use bae_motion::query;
use egui::vec2;
use egui_kittest::kittest::Queryable as _;

fn fixture() -> Ui {
    let mut h = gui();
    for name in ["Parent", "Child", "Other"] {
        h.state_mut().dispatch(Invocation::with_params(
            "comp.create",
            serde_json::json!({"name": name}),
        ));
    }
    settle(&mut h);
    let parent = project_row(&h, "Parent");
    double_click_at(&mut h, parent);
    assert_eq!(h.state().name(&h.state().active_comp().unwrap()), "Parent");
    h
}

fn project_row(h: &Ui, name: &str) -> Pos2 {
    // Project rows are the leftmost copy above the timeline in the Motion workspace.
    find_texts(h, |t| t == name)
        .into_iter()
        .filter(|r| r.center().y < SIZE[1] / 2.0)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .unwrap_or_else(|| panic!("no project item {name}"))
        .center()
}

fn row(h: &Ui, name: &str) -> Rect {
    let header = find_text(h, "Layer Name").unwrap();
    find_texts(h, |t| t == name)
        .into_iter()
        .find(|r| r.top() > header.bottom() && r.left() < header.left() + 350.0)
        .unwrap_or_else(|| panic!("no timeline row {name}"))
}

fn drop_item(h: &mut Ui, name: &str, to: Pos2) {
    let source = project_row(h, name);
    drag(h, source, to, Modifiers::NONE);
}

fn button(h: &Ui, name: &str) -> Rect {
    h.query_by(|n| {
        n.role() == egui::accesskit::Role::Button
            && n.toggled().is_none()
            && n.label().as_deref() == Some(name)
    })
    .unwrap_or_else(|| {
        panic!(
            "no button {name}; actions: {:?}",
            h.state()
                .dispatched
                .iter()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
        )
    })
    .rect()
}

#[test]
fn project_drop_inserts_one_undoable_precomp_and_blocks_cycles() {
    let mut h = fixture();
    let header = find_text(&h, "Layer Name").unwrap();
    let blank = header.center() + vec2(0.0, 100.0);
    drop_item(&mut h, "Child", blank);
    assert_eq!(h.state().layer_names(), ["Child"]);
    let child = query::precomp_source(h.state().stage(), &h.state().layers()[0]).unwrap();
    assert_eq!(h.state().name(&child), "Child");

    let first = row(&h, "Child");
    drop_item(&mut h, "Other", first.center() - vec2(0.0, 3.0));
    assert_eq!(h.state().layer_names(), ["Other", "Child"]);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(h.state().layer_names(), ["Child"]);
    drop_item(&mut h, "Other", first.center() + vec2(0.0, 3.0));
    assert_eq!(h.state().layer_names(), ["Child", "Other"]);
    drop_item(&mut h, "Parent", blank);
    assert_eq!(h.state().layer_names(), ["Child", "Other"]);

    double_click_at(&mut h, first.center());
    assert_eq!(h.state().active_comp(), Some(child));
    drop_item(&mut h, "Parent", blank);
    assert!(h.state().layers().is_empty());
}

#[test]
fn tab_menu_and_toolbar_navigate_direct_parents_and_children() {
    let mut h = fixture();
    let header = find_text(&h, "Layer Name").unwrap();
    let pointer = header.center() + vec2(30.0, 85.0);
    drop_item(&mut h, "Child", pointer);
    let layer = row(&h, "Child").center();
    click_at(&mut h, layer);
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(
        h.root()
            .query_all_by_role(egui::accesskit::Role::TextInput)
            .any(|n| n.value().as_deref() == Some("Child"))
    );
    key(&mut h, Modifiers::NONE, Key::Escape);
    double_click_at(&mut h, layer);
    assert!(find_text(&h, "Timeline Child").is_some());
    assert!(find_text(&h, "Composition Child").is_some());
    send(&mut h, Event::PointerMoved(pointer));
    key(&mut h, Modifiers::NONE, Key::Tab);
    let parent = button(&h, "Parent");
    assert!((parent.left() - pointer.x).abs() < 40.0 && (parent.top() - pointer.y).abs() < 40.0);
    assert!(
        h.query_by(|n| n.role() == egui::accesskit::Role::Button
            && n.toggled().is_none()
            && n.label().as_deref() == Some("Other"))
            .is_none()
    );
    click_at(&mut h, parent.center());
    assert!(find_text(&h, "Timeline Parent").is_some());
    assert!(find_text(&h, "Composition Parent").is_some());

    let menu = h
        .state()
        .menu_items()
        .into_iter()
        .find(|item| item.title == "Composition Mini-Flowchart")
        .unwrap();
    assert!(h.state_mut().menu(&menu.id));
    settle(&mut h);
    let child = button(&h, "Child");
    click_at(&mut h, child.center());
    let toolbar = button(&h, "Composition Mini-Flowchart");
    click_at(&mut h, toolbar.center());
    let parent = button(&h, "Parent");
    click_at(&mut h, parent.center());
    assert!(find_text(&h, "Composition Parent").is_some());
}
