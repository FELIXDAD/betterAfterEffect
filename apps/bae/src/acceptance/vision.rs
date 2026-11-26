use super::*;
use egui_kittest::kittest::Queryable as _;

#[test]
fn roto_brush_layer_strokes_and_undo() {
    let mut h = gui_with(Gui::with_app(crate::app_builder().build()));
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let layer = h.state().layers()[0].clone();
    key(&mut h, Modifiers::ALT, Key::W);
    h.state_mut().invoke(Invocation::with_params(
        "window.panel",
        serde_json::json!({"id":"layer"}),
    ));
    settle(&mut h);
    let center = h.root().get_by_label("Roto Brush Canvas").rect().center();
    drag(
        &mut h,
        center,
        center + egui::vec2(30., 0.),
        Modifiers::NONE,
    );
    let read = |h: &Ui| {
        let effect = bae_motion::effect::effects_of(h.state().stage(), &layer)
            .into_iter()
            .next()?;
        let prim = h.state().stage().prim(&effect)?;
        match prim.value("param:0000")? {
            bae_model::Value::FloatList(v) => Some(v.clone()),
            _ => None,
        }
    };
    let fg = read(&h).expect("foreground stroke on a new Roto Brush effect");
    drag(
        &mut h,
        center + egui::vec2(0., 22.),
        center + egui::vec2(30., 22.),
        Modifiers::ALT,
    );
    let both = read(&h).unwrap();
    assert!(both[fg.len()..].chunks_exact(6).all(|s| s[1] < 0.) && both.len() > fg.len());
    let (modifiers, key_z) = cmd(Key::Z);
    key(&mut h, modifiers, key_z);
    assert_eq!(read(&h), Some(fg));
    key(&mut h, modifiers, key_z);
    assert_eq!(read(&h), None);
}
