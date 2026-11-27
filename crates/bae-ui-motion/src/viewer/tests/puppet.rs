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
    // The Timeline shows Puppet ▸ Mesh 1 ▸ Deform ▸ Puppet Pin 1 ▸ Position.
    let labels = |n: &crate::props::Node| match n {
        crate::props::Node::Group {
            label, children, ..
        } => (label.clone(), children.clone()),
        crate::props::Node::Prop { label, .. } => (label.clone(), Vec::new()),
    };
    let fx = crate::props::effect_nodes(&h.stage, &solid);
    let (_, fx_children) = labels(&fx[0]);
    let (mesh, mesh_children) = labels(fx_children.last().unwrap());
    assert_eq!(mesh, "Mesh 1");
    let (deform, pin_rows) = labels(&mesh_children[0]);
    assert_eq!(deform, "Deform");
    let (first, props) = labels(&pin_rows[0]);
    assert_eq!(first, "Puppet Pin 1");
    assert_eq!(labels(&props[0]).0, "Position");
    // Shift adds the second pin; a drag moves both; Delete removes both in
    // one step (the effect stays).
    let second = bae_motion::puppet::pins_of(&h.stage, &effects[0])[1].clone();
    let pin_at = |h: &H, p: &PrimPath| {
        let ev = bae_anim::evaluate_comp(&h.stage, &h.comp, bae_model::Time::ZERO).unwrap();
        let v = ev.layer(&solid).unwrap().effects[0]
            .children
            .iter()
            .find(|c| c.path == *p)
            .and_then(|c| c.get("position").cloned());
        vec2v(v)
    };
    let before = pin_at(&h, &second);
    h.click(340.0, 190.0, Modifiers::NONE);
    h.click(260.0, 160.0, Modifiers::SHIFT);
    h.drag_to((340.0, 190.0), (350.0, 190.0), Modifiers::NONE);
    let r = h.release((350.0, 190.0), Modifiers::NONE);
    assert_eq!(r.invocations.len(), 1, "one undo step for both");
    h.apply(&r);
    let after = pin_at(&h, &second);
    assert!(close(after, [before[0] + 10.0, before[1]]), "{after:?}");
    let r = h.action_with(crate::actions::ids::DELETE, json!({}));
    assert_eq!(r.invocations.len(), 1);
    assert_eq!(r.invocations[0].id, "puppet.delete_pins");
    h.apply(&r);
    assert!(bae_motion::puppet::pins_of(&h.stage, &effects[0]).is_empty());
    assert_eq!(bae_motion::effect::effects_of(&h.stage, &solid).len(), 1);
}

/// A selected Advanced pin's ring rotates it, its square scales it (live,
/// one undo step each).
#[test]
fn advanced_pins_turn_on_their_ring_and_scale_on_its_square() {
    let mut h = H::new();
    let solid = h.solid([200.0, 100.0], [320.0, 180.0]);
    h.select(&[&solid]);
    h.frame(Vec::new(), Modifiers::NONE);
    h.tools().lock().set(Tool::PuppetAdvanced);
    let r = h.click(320.0, 180.0, Modifiers::NONE);
    h.apply(&r);
    let effects = bae_motion::effect::effects_of(&h.stage, &solid);
    let pin = bae_motion::puppet::pins_of(&h.stage, &effects[0])[0].clone();
    // Select it: the ring appears (radius 24 points).
    h.click(320.0, 180.0, Modifiers::NONE);
    let centre = h.screen(320.0, 180.0);
    let value = |h: &H, attr: &str| {
        let ev = bae_anim::evaluate_comp(&h.stage, &h.comp, bae_model::Time::ZERO).unwrap();
        ev.layer(&solid).unwrap().effects[0]
            .children
            .iter()
            .find(|c| c.path == pin)
            .and_then(|c| c.f64(attr))
            .unwrap()
    };
    let drag = |h: &mut H, from: Pos2, to: Pos2| {
        h.move_to(from, Modifiers::NONE, Vec::new());
        h.pointer_button(from, true, Modifiers::NONE);
        for i in 1..=6 {
            h.move_to(
                from + (to - from) * (i as f32 / 6.0),
                Modifiers::NONE,
                Vec::new(),
            );
        }
        h.pointer_button(to, false, Modifiers::NONE)
    };
    // The ring below the pin, dragged a quarter turn back: −90°.
    let r = drag(
        &mut h,
        centre + egui::vec2(0.0, 24.0),
        centre + egui::vec2(24.0, 0.0),
    );
    assert_eq!(r.invocations.len(), 1, "one undo step");
    h.apply(&r);
    assert!(
        (value(&h, "rotation") + 90.0).abs() < 1.0,
        "{}",
        value(&h, "rotation")
    );
    // The square now sits at −90° (above the pin): dragged out to twice the
    // radius, 200 %.
    let r = drag(
        &mut h,
        centre + egui::vec2(0.0, -24.0),
        centre + egui::vec2(0.0, -48.0),
    );
    h.apply(&r);
    assert!(
        (value(&h, "scale") - 200.0).abs() < 2.0,
        "{}",
        value(&h, "scale")
    );
}
