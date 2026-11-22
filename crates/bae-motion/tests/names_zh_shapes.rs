//! Shapes and masks drawn with the shape and Pen tools are named in the UI
//! language (#152): "椭圆 1", "椭圆路径 1", "形状 1", "蒙版 1". Its own test binary:
//! the naming language is process-wide.

use bae_model::{AuthorId, CommandRegistry, PrimPath, Stage};
use bae_motion::query;
use serde_json::{Value as Json, json};

fn run(reg: &CommandRegistry, stage: &mut Stage, id: &str, params: Json) -> Json {
    reg.execute(stage, &AuthorId::new("user:test").unwrap(), id, params)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
        .output
}

fn name(stage: &Stage, path: &str) -> String {
    query::display_name_of(stage, &PrimPath::new(path).unwrap())
}

#[test]
fn shapes_and_masks_drawn_with_the_tools_are_named_in_chinese() {
    let mut reg = CommandRegistry::with_builtins();
    bae_motion::register_commands(&mut reg);
    let mut stage = Stage::new();
    bae_motion::names::set_chinese(true);
    let comp = run(
        &reg,
        &mut stage,
        "comp.create",
        json!({"width": 640, "height": 360}),
    );
    let comp = comp["path"].as_str().unwrap().to_owned();
    let out = run(
        &reg,
        &mut stage,
        "shape.add",
        json!({"comp": comp, "kind": "ellipse", "size": [10.0, 10.0],
               "fill": {"color": [1.0, 0.0, 0.0, 1.0]}, "stroke": {"color": [1.0, 1.0, 1.0, 1.0]}}),
    );
    let group = out["group"].as_str().unwrap().to_owned();
    assert_eq!(name(&stage, &group), "椭圆 1");
    let items: Vec<String> = stage
        .children(&PrimPath::new(&format!("{group}/contents")).unwrap())
        .iter()
        .map(|c| query::display_name_of(&stage, c))
        .collect();
    assert_eq!(items, ["椭圆路径 1", "描边 1", "填充 1"]);
    let pen = json!({"vertices": [[0.0, 0.0]], "in_tangents": [[0.0, 0.0]],
                     "out_tangents": [[0.0, 0.0]], "closed": false});
    let layer = out["layer"].as_str().unwrap().to_owned();
    let out = run(
        &reg,
        &mut stage,
        "shape.add",
        json!({"layer": layer, "kind": "path", "path": pen}),
    );
    assert_eq!(name(&stage, out["group"].as_str().unwrap()), "形状 1");
    let solid = run(
        &reg,
        &mut stage,
        "layer.create",
        json!({"comp": comp, "kind": "solid"}),
    );
    let mask = run(
        &reg,
        &mut stage,
        "mask.create",
        json!({"layer": solid["path"], "kind": "rect", "size": [10.0, 10.0]}),
    );
    assert_eq!(name(&stage, mask["mask"].as_str().unwrap()), "蒙版 1");
    bae_motion::names::set_chinese(false);
}
