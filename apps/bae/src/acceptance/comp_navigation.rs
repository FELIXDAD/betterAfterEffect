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
    let header = timeline_header(h).unwrap();
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
    let header = timeline_header(&h).unwrap();
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
fn composition_preferences_choose_the_pen_and_double_click_destination() {
    use bae_ui_motion::{SharedTools, Tool};
    let mut h = fixture();
    let parent = h.state().active_comp().unwrap();
    let header = timeline_header(&h).unwrap();
    drop_item(&mut h, "Child", header.center() + vec2(0.0, 80.0));
    let layer = h.state().layers()[0].clone();
    let child = query::precomp_source(h.state().stage(), &layer).unwrap();
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("composition.double_click_comp_layer", "layer_panel");
    prefs.set("composition.pen_shortcut_toggles_mask_feather", false);
    h.state().prefs.set(prefs).unwrap();
    let tools = h.state().app.resource::<SharedTools>().unwrap().clone();
    for expected in [
        Tool::Pen,
        Tool::AddVertex,
        Tool::DeleteVertex,
        Tool::ConvertVertex,
        Tool::MaskFeather,
        Tool::Pen,
    ] {
        key(&mut h, Modifiers::NONE, Key::G);
        assert_eq!(tools.current(), expected);
    }
    key(&mut h, Modifiers::NONE, Key::V);
    let at = row(&h, "Child").center();
    double_click_at(&mut h, at);
    assert!(find_text(&h, "Layer Child").is_some());
    assert_eq!(h.state().active_comp(), Some(parent));
    pause(&mut h);
    let at = row(&h, "Child").center();
    click_at_mods(&mut h, at, Modifiers::ALT);
    click_at_mods(&mut h, at, Modifiers::ALT);
    assert_eq!(h.state().active_comp(), Some(child));
}

#[test]
fn graph_drop_uses_the_ruler_frame_and_precomp_opens_at_its_source_frame() {
    use bae_model::Time;

    let mut h = fixture();
    let ruler = h.get_by_label("Time Ruler").rect();
    let ruler_at = |fraction| {
        egui::pos2(
            ruler.left() + ruler.width() * fraction,
            ruler.bottom() - 2.0,
        )
    };
    click_at(&mut h, ruler_at(0.3));
    let start = h.state().now();
    assert!(start > Time::ZERO);
    click_at(&mut h, ruler_at(0.7));
    let parent_time = h.state().now();
    assert!(parent_time > start);

    let blank_y = timeline_header(&h).unwrap().bottom() + 65.0;
    drop_item(&mut h, "Child", egui::pos2(ruler_at(0.3).x, blank_y));
    let layer = h.state().layers()[0].clone();
    let timing = query::layer_timing(h.state().stage(), &layer);
    assert_eq!((timing.start_time, timing.in_point), (start, start));
    h.state_mut().invoke(Invocation::with_params(
        "attr.set",
        serde_json::json!({"path": layer, "attr": "stretch", "value": {"type": "float", "value": 200.0}}),
    ));
    settle(&mut h);
    let child = query::precomp_source(h.state().stage(), &layer).unwrap();
    let child_info = query::comp_info(h.state().stage(), &child).unwrap();
    let expected = child_info
        .frame_rate
        .snap(Time((parent_time - start).0 / 2));
    let name = row(&h, "Child").center();
    double_click_at(&mut h, name);
    assert_eq!(h.state().active_comp().as_ref(), Some(&child));
    assert_eq!(h.state().now(), expected);

    let parent = project_row(&h, "Parent");
    double_click_at(&mut h, parent);
    assert_eq!(h.state().now(), parent_time);
    for (attr, value) in [
        (
            "time_remap",
            serde_json::json!({"type": "time", "value": Time::from_seconds(1.0)}),
        ),
        (
            "time_remap_enabled",
            serde_json::json!({"type": "bool", "value": true}),
        ),
    ] {
        h.state_mut().invoke(Invocation::with_params(
            "attr.set",
            serde_json::json!({"path": layer, "attr": attr, "value": value}),
        ));
    }
    settle(&mut h);
    let bar = egui::pos2(ruler_at(0.5).x, row(&h, "Child").center().y);
    double_click_at(&mut h, bar);
    assert_eq!(h.state().active_comp().as_ref(), Some(&child));
    assert_eq!(h.state().now(), Time::from_seconds(1.0));

    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("composition.synchronize_time_all_related_items", false);
    h.state().prefs.set(prefs).unwrap();
    click_at(&mut h, ruler_at(0.2));
    let saved_child_time = h.state().now();
    assert_ne!(saved_child_time, Time::from_seconds(1.0));
    double_click_at(&mut h, parent);
    // Two comps open now: the comp tab row is back, the rows moved down.
    let name = row(&h, "Child").center();
    double_click_at(&mut h, name);
    assert_eq!(h.state().active_comp(), Some(child));
    assert_eq!(h.state().now(), saved_child_time);
}

#[test]
fn tab_menu_and_toolbar_navigate_direct_parents_and_children() {
    let mut h = fixture();
    let header = timeline_header(&h).unwrap();
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
    // AE's Timeline tab shows the comp; its name says "Timeline".
    assert!(h.query_all_by_label("Timeline Child").next().is_some());
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
    assert!(h.query_all_by_label("Timeline Parent").next().is_some());
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

/// #735: double-clicking footage in the Project panel opens it in the Footage
/// panel (AE).
#[test]
fn double_clicking_project_footage_opens_the_footage_panel() {
    let mut h = gui();
    let dir = tempfile::tempdir().unwrap();
    let clip = dir.path().join("clip.mov");
    std::fs::write(&clip, b"clip").unwrap();
    h.state_mut().dispatch(Invocation::with_params(
        "item.import",
        serde_json::json!({"files": [{"path": clip, "attrs": {
            "width": {"type": "int", "value": 320},
            "height": {"type": "int", "value": 180}}}]}),
    ));
    settle(&mut h);
    let row = project_row(&h, "clip.mov");
    double_click_at(&mut h, row);
    let open = h.state().workspace.open_panels();
    assert!(
        open.iter().any(|p| p.kind() == "footage"),
        "no Footage panel: {open:?}"
    );
    // A solid opens there too (AE).
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let solid = bae_motion::solid::layer_solid(h.state().stage(), &h.state().layers()[0]).unwrap();
    let name = query::display_name_of(h.state().stage(), &solid);
    let row = project_row(&h, &name);
    double_click_at(&mut h, row);
    let shown = h.state().session().lock().footage_view.clone();
    assert_eq!(shown, Some(solid));
}
