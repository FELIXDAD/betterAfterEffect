use super::*;

/// AE 26 hides these legacy effects (its `_Obsolete` category): they are not
/// in the Effect menu, but old projects and scripts still add them by match
/// name.
#[test]
fn hidden_classic_particles_and_photoshop_styles_apply_by_match_name() {
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
        assert!(
            !h.state().menu_items().into_iter().any(|i| i
                .path
                .first()
                .is_some_and(|p| p == "Effect")
                && i.invocation.params["match_name"] == name),
            "{name} is not in the Effect menu"
        );
        h.state_mut().invoke(Invocation::with_params(
            bae_ui_motion::effects::APPLY,
            serde_json::json!({ "match_name": name }),
        ));
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
