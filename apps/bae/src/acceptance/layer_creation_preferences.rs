use super::*;
use bae_model::Value;
use bae_motion::query;

#[test]
fn new_solids_follow_the_compositions_duration_and_position_preferences() {
    let mut h = gui();
    new_comp(&mut h);
    let comp = h.state().active_comp().unwrap();
    let info = query::comp_info(h.state().stage(), &comp).unwrap();
    let mut p = (*h.state().prefs.current()).clone();
    p.set("composition.default_position_separate_dimensions", true);
    p.set(
        "composition.synthetic_layers_duration_mode",
        "fixed_duration",
    );
    p.set(
        "composition.synthetic_layers_duration_timecode",
        "0:00:01:07",
    );
    h.state().prefs.set(p).unwrap();
    new_solid(&mut h);
    let solid = h.state().layers()[0].clone();
    let stage = h.state().stage();
    let view = stage.prim(&solid).unwrap();
    let rate = info.frame_rate;
    let expected = rate.frame_to_time(rate.as_f64().round() as i64 + 7);
    assert_eq!(
        query::static_value(&view, "out_point"),
        Some(Value::Time(expected))
    );
    assert!(bae_motion::dimensions::is_separated(stage, &solid));
    assert_eq!(
        query::static_value(&view, bae_motion::dimensions::COMPONENTS[0]),
        Some(Value::Float(f64::from(info.width) / 2.0))
    );
    let mut p = (*h.state().prefs.current()).clone();
    p.set(
        "composition.synthetic_layers_duration_mode",
        "length_of_composition",
    );
    p.set("composition.default_position_separate_dimensions", false);
    h.state().prefs.set(p).unwrap();
    new_solid(&mut h);
    let next = &h.state().layers()[0];
    let stage = h.state().stage();
    assert!(!bae_motion::dimensions::is_separated(stage, next));
    let end = query::static_value(&stage.prim(next).unwrap(), "out_point")
        .and_then(|v| v.as_time())
        .unwrap_or(info.duration);
    assert_eq!(end, info.duration);
}
