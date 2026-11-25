//! Right-click menus in the viewer.

use super::*;

#[test]
fn right_click_opens_the_layer_or_the_composition_menu() {
    let mut h = H::new();
    let a = h.solid([200.0, 100.0], [320.0, 180.0]);
    let b = h.solid([100.0, 100.0], [320.0, 180.0]);
    // On a layer: it is selected, and the layer menu shows with Select ▸.
    h.right_click(320.0, 180.0);
    assert_eq!(h.session().lock().selected_layers, std::slice::from_ref(&b));
    assert_eq!(
        h.viewer.menu_layers,
        [b.clone(), a.clone()],
        "topmost first"
    );
    assert!(h.labeled("Pre-compose...").is_some());
    assert!(h.labeled("Select ›").is_some(), "Select ▸");
    assert!(h.labeled("Composition Settings...").is_none());
    // A catalog item runs the catalog's action.
    let Some(crate::catalog::Node::Item(pre)) = crate::context_menu::find("Layer/Pre-compose...")
    else {
        panic!("catalog")
    };
    let r = h.choose("Pre-compose...");
    assert_eq!(r.invocations.len(), 1, "{:?}", r.invocations);
    assert_eq!(r.invocations[0].id, pre.action);
    // Invert Selection (the menu's own item).
    h.right_click(320.0, 180.0);
    h.choose("Invert Selection");
    assert_eq!(h.session().lock().selected_layers, std::slice::from_ref(&a));
    // Elsewhere: the composition menu.
    h.right_click(20.0, 20.0);
    assert!(h.viewer.menu_layers.is_empty());
    assert!(h.labeled("Composition Settings...").is_some());
    assert!(h.labeled("Pre-compose...").is_none());
}
