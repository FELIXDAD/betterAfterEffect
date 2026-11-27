use super::*;

#[test]
fn enabling_motion_blur_from_the_menu_can_enable_the_comp_in_one_undo_step() {
    let mut h = gui();
    new_comp(&mut h);
    new_solid(&mut h);
    let comp = h.state().active_comp().unwrap();
    let layer = h.state().layers()[0].clone();
    let item = h
        .state()
        .menu_items()
        .into_iter()
        .find(|m| m.invocation.params.get("switch").and_then(|v| v.as_str()) == Some("motion_blur"))
        .expect("motion blur menu")
        .id;
    let on = |h: &Ui, path: &PrimPath| {
        h.state()
            .stage()
            .prim(path)
            .and_then(|v| bae_motion::query::static_value(&v, "motion_blur"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };
    assert!(h.state_mut().menu(&item));
    settle(&mut h);
    assert!(on(&h, &layer) && on(&h, &comp));
    let (m, k) = cmd(Key::Z);
    key(&mut h, m, k);
    assert!(!on(&h, &layer) && !on(&h, &comp));
    let mut prefs = (*h.state().prefs.current()).clone();
    prefs.set("previews.auto_enable_frame_blending_motion_blur", false);
    h.state().prefs.set(prefs).unwrap();
    assert!(h.state_mut().menu(&item));
    settle(&mut h);
    assert!(on(&h, &layer) && !on(&h, &comp));
}
