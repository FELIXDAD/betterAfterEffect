use super::*;
use bae_model::Value;
use bae_motion::query;
use bae_ui_motion::{SharedTools, Tool};
use egui_kittest::kittest::Queryable as _;

fn canvas(h: &Ui) -> Rect {
    h.root()
        .query_all_by_label("Composition")
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("composition viewer")
}

fn tool(h: &Ui) -> Tool {
    h.state().app.resource::<SharedTools>().unwrap().current()
}
#[test]
fn vertical_type_creates_ime_text_and_the_text_menu_converts_it() {
    let mut h = gui();
    let command = Modifiers {
        ctrl: !cfg!(target_os = "macos"),
        ..Modifiers::COMMAND
    };
    comp_with(&mut h, &[]);
    key(&mut h, command, Key::T);
    key(&mut h, command, Key::T);
    assert_eq!(tool(&h), Tool::TypeVertical);
    let at = canvas(&h).center();
    click_at(&mut h, at);
    ime_compose(&mut h, &["zhong", "中"], "中");
    type_text(&mut h, "12A");
    key(&mut h, command, Key::Enter);
    let layer = h.state().layers()[0].clone();
    let document = |h: &Ui| {
        let Some(Value::TextDocument(doc)) = query::sample_attr(
            h.state().stage(),
            &layer,
            "text:source",
            bae_model::Time::ZERO,
        ) else {
            panic!("expected Source Text")
        };
        doc
    };
    let d = document(&h);
    assert_eq!(d.text, "中12A");
    assert_eq!(d.line_orientation, bae_model::LineOrientation::Vertical);
    h.state().session().lock().selected_layers = vec![layer.clone()];
    to_front(&mut h, "character");
    let panel_menu = bae_ui::a11y::context(&["Character", "Panel Menu"]);
    let button = h.root().get_by_label(&panel_menu).rect().center();
    click_at(&mut h, button);
    let roman = h
        .root()
        .get_by_label("Standard Vertical Roman Alignment")
        .rect()
        .center();
    click_at(&mut h, roman);
    settle(&mut h);
    assert_eq!(
        document(&h).style_at(3).baseline_direction,
        bae_model::BaselineDirection::UprightRoman
    );
    h.state_mut().invoke(bae_ui::Invocation::with_params("text.set_style",
        serde_json::json!({"layers":[layer],"range":{"start":1,"end":3},"style":{"baseline_direction":"tate_chu_yoko","fill_color":[1.,0.,0.,1.]}})));
    settle(&mut h);
    let runs = document(&h).runs.clone();
    assert_eq!(
        document(&h).style_at(1).baseline_direction,
        bae_model::BaselineDirection::TateChuYoko
    );
    // The viewer's text menu runs Horizontal / Vertical through this action.
    for (direction, orientation) in [
        ("horizontal", bae_model::LineOrientation::Horizontal),
        ("vertical", bae_model::LineOrientation::Vertical),
    ] {
        h.state_mut().invoke(bae_ui::Invocation::with_params(
            "motion.layer.text_orientation",
            serde_json::json!({ "orientation": direction }),
        ));
        settle(&mut h);
        let d = document(&h);
        assert_eq!(d.line_orientation, orientation);
        assert_eq!(d.runs, runs);
    }
}
