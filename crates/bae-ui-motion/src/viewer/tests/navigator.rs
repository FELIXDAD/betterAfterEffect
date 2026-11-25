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
    let at = h.labeled(&name).expect("outer comp on the navigator");
    h.move_to(at, Modifiers::NONE, Vec::new());
    h.pointer_button(at, true, Modifiers::NONE);
    h.pointer_button(at, false, Modifiers::NONE);
    let s = h.session();
    let s = s.lock();
    assert_eq!(s.active_comp.as_ref(), Some(&outer));
    assert_eq!(s.comp_flow, [outer.clone(), inner]);
}

/// Composition Mini-Flowchart (Tab): the pre-comps of the active comp are
/// listed; clicking one opens it one step further along the path.
#[test]
fn the_mini_flowchart_opens_a_pre_comp() {
    let mut h = H::new();
    let inner = path(&h.run("comp.create", json!({"name": "Inner"}))["path"]);
    h.run(
        "layer.create",
        json!({"comp": h.comp, "kind": "pre_comp", "source": inner}),
    );
    let outer = h.comp.clone();
    tool_action(&h, crate::viewer::navigator::MINI_FLOWCHART, json!({}));
    assert!(h.session().lock().mini_flowchart);
    h.frame(Vec::new(), Modifiers::NONE);
    let at = h.labeled("Inner").expect("Inner in the Mini-Flowchart");
    h.move_to(at, Modifiers::NONE, Vec::new());
    h.pointer_button(at, true, Modifiers::NONE);
    h.pointer_button(at, false, Modifiers::NONE);
    let s = h.session();
    let s = s.lock();
    assert_eq!(s.active_comp.as_ref(), Some(&inner));
    assert_eq!(s.comp_flow, [outer, inner.clone()]);
    assert!(!s.mini_flowchart, "closed after the pick");
}
