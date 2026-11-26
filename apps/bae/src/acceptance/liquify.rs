use super::*;
use bae_effects::effects::liquify::{MATCH_NAME, MESH, Mesh};
use egui_kittest::kittest::Queryable as _;

#[test]
fn liquify_tool_opens_the_viewer_and_a_stroke_undoes_once() {
    let mut app = Gui::with_app(crate::app_builder().build());
    if !app.with_gpu() {
        return;
    }
    let mut h = gui_with(app);
    new_comp(&mut h);
    new_solid(&mut h);
    h.state_mut().invoke(Invocation::with_params(
        "motion.effect.apply",
        serde_json::json!({"match_name": MATCH_NAME}),
    ));
    settle(&mut h);
    let tool = h.get_by_label("Bloat").rect().center();
    click_at(&mut h, tool);
    settle(&mut h);
    let area = h
        .root()
        .query_all_by_label("Liquify")
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap();
    assert!(area.height() > 100.);
    let center = area.center();
    drag(
        &mut h,
        center,
        center + egui::vec2(24., 4.),
        Modifiers::NONE,
    );
    let layer = h.state().layers()[0].clone();
    let effect = bae_motion::effect::effects_of(h.state().stage(), &layer)[0].clone();
    let mesh = |h: &Ui| {
        let prim = h.state().stage().prim(&effect).unwrap();
        bae_motion::query::static_value(&prim, &bae_model::effect::param_attr(MESH))
            .and_then(|v| Mesh::from_value(&v))
    };
    let start = std::time::Instant::now();
    while mesh(&h).is_none() && start.elapsed().as_secs() < 10 {
        frame(&mut h);
    }
    assert!(mesh(&h).unwrap().reach() > 0.);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert!(mesh(&h).is_none());
}
