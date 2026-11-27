use super::*;
use bae_motion::{camera_track, effect, query};

#[test]
fn animation_track_camera_applies_the_perspective_effect() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let plate = h
        .state()
        .selected_layers()
        .first()
        .expect("selected plate")
        .clone();
    let item = h
        .state()
        .menu_items()
        .into_iter()
        .find(|i| i.invocation.id == "motion.anim.track_camera")
        .expect("Animation > Track Camera");
    assert!(h.state_mut().menu(&item.id));
    settle(&mut h);
    let stage = h.state().stage();
    let effects = effect::effects_of(stage, &plate);
    assert_eq!(effects.len(), 1);
    assert_eq!(
        stage
            .prim(&effects[0])
            .and_then(|v| query::static_value(&v, "match_name"))
            .and_then(|v| v.as_str().map(str::to_owned))
            .as_deref(),
        Some(camera_track::MATCH_NAME)
    );
}
