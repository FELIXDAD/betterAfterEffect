use super::*;

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
