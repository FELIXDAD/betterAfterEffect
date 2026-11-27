//! Drag-and-drop feedback over the viewer, and moving items into folders.

use super::*;
use crate::project::ItemDrag;

/// A comp dragged over the viewer is taken (its box shows where it lands); a
/// folder or the comp itself is not; a drop makes it a layer at the pointer.
#[test]
fn dragged_items_are_taken_where_they_can_become_layers() {
    let mut h = H::new();
    let other = path(
        &h.run(
            "comp.create",
            json!({"width": 320, "height": 180, "name": "Other"}),
        )["path"],
    );
    let folder = path(&h.run("folder.create", json!({"name": "Folder"}))["path"]);
    crate::drag_feedback::install(&h.ctx);
    let at = h.screen(320.0, 180.0);
    let over = |h: &mut H, items: Vec<PrimPath>| {
        egui::DragAndDrop::set_payload(&h.ctx, ItemDrag(items));
        h.move_to(at, Modifiers::NONE, Vec::new());
        crate::drag_feedback::accepted(&h.ctx)
    };
    assert!(!over(&mut h, vec![folder]), "a folder is not a layer");
    let comp = h.comp.clone();
    assert!(!over(&mut h, vec![comp]), "not the comp into itself");
    assert!(over(&mut h, vec![other.clone()]));
    let r = h.pointer_button(at, false, Modifiers::NONE);
    let made = r
        .invocations
        .iter()
        .find(|i| i.params["source"] == json!(other))
        .expect("a layer of the dropped comp");
    assert_eq!(made.params["position"], json!([320.0, 180.0]));
}

/// Items moved into a folder keep everything that points at them.
#[test]
fn moving_an_item_into_a_folder_keeps_its_layers() {
    let mut h = H::new();
    let other = path(
        &h.run(
            "comp.create",
            json!({"width": 320, "height": 180, "name": "Other"}),
        )["path"],
    );
    let layer = path(
        &h.run(
            "layer.create",
            json!({"comp": h.comp, "kind": "pre_comp", "source": other}),
        )["path"],
    );
    let folder = path(&h.run("folder.create", json!({"name": "Folder"}))["path"]);
    let out = h.run("item.move", json!({"items": [other], "folder": folder}));
    let moved = path(&out["paths"][0]);
    assert_eq!(moved.parent(), Some(folder.clone()));
    assert!(query::comp_info(&h.stage, &moved).is_some());
    assert_eq!(
        h.value(&layer, "source"),
        Some(Value::PrimRef(moved.clone()))
    );
    // A folder does not go into itself.
    let refused = h.reg.execute(
        &mut h.stage,
        &h.author,
        "item.move",
        json!({"items": [folder], "folder": folder}),
    );
    assert!(refused.is_err());
    // Back to the top of the project.
    let out = h.run("item.move", json!({"items": [moved]}));
    assert_eq!(path(&out["paths"][0]).parent(), Some(PrimPath::root()));
}
