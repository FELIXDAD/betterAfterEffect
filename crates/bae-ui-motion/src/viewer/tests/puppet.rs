//! Puppet tools: a click adds a pin (and the Puppet effect), a drag moves it.

use super::*;

#[test]
fn puppet_pins_are_added_on_a_click_and_dragged_in_one_step() {
    let mut h = H::new();
    let solid = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.select(&[&solid]);
    h.frame(Vec::new(), Modifiers::NONE);
    h.tools().lock().set(Tool::PuppetPosition);
    let r = h.click(320.0, 180.0, Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "{:?}", r.invocations);
    assert_eq!(r.invocations[0].id, "puppet.add_pin");
    h.apply(&r);
    let effects = bae_motion::effect::effects_of(&h.stage, &solid);
    assert_eq!(effects.len(), 1, "the first pin adds the Puppet effect");
    let pins = bae_motion::puppet::pins_of(&h.stage, &effects[0]);
    assert_eq!(pins.len(), 1);
    let pin = pins[0].clone();
    // Layer pixels: the solid's centre.
    assert!(close(vec2v(h.value(&pin, "rest")), [100.0, 50.0]));
    // A second pin on the same effect.
    let r = h.click(260.0, 160.0, Modifiers::NONE);
    h.apply(&r);
    assert_eq!(bae_motion::puppet::pins_of(&h.stage, &effects[0]).len(), 2);
    assert_eq!(bae_motion::effect::effects_of(&h.stage, &solid).len(), 1);
    // Dragging the first pin: one undo step at the end.
    h.drag_to((320.0, 180.0), (340.0, 190.0), Modifiers::NONE);
    let r = h.release((340.0, 190.0), Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "one undo step");
    h.apply(&r);
    let ev = bae_anim::evaluate_comp(&h.stage, &h.comp, bae_model::Time::ZERO).unwrap();
    let moved = ev.layer(&solid).unwrap().effects[0]
        .children
        .iter()
        .find(|c| c.path == pin)
        .and_then(|c| c.get("position").cloned());
    let moved = vec2v(moved);
    assert!(close(moved, [120.0, 60.0]), "{moved:?}");
    assert!(
        close(vec2v(h.value(&pin, "rest")), [100.0, 50.0]),
        "rest stays"
    );
}
