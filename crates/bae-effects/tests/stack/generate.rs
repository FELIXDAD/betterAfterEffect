use super::*;

#[test]
fn write_on_defaults_to_the_layer_center_on_apply_and_reset() {
    assert_reference_points(&[("ADBE Write-on", 1, [15.5, 6.5])]);
}

#[test]
fn stroke_reads_a_mask_in_none_mode() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.solid(&comp, [0.2; 3], [64, 32]);
    let mask = fx.run(
        "mask.create",
        json!({"layer": layer, "kind": "rect", "position": [32., 16.], "size": [32., 16.]}),
    );
    fx.run(
        "attr.set",
        json!({"path": mask["mask"], "attr": "mode", "value": {"type": "enum", "value": "none"}}),
    );
    let effect = fx.apply(&layer, "ADBE Stroke");
    fx.param(
        &effect,
        1,
        json!({"type": "prim_ref", "value": mask["mask"]}),
    );
    fx.param(&effect, 7, json!({"type": "int", "value": 2}));
    let Some(edge) = fx.pixel(&comp, 24, 8) else {
        return;
    };
    assert!(edge[3] > 200);
    assert_eq!(fx.pixel(&comp, 24, 16).unwrap(), [0; 4]);
}
