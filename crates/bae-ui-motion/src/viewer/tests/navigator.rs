//! The Composition Navigator bar.

use super::*;

/// Opening a pre-comp from its layer puts both comps on the navigator; a
/// click on the outer comp's name opens it and keeps the path.
#[test]
fn the_navigator_shows_the_path_through_pre_comps() {
    let mut h = H::new();
    let inner = path(&h.run("comp.create", json!({"name": "Inner"}))["path"]);
    let layer = path(
        &h.run(
            "layer.create",
            json!({"comp": h.comp, "kind": "pre_comp", "source": inner}),
        )["path"],
    );
    let outer = h.comp.clone();
    {
        let mut requests = PanelRequests::default();
        let pid = PanelId::new("timeline");
        let mut cx = PanelContext::new(
            &pid,
            "Timeline",
            &h.stage,
            &h.author,
            &h.resources,
            &mut requests,
        );
        crate::layer_panel::open(&mut cx, &h.session(), &layer);
    }
    assert_eq!(h.session().lock().comp_flow, [outer.clone(), inner.clone()]);
    h.frame(Vec::new(), Modifiers::NONE);
    let name = query::display_name_of(&h.stage, &outer);
    let at = h
        .labeled(&format!("Composition Navigator: {name}"))
        .expect("outer comp on the navigator");
    h.move_to(at, Modifiers::NONE, Vec::new());
    h.pointer_button(at, true, Modifiers::NONE);
    h.pointer_button(at, false, Modifiers::NONE);
    {
        let s = h.session();
        let s = s.lock();
        assert_eq!(s.active_comp.as_ref(), Some(&outer));
        assert_eq!(s.comp_flow, [outer.clone(), inner]);
    }
    // An arrow opens the Composition Mini-Flowchart.
    h.frame(Vec::new(), Modifiers::NONE);
    let at = h
        .labeled("Composition Navigator: Composition Mini-Flowchart")
        .expect("an arrow on the navigator");
    h.move_to(at, Modifiers::NONE, Vec::new());
    h.pointer_button(at, true, Modifiers::NONE);
    h.pointer_button(at, false, Modifiers::NONE);
    assert!(h.session().lock().mini_flow.is_some());
}
