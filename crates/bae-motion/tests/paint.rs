//! `paint.add_stroke`: the first stroke adds the Paint effect, strokes keep the
//! brush settings, Write On animates End, and the evaluator sees the strokes.

use bae_model::{AuthorId, CommandRegistry, PrimPath, Stage, Time, Value};
use serde_json::{Value as Json, json};

fn run(reg: &CommandRegistry, stage: &mut Stage, id: &str, params: Json) -> Json {
    let author = AuthorId::new("user:test").unwrap();
    reg.execute(stage, &author, id, params)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
        .output
}

fn p(v: &Json) -> PrimPath {
    PrimPath::new(v.as_str().unwrap()).unwrap()
}

#[test]
fn strokes_go_on_one_paint_effect_and_evaluate() {
    let mut reg = CommandRegistry::with_builtins();
    bae_motion::register_commands(&mut reg);
    let mut stage = Stage::new();
    let comp = p(&run(
        &reg,
        &mut stage,
        "comp.create",
        json!({"width": 640, "height": 360, "fps": 30}),
    )["path"]);
    let layer = p(&run(
        &reg,
        &mut stage,
        "layer.create",
        json!({"comp": comp, "kind": "solid"}),
    )["path"]);
    let sec = Time::from_seconds(1.0);
    let a = run(
        &reg,
        &mut stage,
        "paint.add_stroke",
        json!({"layer": layer, "points": [[10.0, 10.0], [50.0, 20.0]], "time": 0, "color": [1.0, 0.0, 0.0, 1.0], "diameter": 20.0}),
    );
    let b = run(
        &reg,
        &mut stage,
        "paint.add_stroke",
        json!({"layer": layer, "points": [[0.0, 0.0], [5.0, 5.0]], "time": sec.0,
               "duration": "write_on", "draw_time": sec.0}),
    );
    assert_eq!(a["effect"], b["effect"], "one Paint effect");
    let effect = p(&a["effect"]);
    assert_eq!(bae_motion::paint::strokes_of(&stage, &effect).len(), 2);
    let ev = bae_anim::evaluate_comp(&stage, &comp, Time::from_seconds(1.5)).unwrap();
    let fx = &ev.layer(&layer).unwrap().effects[0];
    assert_eq!(fx.match_name, bae_motion::paint::PAINT);
    let (first, second) = (&fx.children[0], &fx.children[1]);
    assert_eq!(first.get("name"), Some(&Value::String("Brush 1".into())));
    assert_eq!(first.get("diameter"), Some(&Value::Float(20.0)));
    assert_eq!(
        first.get("color"),
        Some(&Value::Color([1.0, 0.0, 0.0, 1.0]))
    );
    // Write On: End goes 0 → 100 % over the second it took to draw.
    assert_eq!(second.f64("end"), Some(50.0));
    // One stroke is one undo step: undo removes the second stroke only.
    stage.undo(&AuthorId::new("user:test").unwrap());
    assert_eq!(bae_motion::paint::strokes_of(&stage, &effect).len(), 1);
}
