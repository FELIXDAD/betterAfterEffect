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
    assert_eq!(h.doc(&layer).font_size, 72.0, "style kept");
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
