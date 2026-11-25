//! Layer commands from the timeline: Animate Text, blending modes, the layer menu, layer selection.

use super::*;

#[test]
fn layer_menu_entries_come_from_the_command_catalog() {
    for (top, title) in [
        ("Edit", "Select Keyframe Label Group"),
        ("Animation", "Track Camera"),
        ("Animation", "Track Mask"),
        ("Layer", "Open Layer Source"),
    ] {
        assert!(
            crate::timeline::catalog_entry(top, title).is_some(),
            "{top} ▸ {title}"
        );
    }
}

#[test]
fn invert_selection_and_select_children() {
    let mut h = H::new();
    let comp = h.comp.clone();
    let a = h.layer.clone();
    let b = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    let c = path(&h.run("layer.create", json!({"comp": comp, "kind": "null"}))["path"]);
    let d = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    // c is parented to a, d to c; b is locked.
    h.run(
        "layer.set_parent",
        json!({"layers": [c], "parent": a, "time": 0}),
    );
    h.run(
        "layer.set_parent",
        json!({"layers": [d], "parent": c, "time": 0}),
    );
    h.run(
        "attr.set",
        json!({"path": b, "attr": "locked", "value": {"type": "bool", "value": true}}),
    );
    let session = h.session();
    session.lock().selected_layers = vec![a.clone()];
    crate::timeline::select_children(&h.stage, &session);
    let mut sel = session.lock().selected_layers.clone();
    sel.sort();
    let mut want = vec![a.clone(), c.clone(), d.clone()];
    want.sort();
    assert_eq!(sel, want, "children and grandchildren");
    session.lock().selected_layers = vec![c.clone()];
    crate::timeline::invert_layer_selection(&h.stage, &session, &c);
    let mut sel = session.lock().selected_layers.clone();
    sel.sort();
    let mut want = vec![a, d];
    want.sort();
    assert_eq!(sel, want, "the others, without the locked one");
}

#[test]
fn animate_text_adds_an_animator_that_the_timeline_shows() {
    use crate::actions::ids;
    let mut h = H::new();
    let comp = h.comp.clone();
    let text = path(
        &h.run(
            "layer.create",
            json!({"comp": comp, "kind": "text", "text": "Hi"}),
        )["path"],
    );
    h.session().lock().selected_layers = vec![text.clone()];
    let mut tl = h.bars();
    h.session().lock().selected_layers = vec![text.clone()];
    // Animation > Animate Text > Position: one command, one undo step.
    let r = h.action(
        &mut tl,
        Invocation::with_params(ids::ANIMATE_TEXT, json!({"property": "position"})),
    );
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "text.animate");
    h.apply(&r);
    // Text > Animator 1 > Range Selector 1 (Start, End, Offset, Advanced), Position.
    fn labels(nodes: &[crate::props::Node], out: &mut Vec<String>) {
        for n in nodes {
            match n {
                crate::props::Node::Group {
                    label, children, ..
                } => {
                    out.push(label.clone());
                    labels(children, out);
                }
                crate::props::Node::Prop { label, .. } => out.push(label.clone()),
            }
        }
    }
    let mut got = Vec::new();
    labels(&crate::props::layer_tree(&h.stage, &text), &mut got);
    for want in [
        "Animator 1",
        "Range Selector 1",
        "Start",
        "Advanced",
        "Based On",
        "Ease Low",
        "Position",
    ] {
        assert!(got.iter().any(|g| g == want), "{want} in {got:?}");
    }
    assert!(
        !got.iter().any(|g| g == "Fill Color"),
        "only what was added"
    );
    // Add Text Selector and Remove All Text Animators act on the selected text layer.
    h.apply(&h.action(&mut tl, Invocation::new(ids::ADD_TEXT_SELECTOR)));
    let a = bae_motion::text_animators::animators_of(&h.stage, &text);
    assert_eq!(
        bae_motion::text_animators::selectors_of(&h.stage, &a[0]).len(),
        2
    );
    h.apply(&h.action(&mut tl, Invocation::new(ids::REMOVE_ALL_TEXT_ANIMATORS)));
    assert!(bae_motion::text_animators::animators_of(&h.stage, &text).is_empty());
    // The catalog greys what does not render yet.
    let animate = |p: &str| {
        crate::catalog::menu_items()
            .into_iter()
            .find(|(_, it)| it.action == ids::ANIMATE_TEXT && it.params.contains(p))
            .map(|(_, it)| it.status.works())
    };
    assert_eq!(animate("\"position\""), Some(true));
    assert_eq!(animate("\"skew\""), Some(false));
}

#[test]
fn blending_mode_menu_sets_and_steps_the_selected_layers() {
    use crate::actions::ids;
    let mut h = H::new();
    let comp = h.comp.clone();
    let a = h.layer.clone();
    let b = path(&h.run("layer.create", json!({"comp": comp, "kind": "solid"}))["path"]);
    h.session().lock().selected_layers = vec![a.clone(), b.clone()];
    let mut tl = h.bars();
    h.session().lock().selected_layers = vec![a.clone(), b.clone()];
    let mode = |h: &H, l: &PrimPath| query::static_value(&h.stage.prim(l).unwrap(), "blend_mode");
    // Every mode of the menu is one the model and renderer take.
    for (_, it) in crate::catalog::menu_items() {
        if it.action == ids::BLENDING_MODE {
            assert!(it.status.works(), "{}", it.title);
            let m: Json = serde_json::from_str(it.params).unwrap();
            let r = h.action(
                &mut tl,
                Invocation::with_params(ids::BLENDING_MODE, m.clone()),
            );
            assert_eq!(r.invocations.len(), 1, "one step for both layers");
            h.apply(&r);
            let want = Some(Value::Enum(m["mode"].as_str().unwrap().into()));
            assert_eq!(mode(&h, &a), want);
            assert_eq!(mode(&h, &b), want);
        }
    }
    // Next / Previous Blending Mode step through the menu order and wrap.
    h.apply(&h.action(
        &mut tl,
        Invocation::with_params(ids::BLENDING_MODE, json!({"mode": "normal"})),
    ));
    h.apply(&h.action(
        &mut tl,
        Invocation::with_params(ids::BLENDING_MODE_STEP, json!({"step": 1})),
    ));
    assert_eq!(mode(&h, &a), Some(Value::Enum("dissolve".into())));
    h.apply(&h.action(
        &mut tl,
        Invocation::with_params(ids::BLENDING_MODE_STEP, json!({"step": -1})),
    ));
    h.apply(&h.action(
        &mut tl,
        Invocation::with_params(ids::BLENDING_MODE_STEP, json!({"step": -1})),
    ));
    assert_eq!(mode(&h, &a), Some(Value::Enum("luminescent_premul".into())));
}

#[test]
fn group_names_select_contents_for_duplicate_copy_paste_and_delete() {
    use crate::actions::ids;
    let mut h = H::new();
    let comp = h.comp.clone();
    let out = h.run(
        "shape.add",
        json!({"comp": comp, "kind": "rect", "position": [0.0, 0.0], "size": [100.0, 50.0]}),
    );
    let (layer, group) = (path(&out["layer"]), path(&out["group"]));
    let contents = layer.child("contents").unwrap();
    let names = |h: &H| -> Vec<String> {
        h.stage
            .children(&contents)
            .iter()
            .map(|c| query::display_name_of(&h.stage, c))
            .collect()
    };
    let mut tl = TimelinePanel::default();
    tl.expanded.insert(layer.to_string());
    tl.expanded.insert(format!("{layer}#contents"));
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let id = format!("{layer}#{group}");
    let row = |tl: &TimelinePanel| {
        tl.last_groups
            .iter()
            .find(|(g, ..)| *g == id)
            .cloned()
            .unwrap_or_else(|| panic!("{id} not drawn: {:?}", tl.last_groups))
    };
    // The name selects the row and does not expand it; only the arrow does.
    let (_, twirl, name) = row(&tl);
    h.click(&mut tl, name, Modifiers::NONE);
    assert_eq!(h.session().lock().selected_groups, [id.as_str()]);
    assert!(!tl.expanded.contains(&id));
    h.click(&mut tl, twirl, Modifiers::NONE);
    assert!(tl.expanded.contains(&id));
    assert_eq!(h.session().lock().selected_groups, [id.as_str()]);

    // Cmd/Ctrl+D: "Rectangle 2" above the original, one undo step.
    let r = h.action(&mut tl, Invocation::new(ids::DUPLICATE));
    assert_eq!(r.invocations.len(), 1);
    h.apply(&r);
    assert_eq!(names(&h), ["Rectangle 2", "Rectangle 1"]);
    h.undo();
    assert_eq!(names(&h), ["Rectangle 1"]);

    // Copy, then Paste with the group selected: the next number, above it.
    h.action(&mut tl, Invocation::new(ids::COPY));
    let r = h.action(&mut tl, Invocation::new(ids::PASTE));
    h.apply(&r);
    assert_eq!(names(&h), ["Rectangle 2", "Rectangle 1"]);
    // Paste with only the layer selected: on top of its contents.
    h.session().lock().selected_groups.clear();
    h.apply(&h.action(&mut tl, Invocation::new(ids::PASTE)));
    assert_eq!(names(&h), ["Rectangle 3", "Rectangle 2", "Rectangle 1"]);

    // Delete removes the selected group, not the layer.
    h.session().lock().selected_groups = vec![id.clone()];
    h.apply(&h.action(&mut tl, Invocation::new(ids::DELETE)));
    assert_eq!(names(&h), ["Rectangle 3", "Rectangle 2"]);
    assert!(h.stage.has_prim(&layer));
    assert!(h.session().lock().selected_groups.is_empty());

    // Masks: Duplicate puts "Mask 2" below; Paste onto a layer without masks
    // makes its first one, keeping the name.
    let solid = h.layer.clone();
    let mask = path(
        &h.run(
            "mask.create",
            json!({"layer": solid, "kind": "rect", "position": [10.0, 10.0], "size": [20.0, 20.0]}),
        )["mask"],
    );
    let masks = |h: &H, l: &PrimPath| -> Vec<String> {
        h.stage
            .children(&l.child("masks").unwrap())
            .iter()
            .map(|c| query::display_name_of(&h.stage, c))
            .collect()
    };
    {
        let s = h.session();
        let mut s = s.lock();
        s.selected_groups = vec![format!("{solid}#{mask}")];
        s.selected_layers = vec![solid.clone()];
    }
    h.apply(&h.action(&mut tl, Invocation::new(ids::DUPLICATE)));
    assert_eq!(masks(&h, &solid), ["Mask 1", "Mask 2"]);
    h.action(&mut tl, Invocation::new(ids::COPY));
    {
        let s = h.session();
        let mut s = s.lock();
        s.selected_groups.clear();
        s.selected_layers = vec![layer.clone()];
    }
    h.apply(&h.action(&mut tl, Invocation::new(ids::PASTE)));
    assert_eq!(masks(&h, &layer), ["Mask 1"]);
}

#[test]
fn empty_outline_space_and_deselect_all_clear_the_selection() {
    let mut h = H::new();
    h.keys(ROT, &[(0.0, float(0.0)), (1.0, float(90.0))]);
    let layer = h.layer.clone();
    let mut tl = h.bars();
    let select = |h: &H| {
        let s = h.session();
        let mut s = s.lock();
        s.selected_layers = vec![layer.clone()];
        s.selected_props = vec![(layer.clone(), ROT.to_owned())];
    };
    // A click in the outline below the last row (AE: deselects everything).
    select(&h);
    h.click(&mut tl, pos2(40.0, 700.0), Modifiers::NONE);
    {
        let s = h.session();
        let s = s.lock();
        assert!(s.selected_layers.is_empty() && s.selected_props.is_empty());
    }
    // Edit > Deselect All (F2, Cmd/Ctrl+Shift+A) clears properties too.
    select(&h);
    h.action(&mut tl, Invocation::new(crate::actions::ids::DESELECT_ALL));
    let s = h.session();
    let s = s.lock();
    assert!(s.selected_layers.is_empty() && s.selected_props.is_empty());
}
