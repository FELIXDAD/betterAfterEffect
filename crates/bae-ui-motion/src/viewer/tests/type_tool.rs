//! Type tool: point and paragraph text, input methods, editing existing text.

use super::*;

#[test]
fn type_tool_click_and_type_makes_point_text() {
    let mut h = H::new();
    h.tools().lock().set(Tool::TypeHorizontal);
    let r = h.click(100.0, 200.0, Modifiers::NONE);
    assert!(r.invocations.is_empty());
    assert!(h.viewer.type_tool.editing.is_some());
    let r = h.type_text(vec![Event::Text("Hi".into())]);
    assert!(r.invocations.is_empty(), "nothing until committed");
    // Typing keys go to the field, not to tool shortcuts (V, H …).
    h.type_text(vec![Event::Text("V".into())]);
    assert_eq!(h.tools().lock().current(), Tool::TypeHorizontal);
    let r = h.commit_text();
    let p = one(&r, "text.create");
    assert_eq!(p["text"], "HiV");
    assert_eq!(p["position"], json!([100.0, 200.0]));
    assert!(p.get("box_size").is_none());
    h.apply(&r);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    assert_eq!(h.name(&layer), "HiV");
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&layer)
    );
    assert_eq!(h.doc(&layer).text, "HiV");
    assert!(close3(
        h.v3(&layer, "transform:position"),
        [100.0, 200.0, 0.0]
    ));
    assert!(close3(
        h.v3(&layer, "transform:anchor_point"),
        [0.0, 0.0, 0.0]
    ));
    // One undo step.
    h.stage.undo(&h.author).unwrap();
    assert!(query::comp_layers(&h.stage, &h.comp).is_empty());
    // Esc cancels (AE, #573): no new text layer.
    h.click(300.0, 200.0, Modifiers::NONE);
    h.type_text(vec![Event::Text("Gone".into())]);
    let r = h.escape();
    assert!(r.invocations.is_empty(), "{:?}", r.invocations);
    assert!(h.viewer.type_tool.editing.is_none());
    // Committing without typing still makes an (empty) text layer (AE 26).
    h.click(300.0, 200.0, Modifiers::NONE);
    let r = h.commit_text();
    let p = one(&r, "text.create");
    assert_eq!(p["text"], "");
    assert_eq!(p["name"], "<empty text layer>");
    // While editing, clicking elsewhere in the viewer keeps editing; Cmd/Ctrl+
    // Enter commits.
    h.click(100.0, 300.0, Modifiers::NONE);
    h.type_text(vec![Event::Text("A".into())]);
    let r = h.click(500.0, 100.0, Modifiers::NONE);
    assert!(r.invocations.is_empty(), "{:?}", r.invocations);
    assert!(h.viewer.type_tool.editing.is_some());
    h.type_text(vec![Event::Text("B".into())]);
    let enter = |pressed| Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    };
    let r = h.frame(vec![enter(true)], Modifiers::COMMAND);
    h.frame(vec![enter(false)], Modifiers::COMMAND);
    assert_eq!(one(&r, "text.create")["text"], "AB");
}

#[test]
fn type_tool_takes_input_method_composition() {
    let mut h = H::new();
    h.tools().lock().set(Tool::TypeHorizontal);
    h.click(100.0, 200.0, Modifiers::NONE);
    use egui::ImeEvent;
    let preedit = |t: &str| {
        Event::Ime(ImeEvent::Preedit {
            text: t.into(),
            active_range_chars: None,
        })
    };
    h.type_text(vec![preedit("ni")]);
    h.type_text(vec![preedit("ni'hao")]);
    assert_eq!(h.tools().lock().current(), Tool::TypeHorizontal);
    h.type_text(vec![
        preedit(""),
        Event::Ime(ImeEvent::Commit("你好".into())),
    ]);
    let r = h.commit_text();
    assert_eq!(one(&r, "text.create")["text"], "你好");
}

#[test]
fn type_tool_drag_makes_paragraph_text() {
    let mut h = H::new();
    h.tools().lock().set(Tool::TypeHorizontal);
    let r = h.drag((100.0, 50.0), (300.0, 150.0), Modifiers::NONE);
    assert!(r.invocations.is_empty());
    h.type_text(vec![Event::Text("Box".into())]);
    // Switching tools commits too.
    h.tools().lock().set(Tool::Selection);
    let r = h.frame(Vec::new(), Modifiers::NONE);
    let p = one(&r, "text.create");
    assert_eq!(p["box_size"], json!([200.0, 100.0]));
    assert_eq!(p["position"], json!([100.0, 50.0]));
    h.apply(&r);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    let d = h.doc(&layer);
    assert_eq!(d.box_size, Some([200.0, 100.0]));
    // AE 26: the layer sits at the box's center, the box offset by half.
    assert_eq!(d.box_position, Some([-100.0, -50.0]));
    assert!(close3(
        h.v3(&layer, "transform:position"),
        [200.0, 100.0, 0.0]
    ));
}

#[test]
fn type_tool_edits_an_existing_text_layer() {
    let mut h = H::new();
    let layer = path(
        &h.run(
            "text.create",
            json!({"comp": h.comp, "text": "Hello", "position": [100.0, 200.0]}),
        )["path"],
    );
    h.tools().lock().set(Tool::TypeHorizontal);
    // A click at the start of the text (its first line sits above the
    // baseline): the caret goes there (AE).
    h.click(101.0, 180.0, Modifiers::NONE);
    assert_eq!(
        h.viewer.type_tool.edited_layer(),
        Some(&layer),
        "editing the layer"
    );
    assert_eq!(
        h.session().lock().selected_layers,
        std::slice::from_ref(&layer)
    );
    h.type_text(vec![Event::Text("!".into())]);
    let r = h.commit_text();
    let p = one(&r, "text.set_text");
    assert_eq!(p["layer"], json!(layer));
    let text = p["text"].as_str().unwrap().to_owned();
    assert_eq!(text, "!Hello");
    h.apply(&r);
    assert_eq!(h.doc(&layer).text, text);
    assert_eq!(h.doc(&layer).style.font_size, 72.0, "style kept");
    h.stage.undo(&h.author).unwrap();
    assert_eq!(h.doc(&layer).text, "Hello");
    // Past the end of the text: the caret goes to the end.
    h.click(295.0, 180.0, Modifiers::NONE);
    h.type_text(vec![Event::Text("?".into())]);
    let r = h.commit_text();
    assert_eq!(one(&r, "text.set_text")["text"], "Hello?");
    // Layer > New > Text (E) asks the viewer to edit a layer.
    h.tools().lock().set(Tool::Selection);
    h.tools().lock().edit_text_request = Some(layer.clone());
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(h.tools().lock().current(), Tool::TypeHorizontal);
    assert_eq!(h.viewer.type_tool.edited_layer(), Some(&layer));
}

#[test]
fn type_selection_drags_and_extends_by_character_then_styles_once() {
    let mut h = H::new();
    let layer = path(
        &h.run(
            "text.create",
            json!({"comp":h.comp,"text":"Hello","position":[100.0,200.0]}),
        )["path"],
    );
    h.tools().lock().set(Tool::TypeHorizontal);
    h.drag((101., 180.), (150., 180.), Modifiers::NONE);
    let selected = h.session().lock().text_range(Some(&layer)).unwrap();
    assert_eq!(selected.start, 0);
    assert!((1..5).contains(&selected.end));
    let key = |pressed| Event::Key {
        key: Key::ArrowRight,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::SHIFT,
    };
    h.frame(vec![key(true)], Modifiers::SHIFT);
    h.frame(vec![key(false)], Modifiers::SHIFT);
    let extended = h.session().lock().text_range(Some(&layer)).unwrap();
    assert_eq!(extended, 0..selected.end + 1);
    h.run(
        "text.set_style",
        json!({"layers":[layer],"range":extended,"style":{"font_size":33,"faux_bold":true}}),
    );
    let doc = h.doc(&layer);
    assert!(doc.style_at(selected.end).faux_bold);
    assert_eq!(doc.style_at(extended.end).font_size, 72.);
    h.stage.undo(&h.author).unwrap();
    assert!(h.doc(&layer).runs.is_empty());
}

#[test]
fn vertical_box_and_selected_range_survive_editing() {
    let mut h = H::new();
    h.tools().lock().set(Tool::TypeVertical);
    h.drag((100., 50.), (300., 250.), Modifiers::NONE);
    h.type_text(vec![Event::Text("中12A".into())]);
    let r = h.commit_text();
    h.apply(&r);
    let layer = query::comp_layers(&h.stage, &h.comp)[0].clone();
    let d = h.doc(&layer);
    assert_eq!(d.line_orientation, bae_model::LineOrientation::Vertical);
    assert_eq!(d.box_size, Some([200., 200.]));
    crate::ae_panels::character::prewarm();
    let start = std::time::Instant::now();
    let layout = loop {
        if let Some(l) = crate::ae_panels::character::hit_layout(&d) {
            break l;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(15),
            "font scan"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let pos = h.v3(&layer, "transform:position");
    let at = |i: usize, t: f64| {
        let c = &layout.chars[i];
        (pos[0] + c.origin.x, pos[1] + c.origin.y + c.advance * t)
    };
    h.drag(at(1, 0.05), at(2, 0.95), Modifiers::NONE);
    let selected = h.session().lock().text_range(Some(&layer)).unwrap();
    assert_eq!(selected, 1..3);
    h.run(
        "text.set_style",
        json!({"layers":[layer],"range":selected,"style":{"baseline_direction":"tate_chu_yoko"}}),
    );
    h.escape();
    h.tools().lock().edit_text_request = Some(layer.clone());
    h.frame(Vec::new(), Modifiers::NONE);
    assert_eq!(h.tools().lock().current(), Tool::TypeVertical);
    h.type_text(vec![Event::Text("尾".into())]);
    let r = h.commit_text();
    h.apply(&r);
    let d = h.doc(&layer);
    assert_eq!(d.text, "中12A尾");
    assert_eq!(
        d.style_at(2).baseline_direction,
        bae_model::BaselineDirection::TateChuYoko
    );
    assert_eq!(
        d.style_at(3).baseline_direction,
        bae_model::BaselineDirection::Default
    );
}

/// With a Type tool, right-clicking a text layer shows AE's text menu; its
/// Vertical runs the writing direction action, and the Layer menu has no
/// such item (AE 26).
#[test]
fn the_text_menu_sets_the_writing_direction() {
    let mut h = H::new();
    let text = path(
        &h.run(
            "layer.create",
            json!({"comp": h.comp, "kind": "text", "text": "Hello World"}),
        )["path"],
    );
    h.tools().lock().set(Tool::TypeHorizontal);
    h.frame(Vec::new(), Modifiers::NONE);
    let c = {
        let comp = bae_anim::evaluate_comp(&h.stage, &h.comp, bae_model::Time::ZERO).unwrap();
        let l = comp.layer(&text).unwrap();
        let (a, b) = crate::render::layer_bounds(l).unwrap();
        let c = l.world_matrix.transform_point3(((a + b) / 2.0).extend(0.0));
        (c.x, c.y)
    };
    h.right_click(c.0, c.1);
    assert!(h.labeled("Edit Text...").is_some(), "the text menu");
    assert!(h.labeled("Horizontal").is_some());
    let r = h.choose("Vertical");
    let inv = r
        .invocations
        .iter()
        .find(|i| i.id == crate::actions::ids::TEXT_ORIENTATION)
        .expect("writing direction");
    assert_eq!(inv.params["orientation"], json!("vertical"));
    assert!(
        crate::menus::items()
            .iter()
            .all(|(id, _)| !id.contains("convert_to_vertical")),
        "not in the Layer menu"
    );
}
