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

#[test]
fn e_reveals_the_effects_group_with_its_effects() {
    use crate::actions::ids;
    let mut h = H::new();
    let layer = h.layer.clone();
    let folder = layer.child("effects").unwrap();
    h.run(
        "prim.create",
        json!({"path": folder, "prim_type": "Folder"}),
    );
    let blur = folder.child("Gaussian_Blur").unwrap();
    h.run("prim.create", json!({"path": blur, "prim_type": "Effect"}));
    h.run(
        "attr.set",
        json!({"path": blur, "attr": "name", "value": {"type": "string", "value": "Gaussian Blur"}}),
    );
    let mut tl = h.bars();
    h.session().lock().reveal.clear();
    let groups = |h: &mut H, tl: &mut TimelinePanel| {
        let s = h.session();
        let s = s.lock();
        tl.group_rows(&h.stage, &h.comp, &s)
    };
    // E: Effects, open, with Gaussian Blur (collapsed) under it.
    let r = h.action(
        &mut tl,
        Invocation::with_params(ids::REVEAL, json!({"what": "effects", "add": false})),
    );
    assert!(r.invocations.is_empty());
    let shown = groups(&mut h, &mut tl);
    assert_eq!(
        shown,
        [format!("{layer}#effects"), format!("{layer}#{blur}")]
    );
    // E again hides it.
    h.action(
        &mut tl,
        Invocation::with_params(ids::REVEAL, json!({"what": "effects", "add": false})),
    );
    assert!(groups(&mut h, &mut tl).is_empty());
    // E and P keys are bound.
    let bound = |key: &str| {
        crate::actions::bindings()
            .bindings
            .iter()
            .any(|b| b.chord == bae_ui::Chord::parse(key).unwrap() && b.action == ids::REVEAL)
    };
    assert!(bound("E") && bound("Shift+E"));
}

/// #599: Rename on the Timeline's empty space renames the comp in the Project
/// panel (AE 26).
#[test]
fn timeline_blank_rename_asks_the_project_panel() {
    let mut h = H::new();
    let comp = h.comp.clone();
    let mut requests = PanelRequests::default();
    let id = PanelId::new("timeline");
    let session = h.session();
    {
        let mut cx = PanelContext::new(
            &id,
            "timeline",
            &h.stage,
            &h.author,
            &h.resources,
            &mut requests,
        );
        super::super::rename_comp(&mut cx, &session, &comp);
    }
    assert!(
        requests.open.iter().any(|p| p.kind() == "project"),
        "{:?}",
        requests.open
    );
    let s = session.lock();
    assert_eq!(s.selected_items, vec![comp.clone()]);
    assert_eq!(s.rename_item.as_ref(), Some(&comp));
    drop(s);
    let _ = &mut h;
}

/// The Chinese of an English UI string, from the tables the app registers.
fn zh_of(en: &str) -> String {
    crate::i18n::all()
        .into_iter()
        .chain(crate::catalog::menu_zh().iter().copied())
        .find(|(e, _)| *e == en)
        .map(|(_, z)| z.to_owned())
        .unwrap_or_else(|| panic!("no Chinese for {en:?}"))
}

/// #596: a property name's right-click menu matches AE 26 zh_CN (Anchor Point
/// and Position, no keyframes): titles, separators and greying. Add Property
/// to Essential Graphics is enabled in AE; the panel is not built yet.
#[test]
fn property_name_menu_matches_ae() {
    let h = H::new();
    let ae: [(Option<&str>, bool); 15] = [
        (Some("编辑值..."), true),
        (Some("编辑属性..."), false),
        (Some("单独尺寸"), true),
        (None, true),
        (Some("添加关键帧"), true),
        (Some("移除所有关键帧"), false),
        (Some("上一关键帧"), false),
        (Some("下一关键帧"), false),
        (None, true),
        (Some("重置"), true),
        (Some("编辑表达式"), true),
        (Some("仅复制表达式"), true),
        (Some("在效果控件中显示"), false),
        (Some("关键帧辅助"), true),
        (Some("向基本图形添加属性"), true),
    ];
    for attr in ["transform:anchor_point", POS] {
        let ours =
            crate::timeline::menus::property_menu_entries(&h.stage, &h.layer, attr, Time::ZERO);
        assert_eq!(ours.len(), ae.len(), "{attr}");
        for ((title, enabled), (ae_title, ae_enabled)) in ours.iter().zip(ae) {
            assert_eq!(title.map(zh_of).as_deref(), ae_title, "{attr}");
            if ae_title != Some("向基本图形添加属性") {
                assert_eq!(*enabled, ae_enabled, "{attr}: {ae_title:?}");
            }
        }
    }
}

/// #596: a right-click on a property's value shows the property name's menu;
/// one on the Parent & Link column pops the parent list, the layer itself
/// greyed (AE 26, #591 batches 14/15).
#[test]
fn value_and_parent_column_right_click_menus() {
    let mut h = H::new();
    let comp = h.comp.clone();
    let first = h.layer.clone();
    let second = path(&h.run("layer.create", json!({"comp": comp, "kind": "null"}))["path"]);
    let mut tl = TimelinePanel::default();
    {
        let s = h.session();
        let mut s = s.lock();
        s.graph_editor = false;
        s.reveal.insert(
            first.clone(),
            crate::session::Reveal::Attrs(vec![POS.into()]),
        );
    }
    h.painted_text = Some(Vec::new());
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let find = |h: &H, text: &str| {
        h.painted_text
            .as_ref()
            .unwrap()
            .iter()
            .rev()
            .find(|(s, _)| s == text)
            .map(|(_, r)| r.center())
    };
    let right_click = |h: &mut H, tl: &mut TimelinePanel, p: Pos2| {
        let right = |down| Event::PointerButton {
            pos: p,
            button: PointerButton::Secondary,
            pressed: down,
            modifiers: Modifiers::NONE,
        };
        h.frame(
            tl,
            vec![Event::PointerMoved(p), right(true)],
            Modifiers::NONE,
        );
        h.frame(tl, vec![right(false)], Modifiers::NONE);
        h.frame(tl, Vec::new(), Modifiers::NONE);
    };
    // Position's value text (the solid sits at the comp centre).
    let value = h
        .painted_text
        .as_ref()
        .unwrap()
        .iter()
        .find(|(s, _)| s.starts_with("320"))
        .map(|(_, r)| r.center())
        .expect("position value");
    let edit_value = bae_ui::tr("Edit Value...");
    assert!(find(&h, edit_value).is_none());
    right_click(&mut h, &mut tl, value);
    assert!(find(&h, edit_value).is_some(), "property menu on the value");
    h.frame(&mut tl, vec![press(egui::Key::Escape)], Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);

    // The solid (layer 2 after the null) under its own Parent & Link "None".
    let none = bae_ui::tr("None");
    let nones: Vec<Pos2> = h
        .painted_text
        .as_ref()
        .unwrap()
        .iter()
        .filter(|(s, _)| s == none)
        .map(|(_, r)| r.center())
        .collect();
    assert_eq!(nones.len(), 2, "one dropdown per layer");
    let solid_row = *nones.iter().max_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
    right_click(&mut h, &mut tl, solid_row);
    let solid_name = query::display_name_of(&h.stage, &first);
    let null_name = query::display_name_of(&h.stage, &second);
    let own = find(&h, &format!("2. {solid_name}")).expect("own entry");
    // Greyed: picking the layer itself does nothing.
    h.click(&mut tl, own, Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let parent = |h: &H| {
        h.stage
            .prim(&first)
            .and_then(|p| p.value("parent").and_then(|v| v.as_prim_ref().cloned()))
    };
    assert_eq!(parent(&h), None);
    if find(&h, &format!("1. {null_name}")).is_none() {
        right_click(&mut h, &mut tl, solid_row);
    }
    let other = find(&h, &format!("1. {null_name}")).expect("other layer entry");
    let r = h.click(&mut tl, other, Modifiers::NONE);
    h.apply(&r);
    assert_eq!(parent(&h), Some(second));
}

/// #596: the Graph Editor's empty-space menu matches AE 26 zh_CN.
#[test]
fn graph_area_menu_matches_ae() {
    let ae = [
        Some("自动缩放高度以适合视图"),
        Some("显示选择的属性"),
        None,
        Some("显示动画属性"),
        Some("显示图表编辑器集"),
        Some("自动选择图表类型"),
        Some("编辑值图表"),
        None,
        Some("编辑速度图表"),
        Some("显示参考图表"),
        Some("对齐"),
        Some("显示音频波形"),
        Some("显示图层的入点/出点"),
        None,
        Some("显示图层标记"),
        Some("显示图表工具技巧"),
        Some("显示表达式编辑器"),
    ];
    let ours: Vec<Option<String>> = crate::graph::AREA_MENU
        .iter()
        .map(|t| t.map(zh_of))
        .collect();
    assert_eq!(ours, ae.map(|t| t.map(str::to_owned)));
}

/// The parent pick whip with AE's modifiers: Shift moves the layer onto its
/// new parent; Cmd/Ctrl-click removes the parent.
#[test]
fn parent_pick_whip_shift_and_cmd_click() {
    let mut h = H::new();
    let comp = h.comp.clone();
    let solid = h.layer.clone();
    let nul = path(&h.run("layer.create", json!({"comp": comp, "kind": "null"}))["path"]);
    h.run(
        "attr.set",
        json!({"path": nul, "attr": "transform:position", "value": {"type": "vec3", "value": [100.0, 50.0, 0.0]}}),
    );
    let mut tl = TimelinePanel::default();
    h.session().lock().graph_editor = false;
    h.painted_text = Some(Vec::new());
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let nones = |h: &H| -> Vec<Rect> {
        let mut v: Vec<Rect> = h
            .painted_text
            .as_ref()
            .unwrap()
            .iter()
            .filter(|(s, _)| s == bae_ui::tr("None"))
            .map(|(_, r)| *r)
            .collect();
        v.sort_by(|a, b| a.center().y.total_cmp(&b.center().y));
        v
    };
    let rows = nones(&h);
    assert_eq!(rows.len(), 2);
    // The solid is layer 2; its pick whip sits left of its Parent dropdown.
    let whip = |r: Rect| pos2(r.left() - 18.0, r.center().y);
    let r = h.drag(&mut tl, whip(rows[1]), rows[0].center(), Modifiers::SHIFT);
    assert_eq!(r.invocations.len(), 1, "{:?}", r.invocations);
    assert_eq!(r.invocations[0].params["move_to_parent"], true);
    h.apply(&r);
    let parent = |h: &H| {
        h.stage
            .prim(&solid)
            .and_then(|p| p.value("parent").and_then(|v| v.as_prim_ref().cloned()))
    };
    assert_eq!(parent(&h), Some(nul.clone()));
    let e = bae_anim::evaluate_comp(&h.stage, &comp, Time::ZERO).unwrap();
    // The solid's anchor lands on the null's anchor.
    let anchor_on_screen = |l: &PrimPath| {
        let a = match query::attr_spec(&h.stage, l, "transform:anchor_point")
            .and_then(|s| s.value.clone())
            .or_else(|| {
                h.stage
                    .prim(l)
                    .and_then(|v| query::static_value(&v, "transform:anchor_point"))
            }) {
            Some(Value::Vec3(a)) => bae_anim::glam::DVec3::from_array(a),
            _ => bae_anim::glam::DVec3::ZERO,
        };
        e.layer(l).unwrap().world_matrix.transform_point3(a)
    };
    let (s_at, n_at) = (anchor_on_screen(&solid), anchor_on_screen(&nul));
    assert!((s_at - n_at).length() < 1e-6, "{s_at} vs {n_at}");
    // Cmd/Ctrl-click the pick whip: no parent.
    h.frame(&mut tl, Vec::new(), Modifiers::NONE);
    let r = h.click(&mut tl, whip(rows[1]), cmd());
    h.apply(&r);
    assert_eq!(parent(&h), None);
}
