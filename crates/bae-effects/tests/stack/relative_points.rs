use super::*;

#[test]
fn relative_point_defaults_follow_the_layer_on_apply_and_reset() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.solid(&comp, [1.0, 0.0, 0.0], [31, 13]);
    let before = fx.pixel(&comp, 32, 12);
    // Declarative packages give point defaults as fractions of the layer.
    for (name, id, expected) in [
        ("ADBE Ramp", 1, [15.5, 0.0]),
        ("ADBE Ramp", 3, [15.5, 13.0]),
        ("ADBE Radial Wipe", 3, [15.5, 6.5]),
    ] {
        let effect = fx.apply(&layer, name);
        let attr = format!("param:{id:04}");
        let point = |fx: &Fx| fx.stage.prim(&effect).unwrap().value(&attr).cloned();
        let expected = Some(bae_model::Value::Vec2(expected));
        assert_eq!(point(&fx), expected, "{name} {id}");
        fx.param(&effect, id, json!({"type":"vec2","value":[4.0,5.0]}));
        fx.run("effect.reset", json!({"effects":[effect]}));
        assert_eq!(point(&fx), expected, "{name} {id}");
        fx.run("effect.remove", json!({"effects":[effect]}));
    }
    // The written values render as the implicit defaults did.
    let ramp = fx.apply(&layer, "ADBE Ramp");
    let written = fx.pixel(&comp, 32, 12);
    for id in [1, 3] {
        fx.run(
            "attr.clear",
            json!({"path": ramp, "attr": format!("param:{id:04}")}),
        );
    }
    assert_eq!(fx.pixel(&comp, 32, 12), written);
    assert!(before.is_none() || written != before);
}
