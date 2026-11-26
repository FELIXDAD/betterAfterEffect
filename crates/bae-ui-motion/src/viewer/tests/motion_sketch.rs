//! Motion sketch.

use super::*;

#[test]
fn motion_sketch_records_the_drag_as_position_keyframes() {
    let mut h = H::new();
    let solid = h.layer("solid");
    h.select(&[&solid]);
    h.tools().lock().set(Tool::Selection);
    h.session().lock().motion_sketch = Some(crate::session::SketchArm {
        layer: solid.clone(),
        speed: 100.0,
        smoothing: 0.0,
    });
    // Seven frames of a straight drag (1/60 s each) from (100, 100) to (400, 300).
    h.drag_to((100.0, 100.0), (400.0, 300.0), Modifiers::NONE);
    let r = h.release((400.0, 300.0), Modifiers::NONE);
    assert!(h.session().lock().motion_sketch.is_none(), "disarmed");
    let ids: Vec<&str> = r.invocations.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(
        ids,
        ["attr.set_animated", "attr.set_many"],
        "the Selection tool did not move the layer"
    );
    h.apply(&r);
    let spec = query::attr_spec(&h.stage, &solid, "transform:position").unwrap();
    assert!(spec.keyframes.len() >= 2, "{}", spec.keyframes.len());
    let first = spec.keyframes.values().next().unwrap();
    let last = spec.keyframes.values().last().unwrap();
    assert!(
        matches!(first.value, Value::Vec3(p) if (p[0] - 100.0).abs() < 1.0 && (p[1] - 100.0).abs() < 1.0)
    );
    assert!(
        matches!(last.value, Value::Vec3(p) if (p[0] - 400.0).abs() < 1.0 && (p[1] - 300.0).abs() < 1.0)
    );
}
