use super::*;

#[test]
fn expression_typing_honours_pairing_and_indentation_in_the_workbench() {
    let mut h = gui();
    new_comp(&mut h);
    new_solid(&mut h);
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("scripting_expressions.expr_auto_complete", false);
    prefs.set("scripting_expressions.expr_use_tabs", false);
    prefs.set("scripting_expressions.expr_tab_width", 3);
    h.state().prefs.set(prefs.clone()).unwrap();
    let path = h.state().layers()[0].clone();
    h.state_mut().workspace.focus_panel("timeline");
    key(&mut h, Modifiers::NONE, Key::P);
    h.state_mut().dispatch(Invocation::with_params(
        "attr.set_expressions",
        serde_json::json!({"items":[{"path":path,"attr":"transform:position","source":"value"}]}),
    ));
    settle(&mut h);
    let field = find_text(&h, "value").unwrap();
    click_at(&mut h, field.center());
    let (mods, a) = cmd(Key::A);
    key(&mut h, mods, a);
    type_text(&mut h, "{");
    assert!(find_text(&h, "{}").is_some());
    key(&mut h, Modifiers::NONE, Key::Enter);
    assert!(find_text(&h, "{\n   \n}").is_some());
    key(&mut h, Modifiers::NONE, Key::Backspace);
    assert!(find_text(&h, "{\n\n}").is_some());
    key(&mut h, Modifiers::NONE, Key::Tab);
    send(&mut h, Event::Paste("[".into()));
    settle(&mut h);
    assert!(find_text(&h, "{\n   [\n}").is_some());
    key(&mut h, mods, a);
    key(&mut h, Modifiers::NONE, Key::Tab);
    assert!(find_text(&h, "   {\n      [\n   }").is_some());
    key(&mut h, Modifiers::SHIFT, Key::Tab);
    assert!(find_text(&h, "{\n   [\n}").is_some());
    prefs.set("scripting_expressions.expr_use_tabs", true);
    prefs.set("scripting_expressions.expr_auto_insert_braces", false);
    h.state().prefs.set(prefs).unwrap();
    settle(&mut h);
    key(&mut h, mods, a);
    type_text(&mut h, "[");
    key(&mut h, Modifiers::NONE, Key::Tab);
    type_text(&mut h, "值");
    assert!(find_text(&h, "[\t值").is_some());
}

#[test]
fn expression_gutter_and_guides_follow_preferences_in_the_timeline() {
    let mut h = gui();
    new_comp(&mut h);
    new_solid(&mut h);
    let path = h.state().layers()[0].clone();
    h.state_mut().workspace.focus_panel("timeline");
    key(&mut h, Modifiers::NONE, Key::P);
    h.state_mut().dispatch(Invocation::with_params(
        "attr.set_expressions",
        serde_json::json!({
            "items":[{"path":path,"attr":"transform:position","source":"var x = 1;\n    [x, x]"}]
        }),
    ));
    let numbers = egui::Color32::from_rgb(0xc7, 0x14, 0xad);
    let guides = egui::Color32::from_rgb(0x11, 0xa6, 0xb2);
    for on in [true, false] {
        let mut prefs = (*h.state().prefs.current()).clone();
        prefs.set("scripting_expressions.expr_color_line_numbers", "#c714ad");
        prefs.set("scripting_expressions.expr_color_indent_guides", "#11a6b2");
        prefs.set("scripting_expressions.expr_show_line_numbers", on);
        prefs.set("scripting_expressions.expr_show_indent_guides", on);
        h.state().prefs.set(prefs).unwrap();
        settle(&mut h);
        for number in ["1", "2"] {
            assert_eq!(h.output().shapes.iter().any(|s| matches!(&s.shape,
                egui::Shape::Text(t) if t.galley.text() == number && t.galley.job.sections.iter().any(|s| s.format.color == numbers)
            )), on, "line {number}");
        }
        assert_eq!(
            h.output().shapes.iter().any(|s| matches!(&s.shape,
                egui::Shape::LineSegment { stroke, .. } if stroke.color == guides
            )),
            on,
            "indent guide"
        );
    }
}

#[test]
fn timeline_shows_the_evaluated_expression_error_inline() {
    let mut h = gui();
    new_comp(&mut h);
    new_solid(&mut h);
    let path = h.state().layers()[0].clone();
    h.state_mut().workspace.focus_panel("timeline");
    key(&mut h, Modifiers::NONE, Key::P);
    h.state_mut().dispatch(Invocation::with_params(
        "attr.set_expressions",
        serde_json::json!({"items":[{"path":path,"attr":"transform:position","source":"missingInlineValue + 1"}]}),
    ));
    for enabled in [true, false, true] {
        let mut p = (*h.state().prefs.current()).clone();
        p.set(
            "scripting_expressions.show_expression_error_warning_banner",
            false,
        );
        p.set("scripting_expressions.expr_inline_errors", enabled);
        h.state().prefs.set(p).unwrap();
        settle(&mut h);
        let msg = h
            .state()
            .session()
            .lock()
            .expr_errors
            .iter()
            .find(|e| e.prim == path && e.attr == "transform:position")
            .expect("the viewer evaluated the invalid expression")
            .message
            .clone();
        assert_eq!(
            find_text(&h, msg.lines().next().unwrap()).is_some(),
            enabled,
            "{msg}"
        );
    }
}
