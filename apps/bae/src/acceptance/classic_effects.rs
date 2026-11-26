use super::*;

#[test]
fn obsolete_menu_applies_classic_particles_and_photoshop_styles() {
    let mut h = gui_with(Gui::with_app(crate::app_builder().build()));
    new_comp(&mut h);
    new_solid(&mut h);
    let layer = h.state().layers()[0].clone();
    for name in [
        "CC PS Classic",
        "CC PS LE Classic",
        "ADBE PSL Drop Shadow",
        "ADBE PSL Inner Shadow",
        "ADBE PSL Outer Glow",
        "ADBE PSL Inner Glow",
        "ADBE PSL Bevel Emboss",
    ] {
        let item = h
            .state()
            .menu_items()
            .into_iter()
            .find(|i| i.path == ["Effect", "Obsolete"] && i.invocation.params["match_name"] == name)
            .expect(name);
        assert!(h.state_mut().menu(&item.id), "{name} disabled");
        settle(&mut h);
        let effect = bae_motion::effect::effects_of(h.state().stage(), &layer)
            .pop()
            .unwrap();
        let prim = h.state().stage().prim(&effect).unwrap();
        assert_eq!(
            prim.value("match_name").and_then(|v| v.as_str()),
            Some(name)
        );
    }
}
